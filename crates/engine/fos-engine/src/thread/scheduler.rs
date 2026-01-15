//! Task Scheduler
//!
//! Priority-based task scheduling.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Task ID counter
static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

fn next_task_id() -> u64 {
    NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst)
}

/// Task priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum TaskPriority {
    /// Idle tasks (low priority cleanup)
    Idle = 0,
    /// Background tasks (prefetch, GC)
    Background = 1,
    /// User-visible content
    UserVisible = 2,
    /// User-blocking tasks (input, animation)
    UserBlocking = 3,
}

impl TaskPriority {
    /// Get priority name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Background => "background",
            Self::UserVisible => "user-visible",
            Self::UserBlocking => "user-blocking",
        }
    }
}

/// Scheduled task
#[derive(Debug)]
pub struct Task {
    /// Unique task ID
    pub id: u64,
    /// Task priority
    pub priority: TaskPriority,
    /// Task name (for debugging)
    pub name: String,
    /// Creation time
    pub created: Instant,
    /// Deadline (optional)
    pub deadline: Option<Instant>,
}

impl Task {
    /// Create new task
    pub fn new(priority: TaskPriority, name: &str) -> Self {
        Self {
            id: next_task_id(),
            priority,
            name: name.to_string(),
            created: Instant::now(),
            deadline: None,
        }
    }
    
    /// Create with deadline
    pub fn with_deadline(priority: TaskPriority, name: &str, deadline: Duration) -> Self {
        Self {
            id: next_task_id(),
            priority,
            name: name.to_string(),
            created: Instant::now(),
            deadline: Some(Instant::now() + deadline),
        }
    }
    
    /// Check if past deadline
    pub fn is_overdue(&self) -> bool {
        self.deadline.map(|d| Instant::now() > d).unwrap_or(false)
    }
    
    /// Time until deadline
    pub fn time_until_deadline(&self) -> Option<Duration> {
        self.deadline.and_then(|d| {
            let now = Instant::now();
            if d > now {
                Some(d - now)
            } else {
                None
            }
        })
    }
    
    /// Age of task
    pub fn age(&self) -> Duration {
        self.created.elapsed()
    }
}

/// Task scheduler with priority queues
#[derive(Debug)]
pub struct Scheduler {
    /// User-blocking queue (highest priority)
    user_blocking: VecDeque<Task>,
    /// User-visible queue
    user_visible: VecDeque<Task>,
    /// Background queue
    background: VecDeque<Task>,
    /// Idle queue (lowest priority)
    idle: VecDeque<Task>,
    /// Total tasks scheduled
    total_scheduled: u64,
    /// Total tasks completed
    total_completed: u64,
}

impl Scheduler {
    /// Create new scheduler
    pub fn new() -> Self {
        Self {
            user_blocking: VecDeque::new(),
            user_visible: VecDeque::new(),
            background: VecDeque::new(),
            idle: VecDeque::new(),
            total_scheduled: 0,
            total_completed: 0,
        }
    }
    
    /// Schedule a task
    pub fn schedule(&mut self, task: Task) -> u64 {
        let id = task.id;
        self.total_scheduled += 1;
        
        match task.priority {
            TaskPriority::UserBlocking => self.user_blocking.push_back(task),
            TaskPriority::UserVisible => self.user_visible.push_back(task),
            TaskPriority::Background => self.background.push_back(task),
            TaskPriority::Idle => self.idle.push_back(task),
        }
        
        id
    }
    
    /// Schedule a new task with priority
    pub fn schedule_task(&mut self, priority: TaskPriority, name: &str) -> u64 {
        self.schedule(Task::new(priority, name))
    }
    
    /// Run immediately (for user-blocking tasks)
    pub fn run_immediately(&mut self, name: &str) -> Task {
        let task = Task::new(TaskPriority::UserBlocking, name);
        self.total_scheduled += 1;
        self.total_completed += 1;
        task
    }
    
    /// Get next task to run (respects priority)
    pub fn next(&mut self) -> Option<Task> {
        // Check queues in priority order
        if let Some(task) = self.user_blocking.pop_front() {
            self.total_completed += 1;
            return Some(task);
        }
        if let Some(task) = self.user_visible.pop_front() {
            self.total_completed += 1;
            return Some(task);
        }
        if let Some(task) = self.background.pop_front() {
            self.total_completed += 1;
            return Some(task);
        }
        if let Some(task) = self.idle.pop_front() {
            self.total_completed += 1;
            return Some(task);
        }
        None
    }
    
