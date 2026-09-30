//! Property access: [[Get]], [[Set]], definitions, deletion, keys,
//! inline-cache slow paths, globals and iteration
//!
//! Some own properties are virtual (computed rather than stored): the
//! `length` of arrays and strings and the characters of strings. Functions
//! create their `length`, `name` and `prototype` lazily, the first time
//! anything looks at them.

use std::cell::Cell;

use crate::bytecode::{Ic, IcState};
use crate::gc::Gc;
use crate::number::number_to_string;
use crate::object::*;
use crate::shape::ShapeId;
use crate::string::{Atom, atoms};
use crate::value::Value;

use super::ops::Hint;
use super::{JsResult, Vm};

/// An own property
#[derive(Clone, Copy)]
pub(crate) enum Own {
    Slot(Slot, PropFlags),
    Virtual(Value, PropFlags),
}

#[inline]
fn is_lazy_key(key: PropertyKey) -> bool {
    matches!(key, PropertyKey::Atom(a) if a == atoms::length || a == atoms::name || a == atoms::prototype)
}

impl Vm {
    // ---- own properties ----

    /// Create a function's lazy built-in properties
    pub(crate) fn materialize(&mut self, o: Gc<JsObject>) {
        let lazy = o.get().lazy;
        if lazy == 0 {
            return;
        }
        o.get_mut().lazy = 0;
        let (name, length) = match &o.get().kind {
            ObjectKind::Function(c) => (c.proto.name, c.proto.length as u32),
            ObjectKind::Native(n) => (n.name, n.length),
            _ => return,
        };
        if lazy & LAZY_LENGTH != 0 {
            self.add_prop(o, PropertyKey::Atom(atoms::length), Value::int(length as i32), PropFlags::READONLY_HIDDEN);
        }
        if lazy & LAZY_NAME != 0 {
            let v = self.atom_value(name);
            self.add_prop(o, PropertyKey::Atom(atoms::name), v, PropFlags::READONLY_HIDDEN);
        }
        if lazy & LAZY_PROTOTYPE != 0 {
            let is_gen = matches!(&o.get().kind, ObjectKind::Function(c) if c.proto.is_generator);
            if is_gen {
                // Generator functions: prototype of their generator objects
                let gp = self.realm.generator_proto;
                let p = self.new_object_with(Some(gp), ObjectKind::Ordinary);
                self.add_prop(o, PropertyKey::Atom(atoms::prototype), Value::object(p), PropFlags(PropFlags::WRITABLE));
            } else {
                let p = self.new_object();
                self.add_prop(p, PropertyKey::Atom(atoms::constructor), Value::object(o), PropFlags::HIDDEN);
                self.add_prop(o, PropertyKey::Atom(atoms::prototype), Value::object(p), PropFlags(PropFlags::WRITABLE));
            }
        }
    }

    #[inline]
    pub(crate) fn own_prop(&mut self, o: Gc<JsObject>, key: PropertyKey) -> Option<Own> {
        if o.get().lazy != 0 && is_lazy_key(key) {
            self.materialize(o);
        }
        let ob = o.get();
        match &ob.kind {
            ObjectKind::Array { length } if key == PropertyKey::Atom(atoms::length) => {
                return Some(Own::Virtual(Value::number(*length as f64), PropFlags(PropFlags::WRITABLE)));
            }
            ObjectKind::TypedArray(t) => {
                if let PropertyKey::Index(i) = key {
                    let _ = t;
                    return crate::builtins::typedarray::ta_get(ob, i).map(|v| Own::Virtual(v, PropFlags::DEFAULT));
                }
            }
            ObjectKind::String(s) => match key {
                PropertyKey::Index(i) if i < s.get().len() => {
                    let u = s.get().code_unit_at(i).unwrap();
                    let c = self.new_string_units(&[u]);
                    return Some(Own::Virtual(Value::string(c), PropFlags(PropFlags::ENUMERABLE)));
                }
                PropertyKey::Atom(atoms::length) => return Some(Own::Virtual(Value::int(s.get().len() as i32), PropFlags::NONE)),
                _ => {}
            },
            _ => {}
        }
        ob.find_own(&self.shapes, key).map(|(s, f)| Own::Slot(s, f))
    }

    /// Add a property known to be absent
    pub(crate) fn add_prop(&mut self, o: Gc<JsObject>, key: PropertyKey, v: Value, flags: PropFlags) -> Slot {
        let ob = o.get_mut();
        if let (PropertyKey::Index(i), ObjectKind::Array { length }) = (key, &mut ob.kind) {
            if i >= *length {
                *length = i + 1;
            }
        }
        if ob.is_prototype {
            self.proto_epoch = self.proto_epoch.wrapping_add(1);
        }
        self.heap.note_growth(8);
        ob.add_property(&mut self.shapes, key, v, flags)
    }

    pub fn has_own_property(&mut self, o: Gc<JsObject>, key: PropertyKey) -> bool {
        self.own_prop(o, key).is_some()
    }

    pub fn has_property(&mut self, o: Gc<JsObject>, key: PropertyKey) -> bool {
        let mut cur = o;
        loop {
            if crate::builtins::proxy::is_proxy(cur) {
                return self.proxy_has(cur, key).unwrap_or(false);
            }
            if self.own_prop(cur, key).is_some() {
                return true;
            }
            match cur.get().proto {
                Some(p) => cur = p,
                None => return false,
            }
        }
    }

    // ---- [[Get]] ----

