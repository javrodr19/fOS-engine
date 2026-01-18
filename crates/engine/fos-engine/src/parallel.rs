//! Custom Parallel Iteration Primitives
//!
//! Provides parallel iteration, scoped threads, and work distribution
//! without external dependencies like rayon.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

/// Get the number of available CPU cores
pub fn num_cpus() -> usize {
    thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}

// ============================================================================
// Parallel For Each
// ============================================================================

/// Execute a function for each item in parallel
pub fn parallel_for_each<T, F>(items: Vec<T>, f: F)
where
    T: Send + 'static,
    F: Fn(T) + Send + Sync + Clone + 'static,
{
    let num_threads = num_cpus().min(items.len());
    if num_threads <= 1 || items.is_empty() {
        for item in items {
            f(item);
        }
        return;
    }
    
    let chunk_size = (items.len() + num_threads - 1) / num_threads;
    let mut chunks: Vec<Vec<T>> = Vec::with_capacity(num_threads);
    let mut iter = items.into_iter();
    
    for _ in 0..num_threads {
        let chunk: Vec<T> = iter.by_ref().take(chunk_size).collect();
        if !chunk.is_empty() {
            chunks.push(chunk);
        }
    }
    
    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let f = f.clone();
            thread::spawn(move || {
                for item in chunk {
                    f(item);
                }
            })
        })
        .collect();
    
    for handle in handles {
        let _ = handle.join();
    }
}

/// Execute a function for each item in a slice in parallel
pub fn parallel_for_each_ref<T, F>(items: &[T], f: F)
where
    T: Sync,
    F: Fn(&T) + Send + Sync,
{
    let num_threads = num_cpus().min(items.len());
    if num_threads <= 1 || items.is_empty() {
        for item in items {
            f(item);
        }
        return;
    }
    
    thread::scope(|s| {
        let chunk_size = (items.len() + num_threads - 1) / num_threads;
        let f = &f;
        for chunk in items.chunks(chunk_size) {
            s.spawn(move || {
                for item in chunk {
                    f(item);
                }
            });
        }
    });
}

// ============================================================================
// Parallel Map
// ============================================================================

/// Map a function over items in parallel, collecting results
pub fn parallel_map<T, U, F>(items: Vec<T>, f: F) -> Vec<U>
where
    T: Send + 'static,
    U: Send + 'static,
    F: Fn(T) -> U + Send + Sync + Clone + 'static,
{
    let num_threads = num_cpus().min(items.len());
    if num_threads <= 1 || items.is_empty() {
        return items.into_iter().map(f).collect();
    }
    
    let len = items.len();
    let chunk_size = (len + num_threads - 1) / num_threads;
    let mut chunks: Vec<(usize, Vec<T>)> = Vec::with_capacity(num_threads);
    let mut iter = items.into_iter();
    let mut chunk_start = 0;
    
    for _ in 0..num_threads {
        let chunk: Vec<T> = iter.by_ref().take(chunk_size).collect();
        if !chunk.is_empty() {
            let chunk_len = chunk.len();
            chunks.push((chunk_start, chunk));
            chunk_start += chunk_len;
        }
    }
    
    let handles: Vec<_> = chunks
        .into_iter()
        .map(|(start_idx, chunk)| {
            let f = f.clone();
            thread::spawn(move || {
                let results: Vec<(usize, U)> = chunk
                    .into_iter()
                    .enumerate()
                    .map(|(i, item)| (start_idx + i, f(item)))
                    .collect();
                results
            })
        })
        .collect();
    
    let mut all_results: Vec<(usize, U)> = Vec::with_capacity(len);
    for handle in handles {
        if let Ok(results) = handle.join() {
            all_results.extend(results);
        }
    }
    
    all_results.sort_by_key(|(i, _)| *i);
    all_results.into_iter().map(|(_, u)| u).collect()
}

/// Map a function over items in a slice in parallel
pub fn parallel_map_ref<T, U, F>(items: &[T], f: F) -> Vec<U>
where
    T: Sync,
    U: Send,
    F: Fn(&T) -> U + Send + Sync,
{
    let num_threads = num_cpus().min(items.len());
    if num_threads <= 1 || items.is_empty() {
        return items.iter().map(|x| f(x)).collect();
    }
    
    let chunk_size = (items.len() + num_threads - 1) / num_threads;
    let results: Vec<Vec<U>> = thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(chunk_size)
            .map(|chunk| {
                s.spawn(|| {
                    chunk.iter().map(|item| f(item)).collect::<Vec<U>>()
                })
            })
            .collect();
        
        handles.into_iter()
            .map(|h| h.join().unwrap())
            .collect()
    });
    
    results.into_iter().flatten().collect()
}

// ============================================================================
// Parallel Reduce
// ============================================================================

