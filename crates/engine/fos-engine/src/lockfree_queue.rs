//! Lock-Free Queue Implementations
//!
//! Wait-free and lock-free queue implementations for concurrent communication.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

// ============================================================================
// SPSC Queue - Single Producer, Single Consumer
// ============================================================================

/// Single-producer, single-consumer bounded queue
/// 
/// Uses a ring buffer for high performance communication between two threads.
pub struct SpscQueue<T> {
    /// Ring buffer
    buffer: Box<[UnsafeCell<MaybeUninit<T>>]>,
    /// Capacity (power of 2)
    capacity: usize,
    /// Mask for index wrapping
    mask: usize,
    /// Head index (consumer reads from here)
    head: AtomicUsize,
    /// Tail index (producer writes here)
    tail: AtomicUsize,
}

impl<T> SpscQueue<T> {
    /// Create a new SPSC queue with given capacity (rounded up to power of 2)
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two().max(2);
        let buffer: Vec<_> = (0..capacity)
            .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
            .collect();
        
        Self {
            buffer: buffer.into_boxed_slice(),
            capacity,
            mask: capacity - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
    
    /// Push an item (producer only)
    /// 
    /// Returns `Err(item)` if the queue is full.
    pub fn push(&self, item: T) -> Result<(), T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) & self.mask;
        
        // Check if full
        if next_tail == self.head.load(Ordering::Acquire) {
            return Err(item);
        }
        
        // Write item
        unsafe {
            (*self.buffer[tail].get()).write(item);
        }
        
        // Publish
        self.tail.store(next_tail, Ordering::Release);
        Ok(())
    }
    
    /// Pop an item (consumer only)
    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);
        
        // Check if empty
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        
        // Read item
        let item = unsafe {
            (*self.buffer[head].get()).assume_init_read()
        };
        
        // Advance head
        let next_head = (head + 1) & self.mask;
        self.head.store(next_head, Ordering::Release);
        
        Some(item)
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }
    
    /// Check if full
    pub fn is_full(&self) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) & self.mask;
        next_tail == self.head.load(Ordering::Acquire)
    }
    
    /// Get approximate length
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        if tail >= head {
            tail - head
        } else {
            self.capacity - head + tail
        }
    }
    
    /// Get capacity
    pub fn capacity(&self) -> usize {
        self.capacity - 1 // One slot always empty
    }
}

impl<T> Drop for SpscQueue<T> {
    fn drop(&mut self) {
        // Drop remaining items
        while self.pop().is_some() {}
    }
}

// SAFETY: SpscQueue is thread-safe for one producer and one consumer
unsafe impl<T: Send> Send for SpscQueue<T> {}
unsafe impl<T: Send> Sync for SpscQueue<T> {}

// ============================================================================
// MPSC Queue - Multi Producer, Single Consumer
// ============================================================================

/// Node in the MPSC queue
struct MpscNode<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    next: AtomicPtr<MpscNode<T>>,
}

impl<T> MpscNode<T> {
    fn empty() -> *mut Self {
        Box::into_raw(Box::new(Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            next: AtomicPtr::new(ptr::null_mut()),
        }))
    }
    
    fn with_value(value: T) -> *mut Self {
        Box::into_raw(Box::new(Self {
            value: UnsafeCell::new(MaybeUninit::new(value)),
            next: AtomicPtr::new(ptr::null_mut()),
        }))
    }
}

/// Multi-producer, single-consumer unbounded queue
/// 
/// Lock-free for producers, wait-free for consumer.
pub struct MpscQueue<T> {
    /// Head (consumer side)
    head: AtomicPtr<MpscNode<T>>,
    /// Tail (producer side)
    tail: AtomicPtr<MpscNode<T>>,
    /// Approximate length
    len: AtomicUsize,
}

impl<T> MpscQueue<T> {
    /// Create a new MPSC queue
    pub fn new() -> Self {
        let stub = MpscNode::empty();
        Self {
            head: AtomicPtr::new(stub),
            tail: AtomicPtr::new(stub),
            len: AtomicUsize::new(0),
        }
    }
    
