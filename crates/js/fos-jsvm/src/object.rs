//! Objects
//!
//! An object has named properties (in slots described by its shape, or a
//! hash table in dictionary mode), indexed properties (a dense element
//! vector with holes, spilling very sparse indices into the dictionary),
//! a prototype, and a kind carrying class-specific data (functions,
//! arrays, boxed primitives...).
//!
//! This module only stores and finds own properties; the full [[Get]] /
//! [[Set]] algorithms, which may call getters and setters, live in the VM.

use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::bytecode::FunctionProto;
use crate::gc::{CellKind, Gc, Trace, Tracer};
use crate::shape::{ShapeId, Shapes};
use crate::string::{Atom, JsString};
use crate::value::Value;
use crate::vm::NativeFn;

/// A property name
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PropertyKey {
    Atom(Atom),
    /// Array index (0..=2^32-2)
    Index(u32),
    Symbol(Gc<Symbol>),
}

/// Property attributes
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PropFlags(pub u8);

impl PropFlags {
    pub const NONE: PropFlags = PropFlags(0);
    pub const WRITABLE: u8 = 1;
    pub const ENUMERABLE: u8 = 2;
    pub const CONFIGURABLE: u8 = 4;
    /// The slot holds an accessor pair object
    pub const ACCESSOR: u8 = 8;
    /// Writable, enumerable, configurable: what assignment creates
    pub const DEFAULT: PropFlags = PropFlags(1 | 2 | 4);
    /// Writable and configurable but not enumerable (built-in methods)
    pub const HIDDEN: PropFlags = PropFlags(1 | 4);
    /// Read-only, non-enumerable, configurable (`length`, `name`)
    pub const READONLY_HIDDEN: PropFlags = PropFlags(4);
    /// Nothing (frozen, `prototype` of classes)
    pub const FROZEN: PropFlags = PropFlags(0);

    #[inline]
    pub fn writable(self) -> bool {
        self.0 & Self::WRITABLE != 0
    }
    #[inline]
    pub fn enumerable(self) -> bool {
        self.0 & Self::ENUMERABLE != 0
    }
    #[inline]
    pub fn configurable(self) -> bool {
        self.0 & Self::CONFIGURABLE != 0
    }
    #[inline]
    pub fn is_accessor(self) -> bool {
        self.0 & Self::ACCESSOR != 0
    }
    pub fn with(self, bit: u8, on: bool) -> PropFlags {
        if on { PropFlags(self.0 | bit) } else { PropFlags(self.0 & !bit) }
    }
}

/// Where an own property lives
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// Index into `slots` (shape-described property)
    Named(u32),
    /// Index into `elements`
    Element(u32),
    /// Index into the dictionary's entries
    Dict(u32),
}

/// Dictionary-mode storage (insertion-ordered)
#[derive(Default)]
pub struct Dict {
    index: FxHashMap<PropertyKey, u32>,
    entries: Vec<Option<(PropertyKey, Value, PropFlags)>>,
}

impl Dict {
    fn insert(&mut self, key: PropertyKey, value: Value, flags: PropFlags) -> u32 {
        if let Some(&i) = self.index.get(&key) {
            self.entries[i as usize] = Some((key, value, flags));
            return i;
        }
        let i = self.entries.len() as u32;
        self.entries.push(Some((key, value, flags)));
        self.index.insert(key, i);
        i
    }

    fn remove(&mut self, key: PropertyKey) -> bool {
        match self.index.remove(&key) {
            Some(i) => {
                self.entries[i as usize] = None;
                // Compact when mostly empty
                if self.entries.len() > 32 && self.index.len() * 2 < self.entries.len() {
                    let entries: Vec<_> = self.entries.drain(..).flatten().collect();
                    self.index.clear();
                    for (k, v, f) in entries {
                        self.insert(k, v, f);
                    }
                }
                true
            }
            None => false,
        }
    }
}

