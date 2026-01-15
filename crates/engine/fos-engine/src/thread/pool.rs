//! Thread Pool
//!
//! Custom work-stealing thread pool using std::thread.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Task function type
pub type TaskFn = Box<dyn FnOnce() + Send + 'static>;

/// Worker queue
struct WorkerQueue {
    /// Task queue
    tasks: Mutex<VecDeque<TaskFn>>,
    /// Condition variable for waiting
    condvar: Condvar,
    /// Shutdown flag
    shutdown: AtomicBool,
}

impl WorkerQueue {
    fn new() -> Self {
        Self {
            tasks: Mutex::new(VecDeque::new()),
            condvar: Condvar::new(),
            shutdown: AtomicBool::new(false),
        }
    }
    
    fn push(&self, task: TaskFn) {
        let mut queue = self.tasks.lock().unwrap();
        queue.push_back(task);
        self.condvar.notify_one();
    }
    
    fn pop(&self) -> Option<TaskFn> {
        let mut queue = self.tasks.lock().unwrap();
        queue.pop_front()
    }
    
    fn wait_for_task(&self) -> Option<TaskFn> {
        let mut queue = self.tasks.lock().unwrap();
        
        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                return None;
            }
            
            if let Some(task) = queue.pop_front() {
                return Some(task);
            }
            
            // Wait with timeout to check shutdown
            let result = self.condvar.wait_timeout(queue, Duration::from_millis(100)).unwrap();
            queue = result.0;
        }
    }
    
    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.condvar.notify_all();
    }
    
    fn len(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }
}

/// Thread pool with work stealing
pub struct ThreadPool {
    /// Worker threads
    workers: Vec<Worker>,
    /// Shared work queue
    queue: Arc<WorkerQueue>,
    /// Number of workers
    worker_count: usize,
    /// Active task count
    active_tasks: Arc<AtomicUsize>,
}

impl std::fmt::Debug for ThreadPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadPool")
            .field("workers", &self.workers)
            .field("worker_count", &self.worker_count)
            .field("active_tasks", &self.active_tasks.load(Ordering::Relaxed))
            .finish()
    }
}

struct Worker {
    id: usize,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker")
            .field("id", &self.id)
            .field("running", &self.thread.is_some())
            .finish()
    }
}

impl Worker {
    fn new(id: usize, queue: Arc<WorkerQueue>, active_tasks: Arc<AtomicUsize>) -> Self {
        let thread = thread::spawn(move || {
            loop {
                if let Some(task) = queue.wait_for_task() {
                    active_tasks.fetch_add(1, Ordering::SeqCst);
                    task();
                    active_tasks.fetch_sub(1, Ordering::SeqCst);
                } else {
                    // Shutdown signal received
                    break;
                }
            }
        });
        
        Self {
            id,
            thread: Some(thread),
        }
    }
}

impl ThreadPool {
    /// Create new thread pool with specified worker count
    pub fn new(worker_count: usize) -> Self {
        let queue = Arc::new(WorkerQueue::new());
        let active_tasks = Arc::new(AtomicUsize::new(0));
        
        let workers: Vec<_> = (0..worker_count)
            .map(|id| Worker::new(id, Arc::clone(&queue), Arc::clone(&active_tasks)))
            .collect();
        
        Self {
            workers,
            queue,
            worker_count,
            active_tasks,
        }
    }
    
    /// Create with default worker count (num CPUs)
    pub fn default_size() -> Self {
        let count = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        Self::new(count)
    }
    
    /// Submit a task
    pub fn submit<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.queue.push(Box::new(task));
    }
    
    /// Get worker count
    pub fn worker_count(&self) -> usize {
        self.worker_count
    }
    
    /// Get pending task count
    pub fn pending_tasks(&self) -> usize {
        self.queue.len()
    }
    
    /// Get active task count
    pub fn active_tasks(&self) -> usize {
        self.active_tasks.load(Ordering::SeqCst)
    }
    
    /// Check if pool is idle
    pub fn is_idle(&self) -> bool {
        self.pending_tasks() == 0 && self.active_tasks() == 0
    }
    
    /// Shutdown the pool
    pub fn shutdown(&mut self) {
        self.queue.shutdown();
        
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// I/O thread for blocking operations
pub struct IoThread {
    /// Task queue
    queue: Arc<WorkerQueue>,
    /// Thread handle
    thread: Option<JoinHandle<()>>,
    /// Name
    name: String,
}

impl std::fmt::Debug for IoThread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IoThread")
            .field("name", &self.name)
            .field("running", &self.thread.is_some())
            .finish()
    }
}

impl IoThread {
    /// Create new I/O thread
    pub fn new(name: &str) -> Self {
        let queue = Arc::new(WorkerQueue::new());
        let queue_clone = Arc::clone(&queue);
        let thread_name = name.to_string();
        
        let thread = thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || {
                loop {
                    if let Some(task) = queue_clone.wait_for_task() {
                        task();
                    } else {
                        break;
                    }
                }
            })
            .expect("Failed to spawn I/O thread");
        