/// Reduce items in parallel using an associative operation
pub fn parallel_reduce<T, F>(items: Vec<T>, identity: T, f: F) -> T
where
    T: Send + Clone + 'static,
    F: Fn(T, T) -> T + Send + Sync + Clone + 'static,
{
    let num_threads = num_cpus().min(items.len());
    if num_threads <= 1 || items.is_empty() {
        return items.into_iter().fold(identity, |acc, x| f(acc, x));
    }
    
    let chunk_size = (items.len() + num_threads - 1) / num_threads;
    let mut chunks: Vec<Vec<T>> = Vec::with_capacity(num_threads);
    let mut iter = items.into_iter();
    
    for _ in 0..num_threads {
        let chunk: Vec<T> = iter.by_ref().take(chunk_size).collect();
        if !chunk.is_empty() {
            chunks.push(chunk);
        }
    }
    
    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let f = f.clone();
            let id = identity.clone();
            thread::spawn(move || {
                chunk.into_iter().fold(id, |acc, x| f(acc, x))
            })
        })
        .collect();
    
    let mut partial_results = Vec::new();
    for handle in handles {
        if let Ok(result) = handle.join() {
            partial_results.push(result);
        }
    }
    
    partial_results.into_iter().fold(identity, |acc, x| f(acc, x))
}

// ============================================================================
// Scoped Threads (using std::thread::scope)
// ============================================================================

/// Run a closure with a scope for spawning scoped threads
///
/// All threads spawned within the scope are guaranteed to be joined
/// before this function returns.
pub fn scope<'env, F, R>(f: F) -> R
where
    F: for<'scope> FnOnce(&'scope thread::Scope<'scope, 'env>) -> R,
{
    thread::scope(f)
}

// ============================================================================
// Parallel Chunks
// ============================================================================

/// Process chunks of data in parallel, mutating in place
pub fn parallel_chunks_mut<T, F>(data: &mut [T], chunk_size: usize, f: F)
where
    T: Send,
    F: Fn(&mut [T]) + Send + Sync,
{
    let num_chunks = (data.len() + chunk_size - 1) / chunk_size;
    let num_threads = num_cpus().min(num_chunks);
    
    if num_threads <= 1 || data.is_empty() {
        for chunk in data.chunks_mut(chunk_size) {
            f(chunk);
        }
        return;
    }
    
    thread::scope(|s| {
        for chunk in data.chunks_mut(chunk_size) {
            s.spawn(|| {
                f(chunk);
            });
        }
    });
}

// ============================================================================
// Parallel Join
// ============================================================================

/// Execute two closures in parallel and return both results
pub fn join<A, B, FA, FB>(fa: FA, fb: FB) -> (A, B)
where
    A: Send,
    B: Send,
    FA: FnOnce() -> A + Send,
    FB: FnOnce() -> B + Send,
{
    thread::scope(|s| {
        let handle_b = s.spawn(fb);
        let a = fa();
        let b = handle_b.join().unwrap();
        (a, b)
    })
}

// ============================================================================
// Barrier
// ============================================================================

/// A reusable barrier for synchronizing threads
pub struct Barrier {
    count: AtomicUsize,
    total: usize,
    generation: AtomicUsize,
    mutex: Mutex<()>,
    condvar: Condvar,
}

impl Barrier {
    /// Create a new barrier for n threads
    pub fn new(n: usize) -> Self {
        Self {
            count: AtomicUsize::new(0),
            total: n,
            generation: AtomicUsize::new(0),
            mutex: Mutex::new(()),
            condvar: Condvar::new(),
        }
    }
    
    /// Wait at the barrier until all threads arrive
    pub fn wait(&self) {
        let current_gen = self.generation.load(Ordering::Relaxed);
        let count = self.count.fetch_add(1, Ordering::SeqCst) + 1;
        
        if count >= self.total {
            // Last thread to arrive
            self.count.store(0, Ordering::SeqCst);
            self.generation.fetch_add(1, Ordering::SeqCst);
            self.condvar.notify_all();
        } else {
            // Wait for last thread
            let mut guard = self.mutex.lock().unwrap();
            while self.generation.load(Ordering::SeqCst) == current_gen {
                guard = self.condvar.wait(guard).unwrap();
            }
        }
    }
}

// ============================================================================
// ParallelIterator Trait
// ============================================================================

/// Trait for parallel iteration
pub trait ParallelIterator: Sized {
    type Item;
    
    /// Execute a function for each item in parallel
    fn par_for_each<F>(self, f: F)
    where
        F: Fn(Self::Item) + Send + Sync;
    
    /// Map a function over items in parallel
    fn par_map<U, F>(self, f: F) -> Vec<U>
    where
        U: Send,
        F: Fn(Self::Item) -> U + Send + Sync;
    
    /// Filter items in parallel
    fn par_filter<P>(self, predicate: P) -> Vec<Self::Item>
    where
        P: Fn(&Self::Item) -> bool + Send + Sync,
        Self::Item: Send;
}

impl<'a, T: Sync> ParallelIterator for &'a [T] {
    type Item = &'a T;
    