/// Class-specific data
pub enum ObjectKind {
    Ordinary,
    Array { length: u32 },
    Function(Box<Closure>),
    Native(Box<NativeFunction>),
    Bound(Box<BoundFunction>),
    /// A getter/setter pair (internal: the value of an accessor property)
    Accessor { getter: Value, setter: Value },
    /// An error, with the frames captured when it was made (until its
    /// `stack` is first read and formatted from them)
    Error(Option<Box<CapturedStack>>),
    Boolean(bool),
    Number(f64),
    String(Gc<JsString>),
    Symbol(Gc<Symbol>),
    BigInt(Gc<crate::bigint::BigInt>),
    Date(f64),
    Arguments,
    ForIn(Box<ForInIterator>),
    /// Array iterator: target, next index, kind (0 values, 1 keys, 2 entries)
    ArrayIterator { target: Value, index: u32, kind: u8 },
    StringIterator { string: Gc<JsString>, pos: u32 },
    Map(Box<MapData>),
    Set(Box<MapData>),
    MapIterator { map: Gc<JsObject>, pos: u32, kind: u8 },
    /// A user-defined iterator with its `next` method and state
    IterRecord { iter: Value, next: Value, done: bool },
    /// WeakMap/WeakSet entries are not traced: the collector marks a
    /// WeakMap value only once its key is marked (an ephemeron) and prunes
    /// entries whose keys die
    WeakMap(Box<MapData>),
    WeakSet(Box<MapData>),
    /// A WeakRef's target (untraced; cleared when the target dies)
    WeakRef(Value),
    FinalizationRegistry(Box<FinalizationData>),
    RegExp(Box<RegExpData>),
    ArrayBuffer(Box<Vec<u8>>),
    TypedArray(Box<TypedArrayData>),
    DataView(Box<TypedArrayData>),
    Proxy(Box<ProxyData>),
    /// Generator object or async function state
    Generator(Box<GenState>),
    Promise(Box<PromiseData>),
    /// An object backed by embedder data (e.g. a DOM node): `class` tells
    /// the embedder's kinds apart, `id` names the thing it wraps
    Host { class: u32, id: u64 },
    /// Embedder data owned by the object and freed with it (e.g. a canvas
    /// bitmap)
    HostData(Box<dyn HostData>),
}

/// Data an embedder attaches to an object (`ObjectKind::HostData`). It
/// lives as long as the object; `trace` marks the JS values it holds.
pub trait HostData: std::any::Any {
    fn trace(&self, _tracer: &mut Tracer) {}
}

impl JsObject {
    /// The object's host data, if it is a `T`
    pub fn host_data<T: HostData>(&self) -> Option<&T> {
        match &self.kind {
            ObjectKind::HostData(d) => (&**d as &dyn std::any::Any).downcast_ref(),
            _ => None,
        }
    }

    pub fn host_data_mut<T: HostData>(&mut self) -> Option<&mut T> {
        match &mut self.kind {
            ObjectKind::HostData(d) => (&mut **d as &mut dyn std::any::Any).downcast_mut(),
            _ => None,
        }
    }
}

pub struct ProxyData {
    /// Null once revoked
    pub target: Value,
    pub handler: Value,
    pub callable: bool,
    pub constructor: bool,
}

