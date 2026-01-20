//! Energy-Aware Task Scheduler
//!
//! Schedules tasks based on urgency and available cores.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Task urgency levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Urgency {
    /// Execute eventually, use efficient cores
    Eventually = 0,
    /// Execute soon, use any core
    Soon = 1,
    /// Execute immediately, use performance cores
    Immediate = 2,
}

impl Urgency {
    /// Get maximum latency allowed for this urgency
    pub fn max_latency(&self) -> Duration {
        match self {
            Self::Eventually => Duration::from_secs(10),
            Self::Soon => Duration::from_millis(100),
            Self::Immediate => Duration::from_millis(16),
        }
    }
}

/// Core type for scheduling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreType {
    /// High-performance cores (P-cores, big cores)
    Performance,
    /// Any available core
    Any,
    /// Energy-efficient cores (E-cores, LITTLE cores)
    Efficiency,
}

/// Scheduled task
#[derive(Debug)]
pub struct ScheduledTask {
    /// Task identifier
    pub id: u64,
    /// Task urgency
    pub urgency: Urgency,
    /// Preferred core type
    pub core_type: CoreType,
    /// Task creation time
    pub created: Instant,
    /// Task name (for debugging)
    pub name: String,
}

/// Energy-aware task scheduler
#[derive(Debug)]
pub struct EnergyAwareScheduler {
    /// Performance core queue
    performance_queue: VecDeque<ScheduledTask>,
    /// Any core queue
    any_queue: VecDeque<ScheduledTask>,
    /// Efficiency core queue
    efficiency_queue: VecDeque<ScheduledTask>,
    /// Next task ID
    next_id: u64,
    /// Tasks scheduled
    tasks_scheduled: u64,
    /// Tasks completed
    tasks_completed: u64,
    /// Power mode (affects scheduling)
    power_saving: bool,
}

impl Default for EnergyAwareScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl EnergyAwareScheduler {
    /// Create a new scheduler
    pub fn new() -> Self {
        Self {
            performance_queue: VecDeque::new(),
            any_queue: VecDeque::new(),
            efficiency_queue: VecDeque::new(),
            next_id: 1,
            tasks_scheduled: 0,
            tasks_completed: 0,
            power_saving: false,
        }
    }
    
    /// Set power saving mode
    pub fn set_power_saving(&mut self, enabled: bool) {
        self.power_saving = enabled;
    }
    
    /// Schedule a task
    pub fn schedule(&mut self, name: &str, urgency: Urgency) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let core_type = if self.power_saving {
            // In power saving, use efficiency cores unless urgent
            match urgency {
                Urgency::Immediate => CoreType::Any,
                Urgency::Soon => CoreType::Efficiency,
                Urgency::Eventually => CoreType::Efficiency,
            }
        } else {
            match urgency {
                Urgency::Immediate => CoreType::Performance,
                Urgency::Soon => CoreType::Any,
                Urgency::Eventually => CoreType::Efficiency,
            }
        };
        
        let task = ScheduledTask {
            id,
            urgency,
            core_type,
            created: Instant::now(),
            name: name.to_string(),
        };
        
        match core_type {
            CoreType::Performance => self.performance_queue.push_back(task),
            CoreType::Any => self.any_queue.push_back(task),
            CoreType::Efficiency => self.efficiency_queue.push_back(task),
        }
        
