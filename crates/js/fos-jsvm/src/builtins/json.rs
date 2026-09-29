//! JSON

use crate::gc::Gc;
use crate::number::number_to_string;
use crate::object::*;
use crate::string::{Units, atoms};
use crate::value::Value;
use crate::vm::{JsResult, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let j = vm.new_object();
    vm.def_method(j, "parse", 2, parse);
    vm.def_method(j, "stringify", 3, stringify);
    let tag = vm.sym.to_string_tag;
    let s = vm.str_value("JSON");
    vm.define_value(j, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));
    let g = vm.global;
    vm.def_value(g, "JSON", Value::object(j), PropFlags::HIDDEN);
}

struct Parser<'a> {
    s: Units<'a>,
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u16> {
        if self.pos < self.s.len() { Some(self.s.at(self.pos)) } else { None }
    }

    fn ws(&mut self) {
        while let Some(c) = self.peek() {
            if matches!(c, 0x20 | 0x09 | 0x0A | 0x0D) {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn error(&self, vm: &mut Vm) -> Value {
        let msg = match self.peek() {
            Some(c) => format!("Unexpected token '{}' in JSON at position {}", char::from_u32(c as u32).unwrap_or('?'), self.pos),
            None => "Unexpected end of JSON input".to_string(),
        };
        vm.make_error(crate::vm::ErrorKind::Syntax, &msg)
    }

    fn expect(&mut self, vm: &mut Vm, c: u8) -> JsResult<()> {
        if self.peek() == Some(c as u16) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(vm))
        }
    }

    fn value(&mut self, vm: &mut Vm, depth: usize) -> JsResult<Value> {
        if depth > 5000 {
            return Err(vm.range_error("JSON nesting too deep"));
        }
        self.ws();
        let Some(c) = self.peek() else { return Err(self.error(vm)) };
        match c {
            0x7B => {
                self.pos += 1;
                let o = vm.new_object();
                self.ws();
                if self.peek() == Some(0x7D) {
                    self.pos += 1;
                    return Ok(Value::object(o));
                }
                loop {
                    self.ws();
                    if self.peek() != Some(0x22) {
                        return Err(self.error(vm));
                    }
                    let key = self.string_units(vm)?;
                    let key = match Units::Utf16(&key).as_array_index() {
                        Some(i) => PropertyKey::Index(i),
                        None => PropertyKey::Atom(vm.atoms.intern_units(&vm.heap, &Units::Utf16(&key))),
                    };
                    self.ws();
                    self.expect(vm, b':')?;
                    let v = self.value(vm, depth + 1)?;
                    vm.define_value(o, key, v, PropFlags::DEFAULT);
                    self.ws();
                    match self.peek() {
                        Some(0x2C) => self.pos += 1,
                        Some(0x7D) => {
                            self.pos += 1;
                            return Ok(Value::object(o));
                        }
                        _ => return Err(self.error(vm)),
                    }
                }
            }
            0x5B => {
                self.pos += 1;
                let mut values = Vec::new();
                self.ws();
                if self.peek() == Some(0x5D) {
                    self.pos += 1;
                    return Ok(Value::object(vm.new_array(values)));
                }
                loop {
                    values.push(self.value(vm, depth + 1)?);
                    self.ws();
                    match self.peek() {
                        Some(0x2C) => self.pos += 1,
                        Some(0x5D) => {
                            self.pos += 1;
                            return Ok(Value::object(vm.new_array(values)));
                        }
                        _ => return Err(self.error(vm)),
                    }
                }
            }
            0x22 => {
                let u = self.string_units(vm)?;
                Ok(Value::string(vm.new_string_units(&u)))
            }
            0x74 => self.literal(vm, "true", Value::TRUE),
            0x66 => self.literal(vm, "false", Value::FALSE),
            0x6E => self.literal(vm, "null", Value::NULL),
            _ => self.number(vm),
        }
    }

    fn literal(&mut self, vm: &mut Vm, word: &str, v: Value) -> JsResult<Value> {
        for b in word.bytes() {
            self.expect(vm, b)?;
        }
        Ok(v)
    }

    fn number(&mut self, vm: &mut Vm) -> JsResult<Value> {
        let start = self.pos;
        if self.peek() == Some(b'-' as u16) {
            self.pos += 1;
        }
        let digit = |c: Option<u16>| c.is_some_and(|c| (0x30..=0x39).contains(&c));
        if self.peek() == Some(b'0' as u16) {
            self.pos += 1;
        } else if digit(self.peek()) {
            while digit(self.peek()) {
                self.pos += 1;
            }
        } else {
            return Err(self.error(vm));
        }
        if self.peek() == Some(b'.' as u16) {
            self.pos += 1;
            if !digit(self.peek()) {
                return Err(self.error(vm));
            }
            while digit(self.peek()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(0x65) | Some(0x45)) {
            self.pos += 1;
            if matches!(self.peek(), Some(0x2B) | Some(0x2D)) {
                self.pos += 1;
            }
            if !digit(self.peek()) {
                return Err(self.error(vm));
            }
            while digit(self.peek()) {
                self.pos += 1;
            }
        }
        let text: String = (start..self.pos).map(|i| self.s.at(i) as u8 as char).collect();
        Ok(Value::number(text.parse().unwrap_or(f64::NAN)))
    }

    fn string_units(&mut self, vm: &mut Vm) -> JsResult<Vec<u16>> {
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            let Some(c) = self.peek() else { return Err(self.error(vm)) };
            self.pos += 1;
            match c {
                0x22 => return Ok(out),
                0x5C => {
                    let Some(e) = self.peek() else { return Err(self.error(vm)) };
                    self.pos += 1;
                    out.push(match e {
                        0x22 => 0x22,
                        0x5C => 0x5C,
                        0x2F => 0x2F,
                        0x62 => 0x08,
                        0x66 => 0x0C,
                        0x6E => 0x0A,
                        0x72 => 0x0D,
                        0x74 => 0x09,
                        0x75 => {
                            let mut v = 0u16;
                            for _ in 0..4 {
                                let Some(h) = self.peek() else { return Err(self.error(vm)) };
                                let d = match h {
                                    0x30..=0x39 => h - 0x30,
                                    0x61..=0x66 => h - 0x61 + 10,
                                    0x41..=0x46 => h - 0x41 + 10,
                                    _ => return Err(self.error(vm)),
                                };
                                v = v * 16 + d;
                                self.pos += 1;
                            }
                            v
                        }
                        _ => {
                            self.pos -= 1;
                            return Err(self.error(vm));
                        }
                    });
                }
                c if c < 0x20 => {
                    self.pos -= 1;
                    return Err(self.error(vm));
                }
                c => out.push(c),
            }
        }
    }
}