/// A view on an ArrayBuffer (typed arrays; DataView uses bytes)
#[derive(Clone, Copy, Debug)]
pub struct TypedArrayData {
    pub kind: crate::builtins::typedarray::TaKind,
    pub buffer: Gc<JsObject>,
    /// Byte offset
    pub offset: u32,
    /// Elements (bytes for DataView)
    pub length: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GenStatus {
    SuspendedStart,
    SuspendedYield,
    /// Suspended at an `await` (async functions and async generators)
    SuspendedAwait,
    Running,
    Done,
}

/// A pending `next`/`throw`/`return` call on an async generator
pub struct AsyncGenRequest {
    pub mode: crate::vm::generator::ResumeMode,
    pub value: Value,
    /// Settled with the call's iterator result
    pub promise: Gc<JsObject>,
}

/// A suspended function activation
pub struct GenState {
    pub func: Gc<JsObject>,
    /// Saved registers
    pub regs: Vec<Value>,
    pub pc: u32,
    /// Register receiving the value sent on resumption (u16::MAX: none)
    pub resume_reg: u16,
    /// Upvalues of this activation's registers, closed while suspended
    pub upvals: Vec<(Gc<Upvalue>, u16)>,
    pub new_target: Value,
    pub status: GenStatus,
    /// Async functions: the promise they return
    pub promise: Option<Gc<JsObject>>,
    /// Value of a pending `return()`
    pub return_value: Value,
    /// Async generators: calls waiting for the generator, oldest first
    pub queue: Option<Box<std::collections::VecDeque<AsyncGenRequest>>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PromiseState {
    Pending,
    Fulfilled,
    Rejected,
}

pub struct PromiseData {
    pub state: PromiseState,
    pub value: Value,
    pub reactions: Vec<Reaction>,
    pub handled: bool,
}

#[derive(Clone, Copy)]
pub enum ReactionKind {
    /// `then` callbacks settling `derived`
    Then,
    /// Resume an async function
    Await(Gc<JsObject>),
}

#[derive(Clone, Copy)]
pub struct Reaction {
    pub kind: ReactionKind,
    pub on_fulfilled: Value,
    pub on_rejected: Value,
    pub derived: Option<Gc<JsObject>>,
}

impl Reaction {
    pub fn trace(&self, tracer: &mut Tracer) {
        if let ReactionKind::Await(g) = self.kind {
            tracer.mark(g);
        }
        tracer.mark_value(self.on_fulfilled);
        tracer.mark_value(self.on_rejected);
        if let Some(d) = self.derived {
            tracer.mark(d);
        }
    }
}

pub struct RegExpData {
    pub source: Gc<JsString>,
    pub flags: Gc<JsString>,
    pub regex: Rc<crate::regex::Regex>,
}

pub struct Closure {
    pub proto: Rc<FunctionProto>,
    pub upvalues: Box<[Gc<Upvalue>]>,
    /// Object whose prototype `super` refers to (methods only)
    pub home_object: Option<Gc<JsObject>>,
    /// Instance field initializer (class constructors)
    pub fields: Option<Gc<JsObject>>,
}

pub struct NativeFunction {
    pub name: Atom,
    pub length: u32,
    pub call: NativeFn,
    /// Behavior under `new` (None: not a constructor)
    pub construct: Option<NativeFn>,
    /// Private data for the function (e.g. a bound resolver's promise)
    pub data: Value,
}

pub struct BoundFunction {
    pub target: Gc<JsObject>,
    pub this: Value,
    pub args: Box<[Value]>,
}

pub struct ForInIterator {
    pub keys: Vec<Value>,
    pub pos: usize,
    pub object: Value,
}

/// Insertion-ordered hash map for Map and Set (deleted entries are holes
/// so iterators stay valid)
/// Frames (function, saved pc) captured for an error, innermost first
pub struct CapturedStack {
    pub frames: Box<[(Rc<FunctionProto>, u32)]>,
}

/// A FinalizationRegistry: its cleanup callback and registered cells
pub struct FinalizationData {
    pub cleanup: Value,
    pub cells: Vec<FinalizationCell>,
}

pub struct FinalizationCell {
    pub target: Value,
    pub held: Value,
    /// Unregister token, or undefined
    pub token: Value,
}

#[derive(Default)]
pub struct MapData {
    pub index: FxHashMap<MapKey, u32>,
    pub entries: Vec<Option<(Value, Value)>>,
    pub size: u32,
}

/// SameValueZero key for Map/Set
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum MapKey {
    /// Primitive compared by its bits (numbers normalized, strings by content)
    Bits(u64),
    String(Box<[u16]>),
    BigInt(crate::bigint::BigInt),
}

/// Symbol value
pub struct Symbol {
    pub description: Option<Gc<JsString>>,
    /// Key in the global registry (`Symbol.for`)
    pub registered: bool,
    /// A class private name (`#x`): never listed as a key
    pub is_private: bool,
}

impl Trace for Symbol {
    const KIND: CellKind = CellKind::Symbol;

    fn trace(&self, tracer: &mut Tracer) {
        if let Some(d) = self.description {
            tracer.mark(d);
        }
    }
}

/// A captured variable: open while the variable's register is live, then
/// closed over its final value
pub enum Upvalue {
    Open(usize),
    Closed(Value),
}

impl Trace for Upvalue {
    const KIND: CellKind = CellKind::Upvalue;

