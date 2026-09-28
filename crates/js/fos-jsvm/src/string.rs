//! Strings and atoms
//!
//! JavaScript strings are sequences of UTF-16 code units. A string stores
//! one byte per unit when every unit is below 256 (Latin-1), which is most
//! text, and two otherwise.
//!
//! Concatenation builds a rope (a node pointing at both halves) so that
//! `s = s + x` in a loop is linear instead of quadratic; a rope is
//! flattened in place the first time its contents are needed.
//!
//! Atoms are interned strings used as property names, so that property
//! lookups compare integers.

use std::cell::{Cell, UnsafeCell};

use rustc_hash::FxHashMap;

use crate::gc::{CellKind, Gc, Heap, Trace, Tracer};

/// Ropes shorter than this are flattened immediately (copying is cheaper
/// than a rope node and later flattening)
const MIN_ROPE_LEN: u32 = 64;

pub enum Repr {
    Latin1(Box<[u8]>),
    Utf16(Box<[u16]>),
    Rope(Gc<JsString>, Gc<JsString>),
}

pub struct JsString {
    len: u32,
    /// Cached hash (0 = not computed yet)
    hash: Cell<u32>,
    /// Atom this string is interned as, if any
    atom: Cell<u32>,
    repr: UnsafeCell<Repr>,
}

impl Trace for JsString {
    const KIND: CellKind = CellKind::String;

    fn trace(&self, tracer: &mut Tracer) {
        if let Repr::Rope(a, b) = unsafe { &*self.repr.get() } {
            tracer.mark(*a);
            tracer.mark(*b);
        }
    }
}