fn parse(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_string(arg(args, 0))?;
    let units = s.get().units();
    let mut p = Parser { s: units, pos: 0 };
    let v = p.value(vm, 0)?;
    p.ws();
    if p.pos != p.s.len() {
        return Err(p.error(vm));
    }
    let reviver = arg(args, 1);
    if vm.is_callable(reviver) {
        let root = vm.new_object();
        let empty = atoms::empty;
        vm.define_value(root, PropertyKey::Atom(empty), v, PropFlags::DEFAULT);
        return internalize(vm, root, PropertyKey::Atom(empty), reviver);
    }
    Ok(v)
}

fn internalize(vm: &mut Vm, holder: Gc<JsObject>, key: PropertyKey, reviver: Value) -> JsResult<Value> {
    let val = vm.get(Value::object(holder), key)?;
    if let Some(o) = val.as_object() {
        let keys: Vec<PropertyKey> = if o.get().is_array() {
            let len = match o.get().kind {
                ObjectKind::Array { length } => length,
                _ => 0,
            };
            (0..len).map(PropertyKey::Index).collect()
        } else {
            vm.enumerable_own_keys(o)
        };
        for k in keys {
            let nv = internalize(vm, o, k, reviver)?;
            if nv.is_undefined() {
                vm.delete_property(val, k, false)?;
            } else {
                vm.define_value(o, k, nv, PropFlags::DEFAULT);
            }
        }
    }
    let kv = vm.key_value(key);
    vm.call(reviver, Value::object(holder), &[kv, val])
}