    fn trace(&self, tracer: &mut Tracer) {
        if let Upvalue::Closed(v) = self {
            tracer.mark_value(*v);
        }
    }
}

pub struct JsObject {
    pub shape: ShapeId,
    pub extensible: bool,
    /// Used as some object's prototype (changes invalidate inline caches)
    pub is_prototype: bool,
    /// A class constructor (throws when called without `new`)
    pub class_constructor: bool,
    /// Keep shape mode however many properties it gets (the global object)
    pub keep_shape: bool,
    /// An array whose `length` is not writable (frozen, or so defined)
    pub length_readonly: bool,
    /// Built-in properties not created yet (functions' `length`, `name`
    /// and `prototype`): see `LAZY_*`
    pub lazy: u8,
    pub proto: Option<Gc<JsObject>>,
    pub slots: Vec<Value>,
    pub elements: Vec<Value>,
    pub dict: Option<Box<Dict>>,
    pub kind: ObjectKind,
}

impl Trace for JsObject {
    const KIND: CellKind = CellKind::Object;

    fn trace(&self, tracer: &mut Tracer) {
        if let Some(p) = self.proto {
            tracer.mark(p);
        }
        tracer.mark_values(&self.slots);
        tracer.mark_values(&self.elements);
        if let Some(dict) = &self.dict {
            for (key, value, _) in dict.entries.iter().flatten() {
                if let PropertyKey::Symbol(s) = key {
                    tracer.mark(*s);
                }
                tracer.mark_value(*value);
            }
        }
        match &self.kind {
            ObjectKind::Function(c) => {
                c.proto.trace(tracer);
                for &u in c.upvalues.iter() {
                    tracer.mark(u);
                }
                if let Some(h) = c.home_object {
                    tracer.mark(h);
                }
                if let Some(f) = c.fields {
                    tracer.mark(f);
                }
            }
            ObjectKind::Native(n) => tracer.mark_value(n.data),
            ObjectKind::Bound(b) => {
                tracer.mark(b.target);
                tracer.mark_value(b.this);
                tracer.mark_values(&b.args);
            }
            ObjectKind::Accessor { getter, setter } => {
                tracer.mark_value(*getter);
                tracer.mark_value(*setter);
            }
            ObjectKind::String(s) => tracer.mark(*s),
            ObjectKind::Symbol(s) => tracer.mark(*s),
            ObjectKind::BigInt(b) => tracer.mark(*b),
            ObjectKind::ForIn(it) => {
                tracer.mark_values(&it.keys);
                tracer.mark_value(it.object);
            }
            ObjectKind::ArrayIterator { target, .. } => tracer.mark_value(*target),
            ObjectKind::StringIterator { string, .. } => tracer.mark(*string),
            ObjectKind::Map(m) | ObjectKind::Set(m) => {
                for (k, v) in m.entries.iter().flatten() {
                    tracer.mark_value(*k);
                    tracer.mark_value(*v);
                }
            }
            ObjectKind::MapIterator { map, .. } => tracer.mark(*map),
            ObjectKind::IterRecord { iter, next, .. } => {
                tracer.mark_value(*iter);
                tracer.mark_value(*next);
            }
            ObjectKind::WeakMap(_) | ObjectKind::WeakSet(_) | ObjectKind::WeakRef(_) => {}
            ObjectKind::FinalizationRegistry(f) => {
                // Held values are strong; targets and tokens are weak
                tracer.mark_value(f.cleanup);
                for c in &f.cells {
                    tracer.mark_value(c.held);
                }
            }
            ObjectKind::RegExp(r) => {
                tracer.mark(r.source);
                tracer.mark(r.flags);
            }
            ObjectKind::TypedArray(t) | ObjectKind::DataView(t) => tracer.mark(t.buffer),
            ObjectKind::ArrayBuffer(_) => {}
            ObjectKind::Proxy(p) => {
                tracer.mark_value(p.target);
                tracer.mark_value(p.handler);
            }
            ObjectKind::Generator(g) => {
                tracer.mark(g.func);
                tracer.mark_values(&g.regs);
                for &(u, _) in &g.upvals {
                    tracer.mark(u);
                }
                tracer.mark_value(g.new_target);
                if let Some(p) = g.promise {
                    tracer.mark(p);
                }
                tracer.mark_value(g.return_value);
                if let Some(q) = &g.queue {
                    for r in q.iter() {
                        tracer.mark_value(r.value);
                        tracer.mark(r.promise);
                    }
                }
            }
            ObjectKind::Promise(p) => {
                tracer.mark_value(p.value);
                for r in &p.reactions {
                    r.trace(tracer);
                }
            }
            ObjectKind::Error(stack) => {
                // The functions' constants stay alive for the stack's sake
                for (p, _) in stack.iter().flat_map(|s| s.frames.iter()) {
                    p.trace(tracer);
                }
            }
            ObjectKind::HostData(d) => d.trace(tracer),
            ObjectKind::Ordinary
            | ObjectKind::Array { .. }
            | ObjectKind::Boolean(_)
            | ObjectKind::Host { .. }
            | ObjectKind::Number(_)
            | ObjectKind::Date(_)
            | ObjectKind::Arguments => {}
        }
    }
}

pub const LAZY_LENGTH: u8 = 1;
pub const LAZY_NAME: u8 = 2;
pub const LAZY_PROTOTYPE: u8 = 4;

/// Objects with more named properties than this switch to dictionary mode
const MAX_SHAPE_PROPERTIES: u32 = 64;

/// Largest gap of holes an element store may create before switching the
/// index to sparse (dictionary) storage
const MAX_HOLE_GAP: u32 = 1024;

impl JsObject {
    pub fn new(proto: Option<Gc<JsObject>>, kind: ObjectKind) -> JsObject {
        JsObject {
            shape: ShapeId::ROOT,
            extensible: true,
            is_prototype: false,
            class_constructor: false,
            keep_shape: false,
            length_readonly: false,
            lazy: 0,
            proto,
            slots: Vec::new(),
            elements: Vec::new(),
            dict: None,
            kind,
        }
    }

