//! Garbage-collected heap
//!
//! A precise, non-moving mark-and-sweep collector. Every cell starts with a
//! header linking it into the heap's list of allocations; collection marks
//! from the roots with an explicit stack (no recursion, so deep structures
//! can't overflow the native stack) and frees unmarked cells.
//!
//! Collection only happens at safepoints chosen by the interpreter
//! (function entry and loop back edges). At those points every live value
//! is reachable from the VM's roots, so native code never needs to root the
//! values it holds between allocations.

use std::cell::{Cell, UnsafeCell};
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::object::{JsObject, Symbol, Upvalue};
use crate::string::JsString;
use crate::value::Value;

/// What kind of value a cell holds (needed to trace and free it)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum CellKind {
    String,
    Object,
    Upvalue,
    Symbol,
}

#[repr(C)]
pub struct Header {
    next: Cell<*mut Header>,
    marked: Cell<bool>,
    kind: CellKind,
    /// Approximate size in bytes, for collection scheduling
    size: Cell<u32>,
}

#[repr(C)]
pub struct GcBox<T> {
    header: Header,
    value: UnsafeCell<T>,
}

/// Pointer to a garbage-collected value.
///
/// Valid as long as the value is reachable from the roots at every
/// collection. The VM is single-threaded, and references handed out by
/// `get`/`get_mut` must not be held across operations that may touch the
/// same cell (for example a getter call).
pub struct Gc<T> {
    ptr: NonNull<GcBox<T>>,
    _marker: PhantomData<T>,
}

impl<T> Clone for Gc<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Gc<T> {}

impl<T> PartialEq for Gc<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<T> Eq for Gc<T> {}

impl<T> std::hash::Hash for Gc<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ptr.hash(state)
    }
}

impl<T> std::fmt::Debug for Gc<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Gc({:p})", self.ptr)
    }
}

impl<T> Gc<T> {
    /// Address of the cell (for NaN-boxing)
    #[inline(always)]
    pub fn addr(self) -> usize {
        self.ptr.as_ptr() as usize
    }

    /// Rebuild a pointer from a cell address produced by `addr`
    ///
    /// # Safety
    /// `addr` must come from `Gc::<T>::addr` of a live cell.
    #[inline(always)]
    pub unsafe fn from_addr(addr: usize) -> Self {
        Gc { ptr: unsafe { NonNull::new_unchecked(addr as *mut GcBox<T>) }, _marker: PhantomData }
    }

    #[inline(always)]
    pub fn get(&self) -> &T {
        unsafe { &*(*self.ptr.as_ptr()).value.get() }
    }

    /// Mutable access. The VM is single-threaded; callers must not keep
    /// another reference to the same cell alive meanwhile.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn get_mut(&self) -> &mut T {
        unsafe { &mut *(*self.ptr.as_ptr()).value.get() }
    }

    #[inline(always)]
    fn header(&self) -> &Header {
        unsafe { &(*self.ptr.as_ptr()).header }
    }

    /// Whether the cell survived the marking phase in progress
    pub fn is_marked(&self) -> bool {
        self.header().marked.get()
    }

    /// Update the size estimate (e.g. after flattening a rope)
    pub fn set_size(&self, size: usize) {
        self.header().size.set(size.min(u32::MAX as usize) as u32);
    }
}

/// Types that live in the GC heap
pub trait Trace {
    const KIND: CellKind;
    fn trace(&self, tracer: &mut Tracer);
}

/// Marking state: cells marked but not yet scanned
pub struct Tracer {
    stack: Vec<*mut Header>,
}

impl Tracer {
    #[inline]
    pub fn mark<T: Trace>(&mut self, gc: Gc<T>) {
        let header = gc.header();
        if !header.marked.get() {
            header.marked.set(true);
            self.stack.push(gc.ptr.as_ptr() as *mut Header);
        }
    }

    #[inline]
    pub fn mark_value(&mut self, value: Value) {
        if let Some(s) = value.as_string() {
            self.mark(s);
        } else if let Some(o) = value.as_object() {
            self.mark(o);
        } else if let Some(s) = value.as_symbol() {
            self.mark(s);
        }
    }

    pub fn mark_values(&mut self, values: &[Value]) {
        for &v in values {
            self.mark_value(v);
        }
    }
}

pub struct Heap {
    head: Cell<*mut Header>,
    /// Bytes in live cells (as of the last collection) plus allocated since
    bytes: Cell<usize>,
    /// Collect when `bytes` exceeds this
    threshold: Cell<usize>,
    cells: Cell<usize>,
    collections: Cell<u32>,
}