struct Stringifier {
    replacer: Value,
    property_list: Option<Vec<PropertyKey>>,
    gap: String,
    indent: String,
    stack: Vec<Gc<JsObject>>,
    out: Vec<u16>,
}

pub(crate) fn quote(out: &mut Vec<u16>, s: &Units) {
    out.push(b'"' as u16);
    for i in 0..s.len() {
        let c = s.at(i);
        match c {
            0x22 => out.extend_from_slice(&[0x5C, 0x22]),
            0x5C => out.extend_from_slice(&[0x5C, 0x5C]),
            0x08 => out.extend_from_slice(&[0x5C, b'b' as u16]),
            0x0C => out.extend_from_slice(&[0x5C, b'f' as u16]),
            0x0A => out.extend_from_slice(&[0x5C, b'n' as u16]),
            0x0D => out.extend_from_slice(&[0x5C, b'r' as u16]),
            0x09 => out.extend_from_slice(&[0x5C, b't' as u16]),
            c if c < 0x20 || (0xD800..0xE000).contains(&c) && !paired(s, i) => {
                for b in format!("\\u{c:04x}").bytes() {
                    out.push(b as u16);
                }
            }
            c => out.push(c),
        }
    }
    out.push(b'"' as u16);
}

/// A surrogate that is part of a valid pair
fn paired(s: &Units, i: usize) -> bool {
    let c = s.at(i);
    if c < 0xDC00 {
        i + 1 < s.len() && (0xDC00..0xE000).contains(&s.at(i + 1))
    } else {
        i > 0 && (0xD800..0xDC00).contains(&s.at(i - 1))
    }
}

impl Stringifier {
    fn push_str(&mut self, s: &str) {
        self.out.extend(s.encode_utf16());
    }