    pub fn get(&mut self, v: Value, key: PropertyKey) -> JsResult<Value> {
        let start = match v.as_object() {
            Some(o) => o,
            None => {
                if let Some(s) = v.as_string() {
                    match key {
                        PropertyKey::Index(i) => {
                            if let Some(u) = s.get().code_unit_at(i) {
                                return Ok(Value::string(self.new_string_units(&[u])));
                            }
                        }
                        PropertyKey::Atom(atoms::length) => return Ok(Value::int(s.get().len() as i32)),
                        _ => {}
                    }
                }
                self.primitive_proto(v, key)?
            }
        };
        self.get_from(start, key, v)
    }

    /// Prototype to look up properties of a primitive in
    fn primitive_proto(&mut self, v: Value, key: PropertyKey) -> JsResult<Gc<JsObject>> {
        if v.is_string() {
            Ok(self.realm.string_proto)
        } else if v.is_number() {
            Ok(self.realm.number_proto)
        } else if v.is_bool() {
            Ok(self.realm.boolean_proto)
        } else if v.is_symbol() {
            Ok(self.realm.symbol_proto)
        } else {
            let k = self.key_display(key);
            Err(self.type_error(&format!("Cannot read properties of {v:?} (reading '{k}')")))
        }
    }

    pub(crate) fn key_display(&self, key: PropertyKey) -> String {
        match key {
            PropertyKey::Atom(a) => self.atoms.string(a).get().to_rust_string(),
            PropertyKey::Index(i) => i.to_string(),
            PropertyKey::Symbol(s) => match s.get().description {
                Some(d) => format!("Symbol({})", d.get().to_rust_string()),
                None => "Symbol()".into(),
            },
        }
    }

    pub(crate) fn get_from(&mut self, o: Gc<JsObject>, key: PropertyKey, receiver: Value) -> JsResult<Value> {
        let mut cur = o;
        loop {
            if let ObjectKind::Proxy(_) = cur.get().kind {
                return self.proxy_get(cur, key, receiver);
            }
            match self.own_prop(cur, key) {
                Some(Own::Slot(slot, flags)) => {
                    let v = cur.get().read(slot);
                    if flags.is_accessor() {
                        return self.call_getter(v, receiver);
                    }
                    return Ok(v);
                }
                Some(Own::Virtual(v, _)) => return Ok(v),
                None => {
                    // Typed arrays have no inherited integer properties
                    if matches!(key, PropertyKey::Index(_)) && cur.get().is_typed_array() {
                        return Ok(Value::UNDEFINED);
                    }
                    match cur.get().proto {
                        Some(p) => cur = p,
                        None => return Ok(Value::UNDEFINED),
                    }
                }
            }
        }
    }

    fn call_getter(&mut self, accessor: Value, receiver: Value) -> JsResult<Value> {
        let getter = match &accessor.as_object().unwrap().get().kind {
            ObjectKind::Accessor { getter, .. } => *getter,
            _ => Value::UNDEFINED,
        };
        if getter.is_undefined() {
            return Ok(Value::UNDEFINED);
        }
        self.call(getter, receiver, &[])
    }

    pub fn get_str(&mut self, v: Value, name: &str) -> JsResult<Value> {
        let a = self.intern(name);
        self.get(v, PropertyKey::Atom(a))
    }

    pub(crate) fn get_prop_ic(&mut self, v: Value, cell: &Cell<Ic>) -> JsResult<Value> {
        let atom = cell.get().atom;
        let key = PropertyKey::Atom(atom);
        if let Some(o) = v.as_object() {
            if crate::builtins::proxy::is_proxy(o) {
                return self.proxy_get(o, key, v);
            }
            if atom == atoms::length && o.get().is_array() {
                set_state(cell, IcState::ArrayLength);
                return self.get(v, key);
            }
            let mut cur = o;
            let mut depth = 0;
            loop {
                match self.own_prop(cur, key) {
                    Some(Own::Slot(Slot::Named(i), flags)) if !flags.is_accessor() => {
                        let ob = o.get();
                        if ob.shape != ShapeId::DICT && cur.get().shape != ShapeId::DICT {
                            if depth == 0 {
                                set_state(cell, IcState::Own { shape: ob.shape, slot: i });
                            } else if let Some(p) = ob.proto {
                                set_state(cell, IcState::Proto { shape: ob.shape, proto: p, holder: cur, slot: i, epoch: self.proto_epoch });
                            }
                        }
                        return Ok(cur.get().slots[i as usize]);
                    }
                    Some(Own::Slot(slot, flags)) => {
                        let val = cur.get().read(slot);
                        if flags.is_accessor() {
                            return self.call_getter(val, v);
                        }
                        return Ok(val);
                    }
                    Some(Own::Virtual(val, _)) => return Ok(val),
                    None => match cur.get().proto {
                        Some(p) => {
                            cur = p;
                            depth += 1;
                        }
                        None => return Ok(Value::UNDEFINED),
                    },
                }
            }
        }
        if v.is_string() {
            if atom == atoms::length {
                set_state(cell, IcState::StringLength);
                return self.get(v, key);
            }
            let sp = self.realm.string_proto;
            let mut cur = sp;
            loop {
                match self.own_prop(cur, key) {
                    Some(Own::Slot(Slot::Named(i), flags)) if !flags.is_accessor() => {
                        if cur.get().shape != ShapeId::DICT {
                            set_state(
                                cell,
                                IcState::Proto { shape: ShapeId::PRIMITIVE_STRING, proto: sp, holder: cur, slot: i, epoch: self.proto_epoch },
                            );
                        }
                        return Ok(cur.get().slots[i as usize]);
                    }
                    Some(_) => return self.get_from(cur, key, v),
                    None => match cur.get().proto {
                        Some(p) => cur = p,
                        None => return Ok(Value::UNDEFINED),
                    },
                }
            }
        }
        self.get(v, key)
    }