    /// Get next task at specific priority or higher
    pub fn next_at_priority(&mut self, min_priority: TaskPriority) -> Option<Task> {
        match min_priority {
            TaskPriority::UserBlocking => {
                self.user_blocking.pop_front().map(|t| {
                    self.total_completed += 1;
                    t
                })
            }
            TaskPriority::UserVisible => {
                self.user_blocking.pop_front()
                    .or_else(|| self.user_visible.pop_front())
                    .map(|t| {
                        self.total_completed += 1;
                        t
                    })
            }
            TaskPriority::Background => {
                self.user_blocking.pop_front()
                    .or_else(|| self.user_visible.pop_front())
                    .or_else(|| self.background.pop_front())
                    .map(|t| {
                        self.total_completed += 1;
                        t
                    })
            }
            TaskPriority::Idle => self.next(),
        }
    }
    
    /// Check if any tasks are pending
    pub fn has_pending(&self) -> bool {
        !self.user_blocking.is_empty()
            || !self.user_visible.is_empty()
            || !self.background.is_empty()
            || !self.idle.is_empty()
    }
    
    /// Get pending count at each priority
    pub fn pending_counts(&self) -> PendingCounts {
        PendingCounts {
            user_blocking: self.user_blocking.len(),
            user_visible: self.user_visible.len(),
            background: self.background.len(),
            idle: self.idle.len(),
        }
    }
    
    /// Total pending tasks
    pub fn total_pending(&self) -> usize {
        self.user_blocking.len()
            + self.user_visible.len()
            + self.background.len()
            + self.idle.len()
    }
    
    /// Get scheduler stats
    pub fn stats(&self) -> SchedulerStats {
        SchedulerStats {
            total_scheduled: self.total_scheduled,
            total_completed: self.total_completed,
            pending: self.pending_counts(),
        }
    }
    
    /// Clear all pending tasks
    pub fn clear(&mut self) {
        self.user_blocking.clear();
        self.user_visible.clear();
        self.background.clear();
        self.idle.clear();
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Pending task counts by priority
#[derive(Debug, Clone, Copy, Default)]
pub struct PendingCounts {
    pub user_blocking: usize,
    pub user_visible: usize,
    pub background: usize,
    pub idle: usize,
}

impl PendingCounts {
    pub fn total(&self) -> usize {
        self.user_blocking + self.user_visible + self.background + self.idle
    }
}

/// Scheduler statistics
#[derive(Debug, Clone, Copy)]
pub struct SchedulerStats {
    pub total_scheduled: u64,
    pub total_completed: u64,
    pub pending: PendingCounts,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_task_creation() {
        let task = Task::new(TaskPriority::UserBlocking, "test-task");
        
        assert!(!task.name.is_empty());
        assert_eq!(task.priority, TaskPriority::UserBlocking);
        assert!(task.deadline.is_none());
    }
    
    #[test]
    fn test_task_deadline() {
        let task = Task::with_deadline(
            TaskPriority::UserVisible,
            "deadline-task",
            Duration::from_secs(10),
        );
        
        assert!(task.deadline.is_some());
        assert!(!task.is_overdue());
        assert!(task.time_until_deadline().is_some());
    }
    
    #[test]
    fn test_scheduler_priority() {
        let mut scheduler = Scheduler::new();
        
        // Add tasks in reverse priority order
        scheduler.schedule(Task::new(TaskPriority::Idle, "idle"));
        scheduler.schedule(Task::new(TaskPriority::Background, "background"));
        scheduler.schedule(Task::new(TaskPriority::UserVisible, "visible"));
        scheduler.schedule(Task::new(TaskPriority::UserBlocking, "blocking"));
        
        // Should get them back in priority order
        assert_eq!(scheduler.next().unwrap().name, "blocking");
        assert_eq!(scheduler.next().unwrap().name, "visible");
        assert_eq!(scheduler.next().unwrap().name, "background");
        assert_eq!(scheduler.next().unwrap().name, "idle");
        assert!(scheduler.next().is_none());
    }
    
    #[test]
    fn test_scheduler_stats() {
        let mut scheduler = Scheduler::new();
        
        scheduler.schedule_task(TaskPriority::Background, "task1");
        scheduler.schedule_task(TaskPriority::Background, "task2");
        
        assert_eq!(scheduler.total_pending(), 2);
        
        let stats = scheduler.stats();
        assert_eq!(stats.total_scheduled, 2);
        assert_eq!(stats.total_completed, 0);
        
        scheduler.next();
        let stats = scheduler.stats();
        assert_eq!(stats.total_completed, 1);
    }
}