/// Borrowed contents of a flat string
#[derive(Clone, Copy)]
pub enum Units<'a> {
    Latin1(&'a [u8]),
    Utf16(&'a [u16]),
}

impl<'a> Units<'a> {
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Units::Latin1(b) => b.len(),
            Units::Utf16(u) => u.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn at(&self, i: usize) -> u16 {
        match self {
            Units::Latin1(b) => b[i] as u16,
            Units::Utf16(u) => u[i],
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = u16> + 'a {
        let (a, b) = match *self {
            Units::Latin1(bytes) => (Some(bytes.iter().map(|&b| b as u16)), None),
            Units::Utf16(units) => (None, Some(units.iter().copied())),
        };
        a.into_iter().flatten().chain(b.into_iter().flatten())
    }

    pub fn to_vec(&self) -> Vec<u16> {
        self.iter().collect()
    }

    pub fn eq_units(&self, other: &Units) -> bool {
        match (self, other) {
            (Units::Latin1(a), Units::Latin1(b)) => a == b,
            (Units::Utf16(a), Units::Utf16(b)) => a == b,
            _ => self.len() == other.len() && self.iter().eq(other.iter()),
        }
    }

    pub fn cmp_units(&self, other: &Units) -> std::cmp::Ordering {
        match (self, other) {
            (Units::Latin1(a), Units::Latin1(b)) => a.cmp(b),
            (Units::Utf16(a), Units::Utf16(b)) => a.cmp(b),
            _ => self.iter().cmp(other.iter()),
        }
    }

    /// Lossy conversion to a Rust string (lone surrogates become U+FFFD)
    pub fn to_rust_string(&self) -> String {
        match self {
            Units::Latin1(b) => b.iter().map(|&c| c as char).collect(),
            Units::Utf16(u) => String::from_utf16_lossy(u),
        }
    }

    /// The canonical array index this string denotes (`"0"`..`"4294967294"`)
    pub fn as_array_index(&self) -> Option<u32> {
        let len = self.len();
        if len == 0 || len > 10 {
            return None;
        }
        let first = self.at(0);
        if first == b'0' as u16 {
            return if len == 1 { Some(0) } else { None };
        }
        let mut value: u64 = 0;
        for i in 0..len {
            let c = self.at(i);
            if !(b'0' as u16..=b'9' as u16).contains(&c) {
                return None;
            }
            value = value * 10 + (c - b'0' as u16) as u64;
        }
        if value < u32::MAX as u64 { Some(value as u32) } else { None }
    }

    fn hash(&self) -> u32 {
        // FNV-1a over code units: the same for Latin-1 and UTF-16 storage
        let mut h: u32 = 0x811C_9DC5;
        for u in self.iter() {
            h ^= u as u32;
            h = h.wrapping_mul(0x0100_0193);
        }
        if h == 0 { 1 } else { h }
    }
}

impl JsString {
    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn from_repr(repr: Repr, len: u32) -> JsString {
        JsString { len, hash: Cell::new(0), atom: Cell::new(u32::MAX), repr: UnsafeCell::new(repr) }
    }

    /// Contents, flattening a rope first
    pub fn units(&self) -> Units<'_> {
        self.flatten();
        match unsafe { &*self.repr.get() } {
            Repr::Latin1(b) => Units::Latin1(b),
            Repr::Utf16(u) => Units::Utf16(u),
            Repr::Rope(..) => unreachable!(),
        }
    }

    pub fn is_rope(&self) -> bool {
        matches!(unsafe { &*self.repr.get() }, Repr::Rope(..))
    }

    /// Replace a rope with its flat contents
    fn flatten(&self) {
        let repr = unsafe { &*self.repr.get() };
        let Repr::Rope(left, right) = repr else { return };

        // Collect leaves left to right with an explicit stack (ropes built
        // by repeated concatenation are deep)
        let mut all_latin1 = true;
        let mut leaves: Vec<&JsString> = Vec::new();
        let mut stack: Vec<&JsString> = vec![right.get(), left.get()];
        while let Some(s) = stack.pop() {
            match unsafe { &*s.repr.get() } {
                Repr::Rope(l, r) => {
                    stack.push(r.get());
                    stack.push(l.get());
                }
                Repr::Latin1(_) => leaves.push(s),
                Repr::Utf16(u) => {
                    if u.iter().any(|&c| c > 0xFF) {
                        all_latin1 = false;
                    }
                    leaves.push(s)
                }
            }
        }
        let flat = if all_latin1 {
            let mut out = Vec::with_capacity(self.len as usize);
            for leaf in leaves {
                match unsafe { &*leaf.repr.get() } {
                    Repr::Latin1(b) => out.extend_from_slice(b),
                    Repr::Utf16(u) => out.extend(u.iter().map(|&c| c as u8)),
                    Repr::Rope(..) => unreachable!(),
                }
            }
            Repr::Latin1(out.into_boxed_slice())
        } else {
            let mut out = Vec::with_capacity(self.len as usize);
            for leaf in leaves {
                match unsafe { &*leaf.repr.get() } {
                    Repr::Latin1(b) => out.extend(b.iter().map(|&c| c as u16)),
                    Repr::Utf16(u) => out.extend_from_slice(u),
                    Repr::Rope(..) => unreachable!(),
                }
            }
            Repr::Utf16(out.into_boxed_slice())
        };
        unsafe { *self.repr.get() = flat };
    }

    pub fn hash(&self) -> u32 {
        let h = self.hash.get();
        if h != 0 {
            return h;
        }
        let h = self.units().hash();
        self.hash.set(h);
        h
    }

    pub fn code_unit_at(&self, i: u32) -> Option<u16> {
        if i >= self.len {
            return None;
        }
        Some(self.units().at(i as usize))
    }

    pub fn to_rust_string(&self) -> String {
        self.units().to_rust_string()
    }

    pub fn equals(&self, other: &JsString) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        if self.len != other.len {
            return false;
        }
        let (a, b) = (self.atom.get(), other.atom.get());
        if a != u32::MAX && b != u32::MAX {
            return a == b;
        }
        let (h1, h2) = (self.hash.get(), other.hash.get());
        if h1 != 0 && h2 != 0 && h1 != h2 {
            return false;
        }
        self.units().eq_units(&other.units())
    }

    pub fn atom_id(&self) -> Option<Atom> {
        let a = self.atom.get();
        if a == u32::MAX { None } else { Some(Atom(a)) }
    }
}