    pub fn is_callable(&self) -> bool {
        match &self.kind {
            ObjectKind::Function(_) | ObjectKind::Native(_) | ObjectKind::Bound(_) => true,
            ObjectKind::Proxy(p) => p.callable,
            _ => false,
        }
    }

    pub fn is_array(&self) -> bool {
        matches!(self.kind, ObjectKind::Array { .. })
    }

    pub fn is_typed_array(&self) -> bool {
        matches!(self.kind, ObjectKind::TypedArray(_))
    }

    /// Estimated bytes owned outside the cell (for GC accounting)
    pub fn extra_bytes(&self) -> usize {
        (self.slots.capacity() + self.elements.capacity()) * 8
    }

    // ---- own property lookup ----

    pub fn find_own(&self, shapes: &Shapes, key: PropertyKey) -> Option<(Slot, PropFlags)> {
        if let PropertyKey::Index(i) = key {
            if let Some(&v) = self.elements.get(i as usize) {
                if !v.is_hole() {
                    return Some((Slot::Element(i), PropFlags::DEFAULT));
                }
            }
            return self.find_in_dict(key);
        }
        if self.shape == ShapeId::DICT {
            return self.find_in_dict(key);
        }
        shapes.lookup(self.shape, key).map(|(slot, flags)| (Slot::Named(slot), flags))
    }

    fn find_in_dict(&self, key: PropertyKey) -> Option<(Slot, PropFlags)> {
        let dict = self.dict.as_ref()?;
        let &i = dict.index.get(&key)?;
        let (_, _, flags) = dict.entries[i as usize].as_ref()?;
        Some((Slot::Dict(i), *flags))
    }

    #[inline]
    pub fn read(&self, slot: Slot) -> Value {
        match slot {
            Slot::Named(i) => self.slots[i as usize],
            Slot::Element(i) => self.elements[i as usize],
            Slot::Dict(i) => self.dict.as_ref().unwrap().entries[i as usize].as_ref().unwrap().1,
        }
    }

