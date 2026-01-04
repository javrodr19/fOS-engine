//! Frame Scheduling
//!
//! Intelligent frame timing and budget management.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Frame phase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePhase {
    Input,
    Animation,
    BeginFrame,
    Layout,
    Paint,
    Composite,
    Idle,
}

/// Task priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Idle = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Scheduled task
#[derive(Debug)]
pub struct ScheduledTask {
    pub id: u64,
    pub priority: TaskPriority,
    pub deadline: Option<Instant>,
    pub phase: FramePhase,
}

/// Frame budget allocation
#[derive(Debug, Clone, Copy)]
pub struct FrameBudget {
    pub input_ms: f64,
    pub animation_ms: f64,
    pub layout_ms: f64,
    pub paint_ms: f64,
    pub composite_ms: f64,
    pub idle_ms: f64,
}

impl Default for FrameBudget {
    fn default() -> Self {
        Self::for_60fps()
    }
}

impl FrameBudget {
    /// Budget for 60 FPS (16.67ms per frame)
    pub fn for_60fps() -> Self {
        Self { input_ms: 2.0, animation_ms: 2.0, layout_ms: 4.0, paint_ms: 4.0, composite_ms: 2.0, idle_ms: 2.67 }
    }
    
    /// Budget for 30 FPS
    pub fn for_30fps() -> Self {
        Self { input_ms: 4.0, animation_ms: 4.0, layout_ms: 8.0, paint_ms: 8.0, composite_ms: 4.0, idle_ms: 5.33 }
    }
    
    /// Total budget
    pub fn total_ms(&self) -> f64 {
        self.input_ms + self.animation_ms + self.layout_ms + self.paint_ms + self.composite_ms + self.idle_ms
    }
    
    /// Get budget for phase
    pub fn for_phase(&self, phase: FramePhase) -> f64 {
        match phase {
            FramePhase::Input => self.input_ms,
            FramePhase::Animation => self.animation_ms,
            FramePhase::BeginFrame => 0.5,
            FramePhase::Layout => self.layout_ms,
            FramePhase::Paint => self.paint_ms,
            FramePhase::Composite => self.composite_ms,
            FramePhase::Idle => self.idle_ms,
        }
    }
}

/// Frame statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameStats {
    pub frame_count: u64,
    pub dropped_frames: u64,
    pub avg_frame_ms: f64,
    pub max_frame_ms: f64,
    pub budget_overruns: u64,
}

impl FrameStats {
    pub fn fps(&self) -> f64 {
        if self.avg_frame_ms > 0.0 { 1000.0 / self.avg_frame_ms } else { 0.0 }
    }
}

/// Frame scheduler
#[derive(Debug)]
pub struct FrameScheduler {
    budget: FrameBudget,
    current_phase: FramePhase,
    phase_start: Instant,
    frame_start: Instant,
    tasks: VecDeque<ScheduledTask>,
    idle_callbacks: VecDeque<IdleCallback>,
    next_task_id: u64,
    stats: FrameStats,
    frame_times: VecDeque<f64>,
}

/// Idle callback
#[derive(Debug)]
pub struct IdleCallback {
    pub id: u64,
    pub deadline: Duration,
}

/// Idle deadline info
#[derive(Debug, Clone, Copy)]
pub struct IdleDeadline {
    pub time_remaining_ms: f64,
    pub did_timeout: bool,
}

impl Default for FrameScheduler {
    fn default() -> Self { Self::new() }
}