    /// Push an item (lock-free for multiple producers)
    pub fn push(&self, value: T) {
        let node = MpscNode::with_value(value);
        
        loop {
            let tail = self.tail.load(Ordering::Acquire);
            let next = unsafe { (*tail).next.load(Ordering::Acquire) };
            
            if next.is_null() {
                // Try to link new node
                if unsafe { (*tail).next.compare_exchange(
                    ptr::null_mut(),
                    node,
                    Ordering::Release,
                    Ordering::Relaxed,
                ).is_ok() } {
                    // Try to advance tail (ok if it fails, someone else did it)
                    let _ = self.tail.compare_exchange(
                        tail,
                        node,
                        Ordering::Release,
                        Ordering::Relaxed,
                    );
                    self.len.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            } else {
                // Tail is behind, try to advance it
                let _ = self.tail.compare_exchange(
                    tail,
                    next,
                    Ordering::Release,
                    Ordering::Relaxed,
                );
            }
        }
    }
    
    /// Pop an item (single consumer only)
    pub fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            let tail = self.tail.load(Ordering::Acquire);
            let next = unsafe { (*head).next.load(Ordering::Acquire) };
            
            if head == tail {
                if next.is_null() {
                    // Queue is empty
                    return None;
                }
                // Tail is behind, advance it
                let _ = self.tail.compare_exchange(
                    tail,
                    next,
                    Ordering::Release,
                    Ordering::Relaxed,
                );
            } else if !next.is_null() {
                // Read value from next (head is stub)
                let value = unsafe { (*(*next).value.get()).assume_init_read() };
                
                // Advance head
                if self.head.compare_exchange(
                    head,
                    next,
                    Ordering::Release,
                    Ordering::Relaxed,
                ).is_ok() {
                    // Free old head (was the stub or previous node)
                    unsafe { drop(Box::from_raw(head)) };
                    self.len.fetch_sub(1, Ordering::Relaxed);
                    return Some(value);
                }
            }
        }
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let next = unsafe { (*head).next.load(Ordering::Acquire) };
        next.is_null()
    }
    
    /// Get approximate length
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }
}

impl<T> Default for MpscQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for MpscQueue<T> {
    fn drop(&mut self) {
        // Drain and free all nodes
        while self.pop().is_some() {}
        
        // Free the stub node
        let head = self.head.load(Ordering::Relaxed);
        if !head.is_null() {
            unsafe { drop(Box::from_raw(head)) };
        }
    }
}

// SAFETY: MpscQueue is thread-safe
unsafe impl<T: Send> Send for MpscQueue<T> {}
unsafe impl<T: Send> Sync for MpscQueue<T> {}

// ============================================================================
// Steal Queue - Work Stealing Deque
// ============================================================================

/// Work-stealing deque for thread pools
/// 
/// Owner can push/pop from one end (LIFO), thieves can steal from the other end (FIFO).
pub struct StealQueue<T> {
    /// Ring buffer
    buffer: Box<[UnsafeCell<MaybeUninit<T>>]>,
    /// Capacity
    capacity: usize,
    /// Mask
    mask: usize,
    /// Top index (thieves steal from here)
    top: AtomicUsize,
    /// Bottom index (owner pushes/pops here)
    bottom: AtomicUsize,
}

impl<T> StealQueue<T> {
    /// Create a new steal queue
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two().max(16);
        let buffer: Vec<_> = (0..capacity)
            .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
            .collect();
        
