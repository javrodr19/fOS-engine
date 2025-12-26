//! Web Workers
//!
//! Background script execution in separate contexts.

use crate::{JsValue, JsError};
use crate::engine_trait::JsContextApi;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// Worker message
#[derive(Debug, Clone)]
pub struct WorkerMessage {
    pub data: String, // JSON-serialized data
}

/// Worker state
#[derive(Debug)]
pub struct Worker {
    /// Worker ID
    id: u32,
    /// Script to execute
    script: String,
    /// Incoming messages (from main thread)
    inbox: VecDeque<WorkerMessage>,
    /// Outgoing messages (to main thread)
    outbox: VecDeque<WorkerMessage>,
    /// Whether worker is terminated
    terminated: bool,
}

impl Worker {
    /// Create a new worker
    pub fn new(id: u32, script: String) -> Self {
        Self {
            id,
            script,
            inbox: VecDeque::new(),
            outbox: VecDeque::new(),
            terminated: false,
        }
    }
    
    /// Post a message to the worker
    pub fn post_message(&mut self, data: String) {
        if !self.terminated {
            self.inbox.push_back(WorkerMessage { data });
        }
    }
    
    /// Get a message from the worker
    pub fn get_message(&mut self) -> Option<WorkerMessage> {
        self.outbox.pop_front()
    }
    
    /// Send a message from worker to main thread
    pub fn send_message(&mut self, data: String) {
        self.outbox.push_back(WorkerMessage { data });
    }
    
    /// Get pending messages for the worker
    pub fn receive_message(&mut self) -> Option<WorkerMessage> {
        self.inbox.pop_front()
    }
    
    /// Terminate the worker
    pub fn terminate(&mut self) {
        self.terminated = true;
        self.inbox.clear();
    }
    
    /// Check if terminated
    pub fn is_terminated(&self) -> bool {
        self.terminated
    }
    
    /// Get worker ID
    pub fn id(&self) -> u32 {
        self.id
    }
    
    /// Get script
    pub fn script(&self) -> &str {
        &self.script
    }
}

/// Worker manager
#[derive(Default)]
pub struct WorkerManager {
    workers: Vec<Worker>,
    next_id: u32,
}

impl WorkerManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create a new worker
    pub fn create_worker(&mut self, script: String) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        
        self.workers.push(Worker::new(id, script));
        id
    }
    
    /// Get a worker by ID
    pub fn get_worker(&mut self, id: u32) -> Option<&mut Worker> {
        self.workers.iter_mut().find(|w| w.id() == id)
    }
    
    /// Terminate a worker
    pub fn terminate_worker(&mut self, id: u32) {
        if let Some(worker) = self.get_worker(id) {
            worker.terminate();
        }
    }
    
    /// Remove terminated workers
    pub fn cleanup(&mut self) {
        self.workers.retain(|w| !w.is_terminated());
    }
    
    /// Post message to a worker
    pub fn post_message(&mut self, worker_id: u32, data: String) {
        if let Some(worker) = self.get_worker(worker_id) {
            worker.post_message(data);
        }
    }
    
    /// Get messages from all workers
    pub fn get_messages(&mut self) -> Vec<(u32, WorkerMessage)> {
        let mut messages = Vec::new();
        for worker in &mut self.workers {
            while let Some(msg) = worker.get_message() {
                messages.push((worker.id(), msg));
            }
        }
        messages
    }
    
    /// Get active worker count
    pub fn active_count(&self) -> usize {
        self.workers.iter().filter(|w| !w.is_terminated()).count()
    }
}

/// Install Worker API into global
pub fn install_worker_api<C: JsContextApi>(
    ctx: &C,
    manager: Arc<Mutex<WorkerManager>>,
) -> Result<(), JsError> {
    // Worker constructor (simplified - takes script content, not URL)
    let mgr = manager.clone();
    ctx.set_global_function("Worker", move |args| {
        if let Some(script) = args.first().and_then(|v| v.as_string()) {
            let id = mgr.lock().unwrap().create_worker(script.to_string());
            Ok(JsValue::Number(id as f64))
        } else {
            Ok(JsValue::Number(-1.0))
        }
    })?;
    
    // postMessage to worker
    let mgr = manager.clone();
    ctx.set_global_function("postMessageToWorker", move |args| {
        if args.len() >= 2 {
            if let (Some(id), Some(data)) = (
                args[0].as_number(),
                args[1].as_string(),
            ) {
                mgr.lock().unwrap().post_message(id as u32, data.to_string());
            }
        }
        Ok(JsValue::Undefined)
    })?;
    
    // terminateWorker
    let mgr = manager;
    ctx.set_global_function("terminateWorker", move |args| {
        if let Some(id) = args.first().and_then(|v| v.as_number()) {
            mgr.lock().unwrap().terminate_worker(id as u32);
        }
        Ok(JsValue::Undefined)
    })?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_worker_creation() {
        let mut manager = WorkerManager::new();
        
        let id1 = manager.create_worker("console.log('worker 1')".into());
        let id2 = manager.create_worker("console.log('worker 2')".into());
        
        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(manager.active_count(), 2);
    }
    
    #[test]
    fn test_worker_messaging() {
        let mut manager = WorkerManager::new();
        let id = manager.create_worker("".into());
        
        // Post to worker
        manager.post_message(id, "hello".into());
        
        let worker = manager.get_worker(id).unwrap();
        let msg = worker.receive_message().unwrap();
        assert_eq!(msg.data, "hello");
    }
    
    #[test]
    fn test_worker_response() {
        let mut manager = WorkerManager::new();
        let id = manager.create_worker("".into());
        
        // Worker sends message
        {
            let worker = manager.get_worker(id).unwrap();
            worker.send_message("response".into());
        }
        
        // Main thread receives
        let messages = manager.get_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, id);
        assert_eq!(messages[0].1.data, "response");
    }
    
    #[test]
    fn test_worker_termination() {
        let mut manager = WorkerManager::new();
        let id = manager.create_worker("".into());
        
        assert_eq!(manager.active_count(), 1);
        
        manager.terminate_worker(id);
        assert_eq!(manager.active_count(), 0);
        
        // Messages should be ignored after termination
        manager.post_message(id, "ignored".into());
        // No crash
    }
    
    #[test]
    fn test_cleanup() {
        let mut manager = WorkerManager::new();
        manager.create_worker("".into());
        manager.create_worker("".into());
        let id3 = manager.create_worker("".into());
        
        manager.terminate_worker(id3);
        manager.cleanup();
        
        assert_eq!(manager.active_count(), 2);
    }
}
