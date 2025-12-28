//! Event Loop Implementation
//!
//! JavaScript event loop with microtask and macrotask queues.

use std::collections::VecDeque;

/// Task in the event loop
#[derive(Debug, Clone)]
pub struct Task {
    pub id: u32,
    pub callback_id: u32,
    pub args: Vec<u32>,
}

/// Timer task
#[derive(Debug, Clone)]
pub struct Timer {
    pub id: u32,
    pub callback_id: u32,
    pub delay_ms: u64,
    pub scheduled_at: u64,
    pub is_interval: bool,
}

/// JavaScript Event Loop
#[derive(Debug, Default)]
pub struct EventLoop {
    /// Microtask queue (Promise callbacks, queueMicrotask)
    microtasks: VecDeque<Task>,
    /// Macrotask queue (setTimeout, setInterval, I/O)
    macrotasks: VecDeque<Task>,
    /// Pending timers
    timers: Vec<Timer>,
    /// Next timer ID
    next_timer_id: u32,
    /// Current timestamp (ms)
    current_time: u64,
    /// Running flag
    running: bool,
}

impl EventLoop {
    pub fn new() -> Self { Self::default() }
    
    /// Queue a microtask (Promise.then, queueMicrotask)
    pub fn queue_microtask(&mut self, callback_id: u32) {
        self.microtasks.push_back(Task {
            id: 0,
            callback_id,
            args: Vec::new(),
        });
    }
    
    /// Queue a macrotask
    pub fn queue_macrotask(&mut self, callback_id: u32) {
        self.macrotasks.push_back(Task {
            id: 0,
            callback_id,
            args: Vec::new(),
        });
    }
    
    /// Set a timeout
    pub fn set_timeout(&mut self, callback_id: u32, delay_ms: u64) -> u32 {
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        self.timers.push(Timer {
            id,
            callback_id,
            delay_ms,
            scheduled_at: self.current_time,
            is_interval: false,
        });
        id
    }
    
    /// Set an interval
    pub fn set_interval(&mut self, callback_id: u32, delay_ms: u64) -> u32 {
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        self.timers.push(Timer {
            id,
            callback_id,
            delay_ms,
            scheduled_at: self.current_time,
            is_interval: true,
        });
        id
    }
    
    /// Clear a timeout/interval
    pub fn clear_timer(&mut self, id: u32) {
        self.timers.retain(|t| t.id != id);
    }
    
    /// Advance time and process due timers
    pub fn tick(&mut self, delta_ms: u64) {
        self.current_time += delta_ms;
        
        // Check for due timers
        let due_timers: Vec<Timer> = self.timers
            .iter()
            .filter(|t| self.current_time >= t.scheduled_at + t.delay_ms)
            .cloned()
            .collect();
        
        for timer in due_timers {
            // Queue the callback
            self.macrotasks.push_back(Task {
                id: timer.id,
                callback_id: timer.callback_id,
                args: Vec::new(),
            });
            
            if timer.is_interval {
                // Reschedule interval
                if let Some(t) = self.timers.iter_mut().find(|t| t.id == timer.id) {
                    t.scheduled_at = self.current_time;
                }
            } else {
                // Remove one-shot timer
                self.timers.retain(|t| t.id != timer.id);
            }
        }
    }
    
    /// Process all microtasks
    pub fn run_microtasks(&mut self) -> Vec<Task> {
        let mut executed = Vec::new();
        while let Some(task) = self.microtasks.pop_front() {
            executed.push(task);
        }
        executed
    }
    
    /// Get next macrotask (if any)
    pub fn next_macrotask(&mut self) -> Option<Task> {
        self.macrotasks.pop_front()
    }
    
    /// Check if there's pending work
    pub fn has_pending_work(&self) -> bool {
        !self.microtasks.is_empty() || !self.macrotasks.is_empty() || !self.timers.is_empty()
    }
    
    /// Get current time
    pub fn current_time(&self) -> u64 { self.current_time }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_microtask() {
        let mut loop_ = EventLoop::new();
        loop_.queue_microtask(1);
        loop_.queue_microtask(2);
        let tasks = loop_.run_microtasks();
        assert_eq!(tasks.len(), 2);
    }
    
    #[test]
    fn test_timeout() {
        let mut loop_ = EventLoop::new();
        let id = loop_.set_timeout(42, 100);
        assert!(!loop_.macrotasks.is_empty() || loop_.timers.len() == 1);
        
        loop_.tick(50);
        assert!(loop_.macrotasks.is_empty());
        
        loop_.tick(60);
        assert!(!loop_.macrotasks.is_empty());
    }
    
    #[test]
    fn test_interval() {
        let mut loop_ = EventLoop::new();
        loop_.set_interval(1, 10);
        
        loop_.tick(15);
        assert_eq!(loop_.next_macrotask().is_some(), true);
        
        loop_.tick(10);
        assert_eq!(loop_.next_macrotask().is_some(), true);
    }
}