        Self {
            buffer: buffer.into_boxed_slice(),
            capacity,
            mask: capacity - 1,
            top: AtomicUsize::new(0),
            bottom: AtomicUsize::new(0),
        }
    }
    
    /// Push to bottom (owner only)
    pub fn push(&self, item: T) -> Result<(), T> {
        let bottom = self.bottom.load(Ordering::Relaxed);
        let top = self.top.load(Ordering::Acquire);
        
        // Check if full
        if bottom.wrapping_sub(top) >= self.capacity {
            return Err(item);
        }
        
        // Write item
        let idx = bottom & self.mask;
        unsafe {
            (*self.buffer[idx].get()).write(item);
        }
        
        // Publish
        self.bottom.store(bottom.wrapping_add(1), Ordering::Release);
        Ok(())
    }
    
    /// Pop from bottom (owner only)
    pub fn pop(&self) -> Option<T> {
        let bottom = self.bottom.load(Ordering::Relaxed);
        if bottom == 0 {
            return None;
        }

        let new_bottom = bottom.wrapping_sub(1);
        self.bottom.store(new_bottom, Ordering::SeqCst);

        let top = self.top.load(Ordering::SeqCst);

        if top > new_bottom {
            // Queue was empty after we decremented
            self.bottom.store(top, Ordering::Relaxed);
            return None;
        }

        // Copy the slot out; it only becomes ours once no thief can take it
        let idx = new_bottom & self.mask;
        let item: MaybeUninit<T> = unsafe { std::ptr::read(self.buffer[idx].get()) };

        if top == new_bottom {
            // The last item: race the thieves for it
            let won = self.top.compare_exchange(
                top,
                top.wrapping_add(1),
                Ordering::SeqCst,
                Ordering::Relaxed,
            ).is_ok();
            self.bottom.store(top.wrapping_add(1), Ordering::Relaxed);
            if !won {
                // A thief took it: the copy is not ours to drop
                return None;
            }
        }

        Some(unsafe { item.assume_init() })
    }

    /// Steal from top (thieves)
    pub fn steal(&self) -> Option<T> {
        loop {
            let top = self.top.load(Ordering::Acquire);
            // Order the read of `top` before the read of `bottom` against
            // the owner's `pop` (Chase-Lev)
            std::sync::atomic::fence(Ordering::SeqCst);
            let bottom = self.bottom.load(Ordering::Acquire);

            if top >= bottom {
                return None; // Empty
            }

            // Copy the slot out; it only becomes ours if we advance `top`.
            // A volatile read, as the slot may be reused by the owner once
            // another thief has taken it (the CAS below then fails).
            let idx = top & self.mask;
            let item: MaybeUninit<T> = unsafe { std::ptr::read_volatile(self.buffer[idx].get()) };

            if self.top.compare_exchange(
                top,
                top.wrapping_add(1),
                Ordering::SeqCst,
                Ordering::Relaxed,
            ).is_ok() {
                return Some(unsafe { item.assume_init() });
            }
            // Lost the race: the copy belongs to whoever won, so it is
            // forgotten (MaybeUninit never drops), not dropped
        }
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        let top = self.top.load(Ordering::Relaxed);
        let bottom = self.bottom.load(Ordering::Relaxed);
        top >= bottom
    }
    
    /// Get approximate length
    pub fn len(&self) -> usize {
        let top = self.top.load(Ordering::Relaxed);
        let bottom = self.bottom.load(Ordering::Relaxed);
        bottom.saturating_sub(top)
    }
}

impl<T> Drop for StealQueue<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}