    #[inline]
    pub fn write(&mut self, slot: Slot, value: Value) {
        match slot {
            Slot::Named(i) => self.slots[i as usize] = value,
            Slot::Element(i) => self.elements[i as usize] = value,
            Slot::Dict(i) => self.dict.as_mut().unwrap().entries[i as usize].as_mut().unwrap().1 = value,
        }
    }

    // ---- adding and removing ----

    /// Add a property that does not exist yet (the caller checked
    /// extensibility and absence). Returns where it was stored.
    pub fn add_property(&mut self, shapes: &mut Shapes, key: PropertyKey, value: Value, flags: PropFlags) -> Slot {
        if let PropertyKey::Index(i) = key {
            if flags == PropFlags::DEFAULT {
                if let Some(slot) = self.store_element(i, value) {
                    return slot;
                }
            }
            let dict = self.dict.get_or_insert_with(Default::default);
            return Slot::Dict(dict.insert(key, value, flags));
        }
        if self.shape != ShapeId::DICT && shapes.len(self.shape) >= MAX_SHAPE_PROPERTIES && !self.is_prototype && !self.keep_shape {
            self.to_dictionary(shapes);
        }
        if self.shape == ShapeId::DICT {
            return Slot::Dict(self.dict.get_or_insert_with(Default::default).insert(key, value, flags));
        }
        self.shape = shapes.add(self.shape, key, flags);
        self.slots.push(value);
        Slot::Named(self.slots.len() as u32 - 1)
    }

    /// Store an element densely, unless that would create a large gap
    fn store_element(&mut self, index: u32, value: Value) -> Option<Slot> {
        let len = self.elements.len() as u32;
        if index < len {
            self.elements[index as usize] = value;
            return Some(Slot::Element(index));
        }
        if index - len > MAX_HOLE_GAP && index / 2 > len {
            return None;
        }
        self.elements.resize(index as usize, Value::HOLE);
        self.elements.push(value);
        Some(Slot::Element(index))
    }

    /// Switch to dictionary mode (after deletions, attribute changes or
    /// too many properties). Elements stay where they are.
    pub fn to_dictionary(&mut self, shapes: &Shapes) {
        if self.shape == ShapeId::DICT {
            // Already a dictionary (proxies start out with no table)
            if self.dict.is_none() {
                self.dict = Some(Box::default());
            }
            return;
        }
        let mut dict = self.dict.take().map(|d| *d).unwrap_or_default();
        for (i, (key, flags)) in shapes.properties(self.shape).into_iter().enumerate() {
            dict.insert(key, self.slots[i], flags);
        }
        self.slots = Vec::new();
        self.shape = ShapeId::DICT;
        self.dict = Some(Box::new(dict));
    }

    /// Remove an own property (the caller checked configurability)
    pub fn remove_property(&mut self, shapes: &Shapes, key: PropertyKey) -> bool {
        if let PropertyKey::Index(i) = key {
            if (i as usize) < self.elements.len() && !self.elements[i as usize].is_hole() {
                if i as usize == self.elements.len() - 1 {
                    self.elements.pop();
                } else {
                    self.elements[i as usize] = Value::HOLE;
                }
                return true;
            }
            return self.dict.as_mut().is_some_and(|d| d.remove(key));
        }
        if self.shape != ShapeId::DICT {
            if shapes.lookup(self.shape, key).is_none() {
                return false;
            }
            self.to_dictionary(shapes);
        }
        self.dict.as_mut().is_some_and(|d| d.remove(key))
    }

    /// Change a property's attributes (moves it to dictionary storage)
    pub fn set_flags(&mut self, shapes: &Shapes, key: PropertyKey, flags: PropFlags) {
        if let PropertyKey::Index(i) = key {
            if flags == PropFlags::DEFAULT {
                return;
            }
            if let Some(&v) = self.elements.get(i as usize) {
                if !v.is_hole() {
                    self.elements[i as usize] = Value::HOLE;
                    let dict = self.dict.get_or_insert_with(Default::default);
                    dict.insert(key, v, flags);
                    return;
                }
            }
        } else if self.shape != ShapeId::DICT {
            match shapes.lookup(self.shape, key) {
                Some((_, f)) if f == flags => return,
                Some(_) => self.to_dictionary(shapes),
                None => return,
            }
        }
        if let Some(dict) = &mut self.dict {
            if let Some(&i) = dict.index.get(&key) {
                if let Some(entry) = dict.entries[i as usize].as_mut() {
                    entry.2 = flags;
                }
            }
        }
    }

