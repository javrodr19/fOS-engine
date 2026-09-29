//! Hidden classes (shapes)
//!
//! Objects that get the same properties in the same order share a shape,
//! which maps each property name to a slot index. Adding a property follows
//! (or creates) a transition to a child shape. Inline caches remember
//! "shape S has property P at slot N", which makes property access on
//! monomorphic code a comparison and an indexed load.

use std::cell::OnceCell;

use rustc_hash::FxHashMap;

use crate::gc::Tracer;
use crate::object::{PropFlags, PropertyKey};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ShapeId(pub u32);

impl ShapeId {
    /// The empty shape
    pub const ROOT: ShapeId = ShapeId(0);
    /// Objects in dictionary mode (never matched by inline caches)
    pub const DICT: ShapeId = ShapeId(u32::MAX);
    /// Pseudo-shape of primitive strings (inline caches for their methods)
    pub const PRIMITIVE_STRING: ShapeId = ShapeId(u32::MAX - 1);
}

/// Shapes with more properties than this get a lookup table
const LINEAR_LOOKUP_MAX: u32 = 8;

enum Transitions {
    None,
    One(PropertyKey, PropFlags, u32),
    Many(FxHashMap<(PropertyKey, PropFlags), u32>),
}

struct Node {
    parent: u32,
    key: PropertyKey,
    flags: PropFlags,
    /// Number of properties (the new property's slot is `len - 1`)
    len: u32,
    transitions: Transitions,
    table: OnceCell<FxHashMap<PropertyKey, (u32, PropFlags)>>,
}

pub struct Shapes {
    nodes: Vec<Node>,
}

impl Default for Shapes {
    fn default() -> Self {
        Self::new()
    }
}

impl Shapes {
    pub fn new() -> Shapes {
        Shapes {
            nodes: vec![Node {
                parent: u32::MAX,
                key: PropertyKey::Index(0),
                flags: PropFlags::NONE,
                len: 0,
                transitions: Transitions::None,
                table: OnceCell::new(),
            }],
        }
    }

    #[inline]
    pub fn len(&self, shape: ShapeId) -> u32 {
        self.nodes[shape.0 as usize].len
    }

    pub fn count(&self) -> usize {
        self.nodes.len()
    }

    /// Shape with `key` added
    pub fn add(&mut self, shape: ShapeId, key: PropertyKey, flags: PropFlags) -> ShapeId {
        let node = &self.nodes[shape.0 as usize];
        match &node.transitions {
            Transitions::One(k, f, child) if *k == key && *f == flags => return ShapeId(*child),
            Transitions::Many(map) => {
                if let Some(&child) = map.get(&(key, flags)) {
                    return ShapeId(child);
                }
            }
            _ => {}
        }
        let len = node.len + 1;
        let child = self.nodes.len() as u32;
        self.nodes.push(Node {
            parent: shape.0,
            key,
            flags,
            len,
            transitions: Transitions::None,
            table: OnceCell::new(),
        });
        let node = &mut self.nodes[shape.0 as usize];
        node.transitions = match std::mem::replace(&mut node.transitions, Transitions::None) {
            Transitions::None => Transitions::One(key, flags, child),
            Transitions::One(k, f, c) => {
                let mut map = FxHashMap::default();
                map.insert((k, f), c);
                map.insert((key, flags), child);
                Transitions::Many(map)
            }
            Transitions::Many(mut map) => {
                map.insert((key, flags), child);
                Transitions::Many(map)
            }
        };
        ShapeId(child)
    }

    /// Slot and flags of `key`
    pub fn lookup(&self, shape: ShapeId, key: PropertyKey) -> Option<(u32, PropFlags)> {
        let node = &self.nodes[shape.0 as usize];
        if node.len > LINEAR_LOOKUP_MAX {
            let table = node.table.get_or_init(|| {
                let mut table = FxHashMap::default();
                let mut id = shape.0;
                while id != 0 {
                    let n = &self.nodes[id as usize];
                    table.insert(n.key, (n.len - 1, n.flags));
                    id = n.parent;
                }
                table
            });
            return table.get(&key).copied();
        }
        let mut id = shape.0;
        while id != 0 {
            let n = &self.nodes[id as usize];
            if n.key == key {
                return Some((n.len - 1, n.flags));
            }
            id = n.parent;
        }
        None
    }

    /// Properties in insertion (slot) order
    pub fn properties(&self, shape: ShapeId) -> Vec<(PropertyKey, PropFlags)> {
        let mut out = Vec::with_capacity(self.len(shape) as usize);
        let mut id = shape.0;
        while id != 0 {
            let n = &self.nodes[id as usize];
            out.push((n.key, n.flags));
            id = n.parent;
        }
        out.reverse();
        out
    }

    /// Symbols used as property keys stay alive with their shapes (shapes
    /// compare keys by identity and are never freed)
    pub fn trace(&self, tracer: &mut Tracer) {
        for node in &self.nodes {
            if let PropertyKey::Symbol(s) = node.key {
                tracer.mark(s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::string::Atom;

    #[test]
    fn test_transitions_are_shared() {
        let mut shapes = Shapes::new();
        let x = PropertyKey::Atom(Atom(100));
        let y = PropertyKey::Atom(Atom(101));
        let a = shapes.add(ShapeId::ROOT, x, PropFlags::DEFAULT);
        let ab = shapes.add(a, y, PropFlags::DEFAULT);
        assert_eq!(shapes.add(ShapeId::ROOT, x, PropFlags::DEFAULT), a);
        assert_eq!(shapes.add(a, y, PropFlags::DEFAULT), ab);
        assert_eq!(shapes.lookup(ab, x), Some((0, PropFlags::DEFAULT)));
        assert_eq!(shapes.lookup(ab, y), Some((1, PropFlags::DEFAULT)));
        assert_eq!(shapes.lookup(a, y), None);
        // Different order, different shape
        let b = shapes.add(ShapeId::ROOT, y, PropFlags::DEFAULT);
        let ba = shapes.add(b, x, PropFlags::DEFAULT);
        assert_ne!(ab, ba);
        assert_eq!(shapes.properties(ba), vec![(y, PropFlags::DEFAULT), (x, PropFlags::DEFAULT)]);
    }

    #[test]
    fn test_large_shapes_use_a_table() {
        let mut shapes = Shapes::new();
        let mut s = ShapeId::ROOT;
        for i in 0..100 {
            s = shapes.add(s, PropertyKey::Atom(Atom(i)), PropFlags::DEFAULT);
        }
        for i in 0..100 {
            assert_eq!(shapes.lookup(s, PropertyKey::Atom(Atom(i))), Some((i, PropFlags::DEFAULT)));
        }
        assert_eq!(shapes.lookup(s, PropertyKey::Atom(Atom(1000))), None);
    }
}