        self.tasks_scheduled += 1;
        id
    }
    
    /// Schedule for specific core type
    pub fn schedule_on(&mut self, name: &str, urgency: Urgency, core_type: CoreType) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let task = ScheduledTask {
            id,
            urgency,
            core_type,
            created: Instant::now(),
            name: name.to_string(),
        };
        
        match core_type {
            CoreType::Performance => self.performance_queue.push_back(task),
            CoreType::Any => self.any_queue.push_back(task),
            CoreType::Efficiency => self.efficiency_queue.push_back(task),
        }
        
        self.tasks_scheduled += 1;
        id
    }
    
    /// Get next task for performance cores
    pub fn next_performance(&mut self) -> Option<ScheduledTask> {
        // Performance cores take from performance queue first, then any
        self.performance_queue.pop_front()
            .or_else(|| self.any_queue.pop_front())
    }
    
    /// Get next task for efficiency cores
    pub fn next_efficiency(&mut self) -> Option<ScheduledTask> {
        // Efficiency cores take from efficiency queue first
        self.efficiency_queue.pop_front()
            .or_else(|| {
                // Only take from any_queue if old enough
                if let Some(task) = self.any_queue.front() {
                    if task.created.elapsed() > task.urgency.max_latency() {
                        return self.any_queue.pop_front();
                    }
                }
                None
            })
    }
    
    /// Get next task for any core
    pub fn next_any(&mut self) -> Option<ScheduledTask> {
        // Priority: immediate > soon > eventually
        self.performance_queue.pop_front()
            .or_else(|| self.any_queue.pop_front())
            .or_else(|| self.efficiency_queue.pop_front())
    }
    
    /// Mark task as complete
    pub fn complete(&mut self, _id: u64) {
        self.tasks_completed += 1;
    }
    
    /// Get queue lengths
    pub fn queue_lengths(&self) -> (usize, usize, usize) {
        (
            self.performance_queue.len(),
            self.any_queue.len(),
            self.efficiency_queue.len(),
        )
    }
    
    /// Get total pending tasks
    pub fn pending_count(&self) -> usize {
        self.performance_queue.len() + self.any_queue.len() + self.efficiency_queue.len()
    }
    
    /// Check if there are urgent tasks
    pub fn has_urgent(&self) -> bool {
        !self.performance_queue.is_empty()
    }
    
    /// Get statistics
    pub fn stats(&self) -> SchedulerStats {
        let (perf, any, eff) = self.queue_lengths();
        SchedulerStats {
            pending_performance: perf,
            pending_any: any,
            pending_efficiency: eff,
            total_scheduled: self.tasks_scheduled,
            total_completed: self.tasks_completed,
            power_saving: self.power_saving,
        }
    }
    
    /// Clear all queues
    pub fn clear(&mut self) {
        self.performance_queue.clear();
        self.any_queue.clear();
        self.efficiency_queue.clear();
    }
}

/// Scheduler statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct SchedulerStats {
    pub pending_performance: usize,
    pub pending_any: usize,
    pub pending_efficiency: usize,
    pub total_scheduled: u64,
    pub total_completed: u64,
    pub power_saving: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_urgency_order() {
        assert!(Urgency::Eventually < Urgency::Soon);
        assert!(Urgency::Soon < Urgency::Immediate);
    }
    
    #[test]
    fn test_schedule_immediate() {
        let mut scheduler = EnergyAwareScheduler::new();
        scheduler.schedule("test", Urgency::Immediate);
        
        let (perf, any, _) = scheduler.queue_lengths();
        assert_eq!(perf, 1);
        assert_eq!(any, 0);
    }
    
    #[test]
    fn test_schedule_eventually() {
        let mut scheduler = EnergyAwareScheduler::new();
        scheduler.schedule("test", Urgency::Eventually);
        
        let (_, _, eff) = scheduler.queue_lengths();
        assert_eq!(eff, 1);
    }
    
    #[test]
    fn test_power_saving_mode() {
        let mut scheduler = EnergyAwareScheduler::new();
        scheduler.set_power_saving(true);
        scheduler.schedule("test", Urgency::Immediate);
        
        // In power saving, Immediate goes to Any queue instead of Performance
        let (perf, any, _) = scheduler.queue_lengths();
        assert_eq!(perf, 0);
        assert_eq!(any, 1);
    }
    
    #[test]
    fn test_next_performance() {
        let mut scheduler = EnergyAwareScheduler::new();
        scheduler.schedule("perf", Urgency::Immediate);
        scheduler.schedule("any", Urgency::Soon);
        
        // Performance core should get performance task first
        let task = scheduler.next_performance().unwrap();
        assert_eq!(task.name, "perf");
        
        // Then any task
        let task = scheduler.next_performance().unwrap();
        assert_eq!(task.name, "any");
    }
}