    fn par_for_each<F>(self, f: F)
    where
        F: Fn(&'a T) + Send + Sync,
    {
        let num_threads = num_cpus().min(self.len());
        if num_threads <= 1 || self.is_empty() {
            for item in self {
                f(item);
            }
            return;
        }
        
        thread::scope(|s| {
            let chunk_size = (self.len() + num_threads - 1) / num_threads;
            let f = &f;
            for chunk in self.chunks(chunk_size) {
                s.spawn(move || {
                    for item in chunk {
                        f(item);
                    }
                });
            }
        });
    }
    
    fn par_map<U, F>(self, f: F) -> Vec<U>
    where
        U: Send,
        F: Fn(&'a T) -> U + Send + Sync,
    {
        let num_threads = num_cpus().min(self.len());
        if num_threads <= 1 || self.is_empty() {
            return self.iter().map(|x| f(x)).collect();
        }
        
        let chunk_size = (self.len() + num_threads - 1) / num_threads;
        let results: Vec<Vec<U>> = thread::scope(|s| {
            let handles: Vec<_> = self
                .chunks(chunk_size)
                .map(|chunk| {
                    s.spawn(|| {
                        chunk.iter().map(|item| f(item)).collect::<Vec<U>>()
                    })
                })
                .collect();
            
            handles.into_iter()
                .map(|h| h.join().unwrap())
                .collect()
        });
        
        results.into_iter().flatten().collect()
    }
    
    fn par_filter<P>(self, predicate: P) -> Vec<&'a T>
    where
        P: Fn(&&'a T) -> bool + Send + Sync,
    {
        let num_threads = num_cpus().min(self.len());
        if num_threads <= 1 || self.is_empty() {
            return self.iter().filter(|x| predicate(x)).collect();
        }
        
        let chunk_size = (self.len() + num_threads - 1) / num_threads;
        let results: Vec<Vec<&T>> = thread::scope(|s| {
            let handles: Vec<_> = self
                .chunks(chunk_size)
                .map(|chunk| {
                    s.spawn(|| {
                        chunk.iter().filter(|x| predicate(x)).collect::<Vec<&T>>()
                    })
                })
                .collect();
            
            handles.into_iter()
                .map(|h| h.join().unwrap())
                .collect()
        });
        
        results.into_iter().flatten().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI32;
    
    #[test]
    fn test_parallel_for_each() {
        let counter = Arc::new(AtomicI32::new(0));
        let items: Vec<i32> = (0..100).collect();
        
        let counter2 = Arc::clone(&counter);
        parallel_for_each(items, move |x| {
            counter2.fetch_add(x, Ordering::Relaxed);
        });
        
        assert_eq!(counter.load(Ordering::Relaxed), (0..100).sum::<i32>());
    }
    
    #[test]
    fn test_parallel_map() {
        let items: Vec<i32> = (0..100).collect();
        let results = parallel_map(items, |x| x * 2);
        
        let expected: Vec<i32> = (0..100).map(|x| x * 2).collect();
        assert_eq!(results, expected);
    }
    
    #[test]
    fn test_parallel_reduce() {
        let items: Vec<i32> = (1..=100).collect();
        let sum = parallel_reduce(items, 0, |a, b| a + b);
        
        assert_eq!(sum, (1..=100).sum::<i32>());
    }
    
    #[test]
    fn test_scope() {
        let data = Arc::new(Mutex::new(Vec::new()));
        
        scope(|s| {
            for i in 0..4 {
                let data = Arc::clone(&data);
                s.spawn(move || {
                    data.lock().unwrap().push(i);
                });
            }
        });
        
        let mut result = data.lock().unwrap().clone();
        result.sort();
        assert_eq!(result, vec![0, 1, 2, 3]);
    }
    
    #[test]
    fn test_join() {
        let (a, b) = join(
            || 1 + 2,
            || 3 + 4,
        );
        
        assert_eq!(a, 3);
        assert_eq!(b, 7);
    }
    
    #[test]
    fn test_barrier() {
        let barrier = Arc::new(Barrier::new(4));
        let counter = Arc::new(AtomicI32::new(0));
        
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                    barrier.wait();
                    // All threads should have incremented by now
                    assert_eq!(counter.load(Ordering::SeqCst), 4);
                })
            })
            .collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
    }
    
    #[test]
    fn test_parallel_iterator_trait() {
        let data = vec![1, 2, 3, 4, 5];
        let slice: &[i32] = &data;
        
        let doubled = slice.par_map(|x| x * 2);
        assert_eq!(doubled, vec![2, 4, 6, 8, 10]);
    }
    
    #[test]
    fn test_parallel_chunks_mut() {
        let mut data: Vec<i32> = (0..100).collect();
        
        parallel_chunks_mut(&mut data, 10, |chunk| {
            for x in chunk {
                *x *= 2;
            }
        });
        
        let expected: Vec<i32> = (0..100).map(|x| x * 2).collect();
        assert_eq!(data, expected);
    }
}