/// Never collect below this heap size
const MIN_THRESHOLD: usize = 4 * 1024 * 1024;

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

impl Heap {
    pub fn new() -> Self {
        Heap {
            head: Cell::new(std::ptr::null_mut()),
            bytes: Cell::new(0),
            threshold: Cell::new(MIN_THRESHOLD),
            cells: Cell::new(0),
            collections: Cell::new(0),
        }
    }

    /// Allocate a cell. `extra` estimates heap memory the value owns
    /// outside the cell (string contents, vectors).
    pub fn alloc<T: Trace>(&self, value: T, extra: usize) -> Gc<T> {
        let size = std::mem::size_of::<GcBox<T>>() + extra;
        let boxed = Box::new(GcBox {
            header: Header {
                next: Cell::new(self.head.get()),
                marked: Cell::new(false),
                kind: T::KIND,
                size: Cell::new(size.min(u32::MAX as usize) as u32),
            },
            value: UnsafeCell::new(value),
        });
        let ptr = Box::into_raw(boxed);
        self.head.set(ptr as *mut Header);
        self.bytes.set(self.bytes.get() + size);
        self.cells.set(self.cells.get() + 1);
        Gc { ptr: unsafe { NonNull::new_unchecked(ptr) }, _marker: PhantomData }
    }

    /// Record memory a cell gained after allocation (e.g. a growing array)
    pub fn note_growth(&self, bytes: usize) {
        self.bytes.set(self.bytes.get() + bytes);
    }

    /// Whether allocation since the last collection warrants another
    #[inline]
    pub fn should_collect(&self) -> bool {
        self.bytes.get() > self.threshold.get()
    }

    pub fn bytes(&self) -> usize {
        self.bytes.get()
    }

    pub fn cell_count(&self) -> usize {
        self.cells.get()
    }

    pub fn collections(&self) -> u32 {
        self.collections.get()
    }

    /// Collect garbage. `mark_roots` marks every root; `before_sweep`
    /// runs after marking completes, when `Gc::is_marked` tells what will
    /// survive (for weak tables).
    pub fn collect(&self, mark_roots: impl FnOnce(&mut Tracer), before_sweep: impl FnOnce()) {
        let mut tracer = Tracer { stack: Vec::with_capacity(256) };
        mark_roots(&mut tracer);
        while let Some(header) = tracer.stack.pop() {
            unsafe { trace_cell(header, &mut tracer) };
        }
        before_sweep();

        // Sweep
        let mut live_bytes = 0usize;
        let mut live_cells = 0usize;
        let mut prev: *mut Header = std::ptr::null_mut();
        let mut cur = self.head.get();
        while !cur.is_null() {
            let header = unsafe { &*cur };
            let next = header.next.get();
            if header.marked.get() {
                header.marked.set(false);
                live_bytes += header.size.get() as usize;
                live_cells += 1;
                prev = cur;
            } else {
                if prev.is_null() {
                    self.head.set(next);
                } else {
                    unsafe { (*prev).next.set(next) };
                }
                unsafe { free_cell(cur) };
            }
            cur = next;
        }
        self.bytes.set(live_bytes);
        self.cells.set(live_cells);
        self.threshold.set((live_bytes * 2).max(MIN_THRESHOLD));
        self.collections.set(self.collections.get() + 1);
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        let mut cur = self.head.get();
        while !cur.is_null() {
            let next = unsafe { (*cur).next.get() };
            unsafe { free_cell(cur) };
            cur = next;
        }
    }
}

unsafe fn trace_cell(header: *mut Header, tracer: &mut Tracer) {
    unsafe {
        match (*header).kind {
            CellKind::String => (*(*(header as *mut GcBox<JsString>)).value.get()).trace(tracer),
            CellKind::Object => (*(*(header as *mut GcBox<JsObject>)).value.get()).trace(tracer),
            CellKind::Upvalue => (*(*(header as *mut GcBox<Upvalue>)).value.get()).trace(tracer),
            CellKind::Symbol => (*(*(header as *mut GcBox<Symbol>)).value.get()).trace(tracer),
        }
    }
}

unsafe fn free_cell(header: *mut Header) {
    unsafe {
        match (*header).kind {
            CellKind::String => drop(Box::from_raw(header as *mut GcBox<JsString>)),
            CellKind::Object => drop(Box::from_raw(header as *mut GcBox<JsObject>)),
            CellKind::Upvalue => drop(Box::from_raw(header as *mut GcBox<Upvalue>)),
            CellKind::Symbol => drop(Box::from_raw(header as *mut GcBox<Symbol>)),
        }
    }
}