    /// Serialize holder[key]; returns false if the value is not
    /// serializable (undefined, functions, symbols)
    fn property(&mut self, vm: &mut Vm, holder: Value, key: PropertyKey, value: Value) -> JsResult<bool> {
        let mut value = value;
        if value.is_object() || value.is_string() && false {
            let to_json = vm.get(value, PropertyKey::Atom(atoms::toJSON))?;
            if vm.is_callable(to_json) {
                let kv = vm.key_value(key);
                value = vm.call(to_json, value, &[kv])?;
            }
        }
        if !self.replacer.is_undefined() {
            let kv = vm.key_value(key);
            value = vm.call(self.replacer, holder, &[kv, value])?;
        }
        if let Some(o) = value.as_object() {
            value = match o.get().kind {
                ObjectKind::Number(_) => Value::number(vm.to_number(value)?),
                ObjectKind::String(_) => Value::string(vm.to_string(value)?),
                ObjectKind::Boolean(b) => Value::bool(b),
                _ => value,
            };
        }
        match value {
            Value::NULL => self.push_str("null"),
            Value::TRUE => self.push_str("true"),
            Value::FALSE => self.push_str("false"),
            _ => {
                if let Some(s) = value.as_string() {
                    quote(&mut self.out, &s.get().units());
                } else if let Some(n) = value.as_number() {
                    if n.is_finite() {
                        self.push_str(&number_to_string(n));
                    } else {
                        self.push_str("null");
                    }
                } else if let Some(o) = value.as_object() {
                    if o.get().is_callable() {
                        return Ok(false);
                    }
                    if self.stack.contains(&o) {
                        return Err(vm.type_error("Converting circular structure to JSON"));
                    }
                    if self.stack.len() > 5000 {
                        return Err(vm.range_error("Maximum call stack size exceeded"));
                    }
                    self.stack.push(o);
                    let r = if o.get().is_array() { self.array(vm, o) } else { self.object(vm, o) };
                    self.stack.pop();
                    r?;
                } else {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn object(&mut self, vm: &mut Vm, o: Gc<JsObject>) -> JsResult<()> {
        let stepback = self.indent.clone();
        self.indent.push_str(&self.gap.clone());
        let keys = match &self.property_list {
            Some(l) => l.clone(),
            None => vm.enumerable_own_keys(o),
        };
        self.out.push(b'{' as u16);
        let mut any = false;
        for k in keys {
            let v = vm.get(Value::object(o), k)?;
            let mark = self.out.len();
            if any {
                self.out.push(b',' as u16);
            }
            if !self.gap.is_empty() {
                self.out.push(b'\n' as u16);
                let ind = self.indent.clone();
                self.push_str(&ind);
            }
            let kv = vm.key_value(k);
            quote(&mut self.out, &kv.as_string().unwrap().get().units());
            self.out.push(b':' as u16);
            if !self.gap.is_empty() {
                self.out.push(b' ' as u16);
            }
            if self.property(vm, Value::object(o), k, v)? {
                any = true;
            } else {
                self.out.truncate(mark);
            }
        }
        if any && !self.gap.is_empty() {
            self.out.push(b'\n' as u16);
            self.push_str(&stepback);
        }
        self.out.push(b'}' as u16);
        self.indent = stepback;
        Ok(())
    }

    fn array(&mut self, vm: &mut Vm, o: Gc<JsObject>) -> JsResult<()> {
        let stepback = self.indent.clone();
        self.indent.push_str(&self.gap.clone());
        let len = vm.get(Value::object(o), PropertyKey::Atom(atoms::length))?;
        let len = vm.to_length(len)? as u32;
        self.out.push(b'[' as u16);
        for i in 0..len {
            if i > 0 {
                self.out.push(b',' as u16);
            }
            if !self.gap.is_empty() {
                self.out.push(b'\n' as u16);
                let ind = self.indent.clone();
                self.push_str(&ind);
            }
            let v = vm.get(Value::object(o), PropertyKey::Index(i))?;
            if !self.property(vm, Value::object(o), PropertyKey::Index(i), v)? {
                self.push_str("null");
            }
        }
        if len > 0 && !self.gap.is_empty() {
            self.out.push(b'\n' as u16);
            self.push_str(&stepback);
        }
        self.out.push(b']' as u16);
        self.indent = stepback;
        Ok(())
    }
}

fn stringify(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let value = arg(args, 0);
    let replacer = arg(args, 1);
    let mut st = Stringifier { replacer: Value::UNDEFINED, property_list: None, gap: String::new(), indent: String::new(), stack: Vec::new(), out: Vec::new() };
    if vm.is_callable(replacer) {
        st.replacer = replacer;
    } else if let Some(r) = replacer.as_object().filter(|r| r.get().is_array()) {
        let mut list = Vec::new();
        let len = match r.get().kind {
            ObjectKind::Array { length } => length,
            _ => 0,
        };
        for i in 0..len {
            let v = vm.get(replacer, PropertyKey::Index(i))?;
            if v.is_string() || v.is_number() || v.as_object().is_some_and(|o| matches!(o.get().kind, ObjectKind::String(_) | ObjectKind::Number(_))) {
                let k = vm.to_string(v)?;
                let k = vm.to_property_key(Value::string(k))?;
                if !list.contains(&k) {
                    list.push(k);
                }
            }
        }
        st.property_list = Some(list);
    }
    let mut space = arg(args, 2);
    if let Some(o) = space.as_object() {
        space = match o.get().kind {
            ObjectKind::Number(n) => Value::number(n),
            ObjectKind::String(s) => Value::string(s),
            _ => space,
        };
    }
    if let Some(n) = space.as_number() {
        st.gap = " ".repeat(n.clamp(0.0, 10.0) as usize);
    } else if let Some(s) = space.as_string() {
        st.gap = s.get().to_rust_string().chars().take(10).collect();
    }
    let wrapper = vm.new_object();
    vm.define_value(wrapper, PropertyKey::Atom(atoms::empty), value, PropFlags::DEFAULT);
    if !st.property(vm, Value::object(wrapper), PropertyKey::Atom(atoms::empty), value)? {
        return Ok(Value::UNDEFINED);
    }
    Ok(Value::string(vm.new_string_units(&st.out)))
}
