//! History API
//!
//! Implements history.pushState, back, forward, go.

use rquickjs::{Ctx, Function, Object, Value};
use std::sync::{Arc, Mutex};

/// History entry
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub state: Option<String>, // JSON-serialized state
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
    
    /// Push a new history entry
    pub fn push_state(&mut self, state: Option<String>, title: String, url: String) {
        // Remove forward history
        self.entries.truncate(self.current + 1);
        
        self.entries.push(HistoryEntry { url, title, state });
        self.current = self.entries.len() - 1;
    }
    
    /// Replace current entry
    pub fn replace_state(&mut self, state: Option<String>, title: String, url: String) {
        if let Some(entry) = self.entries.get_mut(self.current) {
            entry.url = url;
            entry.title = title;
            entry.state = state;
        }
    }
    
    /// Go back
    pub fn back(&mut self) -> Option<&HistoryEntry> {
        if self.current > 0 {
            self.current -= 1;
            Some(&self.entries[self.current])
        } else {
            None
        }
    }
    
    /// Go forward
    pub fn forward(&mut self) -> Option<&HistoryEntry> {
        if self.current + 1 < self.entries.len() {
            self.current += 1;
            Some(&self.entries[self.current])
        } else {
            None
        }
    }
    
    /// Go to specific offset
    pub fn go(&mut self, delta: i32) -> Option<&HistoryEntry> {
        let new_index = (self.current as i32 + delta) as usize;
        if new_index < self.entries.len() {
            self.current = new_index;
            Some(&self.entries[self.current])
        } else {
            None
        }
    }
    
    /// Get current entry
    pub fn current(&self) -> &HistoryEntry {
        &self.entries[self.current]
    }
    
    /// Get history length
    pub fn length(&self) -> usize {
        self.entries.len()
    }
}

/// Install history API into global
pub fn install_history(ctx: &Ctx, history: Arc<Mutex<HistoryManager>>) -> Result<(), rquickjs::Error> {
    let globals = ctx.globals();
    let obj = Object::new(ctx.clone())?;
    
    // pushState
    let h = history.clone();
    obj.set("pushState", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        let state = args.get(0).and_then(|v| {
            if v.is_null() || v.is_undefined() { None }
            else { Some("{}".to_string()) } // Simplified
        });
        let title = args.get(1).and_then(|v| v.as_string()).map(|s| s.to_string().unwrap_or_default()).unwrap_or_default();
        let url = args.get(2).and_then(|v| v.as_string()).map(|s| s.to_string().unwrap_or_default()).unwrap_or_default();
        
        h.lock().unwrap().push_state(state, title, url);
        Ok(())
    })?)?;
    
    // replaceState
    let h = history.clone();
    obj.set("replaceState", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        let state = args.get(0).and_then(|v| {
            if v.is_null() || v.is_undefined() { None }
            else { Some("{}".to_string()) }
        });
        let title = args.get(1).and_then(|v| v.as_string()).map(|s| s.to_string().unwrap_or_default()).unwrap_or_default();
        let url = args.get(2).and_then(|v| v.as_string()).map(|s| s.to_string().unwrap_or_default()).unwrap_or_default();
        
        h.lock().unwrap().replace_state(state, title, url);
        Ok(())
    })?)?;
    
    // back
    let h = history.clone();
    obj.set("back", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        h.lock().unwrap().back();
        Ok(())
    })?)?;
    
    // forward
    let h = history.clone();
    obj.set("forward", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        h.lock().unwrap().forward();
        Ok(())
    })?)?;
    
    // go
    let h = history.clone();
    obj.set("go", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        let delta = args.first().and_then(|v| v.as_int()).unwrap_or(0);
        h.lock().unwrap().go(delta);
        Ok(())
    })?)?;
    
    // length
    let h = history;
    obj.set("getLength", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<i32, rquickjs::Error> {
        Ok(h.lock().unwrap().length() as i32)
    })?)?;
    
    globals.set("history", obj)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_history_push() {
        let mut history = HistoryManager::new("https://example.com");
        
        history.push_state(None, "Page 2".into(), "https://example.com/page2".into());
        assert_eq!(history.length(), 2);
        assert_eq!(history.current().url, "https://example.com/page2");
    }
    
    #[test]
    fn test_history_navigation() {
        let mut history = HistoryManager::new("https://example.com");
        history.push_state(None, "".into(), "/page1".into());
        history.push_state(None, "".into(), "/page2".into());
        
        history.back();
        assert_eq!(history.current().url, "/page1");
        
        history.back();
        assert_eq!(history.current().url, "https://example.com");
        
        history.forward();
        assert_eq!(history.current().url, "/page1");
    }
    
    #[test]
    fn test_history_replace() {
        let mut history = HistoryManager::new("https://example.com");
        history.replace_state(None, "New Title".into(), "https://example.com/new".into());
        
        assert_eq!(history.length(), 1);
        assert_eq!(history.current().url, "https://example.com/new");
    }
}