    pub(crate) fn get_elem(&mut self, v: Value, k: Value) -> JsResult<Value> {
        let key = self.to_property_key(k)?;
        self.get(v, key)
    }

    // ---- [[Set]] ----

    /// OrdinarySet on `o` with `receiver`; false if the assignment failed
    pub(crate) fn set_on(&mut self, o: Gc<JsObject>, key: PropertyKey, val: Value, receiver: Value) -> JsResult<bool> {
        if crate::builtins::proxy::is_proxy(o) {
            return self.proxy_set(o, key, val, receiver);
        }
        if let PropertyKey::Index(i) = key {
            if o.get().is_typed_array() && Value::object(o) == receiver {
                let n = self.to_number(val)?;
                crate::builtins::typedarray::ta_set(o.get(), i, n);
                return Ok(true);
            }
        }
        let mut cur = o;
        loop {
            match self.own_prop(cur, key) {
                Some(Own::Slot(slot, flags)) => {
                    if flags.is_accessor() {
                        let acc = cur.get().read(slot);
                        let setter = match &acc.as_object().unwrap().get().kind {
                            ObjectKind::Accessor { setter, .. } => *setter,
                            _ => Value::UNDEFINED,
                        };
                        if setter.is_undefined() {
                            return Ok(false);
                        }
                        self.call(setter, receiver, &[val])?;
                        return Ok(true);
                    }
                    if !flags.writable() {
                        return Ok(false);
                    }
                    if Value::object(cur) == receiver {
                        cur.get_mut().write(slot, val);
                        return Ok(true);
                    }
                    break;
                }
                Some(Own::Virtual(_, flags)) => {
                    if Value::object(cur) == receiver && cur.get().is_array() && key == PropertyKey::Atom(atoms::length) {
                        self.set_array_length(cur, val)?;
                        return Ok(true);
                    }
                    if !flags.writable() {
                        return Ok(false);
                    }
                    break;
                }
                None => match cur.get().proto {
                    Some(p) => cur = p,
                    None => break,
                },
            }
        }
        let Some(r) = receiver.as_object() else { return Ok(false) };
        if crate::builtins::proxy::is_proxy(r) {
            // Receiver.[[DefineOwnProperty]] through the proxy: just the
            // value if the property exists, else a new data property
            let existing = self.proxy_get_own_property(r, key)?;
            let d = self.new_object();
            self.define_value(d, PropertyKey::Atom(atoms::value), val, PropFlags::DEFAULT);
            if existing.is_undefined() {
                for a in [atoms::writable, atoms::enumerable, atoms::configurable] {
                    self.define_value(d, PropertyKey::Atom(a), Value::TRUE, PropFlags::DEFAULT);
                }
            } else {
                let g = self.get(existing, PropertyKey::Atom(atoms::get))?;
                let st = self.get(existing, PropertyKey::Atom(atoms::set))?;
                let w = self.get(existing, PropertyKey::Atom(atoms::writable))?;
                if !g.is_undefined() || !st.is_undefined() || !super::ops::truthy(w) {
                    return Ok(false);
                }
            }
            return self.proxy_define(r, key, Value::object(d));
        }
        if r != o {
            // Assignment through a prototype (or super): define on the receiver
            if let Some(Own::Slot(slot, flags)) = self.own_prop(r, key) {
                if flags.is_accessor() || !flags.writable() {
                    return Ok(false);
                }
                r.get_mut().write(slot, val);
                return Ok(true);
            }
        }
        if !r.get().extensible {
            return Ok(false);
        }
        self.add_prop(r, key, val, PropFlags::DEFAULT);
        Ok(true)
    }

    /// [[Set]] on any value, throwing on failure in strict mode
    pub fn set(&mut self, v: Value, key: PropertyKey, val: Value, strict: bool) -> JsResult<()> {
        let ok = match v.as_object() {
            Some(o) => self.set_on(o, key, val, v)?,
            None => {
                let p = match self.primitive_proto(v, key) {
                    Ok(p) => p,
                    Err(_) => {
                        let k = self.key_display(key);
                        return Err(self.type_error(&format!("Cannot set properties of {v:?} (setting '{k}')")));
                    }
                };
                // Only setters on the prototype chain do anything
                self.set_on(p, key, val, v)? && false
            }
        };
        if !ok && strict {
            let k = self.key_display(key);
            return Err(self.type_error(&format!("Cannot assign to read only property '{k}' of object")));
        }
        Ok(())
    }

