//! Compressed Pointers (Phase 24.4)
//!
//! 32-bit relative offsets instead of 64-bit pointers.
//! Tagged pointers (type in unused low 3 bits).
//! 50% less memory for pointer-heavy structures.

use std::marker::PhantomData;

/// Compressed pointer using 32-bit relative offset
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct CompressedPtr<T> {
    /// Relative offset (signed for bidirectional)
    offset: i32,
    _marker: PhantomData<*const T>,
}

impl<T> CompressedPtr<T> {
    /// Null pointer
    pub const NULL: Self = Self {
        offset: i32::MIN,
        _marker: PhantomData,
    };
    
    /// Check if null
    pub fn is_null(&self) -> bool {
        self.offset == i32::MIN
    }
    
    /// Create from absolute pointer and base
    pub fn from_ptr(ptr: *const T, base: *const u8) -> Option<Self> {
        if ptr.is_null() {
            return Some(Self::NULL);
        }
        
        let offset = (ptr as isize) - (base as isize);
        
        // Check if offset fits in i32
        if offset >= i32::MIN as isize && offset <= i32::MAX as isize {
            Some(Self {
                offset: offset as i32,
                _marker: PhantomData,
            })
        } else {
            None // Offset too large
        }
    }
    
    /// Convert back to pointer
    pub fn to_ptr(&self, base: *const u8) -> *const T {
        if self.is_null() {
            std::ptr::null()
        } else {
            unsafe { base.offset(self.offset as isize) as *const T }
        }
    }
    
    /// Get offset
    pub fn offset(&self) -> i32 {
        self.offset
    }
}

// Unsafe: only use when you know what you're doing
unsafe impl<T: Send> Send for CompressedPtr<T> {}
unsafe impl<T: Sync> Sync for CompressedPtr<T> {}

/// Tagged pointer - store type in low 3 bits
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct TaggedPtr {
    /// Pointer with tag in low bits
    bits: usize,
}

impl TaggedPtr {
    /// Tag mask (3 bits)
    const TAG_MASK: usize = 0b111;
    
    /// Create from raw pointer and tag
    pub fn new(ptr: *const u8, tag: u8) -> Self {
        debug_assert!(tag <= 7, "Tag must fit in 3 bits");
        
        // Pointers should be at least 8-byte aligned
        let ptr_bits = ptr as usize;
        debug_assert!(ptr_bits & Self::TAG_MASK == 0, "Pointer must be 8-byte aligned");
        
        Self {
            bits: ptr_bits | (tag as usize),
        }
    }
    
    /// Get the tag
    pub fn tag(&self) -> u8 {
        (self.bits & Self::TAG_MASK) as u8
    }
    
    /// Get the pointer (with tag cleared)
    pub fn ptr(&self) -> *const u8 {
        (self.bits & !Self::TAG_MASK) as *const u8
    }
    
    /// Get as typed pointer
    pub fn as_ptr<T>(&self) -> *const T {
        self.ptr() as *const T
    }
    
    /// Set tag
    pub fn with_tag(mut self, tag: u8) -> Self {
        debug_assert!(tag <= 7);
        self.bits = (self.bits & !Self::TAG_MASK) | (tag as usize);
        self
    }
    
    /// Check if null
    pub fn is_null(&self) -> bool {
        self.ptr().is_null()
    }
    
    /// Null tagged pointer
    pub const fn null(tag: u8) -> Self {
        Self { bits: tag as usize }
    }
    
    /// Raw bits
    pub fn bits(&self) -> usize {
        self.bits
    }
}

/// DOM node types for tagging
#[repr(u8)]
pub enum NodeTag {
    Element = 0,
    Text = 1,
    Comment = 2,
    Document = 3,
    DocumentFragment = 4,
    Attribute = 5,
    // 6, 7 reserved
}

impl NodeTag {
    pub fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Element),
            1 => Some(Self::Text),
            2 => Some(Self::Comment),
            3 => Some(Self::Document),
            4 => Some(Self::DocumentFragment),
            5 => Some(Self::Attribute),
            _ => None,
        }
    }
}

/// Compact node ID (32-bit instead of 64-bit pointer)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct CompactNodeId(pub u32);

impl CompactNodeId {
    pub const NULL: Self = Self(u32::MAX);
    
    pub fn new(id: u32) -> Self {
        Self(id)
    }
    
    pub fn is_null(&self) -> bool {
        self.0 == u32::MAX
    }
    
    pub fn index(&self) -> usize {
        self.0 as usize
    }
}

/// Arena-relative pointer
#[derive(Debug, Clone, Copy)]
pub struct ArenaPtr<T> {
    /// Offset into arena
    offset: u32,
    _marker: PhantomData<T>,
}

impl<T> ArenaPtr<T> {
    pub const NULL: Self = Self {
        offset: u32::MAX,
        _marker: PhantomData,
    };
    
    pub fn new(offset: u32) -> Self {
        Self {
            offset,
            _marker: PhantomData,
        }
    }
    
    pub fn is_null(&self) -> bool {
        self.offset == u32::MAX
    }
    
    pub fn offset(&self) -> u32 {
        self.offset
    }
    
    /// Resolve against arena base
    pub unsafe fn resolve(&self, base: *const u8) -> *const T {
        if self.is_null() {
            std::ptr::null()
        } else {
            base.add(self.offset as usize) as *const T
        }
    }
    
    /// Resolve mutable
    pub unsafe fn resolve_mut(&self, base: *mut u8) -> *mut T {
        if self.is_null() {
            std::ptr::null_mut()
        } else {
            base.add(self.offset as usize) as *mut T
        }
    }
}

/// Calculate savings from compressed pointers
pub fn pointer_savings(pointer_count: usize, bits_64: bool) -> usize {
    if bits_64 {
        pointer_count * 4 // 8 bytes -> 4 bytes = 4 bytes saved per pointer
    } else {
        0 // Already 32-bit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tagged_pointer() {
        let data = [0u8; 64]; // 64-byte aligned
        let base = data.as_ptr();
        
        // Ensure alignment (find 8-byte aligned address)
        let aligned = ((base as usize + 7) & !7) as *const u8;
        
        let tagged = TaggedPtr::new(aligned, 3);
        
        assert_eq!(tagged.tag(), 3);
        assert_eq!(tagged.ptr(), aligned);
    }
    
    #[test]
    fn test_null_tagged() {
        let null = TaggedPtr::null(NodeTag::Element as u8);
        
        assert!(null.is_null());
        assert_eq!(null.tag(), 0);
    }
    
    #[test]
    fn test_compact_node_id() {
        let id = CompactNodeId::new(42);
        
        assert!(!id.is_null());
        assert_eq!(id.index(), 42);
        
        let null = CompactNodeId::NULL;
        assert!(null.is_null());
    }
    
    #[test]
    fn test_pointer_savings() {
        // 1000 pointers on 64-bit
        let savings = pointer_savings(1000, true);
        assert_eq!(savings, 4000); // 4KB saved
    }
    
    #[test]
    fn test_arena_ptr() {
        let arena = [0u8; 1024];
        let base = arena.as_ptr();
        
        let ptr: ArenaPtr<u32> = ArenaPtr::new(100);
        
        unsafe {
            let resolved = ptr.resolve(base);
            assert_eq!(resolved as usize - base as usize, 100);
        }
    }
}
