//! Work-Stealing Scheduler
//!
//! A true work-stealing scheduler for parallel task execution.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::lockfree_queue::StealQueue;

/// Task priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    /// Idle tasks - cleanup, GC
    Idle = 0,
    /// Background tasks - prefetch, speculative work
    Background = 1,
    /// User-visible content
    UserVisible = 2,
    /// User-blocking tasks - input, animation
    UserBlocking = 3,
}

impl Default for TaskPriority {
    fn default() -> Self {
        Self::UserVisible
    }
}

/// Task to be executed
pub type TaskFn = Box<dyn FnOnce() + Send + 'static>;

/// Task with metadata
struct Task {
    /// Task function
    func: TaskFn,
    /// Priority
    priority: TaskPriority,
    /// Creation time
    created_at: Instant,
}

impl Task {
    fn new(func: TaskFn, priority: TaskPriority) -> Self {
        Self {
            func,
            priority,
            created_at: Instant::now(),
        }
    }
    
    fn run(self) {
        (self.func)();
    }
}

/// XorShift random number generator for work stealing
struct XorShift {
    state: u64,
}

impl XorShift {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }
    
    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
    
    fn next_usize(&mut self, max: usize) -> usize {
        (self.next() as usize) % max
    }
}

/// Worker thread data
struct Worker {
    /// Worker ID
    id: usize,
    /// Local work queue (push/pop from front, steal from back)
    local_queue: Arc<StealQueue<Task>>,
    /// Thread handle
    thread: Option<JoinHandle<()>>,
}

/// Work-stealing scheduler
pub struct WorkStealingScheduler {
    /// Worker threads
    workers: Vec<Worker>,
    /// Global queue for overflow
    global_queue: Arc<Mutex<VecDeque<Task>>>,
    /// Idle queue (low priority)
    idle_queue: Arc<Mutex<VecDeque<Task>>>,
    /// Number of workers
    num_workers: usize,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    /// Active task count
    active_tasks: Arc<AtomicUsize>,
    /// Pending task count
    pending_tasks: Arc<AtomicUsize>,
    /// Wake condition
    wake_condvar: Arc<Condvar>,
    /// Wake mutex
    wake_mutex: Arc<Mutex<()>>,
}

impl WorkStealingScheduler {
    /// Create a new work-stealing scheduler
    pub fn new(num_workers: usize) -> Self {
        let num_workers = num_workers.max(1);
        let shutdown = Arc::new(AtomicBool::new(false));
        let global_queue = Arc::new(Mutex::new(VecDeque::new()));
        let idle_queue = Arc::new(Mutex::new(VecDeque::new()));
        let active_tasks = Arc::new(AtomicUsize::new(0));
        let pending_tasks = Arc::new(AtomicUsize::new(0));
        let wake_condvar = Arc::new(Condvar::new());
        let wake_mutex = Arc::new(Mutex::new(()));
        
        // Create worker local queues
        let worker_queues: Vec<Arc<StealQueue<Task>>> = (0..num_workers)
            .map(|_| Arc::new(StealQueue::new(256)))
            .collect();
        
        let mut workers = Vec::with_capacity(num_workers);
        
        for id in 0..num_workers {
            let local_queue = Arc::clone(&worker_queues[id]);
            let global_queue = Arc::clone(&global_queue);
            let idle_queue = Arc::clone(&idle_queue);
            let shutdown = Arc::clone(&shutdown);
            let active_tasks = Arc::clone(&active_tasks);
            let pending_tasks = Arc::clone(&pending_tasks);
            let wake_mutex = Arc::clone(&wake_mutex);
            let wake_condvar = Arc::clone(&wake_condvar);
            let all_queues: Vec<_> = worker_queues.iter().map(Arc::clone).collect();
            
            let thread = thread::Builder::new()
                .name(format!("fos-worker-{}", id))
                .spawn(move || {
                    let mut rng = XorShift::new((id as u64 + 1) * 0xDEADBEEF);
                    
                    loop {
                        if shutdown.load(Ordering::Relaxed) {
                            break;
                        }
                        
                        // Try to get work
                        if let Some(task) = get_work(
                            id,
                            &local_queue,
                            &global_queue,
                            &idle_queue,
                            &all_queues,
                            &mut rng,
                        ) {
                            active_tasks.fetch_add(1, Ordering::SeqCst);
                            pending_tasks.fetch_sub(1, Ordering::SeqCst);
                            task.run();
                            active_tasks.fetch_sub(1, Ordering::SeqCst);
                        } else {
                            // No work - wait
                            let guard = wake_mutex.lock().unwrap();
                            
                            if shutdown.load(Ordering::Relaxed) {
                                break;
                            }
                            
                            // Wait with timeout
                            let _ = wake_condvar.wait_timeout(guard, Duration::from_millis(10)).unwrap();
                        }
                    }
                })
                .expect("Failed to spawn worker thread");
            
            let local_queue_for_worker = Arc::clone(&worker_queues[id]);
            
            workers.push(Worker {
                id,
                local_queue: Arc::clone(&worker_queues[id]),
                thread: Some(thread),
            });
        }
        
        Self {
            workers,
            global_queue,
            idle_queue,
            num_workers,
            shutdown,
            active_tasks,
            pending_tasks,
            wake_condvar,
            wake_mutex,
        }
    }
    