    pub(crate) fn set_prop_ic(&mut self, v: Value, val: Value, cell: &Cell<Ic>, strict: bool) -> JsResult<()> {
        let key = PropertyKey::Atom(cell.get().atom);
        let Some(o) = v.as_object() else {
            return self.set(v, key, val, strict);
        };
        if let Some(Own::Slot(Slot::Named(i), flags)) = self.own_prop(o, key) {
            if flags.writable() && !flags.is_accessor() {
                o.get_mut().slots[i as usize] = val;
                if o.get().shape != ShapeId::DICT {
                    set_state(cell, IcState::Own { shape: o.get().shape, slot: i });
                }
                return Ok(());
            }
        }
        let before = o.get().shape;
        let before_len = o.get().slots.len();
        let ok = self.set_on(o, key, val, v)?;
        let ob = o.get();
        if ok && before != ShapeId::DICT && ob.shape != ShapeId::DICT && ob.shape != before && ob.slots.len() == before_len + 1 && !ob.is_prototype {
            set_state(cell, IcState::Add { from: before, to: ob.shape, slot: before_len as u32, proto: ob.proto, epoch: self.proto_epoch });
        }
        if !ok && strict {
            let k = self.key_display(key);
            return Err(self.type_error(&format!("Cannot assign to read only property '{k}' of object")));
        }
        Ok(())
    }

    pub(crate) fn set_elem(&mut self, v: Value, k: Value, val: Value, strict: bool) -> JsResult<()> {
        let key = self.to_property_key(k)?;
        self.set(v, key, val, strict)
    }

    pub(crate) fn set_array_length(&mut self, o: Gc<JsObject>, v: Value) -> JsResult<()> {
        let n = self.to_number(v)?;
        if !(0.0..4294967296.0).contains(&n) || n.fract() != 0.0 {
            return Err(self.range_error("Invalid array length"));
        }
        let n = n as u32;
        let ob = o.get_mut();
        if (n as usize) < ob.elements.len() {
            ob.elements.truncate(n as usize);
        }
        if let Some(dict) = &ob.dict {
            let _ = dict;
            let doomed: Vec<PropertyKey> = ob
                .own_keys(&self.shapes)
                .into_iter()
                .filter_map(|(k, _)| match k {
                    PropertyKey::Index(i) if i >= n => Some(k),
                    _ => None,
                })
                .collect();
            for k in doomed {
                ob.remove_property(&self.shapes, k);
            }
        }
        if let ObjectKind::Array { length } = &mut ob.kind {
            *length = n;
        }
        Ok(())
    }

    // ---- definitions ----

    /// Define (or redefine) an own data property. Never calls scripts.
    pub fn define_value(&mut self, o: Gc<JsObject>, key: PropertyKey, v: Value, flags: PropFlags) {
        if let (PropertyKey::Index(i), true) = (key, o.get().is_typed_array()) {
            if let Some(n) = v.as_number() {
                crate::builtins::typedarray::ta_set(o.get(), i, n);
            }
            return;
        }
        match self.own_prop(o, key) {
            Some(Own::Slot(slot, old)) => {
                if old == flags {
                    o.get_mut().write(slot, v);
                    return;
                }
                if o.get().is_prototype {
                    self.proto_epoch = self.proto_epoch.wrapping_add(1);
                }
                o.get_mut().set_flags(&self.shapes, key, flags);
                if let Some((slot, _)) = o.get().find_own(&self.shapes, key) {
                    o.get_mut().write(slot, v);
                }
            }
            Some(Own::Virtual(..)) => {
                if o.get().is_array() && key == PropertyKey::Atom(atoms::length) {
                    let _ = self.set_array_length(o, v);
                }
            }
            None => {
                self.add_prop(o, key, v, flags);
            }
        }
    }

    pub(crate) fn define_prop_ic(&mut self, o: Gc<JsObject>, v: Value, cell: &Cell<Ic>) {
        let key = PropertyKey::Atom(cell.get().atom);
        let before = o.get().shape;
        let before_len = o.get().slots.len();
        let existed = self.own_prop(o, key).is_some();
        self.define_value(o, key, v, PropFlags::DEFAULT);
        let ob = o.get();
        if !existed && before != ShapeId::DICT && ob.shape != ShapeId::DICT && ob.slots.len() == before_len + 1 && !ob.is_prototype && ob.lazy == 0 {
            set_state(cell, IcState::Add { from: before, to: ob.shape, slot: before_len as u32, proto: None, epoch: 0 });
        }
    }

    /// Define or update an accessor property (`None` keeps that half)
    pub fn define_accessor(&mut self, o: Gc<JsObject>, key: PropertyKey, getter: Option<Value>, setter: Option<Value>, flags: PropFlags) {
        let flags = PropFlags(flags.0 | PropFlags::ACCESSOR) .with(PropFlags::WRITABLE, false);
        if let Some(Own::Slot(slot, old)) = self.own_prop(o, key) {
            if old.is_accessor() {
                let acc = o.get().read(slot).as_object().unwrap();
                if let ObjectKind::Accessor { getter: g, setter: s } = &mut acc.get_mut().kind {
                    if let Some(v) = getter {
                        *g = v;
                    }
                    if let Some(v) = setter {
                        *s = v;
                    }
                }
                if old != flags {
                    if o.get().is_prototype {
                        self.proto_epoch = self.proto_epoch.wrapping_add(1);
                    }
                    o.get_mut().set_flags(&self.shapes, key, flags);
                }
                return;
            }
        }
        let acc = self.new_object_with(
            None,
            ObjectKind::Accessor { getter: getter.unwrap_or(Value::UNDEFINED), setter: setter.unwrap_or(Value::UNDEFINED) },
        );
        self.define_value(o, key, Value::object(acc), flags);
    }

    /// Accessor pair of an accessor property
    pub(crate) fn accessor_pair(&self, v: Value) -> (Value, Value) {
        match &v.as_object().unwrap().get().kind {
            ObjectKind::Accessor { getter, setter } => (*getter, *setter),
            _ => (Value::UNDEFINED, Value::UNDEFINED),
        }
    }