    /// Own property keys in specification order: array indices ascending,
    /// then strings, then symbols, each in insertion order
    pub fn own_keys(&self, shapes: &Shapes) -> Vec<(PropertyKey, PropFlags)> {
        let mut indices: Vec<(PropertyKey, PropFlags)> = self
            .elements
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.is_hole())
            .map(|(i, _)| (PropertyKey::Index(i as u32), PropFlags::DEFAULT))
            .collect();
        let mut named: Vec<(PropertyKey, PropFlags)> = Vec::new();
        if let Some(dict) = &self.dict {
            let mut sparse = Vec::new();
            for (key, _, flags) in dict.entries.iter().flatten() {
                match key {
                    PropertyKey::Index(_) => sparse.push((*key, *flags)),
                    _ => named.push((*key, *flags)),
                }
            }
            if !sparse.is_empty() {
                indices.extend(sparse);
                indices.sort_by_key(|(k, _)| match k {
                    PropertyKey::Index(i) => *i,
                    _ => 0,
                });
            }
        }
        if self.shape != ShapeId::DICT {
            named = shapes.properties(self.shape);
        }
        let (strings, symbols): (Vec<_>, Vec<_>) = named.into_iter().partition(|(k, _)| !matches!(k, PropertyKey::Symbol(_)));
        indices.extend(strings);
        indices.extend(symbols);
        indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_named_and_indexed_properties() {
        let mut shapes = Shapes::new();
        let mut o = JsObject::new(None, ObjectKind::Ordinary);
        let x = PropertyKey::Atom(Atom(50));
        let y = PropertyKey::Atom(Atom(51));
        o.add_property(&mut shapes, x, Value::int(1), PropFlags::DEFAULT);
        o.add_property(&mut shapes, PropertyKey::Index(2), Value::int(3), PropFlags::DEFAULT);
        o.add_property(&mut shapes, y, Value::int(2), PropFlags::DEFAULT);
        let (slot, _) = o.find_own(&shapes, y).unwrap();
        assert_eq!(o.read(slot), Value::int(2));
        assert!(o.find_own(&shapes, PropertyKey::Index(0)).is_none(), "holes are absent");
        assert_eq!(o.elements.len(), 3);
        let keys: Vec<_> = o.own_keys(&shapes).into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![PropertyKey::Index(2), x, y]);
    }

    #[test]
    fn test_sparse_indices_and_delete() {
        let mut shapes = Shapes::new();
        let mut o = JsObject::new(None, ObjectKind::Ordinary);
        o.add_property(&mut shapes, PropertyKey::Index(4_000_000_000), Value::TRUE, PropFlags::DEFAULT);
        assert!(o.elements.is_empty(), "a huge index must not allocate billions of holes");
        assert!(o.find_own(&shapes, PropertyKey::Index(4_000_000_000)).is_some());

        let x = PropertyKey::Atom(Atom(60));
        o.add_property(&mut shapes, x, Value::int(1), PropFlags::DEFAULT);
        assert!(o.remove_property(&shapes, x));
        assert!(o.find_own(&shapes, x).is_none());
        assert_eq!(o.shape, ShapeId::DICT);
    }

    #[test]
    fn test_many_properties_switch_to_dictionary() {
        let mut shapes = Shapes::new();
        let mut o = JsObject::new(None, ObjectKind::Ordinary);
        for i in 0..200 {
            o.add_property(&mut shapes, PropertyKey::Atom(Atom(1000 + i)), Value::int(i as i32), PropFlags::DEFAULT);
        }
        assert_eq!(o.shape, ShapeId::DICT);
        assert!(shapes.count() < 100, "map-like objects must not grow the shape tree");
        let (slot, _) = o.find_own(&shapes, PropertyKey::Atom(Atom(1150))).unwrap();
        assert_eq!(o.read(slot), Value::int(150));
    }
}