// SAFETY: StealQueue is thread-safe
unsafe impl<T: Send> Send for StealQueue<T> {}
unsafe impl<T: Send> Sync for StealQueue<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    
    #[test]
    fn test_spsc_basic() {
        let queue = SpscQueue::new(8);
        
        assert!(queue.is_empty());
        
        queue.push(1).unwrap();
        queue.push(2).unwrap();
        queue.push(3).unwrap();
        
        assert_eq!(queue.len(), 3);
        
        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), Some(3));
        assert_eq!(queue.pop(), None);
    }
    
    #[test]
    fn test_spsc_full() {
        let queue = SpscQueue::new(4); // Actually 4, but 3 usable
        
        queue.push(1).unwrap();
        queue.push(2).unwrap();
        queue.push(3).unwrap();
        
        // Should be full now
        assert!(queue.push(4).is_err());
        
        queue.pop();
        queue.push(4).unwrap();
    }
    
    #[test]
    fn test_spsc_concurrent() {
        let queue = Arc::new(SpscQueue::new(1024));
        let queue2 = Arc::clone(&queue);
        
        let producer = thread::spawn(move || {
            for i in 0..1000 {
                while queue2.push(i).is_err() {
                    thread::yield_now();
                }
            }
        });
        
        let consumer = thread::spawn(move || {
            let mut received = Vec::new();
            while received.len() < 1000 {
                if let Some(v) = queue.pop() {
                    received.push(v);
                } else {
                    thread::yield_now();
                }
            }
            received
        });
        
        producer.join().unwrap();
        let received = consumer.join().unwrap();
        
        assert_eq!(received, (0..1000).collect::<Vec<_>>());
    }
    
    #[test]
    fn test_mpsc_basic() {
        let queue: MpscQueue<i32> = MpscQueue::new();
        
        assert!(queue.is_empty());
        
        queue.push(1);
        queue.push(2);
        queue.push(3);
        
        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), Some(3));
        assert_eq!(queue.pop(), None);
    }
    
    #[test]
    fn test_mpsc_concurrent() {
        let queue = Arc::new(MpscQueue::new());
        let mut handles = vec![];
        
        for t in 0..4 {
            let queue = Arc::clone(&queue);
            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    queue.push(t * 100 + i);
                }
            }));
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        let mut values = vec![];
        while let Some(v) = queue.pop() {
            values.push(v);
        }
        
        assert_eq!(values.len(), 400);
        values.sort();
        assert_eq!(values, (0..400).collect::<Vec<_>>());
    }
    
    #[test]
    fn test_steal_queue_basic() {
        let queue: StealQueue<i32> = StealQueue::new(16);
        
        queue.push(1).unwrap();
        queue.push(2).unwrap();
        queue.push(3).unwrap();
        
        // Pop returns LIFO (from bottom)
        assert_eq!(queue.pop(), Some(3));
        
        // Steal returns FIFO (from top)
        assert_eq!(queue.steal(), Some(1));
        assert_eq!(queue.steal(), Some(2));
        assert_eq!(queue.steal(), None);
    }
    
    #[test]
    fn test_steal_queue_concurrent() {
        use std::sync::atomic::AtomicBool;
        let queue = Arc::new(StealQueue::new(256));
        let queue2 = Arc::clone(&queue);
        let done = Arc::new(AtomicBool::new(false));
        let done2 = Arc::clone(&done);

        // Owner pushes
        let owner = thread::spawn(move || {
            for i in 0..100 {
                while queue2.push(i).is_err() {
                    thread::yield_now();
                }
            }
            done2.store(true, Ordering::Release);
        });

        // Thief steals until it has half of them (the owner never pops, so
        // they are all there to be stolen eventually)
        let thief = thread::spawn(move || {
            let mut stolen = vec![];
            while stolen.len() < 50 {
                match queue.steal() {
                    Some(v) => stolen.push(v),
                    None if done.load(Ordering::Acquire) && queue.is_empty() => break,
                    None => thread::yield_now(),
                }
            }
            stolen
        });

        owner.join().unwrap();
        let stolen = thief.join().unwrap();

        // Stealing takes the oldest items, in order, each once
        assert_eq!(stolen, (0..50).collect::<Vec<_>>());
    }

    /// Owner pushing and popping against several thieves: every item is
    /// taken exactly once, and none is dropped twice
    #[test]
    fn test_steal_queue_every_item_once() {
        use std::sync::atomic::{AtomicBool, AtomicUsize};
        static DROPS: AtomicUsize = AtomicUsize::new(0);
        struct Item(usize, Box<u64>);
        impl Drop for Item {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }

        const N: usize = 20_000;
        for _round in 0..5 {
            DROPS.store(0, Ordering::Relaxed);
            let queue: Arc<StealQueue<Item>> = Arc::new(StealQueue::new(64));
            let done = Arc::new(AtomicBool::new(false));
            let thieves: Vec<_> = (0..3)
                .map(|_| {
                    let q = Arc::clone(&queue);
                    let done = Arc::clone(&done);
                    thread::spawn(move || {
                        let mut got: Vec<usize> = Vec::new();
                        loop {
                            match q.steal() {
                                Some(item) => {
                                    assert_eq!(*item.1, item.0 as u64);
                                    got.push(item.0);
                                }
                                None if done.load(Ordering::Acquire) => return got,
                                None => thread::yield_now(),
                            }
                        }
                    })
                })
                .collect();
            let mut mine = Vec::new();
            for i in 0..N {
                let mut item = Item(i, Box::new(i as u64));
                loop {
                    match queue.push(item) {
                        Ok(()) => break,
                        Err(back) => {
                            item = back;
                            if let Some(item) = queue.pop() {
                                assert_eq!(*item.1, item.0 as u64);
                                mine.push(item.0);
                            }
                        }
                    }
                }
                if i % 3 == 0 {
                    if let Some(item) = queue.pop() {
                        assert_eq!(*item.1, item.0 as u64);
                        mine.push(item.0);
                    }
                }
            }
            while let Some(item) = queue.pop() {
                mine.push(item.0);
            }
            done.store(true, Ordering::Release);
            for t in thieves {
                mine.extend(t.join().unwrap());
            }
            mine.sort_unstable();
            assert_eq!(mine, (0..N).collect::<Vec<_>>(), "items lost or duplicated");
            assert_eq!(DROPS.load(Ordering::Relaxed), N, "an item was dropped twice or not at all");
        }
    }
}