    pub fn delete_property(&mut self, v: Value, key: PropertyKey, strict: bool) -> JsResult<bool> {
        if v.is_nullish() {
            return Err(self.type_error("Cannot convert undefined or null to object"));
        }
        let Some(o) = v.as_object() else {
            return Ok(true);
        };
        if crate::builtins::proxy::is_proxy(o) {
            let ok = self.proxy_delete(o, key)?;
            if !ok && strict {
                let k = self.key_display(key);
                return Err(self.type_error(&format!("Cannot delete property '{k}'")));
            }
            return Ok(ok);
        }
        self.materialize(o);
        match self.own_prop(o, key) {
            None => Ok(true),
            Some(Own::Virtual(..)) => {
                if strict {
                    let k = self.key_display(key);
                    return Err(self.type_error(&format!("Cannot delete property '{k}'")));
                }
                Ok(false)
            }
            Some(Own::Slot(_, flags)) => {
                if !flags.configurable() {
                    if strict {
                        let k = self.key_display(key);
                        return Err(self.type_error(&format!("Cannot delete property '{k}'")));
                    }
                    return Ok(false);
                }
                if o.get().is_prototype {
                    self.proto_epoch = self.proto_epoch.wrapping_add(1);
                }
                o.get_mut().remove_property(&self.shapes, key);
                Ok(true)
            }
        }
    }

    // ---- keys ----

    pub fn to_property_key(&mut self, v: Value) -> JsResult<PropertyKey> {
        if let Some(i) = v.as_int() {
            if i >= 0 {
                return Ok(PropertyKey::Index(i as u32));
            }
            let a = self.intern(&i.to_string());
            return Ok(PropertyKey::Atom(a));
        }
        if let Some(s) = v.as_string() {
            let st = s.get();
            let units = st.units();
            if let Some(i) = units.as_array_index() {
                return Ok(PropertyKey::Index(i));
            }
            if let Some(a) = st.atom_id() {
                return Ok(PropertyKey::Atom(a));
            }
            return Ok(PropertyKey::Atom(self.atoms.intern(s)));
        }
        if let Some(n) = v.as_number() {
            if n >= 0.0 && n < 4294967295.0 && n.fract() == 0.0 {
                return Ok(PropertyKey::Index(n as u32));
            }
            let a = self.intern(&number_to_string(n));
            return Ok(PropertyKey::Atom(a));
        }
        if let Some(s) = v.as_symbol() {
            return Ok(PropertyKey::Symbol(s));
        }
        let atom = match v {
            Value::UNDEFINED => atoms::undefined,
            Value::NULL => atoms::null,
            Value::TRUE => atoms::true_,
            Value::FALSE => atoms::false_,
            _ => {
                let p = self.to_primitive(v, Hint::String)?;
                return self.to_property_key(p);
            }
        };
        Ok(PropertyKey::Atom(atom))
    }

    /// A key as a value (strings for names and indices)
    pub fn key_value(&mut self, key: PropertyKey) -> Value {
        match key {
            PropertyKey::Atom(a) => self.atom_value(a),
            PropertyKey::Index(i) => Value::string(self.number_to_js_string(i as f64)),
            PropertyKey::Symbol(s) => Value::symbol(s),
        }
    }

    pub fn key_from_str(&mut self, s: &str) -> PropertyKey {
        match crate::string::Units::Latin1(s.as_bytes()).as_array_index() {
            Some(i) if s.is_ascii() => PropertyKey::Index(i),
            _ => PropertyKey::Atom(self.intern(s)),
        }
    }

    /// Own keys with attributes, in property order, including virtual
    /// ones and excluding private names
    pub fn own_keys(&mut self, o: Gc<JsObject>) -> Vec<(PropertyKey, PropFlags)> {
        if crate::builtins::proxy::is_proxy(o) {
            return self.own_keys_js(o).unwrap_or_default();
        }
        self.materialize(o);
        let mut keys = o.get().own_keys(&self.shapes);
        match &o.get().kind {
            ObjectKind::String(s) => {
                let n = s.get().len();
                let mut v: Vec<(PropertyKey, PropFlags)> = (0..n).map(|i| (PropertyKey::Index(i), PropFlags(PropFlags::ENUMERABLE))).collect();
                v.push((PropertyKey::Atom(atoms::length), PropFlags::NONE));
                v.extend(keys);
                keys = v;
            }
            ObjectKind::TypedArray(t) => {
                let mut v: Vec<(PropertyKey, PropFlags)> = (0..t.length).map(|i| (PropertyKey::Index(i), PropFlags::DEFAULT)).collect();
                v.extend(keys);
                keys = v;
            }
            ObjectKind::Array { .. } => {
                let split = keys.iter().position(|(k, _)| !matches!(k, PropertyKey::Index(_))).unwrap_or(keys.len());
                keys.insert(split, (PropertyKey::Atom(atoms::length), PropFlags(PropFlags::WRITABLE)));
            }
            _ => {}
        }
        keys.retain(|(k, _)| !matches!(k, PropertyKey::Symbol(s) if s.get().is_private));
        keys
    }

    /// Values of own enumerable string-keyed properties (Object.keys order)
    pub fn enumerable_own_keys(&mut self, o: Gc<JsObject>) -> Vec<PropertyKey> {
        self.own_keys(o).into_iter().filter(|(k, f)| f.enumerable() && !matches!(k, PropertyKey::Symbol(_))).map(|(k, _)| k).collect()
    }