impl FrameScheduler {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            budget: FrameBudget::default(),
            current_phase: FramePhase::Idle,
            phase_start: now,
            frame_start: now,
            tasks: VecDeque::new(),
            idle_callbacks: VecDeque::new(),
            next_task_id: 1,
            stats: FrameStats::default(),
            frame_times: VecDeque::with_capacity(60),
        }
    }
    
    pub fn set_budget(&mut self, budget: FrameBudget) { self.budget = budget; }
    
    /// Begin new frame
    pub fn begin_frame(&mut self) {
        let now = Instant::now();
        let frame_time = now.duration_since(self.frame_start).as_secs_f64() * 1000.0;
        
        // Update stats
        self.stats.frame_count += 1;
        self.frame_times.push_back(frame_time);
        if self.frame_times.len() > 60 { self.frame_times.pop_front(); }
        
        self.stats.avg_frame_ms = self.frame_times.iter().sum::<f64>() / self.frame_times.len() as f64;
        self.stats.max_frame_ms = self.stats.max_frame_ms.max(frame_time);
        
        if frame_time > self.budget.total_ms() * 1.5 {
            self.stats.dropped_frames += 1;
        }
        
        self.frame_start = now;
        self.phase_start = now;
        self.current_phase = FramePhase::BeginFrame;
    }
    
    /// Transition to phase
    pub fn enter_phase(&mut self, phase: FramePhase) {
        self.phase_start = Instant::now();
        self.current_phase = phase;
    }
    
    /// Check if current phase budget exhausted
    pub fn phase_budget_exhausted(&self) -> bool {
        let elapsed = self.phase_start.elapsed().as_secs_f64() * 1000.0;
        elapsed >= self.budget.for_phase(self.current_phase)
    }
    
    /// Time remaining in current phase
    pub fn phase_time_remaining_ms(&self) -> f64 {
        let budget = self.budget.for_phase(self.current_phase);
        let elapsed = self.phase_start.elapsed().as_secs_f64() * 1000.0;
        (budget - elapsed).max(0.0)
    }
    
    /// Frame time remaining
    pub fn frame_time_remaining_ms(&self) -> f64 {
        let budget = self.budget.total_ms();
        let elapsed = self.frame_start.elapsed().as_secs_f64() * 1000.0;
        (budget - elapsed).max(0.0)
    }
    
    /// Schedule task
    pub fn schedule(&mut self, priority: TaskPriority, phase: FramePhase) -> u64 {
        let id = self.next_task_id;
        self.next_task_id += 1;
        
        let task = ScheduledTask { id, priority, deadline: None, phase };
        
        // Insert by priority
        let pos = self.tasks.iter()
            .position(|t| t.priority < priority)
            .unwrap_or(self.tasks.len());
        self.tasks.insert(pos, task);
        
        id
    }
    
    /// Get next task for current phase
    pub fn next_task(&mut self) -> Option<ScheduledTask> {
        let phase = self.current_phase;
        if let Some(pos) = self.tasks.iter().position(|t| t.phase == phase) {
            self.tasks.remove(pos)
        } else {
            None
        }
    }
    
    /// Request idle callback
    pub fn request_idle_callback(&mut self, timeout: Duration) -> u64 {
        let id = self.next_task_id;
        self.next_task_id += 1;
        self.idle_callbacks.push_back(IdleCallback { id, deadline: timeout });
        id
    }
    
    /// Get idle deadline
    pub fn idle_deadline(&self) -> IdleDeadline {
        let remaining = self.frame_time_remaining_ms();
        IdleDeadline { time_remaining_ms: remaining.min(self.budget.idle_ms), did_timeout: remaining <= 0.0 }
    }
    
    /// Run idle callbacks
    pub fn run_idle_callbacks<F>(&mut self, mut callback: F)
    where F: FnMut(u64, IdleDeadline)
    {
        while let Some(cb) = self.idle_callbacks.pop_front() {
            let deadline = self.idle_deadline();
            if deadline.time_remaining_ms > 0.0 {
                callback(cb.id, deadline);
            } else {
                self.idle_callbacks.push_front(cb);
                break;
            }
        }
    }
    
    /// Get stats
    pub fn stats(&self) -> &FrameStats { &self.stats }
    
    /// Current phase
    pub fn current_phase(&self) -> FramePhase { self.current_phase }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_budget() {
        let budget = FrameBudget::for_60fps();
        assert!((budget.total_ms() - 16.67).abs() < 0.1);
    }
    
    #[test]
    fn test_scheduler() {
        let mut scheduler = FrameScheduler::new();
        scheduler.begin_frame();
        
        scheduler.schedule(TaskPriority::High, FramePhase::Layout);
        scheduler.schedule(TaskPriority::Normal, FramePhase::Layout);
        
        scheduler.enter_phase(FramePhase::Layout);
        
        let task = scheduler.next_task().unwrap();
        assert_eq!(task.priority, TaskPriority::High);
    }
    
    #[test]
    fn test_idle_deadline() {
        let scheduler = FrameScheduler::new();
        let deadline = scheduler.idle_deadline();
        assert!(deadline.time_remaining_ms >= 0.0);
    }
}