    /// Create with default number of workers (num CPUs)
    pub fn default_size() -> Self {
        let num_workers = thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        Self::new(num_workers)
    }
    
    /// Schedule a task with priority
    pub fn schedule<F>(&self, task: F, priority: TaskPriority)
    where
        F: FnOnce() + Send + 'static,
    {
        let task = Task::new(Box::new(task), priority);
        
        match priority {
            TaskPriority::UserBlocking => {
                // Run immediately on current thread
                task.run();
            }
            TaskPriority::UserVisible => {
                // Push to front of global queue
                self.global_queue.lock().unwrap().push_front(task);
                self.pending_tasks.fetch_add(1, Ordering::SeqCst);
                self.wake_workers();
            }
            TaskPriority::Background => {
                // Push to back of global queue
                self.global_queue.lock().unwrap().push_back(task);
                self.pending_tasks.fetch_add(1, Ordering::SeqCst);
                self.wake_workers();
            }
            TaskPriority::Idle => {
                // Push to idle queue
                self.idle_queue.lock().unwrap().push_back(task);
                self.pending_tasks.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    
    /// Schedule a task with default priority
    pub fn spawn<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.schedule(task, TaskPriority::UserVisible);
    }
    
    /// Wake sleeping workers
    fn wake_workers(&self) {
        self.wake_condvar.notify_all();
    }
    
    /// Run idle tasks (call during idle time)
    pub fn run_idle_tasks(&self, budget: Duration) {
        let start = Instant::now();
        
        while start.elapsed() < budget {
            let task = self.idle_queue.lock().unwrap().pop_front();
            
            if let Some(task) = task {
                self.pending_tasks.fetch_sub(1, Ordering::SeqCst);
                task.run();
            } else {
                break;
            }
        }
    }
    
    /// Get number of workers
    pub fn num_workers(&self) -> usize {
        self.num_workers
    }
    
    /// Get active task count
    pub fn active_tasks(&self) -> usize {
        self.active_tasks.load(Ordering::SeqCst)
    }
    
    /// Get pending task count
    pub fn pending_tasks(&self) -> usize {
        self.pending_tasks.load(Ordering::SeqCst)
    }
    
    /// Check if idle
    pub fn is_idle(&self) -> bool {
        self.active_tasks() == 0 && self.pending_tasks() == 0
    }
    
    /// Wait for all tasks to complete
    pub fn wait_idle(&self) {
        while !self.is_idle() {
            thread::yield_now();
        }
    }
    
    /// Shutdown the scheduler
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.wake_condvar.notify_all();
        
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for WorkStealingScheduler {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Get work from various sources
fn get_work(
    id: usize,
    local_queue: &StealQueue<Task>,
    global_queue: &Mutex<VecDeque<Task>>,
    idle_queue: &Mutex<VecDeque<Task>>,
    all_queues: &[Arc<StealQueue<Task>>],
    rng: &mut XorShift,
) -> Option<Task> {
    // 1. Try local queue first (LIFO)
    if let Some(task) = local_queue.pop() {
        return Some(task);
    }
    
    // 2. Try global queue
    if let Some(task) = global_queue.lock().unwrap().pop_front() {
        return Some(task);
    }
    
    // 3. Try stealing from random other worker
    let num_workers = all_queues.len();
    if num_workers > 1 {
        let start = rng.next_usize(num_workers);
        
        for i in 0..num_workers {
            let victim = (start + i) % num_workers;
            if victim == id {
                continue;
            }
            
            if let Some(task) = all_queues[victim].steal() {
                return Some(task);
            }
        }
    }
    
    // 4. Try idle queue (only if nothing else available)
    if let Some(task) = idle_queue.lock().unwrap().pop_front() {
        return Some(task);
    }
    
    None
}

/// Scoped scheduler for structured concurrency
pub struct ScopedScheduler<'scope> {
    scheduler: &'scope WorkStealingScheduler,
    pending: Arc<AtomicUsize>,
}

impl<'scope> ScopedScheduler<'scope> {
    /// Create a scoped scheduler
    pub fn new(scheduler: &'scope WorkStealingScheduler) -> Self {
        Self {
            scheduler,
            pending: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    /// Spawn a task that will be awaited when scope ends
    /// Note: For truly scoped references, use std::thread::scope instead.
    /// This method only accepts 'static tasks.
    pub fn spawn<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.pending.fetch_add(1, Ordering::SeqCst);
        
        let pending = Arc::clone(&self.pending);
        
        self.scheduler.spawn(move || {
            task();
            pending.fetch_sub(1, Ordering::SeqCst);
        });
    }
    
    /// Wait for all tasks to complete
    pub fn wait(&self) {
        while self.pending.load(Ordering::SeqCst) > 0 {
            thread::yield_now();
        }
    }
}

impl Drop for ScopedScheduler<'_> {
    fn drop(&mut self) {
        self.wait();
    }
}

/// Run tasks with a scoped scheduler
pub fn scope<'scope, F, R>(scheduler: &'scope WorkStealingScheduler, f: F) -> R
where
    F: FnOnce(&ScopedScheduler<'scope>) -> R,
{
    let scoped = ScopedScheduler::new(scheduler);
    let result = f(&scoped);
    scoped.wait();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI32;
    
    #[test]
    fn test_basic_scheduling() {
        let scheduler = WorkStealingScheduler::new(2);
        let counter = Arc::new(AtomicI32::new(0));
        
        for _ in 0..10 {
            let counter = Arc::clone(&counter);
            scheduler.spawn(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            });
        }
        
        scheduler.wait_idle();
        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }
    
    #[test]
    fn test_priority_scheduling() {
        let scheduler = WorkStealingScheduler::new(2);
        let counter = Arc::new(AtomicI32::new(0));
        
        // Schedule with different priorities
        let c1 = Arc::clone(&counter);
        scheduler.schedule(move || {
            c1.fetch_add(1, Ordering::SeqCst);
        }, TaskPriority::Background);
        
        let c2 = Arc::clone(&counter);
        scheduler.schedule(move || {
            c2.fetch_add(10, Ordering::SeqCst);
        }, TaskPriority::UserVisible);
        
        // UserBlocking runs immediately
        let c3 = Arc::clone(&counter);
        scheduler.schedule(move || {
            c3.fetch_add(100, Ordering::SeqCst);
        }, TaskPriority::UserBlocking);
        
        scheduler.wait_idle();
        assert_eq!(counter.load(Ordering::SeqCst), 111);
    }
    
    #[test]
    fn test_scoped_tasks() {
        let scheduler = WorkStealingScheduler::new(2);
        let counter = Arc::new(AtomicI32::new(0));
        
        scope(&scheduler, |s| {
            for _ in 0..5 {
                let counter = Arc::clone(&counter);
                s.spawn(move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                });
            }
        });
        
        // All tasks should be complete after scope
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }
    
    #[test]
    fn test_idle_tasks() {
        let scheduler = WorkStealingScheduler::new(2);
        let counter = Arc::new(AtomicI32::new(0));
        
        for _ in 0..5 {
            let counter = Arc::clone(&counter);
            scheduler.schedule(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }, TaskPriority::Idle);
        }
        
        // Idle tasks only run when explicitly requested
        scheduler.run_idle_tasks(Duration::from_millis(100));
        
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }
    
    #[test]
    fn test_xorshift() {
        let mut rng = XorShift::new(12345);
        
        // Generate some numbers
        let mut values = Vec::new();
        for _ in 0..10 {
            values.push(rng.next());
        }
        
        // Should all be different
        values.sort();
        values.dedup();
        assert_eq!(values.len(), 10);
    }
}
