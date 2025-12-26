//! History API
//!
//! Implements window.history object.

use crate::{JsValue, JsError};
use crate::engine_trait::JsContextApi;
use std::sync::{Arc, Mutex};

/// History entry
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub state: Option<String>,
}

/// History manager
pub struct HistoryManager {
    entries: Vec<HistoryEntry>,
    current: usize,
}

impl HistoryManager {
    pub fn new(initial_url: &str) -> Self {
        Self {
            entries: vec![HistoryEntry {
                url: initial_url.to_string(),
                title: String::new(),
                state: None,
            }],
            current: 0,
        }
    }
    
    /// Get current entry
    pub fn current(&self) -> &HistoryEntry {
        &self.entries[self.current]
    }
    
    /// Push a new state
    pub fn push_state(&mut self, state: Option<String>, title: String, url: String) {
        // Remove forward history
        self.entries.truncate(self.current + 1);
        
        self.entries.push(HistoryEntry { url, title, state });
        self.current = self.entries.len() - 1;
    }
    
    /// Replace current state
    pub fn replace_state(&mut self, state: Option<String>, title: String, url: String) {
        self.entries[self.current] = HistoryEntry { url, title, state };
    }
    
    /// Go back
    pub fn back(&mut self) {
        if self.current > 0 {
            self.current -= 1;
        }
    }
    
    /// Go forward
    pub fn forward(&mut self) {
        if self.current < self.entries.len() - 1 {
            self.current += 1;
        }
    }
    
    /// Go by delta
    pub fn go(&mut self, delta: i32) {
        let new_index = (self.current as i32 + delta) as usize;
        if new_index < self.entries.len() {
            self.current = new_index;
        }
    }
    
    /// History length
    pub fn length(&self) -> usize {
        self.entries.len()
    }
}

/// Install history API into global
pub fn install_history<C: JsContextApi>(ctx: &C, history: Arc<Mutex<HistoryManager>>) -> Result<(), JsError> {
    let obj = ctx.create_object()?;
    
    // pushState
    let h = history.clone();
    ctx.set_function(&obj, "pushState", move |args| {
        let state = args.first().and_then(|v| v.as_string()).map(|s| s.to_string());
        let title = args.get(1).and_then(|v| v.as_string()).unwrap_or("").to_string();
        let url = args.get(2).and_then(|v| v.as_string()).unwrap_or("").to_string();
        
        h.lock().unwrap().push_state(state, title, url);
        Ok(JsValue::Undefined)
    })?;
    
    // replaceState
    let h = history.clone();
    ctx.set_function(&obj, "replaceState", move |args| {
        let state = args.first().and_then(|v| v.as_string()).map(|s| s.to_string());
        let title = args.get(1).and_then(|v| v.as_string()).unwrap_or("").to_string();
        let url = args.get(2).and_then(|v| v.as_string()).unwrap_or("").to_string();
        
        h.lock().unwrap().replace_state(state, title, url);
        Ok(JsValue::Undefined)
    })?;
    
    // back
    let h = history.clone();
    ctx.set_function(&obj, "back", move |_args| {
        h.lock().unwrap().back();
        Ok(JsValue::Undefined)
    })?;
    
    // forward
    let h = history.clone();
    ctx.set_function(&obj, "forward", move |_args| {
        h.lock().unwrap().forward();
        Ok(JsValue::Undefined)
    })?;
    
    // go
    let h = history.clone();
    ctx.set_function(&obj, "go", move |args| {
        let delta = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as i32;
        h.lock().unwrap().go(delta);
        Ok(JsValue::Undefined)
    })?;
    
    // getLength
    let h = history;
    ctx.set_function(&obj, "getLength", move |_args| {
        let len = h.lock().unwrap().length();
        Ok(JsValue::Number(len as f64))
    })?;
    
    ctx.set_global("history", JsValue::Object)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_history_navigation() {
        let mut history = HistoryManager::new("https://example.com/");
        
        history.push_state(None, "".into(), "/page1".into());
        history.push_state(None, "".into(), "/page2".into());
        
        assert_eq!(history.length(), 3);
        assert_eq!(history.current().url, "/page2");
        
        history.back();
        assert_eq!(history.current().url, "/page1");
        
        history.forward();
        assert_eq!(history.current().url, "/page2");
    }
    
    #[test]
    fn test_history_replace() {
        let mut history = HistoryManager::new("https://example.com/old");
        
        history.replace_state(None, "".into(), "https://example.com/new".into());
        assert_eq!(history.length(), 1);
        assert_eq!(history.current().url, "https://example.com/new");
    }
}