        Self {
            queue,
            thread: Some(thread),
            name: name.to_string(),
        }
    }
    
    /// Submit I/O task
    pub fn submit<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.queue.push(Box::new(task));
    }
    
    /// Get thread name
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Get pending task count
    pub fn pending_tasks(&self) -> usize {
        self.queue.len()
    }
    
    /// Shutdown
    pub fn shutdown(&mut self) {
        self.queue.shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for IoThread {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Compositor thread for timing-critical rendering
pub struct CompositorThread {
    /// Task queue
    queue: Arc<WorkerQueue>,
    /// Thread handle
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for CompositorThread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositorThread")
            .field("running", &self.thread.is_some())
            .finish()
    }
}

impl CompositorThread {
    /// Create new compositor thread
    pub fn new() -> Self {
        let queue = Arc::new(WorkerQueue::new());
        let queue_clone = Arc::clone(&queue);
        
        let thread = thread::Builder::new()
            .name("fos-compositor".to_string())
            .spawn(move || {
                loop {
                    if let Some(task) = queue_clone.wait_for_task() {
                        task();
                    } else {
                        break;
                    }
                }
            })
            .expect("Failed to spawn compositor thread");
        
        Self {
            queue,
            thread: Some(thread),
        }
    }
    
    /// Submit compositing task (high priority)
    pub fn submit<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.queue.push(Box::new(task));
    }
    
    /// Get pending task count
    pub fn pending_tasks(&self) -> usize {
        self.queue.len()
    }
    
    /// Shutdown
    pub fn shutdown(&mut self) {
        self.queue.shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Default for CompositorThread {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CompositorThread {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Audio thread for real-time audio processing
pub struct AudioThread {
    /// Task queue
    queue: Arc<WorkerQueue>,
    /// Thread handle
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for AudioThread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioThread")
            .field("running", &self.thread.is_some())
            .finish()
    }
}

impl AudioThread {
    /// Create new audio thread
    pub fn new() -> Self {
        let queue = Arc::new(WorkerQueue::new());
        let queue_clone = Arc::clone(&queue);
        
        let thread = thread::Builder::new()
            .name("fos-audio".to_string())
            .spawn(move || {
                loop {
                    if let Some(task) = queue_clone.wait_for_task() {
                        task();
                    } else {
                        break;
                    }
                }
            })
            .expect("Failed to spawn audio thread");
        
        Self {
            queue,
            thread: Some(thread),
        }
    }
    
    /// Submit audio task (real-time priority)
    pub fn submit<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.queue.push(Box::new(task));
    }
    
    /// Shutdown
    pub fn shutdown(&mut self) {
        self.queue.shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Default for AudioThread {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AudioThread {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Combined thread pool architecture
#[derive(Debug)]
pub struct ThreadPoolArchitecture {
    /// CPU worker pool
    pub cpu_workers: ThreadPool,
    /// I/O threads
    pub io_threads: Vec<IoThread>,
    /// Compositor thread
    pub compositor: CompositorThread,
    /// Audio thread
    pub audio: AudioThread,
}

impl ThreadPoolArchitecture {
    /// Create with default configuration
    pub fn new() -> Self {
        let cpu_count = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        
        // I/O threads: 2 by default
        let io_threads = vec![
            IoThread::new("fos-io-0"),
            IoThread::new("fos-io-1"),
        ];
        
        Self {
            cpu_workers: ThreadPool::new(cpu_count),
            io_threads,
            compositor: CompositorThread::new(),
            audio: AudioThread::new(),
        }
    }
    
    /// Create with custom CPU worker count
    pub fn with_cpu_workers(cpu_workers: usize) -> Self {
        let io_threads = vec![
            IoThread::new("fos-io-0"),
            IoThread::new("fos-io-1"),
        ];
        
        Self {
            cpu_workers: ThreadPool::new(cpu_workers),
            io_threads,
            compositor: CompositorThread::new(),
            audio: AudioThread::new(),
        }
    }
}

impl Default for ThreadPoolArchitecture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    
    #[test]
    fn test_thread_pool() {
        let pool = ThreadPool::new(2);
        let counter = Arc::new(AtomicU32::new(0));
        
        for _ in 0..10 {
            let counter = Arc::clone(&counter);
            pool.submit(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            });
        }
        
        // Wait for tasks to complete
        thread::sleep(Duration::from_millis(100));
        
        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }
    
    #[test]
    fn test_io_thread() {
        let mut io = IoThread::new("test-io");
        let counter = Arc::new(AtomicU32::new(0));
        
        let counter_clone = Arc::clone(&counter);
        io.submit(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });
        
        thread::sleep(Duration::from_millis(50));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        
        io.shutdown();
    }
    
    #[test]
    fn test_pool_architecture() {
        let arch = ThreadPoolArchitecture::new();
        
        assert!(arch.cpu_workers.worker_count() > 0);
        assert!(!arch.io_threads.is_empty());
    }
}