    /// `with` scope lookup: the object has the property and
    /// @@unscopables doesn't hide it
    pub(crate) fn with_has(&mut self, obj: Value, name: Atom) -> JsResult<bool> {
        let o = self.to_object(obj)?;
        let key = PropertyKey::Atom(name);
        if !self.has_property_js(o, key)? {
            return Ok(false);
        }
        let unscopables = self.get(Value::object(o), PropertyKey::Symbol(self.sym.unscopables))?;
        if unscopables.is_object() {
            let blocked = self.get(unscopables, key)?;
            if super::ops::truthy(blocked) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    // ---- globals ----

    pub(crate) fn get_global_slow(&mut self, cell: &Cell<Ic>) -> JsResult<Value> {
        let atom = cell.get().atom;
        let key = PropertyKey::Atom(atom);
        let lex = self.global_lex;
        if let Some((Slot::Named(i), _)) = lex.get().find_own(&self.shapes, key) {
            set_state(cell, IcState::GlobalLex { slot: i });
            let v = lex.get().slots[i as usize];
            if v.is_hole() {
                let n = self.atoms.string(atom).get().to_rust_string();
                return Err(self.reference_error(&format!("Cannot access '{n}' before initialization")));
            }
            return Ok(v);
        }
        let g = self.global;
        if let Some(Own::Slot(Slot::Named(i), flags)) = self.own_prop(g, key) {
            if !flags.is_accessor() && g.get().shape != ShapeId::DICT {
                set_state(cell, IcState::Own { shape: g.get().shape, slot: i });
                return Ok(g.get().slots[i as usize]);
            }
        }
        if self.has_property(g, key) {
            return self.get_from(g, key, Value::object(g));
        }
        let n = self.atoms.string(atom).get().to_rust_string();
        Err(self.reference_error(&format!("{n} is not defined")))
    }

    pub(crate) fn set_global_slow(&mut self, cell: &Cell<Ic>, v: Value, strict: bool) -> JsResult<()> {
        let atom = cell.get().atom;
        let key = PropertyKey::Atom(atom);
        let lex = self.global_lex;
        if let Some((Slot::Named(i), flags)) = lex.get().find_own(&self.shapes, key) {
            let n = self.atoms.string(atom).get().to_rust_string();
            if lex.get().slots[i as usize].is_hole() {
                return Err(self.reference_error(&format!("Cannot access '{n}' before initialization")));
            }
            if !flags.writable() {
                return Err(self.type_error("Assignment to constant variable."));
            }
            lex.get_mut().slots[i as usize] = v;
            set_state(cell, IcState::GlobalLex { slot: i });
            return Ok(());
        }
        let g = self.global;
        if let Some(Own::Slot(Slot::Named(i), flags)) = self.own_prop(g, key) {
            if flags.writable() && !flags.is_accessor() && g.get().shape != ShapeId::DICT {
                g.get_mut().slots[i as usize] = v;
                set_state(cell, IcState::Own { shape: g.get().shape, slot: i });
                return Ok(());
            }
        }
        if self.has_property(g, key) {
            return self.set(Value::object(g), key, v, strict);
        }
        if strict {
            let n = self.atoms.string(atom).get().to_rust_string();
            return Err(self.reference_error(&format!("{n} is not defined")));
        }
        self.add_prop(g, key, v, PropFlags::DEFAULT);
        Ok(())
    }

    pub(crate) fn typeof_global(&mut self, cell: &Cell<Ic>) -> JsResult<Value> {
        let key = PropertyKey::Atom(cell.get().atom);
        let lex = self.global_lex;
        let g = self.global;
        let v = if lex.get().find_own(&self.shapes, key).is_some() || self.has_property(g, key) {
            self.get_global_slow(cell)?
        } else {
            Value::UNDEFINED
        };
        let t = self.typeof_atom(v);
        Ok(self.atom_value(t))
    }

    pub(crate) fn declare_global_var(&mut self, atom: Atom) {
        let g = self.global;
        let key = PropertyKey::Atom(atom);
        if self.own_prop(g, key).is_none() {
            self.add_prop(g, key, Value::UNDEFINED, PropFlags(PropFlags::WRITABLE | PropFlags::ENUMERABLE));
        }
    }

    pub(crate) fn declare_global_func(&mut self, atom: Atom, f: Value) -> JsResult<()> {
        let g = self.global;
        let key = PropertyKey::Atom(atom);
        match self.own_prop(g, key) {
            Some(Own::Slot(slot, flags)) if flags.writable() && !flags.is_accessor() => {
                g.get_mut().write(slot, f);
            }
            _ => self.define_value(g, key, f, PropFlags(PropFlags::WRITABLE | PropFlags::ENUMERABLE)),
        }
        Ok(())
    }

    pub(crate) fn declare_global_lex(&mut self, atom: Atom, is_const: bool) -> JsResult<()> {
        let key = PropertyKey::Atom(atom);
        let lex = self.global_lex;
        let clash = lex.get().find_own(&self.shapes, key).is_some()
            || matches!(self.own_prop(self.global, key), Some(Own::Slot(_, f)) if !f.configurable());
        if clash {
            let n = self.atoms.string(atom).get().to_rust_string();
            return Err(self.make_error(super::ErrorKind::Syntax, &format!("Identifier '{n}' has already been declared")));
        }
        let flags = if is_const { PropFlags::NONE } else { PropFlags(PropFlags::WRITABLE) };
        lex.get_mut().add_property(&mut self.shapes, key, Value::HOLE, flags);
        Ok(())
    }

    pub(crate) fn init_global_lex(&mut self, atom: Atom, v: Value) {
        let lex = self.global_lex;
        if let Some((slot, _)) = lex.get().find_own(&self.shapes, PropertyKey::Atom(atom)) {
            lex.get_mut().write(slot, v);
        }
    }

    // ---- iteration ----

    pub(crate) fn for_in_init(&mut self, v: Value) -> JsResult<Value> {
        let mut keys = Vec::new();
        if !v.is_nullish() {
            let o = self.to_object(v)?;
            let mut seen: rustc_hash::FxHashSet<PropertyKey> = Default::default();
            let mut cur = Some(o);
            while let Some(c) = cur {
                for (k, f) in self.own_keys(c) {
                    if matches!(k, PropertyKey::Symbol(_)) || !seen.insert(k) {
                        continue;
                    }
                    if f.enumerable() {
                        keys.push(self.key_value(k));
                    }
                }
                cur = c.get().proto;
            }
        }
        let it = self.new_object_with(None, ObjectKind::ForIn(Box::new(ForInIterator { keys, pos: 0, object: v })));
        Ok(Value::object(it))
    }

    pub(crate) fn for_in_next(&mut self, it: Value) -> Value {
        let o = it.as_object().unwrap();
        if let ObjectKind::ForIn(f) = &mut o.get_mut().kind {
            if f.pos < f.keys.len() {
                f.pos += 1;
                return f.keys[f.pos - 1];
            }
        }
        Value::HOLE
    }

    pub fn get_iterator(&mut self, v: Value) -> JsResult<Value> {
        let method = self.get(v, PropertyKey::Symbol(self.sym.iterator))?;
        if let Some(m) = method.as_object() {
            if m == self.realm.array_values && v.is_object() {
                let proto = self.realm.array_iterator_proto;
                let it = self.new_object_with(Some(proto), ObjectKind::ArrayIterator { target: v, index: 0, kind: 0 });
                return Ok(Value::object(it));
            }
        }
        if !self.is_callable(method) {
            let d = self.describe(v);
            return Err(self.type_error(&format!("{d} is not iterable")));
        }
        let it = self.call(method, v, &[])?;
        let Some(io) = it.as_object() else {
            return Err(self.type_error("Result of the Symbol.iterator method is not an object"));
        };
        if matches!(io.get().kind, ObjectKind::ArrayIterator { .. } | ObjectKind::StringIterator { .. } | ObjectKind::MapIterator { .. }) {
            return Ok(it);
        }
        let next = self.get(it, PropertyKey::Atom(atoms::next))?;
        let rec = self.new_object_with(None, ObjectKind::IterRecord { iter: it, next, done: false });
        Ok(Value::object(rec))
    }

    /// Next value, or None when done
    pub fn iter_step(&mut self, it: Value) -> JsResult<Option<Value>> {
        let o = it.as_object().unwrap();
        match &mut o.get_mut().kind {
            ObjectKind::ArrayIterator { target, index, kind } => {
                let (t, i, k) = (*target, *index, *kind);
                if t.is_undefined() {
                    return Ok(None);
                }
                let tobj = t.as_object().unwrap();
                let len = match &tobj.get().kind {
                    ObjectKind::Array { length } => *length as f64,
                    _ => {
                        let l = self.get(t, PropertyKey::Atom(atoms::length))?;
                        self.to_length(l)?
                    }
                };
                if i as f64 >= len {
                    if let ObjectKind::ArrayIterator { target, .. } = &mut o.get_mut().kind {
                        *target = Value::UNDEFINED;
                    }
                    return Ok(None);
                }
                if let ObjectKind::ArrayIterator { index, .. } = &mut o.get_mut().kind {
                    *index += 1;
                }
                if k == 1 {
                    return Ok(Some(Value::number(i as f64)));
                }
                let value = match tobj.get().elements.get(i as usize) {
                    Some(&e) if !e.is_hole() => e,
                    _ => self.get(t, PropertyKey::Index(i))?,
                };
                if k == 0 {
                    return Ok(Some(value));
                }
                let pair = self.new_array(vec![Value::number(i as f64), value]);
                Ok(Some(Value::object(pair)))
            }
            ObjectKind::StringIterator { string, pos } => {
                let s = *string;
                let p = *pos;
                let st = s.get();
                if p >= st.len() {
                    return Ok(None);
                }
                let u = st.code_unit_at(p).unwrap();
                let mut end = p + 1;
                if (0xD800..0xDC00).contains(&u) {
                    if let Some(u2) = st.code_unit_at(p + 1) {
                        if (0xDC00..0xE000).contains(&u2) {
                            end = p + 2;
                        }
                    }
                }
                *pos = end;
                Ok(Some(Value::string(self.substring(s, p, end))))
            }
            ObjectKind::MapIterator { map, pos, kind } => {
                let (m, k) = (*map, *kind);
                loop {
                    let p = match &o.get().kind {
                        ObjectKind::MapIterator { pos, .. } => *pos as usize,
                        _ => unreachable!(),
                    };
                    let entry = match &m.get().kind {
                        ObjectKind::Map(d) | ObjectKind::Set(d) => {
                            if p >= d.entries.len() {
                                None
                            } else {
                                Some(d.entries[p])
                            }
                        }
                        _ => None,
                    };
                    let Some(entry) = entry else {
                        if let ObjectKind::MapIterator { pos, .. } = &mut o.get_mut().kind {
                            *pos = u32::MAX;
                        }
                        return Ok(None);
                    };
                    if let ObjectKind::MapIterator { pos, .. } = &mut o.get_mut().kind {
                        *pos += 1;
                    }
                    let _ = pos;
                    if let Some((key, value)) = entry {
                        return Ok(Some(match k {
                            0 => key,
                            1 => value,
                            _ => Value::object(self.new_array(vec![key, value])),
                        }));
                    }
                }
            }
            ObjectKind::IterRecord { iter, next, done } => {
                if *done {
                    return Ok(None);
                }
                let (iter, next) = (*iter, *next);
                // Generators with the built-in next: resume directly
                if next == Value::object(self.realm.generator_next) {
                    if let Some(g) = iter.as_object().filter(|g| matches!(g.get().kind, ObjectKind::Generator(_))) {
                        let (v, finished) = self.resume(g, super::generator::ResumeMode::Next, Value::UNDEFINED)?;
                        if finished {
                            if let ObjectKind::IterRecord { done, .. } = &mut o.get_mut().kind {
                                *done = true;
                            }
                            return Ok(None);
                        }
                        return Ok(Some(v));
                    }
                }
                let r = self.call(next, iter, &[])?;
                if !r.is_object() {
                    return Err(self.type_error("Iterator result is not an object"));
                }
                let d = self.get(r, PropertyKey::Atom(atoms::done))?;
                if super::ops::truthy(d) {
                    if let ObjectKind::IterRecord { done, .. } = &mut o.get_mut().kind {
                        *done = true;
                    }
                    return Ok(None);
                }
                Ok(Some(self.get(r, PropertyKey::Atom(atoms::value))?))
            }
            _ => Ok(None),
        }
    }

    /// `yield*` step: the inner iterator's result object for next(v)
    pub(crate) fn iter_send(&mut self, it: Value, v: Value) -> JsResult<Value> {
        let o = it.as_object().unwrap();
        if let ObjectKind::IterRecord { iter, next, .. } = o.get().kind {
            let r = self.call(next, iter, &[v])?;
            if !r.is_object() {
                return Err(self.type_error("Iterator result is not an object"));
            }
            return Ok(r);
        }
        let r = self.iter_step(it)?;
        Ok(crate::builtins::array::iter_result(self, r.unwrap_or(Value::UNDEFINED), r.is_none()))
    }

    pub fn iter_close(&mut self, it: Value) -> JsResult<()> {
        let o = it.as_object().unwrap();
        if let ObjectKind::IterRecord { iter, done, .. } = &mut o.get_mut().kind {
            if *done {
                return Ok(());
            }
            *done = true;
            let iter = *iter;
            let ret = self.get(iter, PropertyKey::Atom(atoms::return_))?;
            if !ret.is_nullish() {
                self.call(ret, iter, &[])?;
            }
        }
        Ok(())
    }

    /// Append the values of an iterable to an array
    pub(crate) fn array_spread(&mut self, arr: Gc<JsObject>, src: Value) -> JsResult<()> {
        if let Some(s) = src.as_object() {
            if s.get().is_array() && self.array_iteration_is_default(s) {
                let len = match s.get().kind {
                    ObjectKind::Array { length } => length as usize,
                    _ => 0,
                };
                let mut values = Vec::with_capacity(len);
                for i in 0..len {
                    let v = match s.get().elements.get(i) {
                        Some(&e) if !e.is_hole() => e,
                        _ => self.get(src, PropertyKey::Index(i as u32))?,
                    };
                    values.push(v);
                }
                push_values(arr, &values);
                return Ok(());
            }
        }
        let it = self.get_iterator(src)?;
        while let Some(v) = self.iter_step(it)? {
            push_values(arr, &[v]);
        }
        Ok(())
    }

    /// Arrays whose iteration is unmodified (so spreading can read elements)
    pub(crate) fn array_iteration_is_default(&mut self, a: Gc<JsObject>) -> bool {
        let key = PropertyKey::Symbol(self.sym.iterator);
        if a.get().find_own(&self.shapes, key).is_some() {
            return false;
        }
        let ap = self.realm.array_proto;
        matches!(ap.get().find_own(&self.shapes, key), Some((slot, _)) if ap.get().read(slot) == Value::object(self.realm.array_values))
            && a.get().proto == Some(ap)
    }

    /// Copy own enumerable properties (object spread, Object.assign-like)
    pub(crate) fn copy_data_props(&mut self, target: Gc<JsObject>, src: Value, excluded: &[PropertyKey]) -> JsResult<()> {
        if src.is_nullish() {
            return Ok(());
        }
        let from = self.to_object(src)?;
        for (k, f) in self.own_keys(from) {
            if !f.enumerable() || excluded.contains(&k) {
                continue;
            }
            let v = self.get(Value::object(from), k)?;
            self.define_value(target, k, v, PropFlags::DEFAULT);
        }
        Ok(())
    }
}

#[inline]
fn set_state(cell: &Cell<Ic>, state: IcState) {
    let mut ic = cell.get();
    ic.state = state;
    cell.set(ic);
}

fn push_values(arr: Gc<JsObject>, values: &[Value]) {
    let ob = arr.get_mut();
    ob.elements.extend_from_slice(values);
    if let ObjectKind::Array { length } = &mut ob.kind {
        *length += values.len() as u32;
    }
}
