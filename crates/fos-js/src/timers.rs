//! Timer APIs
//!
//! Implements setTimeout and setInterval.

use crate::{JsValue, JsError};
use crate::engine_trait::JsContextApi;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

static TIMER_ID: AtomicU32 = AtomicU32::new(1);

/// Timer entry
#[derive(Clone)]
pub struct Timer {
    pub id: u32,
    pub callback: String, // Store as code string for now
    pub delay_ms: u64,
    pub repeat: bool,
    pub scheduled_at: Instant,
}

/// Timer manager
#[derive(Default)]
pub struct TimerManager {
    timers: HashMap<u32, Timer>,
}

impl TimerManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add a timeout
    pub fn set_timeout(&mut self, callback: String, delay_ms: u64) -> u32 {
        let id = TIMER_ID.fetch_add(1, Ordering::SeqCst);
        self.timers.insert(id, Timer {
            id,
            callback,
            delay_ms,
            repeat: false,
            scheduled_at: Instant::now(),
        });
        id
    }
    
    /// Add an interval
    pub fn set_interval(&mut self, callback: String, delay_ms: u64) -> u32 {
        let id = TIMER_ID.fetch_add(1, Ordering::SeqCst);
        self.timers.insert(id, Timer {
            id,
            callback,
            delay_ms,
            repeat: true,
            scheduled_at: Instant::now(),
        });
        id
    }
    
    /// Clear a timer
    pub fn clear(&mut self, id: u32) {
        self.timers.remove(&id);
    }
    
    /// Get ready timers and remove non-repeating ones
    pub fn get_ready_timers(&mut self) -> Vec<Timer> {
        let now = Instant::now();
        let mut ready = Vec::new();
        let mut to_remove = Vec::new();
        
        for (id, timer) in &self.timers {
            let elapsed = now.duration_since(timer.scheduled_at);
            if elapsed >= Duration::from_millis(timer.delay_ms) {
                ready.push(timer.clone());
                if !timer.repeat {
                    to_remove.push(*id);
                }
            }
        }
        
        // Remove non-repeating timers
        for id in to_remove {
            self.timers.remove(&id);
        }
        
        // Reset scheduled_at for repeating timers
        for timer in &ready {
            if timer.repeat {
                if let Some(t) = self.timers.get_mut(&timer.id) {
                    t.scheduled_at = now;
                }
            }
        }
        
        ready
    }
    
    /// Check if there are pending timers
    pub fn has_pending(&self) -> bool {
        !self.timers.is_empty()
    }
    
    /// Get time until next timer fires
    pub fn time_until_next(&self) -> Option<Duration> {
        let now = Instant::now();
        
        self.timers.values()
            .map(|t| {
                let elapsed = now.duration_since(t.scheduled_at);
                let delay = Duration::from_millis(t.delay_ms);
                if elapsed >= delay {
                    Duration::ZERO
                } else {
                    delay - elapsed
                }
            })
            .min()
    }
}

/// Install timer APIs into global object
pub fn install_timers<C: JsContextApi>(ctx: &C, timer_manager: Arc<Mutex<TimerManager>>) -> Result<(), JsError> {
    // setTimeout
    let tm = timer_manager.clone();
    ctx.set_global_function("setTimeout", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Number(0.0));
        }
        
        let callback = args[0].as_string().unwrap_or("").to_string();
        let delay = if args.len() > 1 {
            args[1].as_number().unwrap_or(0.0) as u64
        } else {
            0
        };
        
        let id = tm.lock().unwrap().set_timeout(callback, delay);
        Ok(JsValue::Number(id as f64))
    })?;
    
    // clearTimeout
    let tm = timer_manager.clone();
    ctx.set_global_function("clearTimeout", move |args| {
        if let Some(id) = args.first().and_then(|v| v.as_number()) {
            tm.lock().unwrap().clear(id as u32);
        }
        Ok(JsValue::Undefined)
    })?;
    
    // setInterval
    let tm = timer_manager.clone();
    ctx.set_global_function("setInterval", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Number(0.0));
        }
        
        let callback = args[0].as_string().unwrap_or("").to_string();
        let delay = if args.len() > 1 {
            args[1].as_number().unwrap_or(1.0).max(1.0) as u64
        } else {
            1
        };
        
        let id = tm.lock().unwrap().set_interval(callback, delay);
        Ok(JsValue::Number(id as f64))
    })?;
    
    // clearInterval
    let tm = timer_manager;
    ctx.set_global_function("clearInterval", move |args| {
        if let Some(id) = args.first().and_then(|v| v.as_number()) {
            tm.lock().unwrap().clear(id as u32);
        }
        Ok(JsValue::Undefined)
    })?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_timer_manager() {
        let mut tm = TimerManager::new();
        
        let id1 = tm.set_timeout("cb1".to_string(), 100);
        let id2 = tm.set_timeout("cb2".to_string(), 200);
        
        assert!(tm.has_pending());
        
        tm.clear(id1);
        assert!(tm.has_pending()); // Still has id2
        
        tm.clear(id2);
        assert!(!tm.has_pending());
    }
    
    #[test]
    fn test_interval() {
        let mut tm = TimerManager::new();
        
        let id = tm.set_interval("repeat".to_string(), 50);
        assert!(tm.has_pending());
        
        tm.clear(id);
        assert!(!tm.has_pending());
    }
}