/// Size estimate of a string's contents, for GC accounting
fn repr_bytes(repr: &Repr) -> usize {
    match repr {
        Repr::Latin1(b) => b.len(),
        Repr::Utf16(u) => u.len() * 2,
        Repr::Rope(..) => 0,
    }
}

/// Allocate a string from UTF-16 units (stored as Latin-1 when possible)
pub fn alloc_units(heap: &Heap, units: &[u16]) -> Gc<JsString> {
    let repr = if units.iter().all(|&u| u <= 0xFF) {
        Repr::Latin1(units.iter().map(|&u| u as u8).collect())
    } else {
        Repr::Utf16(units.into())
    };
    let extra = repr_bytes(&repr);
    heap.alloc(JsString::from_repr(repr, units.len() as u32), extra)
}

pub fn alloc_latin1(heap: &Heap, bytes: Vec<u8>) -> Gc<JsString> {
    let len = bytes.len() as u32;
    let extra = bytes.len();
    heap.alloc(JsString::from_repr(Repr::Latin1(bytes.into_boxed_slice()), len), extra)
}

pub fn alloc_str(heap: &Heap, s: &str) -> Gc<JsString> {
    if s.is_ascii() {
        return alloc_latin1(heap, s.as_bytes().to_vec());
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    alloc_units(heap, &units)
}

/// Concatenate, building a rope for long results
pub fn concat(heap: &Heap, a: Gc<JsString>, b: Gc<JsString>) -> Gc<JsString> {
    let (la, lb) = (a.get().len, b.get().len);
    if la == 0 {
        return b;
    }
    if lb == 0 {
        return a;
    }
    let len = la.checked_add(lb).expect("string too long");
    if len >= MIN_ROPE_LEN {
        return heap.alloc(JsString::from_repr(Repr::Rope(a, b), len), 0);
    }
    let (ua, ub) = (a.get().units(), b.get().units());
    match (ua, ub) {
        (Units::Latin1(x), Units::Latin1(y)) => {
            let mut out = Vec::with_capacity(len as usize);
            out.extend_from_slice(x);
            out.extend_from_slice(y);
            alloc_latin1(heap, out)
        }
        _ => {
            let mut out = Vec::with_capacity(len as usize);
            out.extend(ua.iter());
            out.extend(ub.iter());
            alloc_units(heap, &out)
        }
    }
}

/// Substring by code unit range
pub fn substring(heap: &Heap, s: Gc<JsString>, start: u32, end: u32) -> Gc<JsString> {
    let st = s.get();
    if start == 0 && end == st.len {
        return s;
    }
    match st.units() {
        Units::Latin1(b) => alloc_latin1(heap, b[start as usize..end as usize].to_vec()),
        Units::Utf16(u) => alloc_units(heap, &u[start as usize..end as usize]),
    }
}

// ---- atoms ----

/// An interned property name
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Atom(pub u32);

macro_rules! predefined_atoms {
    ($($name:ident = $text:literal),* $(,)?) => {
        #[allow(non_upper_case_globals)]
        pub mod atoms {
            use super::Atom;
            predefined_atoms!(@consts 0u32; $($name),*);
        }

        const PREDEFINED: &[&str] = &[$($text),*];
    };
    (@consts $n:expr; $name:ident $(, $rest:ident)*) => {
        pub const $name: Atom = Atom($n);
        predefined_atoms!(@consts $n + 1u32; $($rest),*);
    };
    (@consts $n:expr;) => {};
}

predefined_atoms! {
    empty = "", length = "length", prototype = "prototype", constructor = "constructor",
    name = "name", message = "message", toString = "toString", valueOf = "valueOf",
    value = "value", done = "done", next = "next", get = "get", set = "set",
    writable = "writable", enumerable = "enumerable", configurable = "configurable",
    undefined = "undefined", null = "null", true_ = "true", false_ = "false",
    number = "number", string = "string", boolean = "boolean", object = "object",
    function = "function", symbol = "symbol", bigint = "bigint", NaN = "NaN",
    Infinity = "Infinity", arguments = "arguments", callee = "callee", caller = "caller",
    stack = "stack", cause = "cause", errors = "errors", join = "join", lastIndex = "lastIndex",
    index = "index", input = "input", groups = "groups", raw = "raw", then = "then",
    toJSON = "toJSON", default_ = "default", anonymous = "anonymous", global = "global",
    globalThis = "globalThis", eval = "eval", this_ = "this", new_target = "new.target",
    size = "size", source = "source", flags = "flags", return_ = "return", throw_ = "throw",
    Symbol_iterator = "Symbol.iterator", Symbol_asyncIterator = "Symbol.asyncIterator",
    Symbol_hasInstance = "Symbol.hasInstance", Symbol_toPrimitive = "Symbol.toPrimitive",
    Symbol_toStringTag = "Symbol.toStringTag", Symbol_species = "Symbol.species",
    Symbol_isConcatSpreadable = "Symbol.isConcatSpreadable", Symbol_unscopables = "Symbol.unscopables",
    Symbol_match = "Symbol.match", Symbol_matchAll = "Symbol.matchAll", Symbol_replace = "Symbol.replace",
    Symbol_search = "Symbol.search", Symbol_split = "Symbol.split",
}

/// Owned key for the intern table (units as a Latin-1 or UTF-16 vector)
#[derive(PartialEq, Eq, Hash)]
enum AtomKey {
    Latin1(Box<[u8]>),
    Utf16(Box<[u16]>),
}

impl AtomKey {
    fn from_units(units: &Units) -> AtomKey {
        match units {
            Units::Latin1(b) => AtomKey::Latin1((*b).into()),
            Units::Utf16(u) => {
                if u.iter().all(|&c| c <= 0xFF) {
                    AtomKey::Latin1(u.iter().map(|&c| c as u8).collect())
                } else {
                    AtomKey::Utf16((*u).into())
                }
            }
        }
    }
}

/// The atom table. Atoms live as long as the VM (they are GC roots).
pub struct Atoms {
    strings: Vec<Gc<JsString>>,
    map: FxHashMap<AtomKey, u32>,
}

impl Atoms {
    pub fn new(heap: &Heap) -> Atoms {
        let mut atoms = Atoms { strings: Vec::new(), map: FxHashMap::default() };
        for text in PREDEFINED {
            atoms.intern_str(heap, text);
        }
        atoms
    }

    pub fn intern_units(&mut self, heap: &Heap, units: &Units) -> Atom {
        let key = AtomKey::from_units(units);
        if let Some(&id) = self.map.get(&key) {
            return Atom(id);
        }
        let s = match units {
            Units::Latin1(b) => alloc_latin1(heap, b.to_vec()),
            Units::Utf16(u) => alloc_units(heap, u),
        };
        self.add(key, s)
    }

    fn add(&mut self, key: AtomKey, s: Gc<JsString>) -> Atom {
        let id = self.strings.len() as u32;
        s.get().atom.set(id);
        self.strings.push(s);
        self.map.insert(key, id);
        Atom(id)
    }

    pub fn intern_str(&mut self, heap: &Heap, s: &str) -> Atom {
        if s.is_ascii() {
            self.intern_units(heap, &Units::Latin1(s.as_bytes()))
        } else {
            let units: Vec<u16> = s.encode_utf16().collect();
            self.intern_units(heap, &Units::Utf16(&units))
        }
    }

    /// Intern a heap string, reusing it as the atom's string when new
    pub fn intern(&mut self, s: Gc<JsString>) -> Atom {
        if let Some(atom) = s.get().atom_id() {
            return atom;
        }
        let units = s.get().units();
        let key = AtomKey::from_units(&units);
        if let Some(&id) = self.map.get(&key) {
            s.get().atom.set(id);
            return Atom(id);
        }
        self.add(key, s)
    }

    /// Look up without interning
    pub fn find(&self, units: &Units) -> Option<Atom> {
        self.map.get(&AtomKey::from_units(units)).map(|&id| Atom(id))
    }

    #[inline]
    pub fn string(&self, atom: Atom) -> Gc<JsString> {
        self.strings[atom.0 as usize]
    }

    pub fn trace(&self, tracer: &mut Tracer) {
        for &s in &self.strings {
            tracer.mark(s);
        }
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latin1_and_utf16() {
        let heap = Heap::new();
        let a = alloc_str(&heap, "héllo");
        assert!(matches!(a.get().units(), Units::Latin1(_)));
        assert_eq!(a.get().len(), 5);
        let b = alloc_str(&heap, "日本");
        assert!(matches!(b.get().units(), Units::Utf16(_)));
        let emoji = alloc_str(&heap, "😀");
        assert_eq!(emoji.get().len(), 2, "astral code points are two units");
        assert_eq!(a.get().to_rust_string(), "héllo");
    }

    #[test]
    fn test_ropes_flatten() {
        let heap = Heap::new();
        let mut s = alloc_str(&heap, "");
        for i in 0..2000 {
            let piece = alloc_str(&heap, if i % 2 == 0 { "ab" } else { "ç" });
            s = concat(&heap, s, piece);
        }
        assert!(s.get().is_rope());
        assert_eq!(s.get().len(), 3000);
        assert_eq!(s.get().code_unit_at(0), Some(b'a' as u16));
        assert!(!s.get().is_rope());
        let text = s.get().to_rust_string();
        assert!(text.starts_with("abçabç"));

        // Mixed widths promote to UTF-16
        let mut t = alloc_str(&heap, &"x".repeat(100));
        t = concat(&heap, t, alloc_str(&heap, "日"));
        assert_eq!(t.get().len(), 101);
        assert!(matches!(t.get().units(), Units::Utf16(_)));
    }

    #[test]
    fn test_equality_and_hash() {
        let heap = Heap::new();
        let a = alloc_str(&heap, "same");
        let b = alloc_units(&heap, &"same".encode_utf16().collect::<Vec<_>>());
        assert!(a.get().equals(b.get()));
        assert_eq!(a.get().hash(), b.get().hash());
        let c = concat(&heap, alloc_str(&heap, &"x".repeat(40)), alloc_str(&heap, &"y".repeat(40)));
        let d = alloc_str(&heap, &format!("{}{}", "x".repeat(40), "y".repeat(40)));
        assert!(c.get().equals(d.get()));
        assert!(!a.get().equals(c.get()));
    }

    #[test]
    fn test_array_index() {
        let idx = |s: &str| Units::Latin1(s.as_bytes()).as_array_index();
        assert_eq!(idx("0"), Some(0));
        assert_eq!(idx("123"), Some(123));
        assert_eq!(idx("4294967294"), Some(4294967294));
        assert_eq!(idx("4294967295"), None);
        assert_eq!(idx("01"), None);
        assert_eq!(idx("-1"), None);
        assert_eq!(idx("1.5"), None);
        assert_eq!(idx(""), None);
    }

    #[test]
    fn test_atoms() {
        let heap = Heap::new();
        let mut atoms = Atoms::new(&heap);
        assert_eq!(atoms.intern_str(&heap, "length"), atoms::length);
        let x = atoms.intern_str(&heap, "x");
        let s = alloc_str(&heap, "x");
        assert_eq!(atoms.intern(s), x);
        assert_eq!(s.get().atom_id(), Some(x));
        let u = alloc_units(&heap, &[0x78]);
        assert_eq!(atoms.intern(u), x, "Latin-1 and UTF-16 spellings share an atom");
        assert_eq!(atoms.string(atoms::prototype).get().to_rust_string(), "prototype");
    }
}
