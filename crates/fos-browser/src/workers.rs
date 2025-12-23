//! Worker Integration
//!
//! Integrates fos-js web workers: Web Workers, Service Workers, Shared Workers.


// Re-export worker types from fos-js worker module
// Note: fos-js worker module is not fully exported at root level
// We provide a simplified integration here

/// Worker manager for the browser
#[derive(Default)]
pub struct WorkerIntegration {
    /// Web worker scripts by ID
    workers: Vec<BrowserWorker>,
    /// Next worker ID
    next_id: u32,
    /// Service worker registrations
    service_workers: Vec<ServiceWorkerReg>,
}

/// Browser web worker
#[derive(Debug)]
pub struct BrowserWorker {
    pub id: u32,
    pub url: String,
    pub script: String,
    pub terminated: bool,
    /// Messages from main to worker
    inbox: Vec<String>,
    /// Messages from worker to main
    outbox: Vec<String>,
}

/// Service worker registration
#[derive(Debug, Clone)]
pub struct ServiceWorkerReg {
    pub scope: String,
    pub script_url: String,
    pub state: ServiceWorkerState,
}

/// Service worker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceWorkerState {
    Installing,
    Installed,
    Activating,
    Activated,
    Redundant,
}

impl WorkerIntegration {
    /// Create new worker integration
    pub fn new() -> Self {
        Self::default()
    }
    
    // === Web Workers ===
    
    /// Create a new web worker
    pub fn create_worker(&mut self, url: &str, script: &str) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        
        self.workers.push(BrowserWorker {
            id,
            url: url.to_string(),
            script: script.to_string(),
            terminated: false,
            inbox: Vec::new(),
            outbox: Vec::new(),
        });
        
        log::debug!("Created worker {} from {}", id, url);
        id
    }
    
    /// Post message to worker
    pub fn post_message(&mut self, worker_id: u32, data: &str) {
        if let Some(worker) = self.workers.iter_mut().find(|w| w.id == worker_id) {
            if !worker.terminated {
                worker.inbox.push(data.to_string());
            }
        }
    }
    
    /// Get messages from worker
    pub fn get_worker_messages(&mut self, worker_id: u32) -> Vec<String> {
        if let Some(worker) = self.workers.iter_mut().find(|w| w.id == worker_id) {
            std::mem::take(&mut worker.outbox)
        } else {
            Vec::new()
        }
    }
    
    /// Terminate worker
    pub fn terminate_worker(&mut self, worker_id: u32) {
        if let Some(worker) = self.workers.iter_mut().find(|w| w.id == worker_id) {
            worker.terminated = true;
            worker.inbox.clear();
            log::debug!("Terminated worker {}", worker_id);
        }
    }
    
    /// Get active worker count
    pub fn active_workers(&self) -> usize {
        self.workers.iter().filter(|w| !w.terminated).count()
    }
    
    // === Service Workers ===
    
    /// Register a service worker
    pub fn register_service_worker(&mut self, scope: &str, script_url: &str) -> Result<(), String> {
        // Check if already registered for this scope
        if self.service_workers.iter().any(|sw| sw.scope == scope) {
            return Err("Service worker already registered for this scope".to_string());
        }
        
        self.service_workers.push(ServiceWorkerReg {
            scope: scope.to_string(),
            script_url: script_url.to_string(),
            state: ServiceWorkerState::Installing,
        });
        
        log::info!("Registered service worker for scope {}", scope);
        Ok(())
    }
    
    /// Unregister service worker
    pub fn unregister_service_worker(&mut self, scope: &str) -> bool {
        let len = self.service_workers.len();
        self.service_workers.retain(|sw| sw.scope != scope);
        self.service_workers.len() < len
    }
    
    /// Get service workers for URL
    pub fn get_service_workers(&self, url: &str) -> Vec<&ServiceWorkerReg> {
        self.service_workers.iter()
            .filter(|sw| url.starts_with(&sw.scope))
            .collect()
    }
    
    /// Update service worker state
    pub fn update_service_worker_state(&mut self, scope: &str, state: ServiceWorkerState) {
        if let Some(sw) = self.service_workers.iter_mut().find(|sw| sw.scope == scope) {
            sw.state = state;
        }
    }
    
    /// Cleanup terminated workers
    pub fn cleanup(&mut self) {
        self.workers.retain(|w| !w.terminated);
        self.service_workers.retain(|sw| sw.state != ServiceWorkerState::Redundant);
    }
    
    /// Get statistics
    pub fn stats(&self) -> WorkerStats {
        WorkerStats {
            web_workers: self.active_workers(),
            service_workers: self.service_workers.len(),
            terminated: self.workers.iter().filter(|w| w.terminated).count(),
        }
    }
}

/// Worker statistics
#[derive(Debug, Clone)]
pub struct WorkerStats {
    pub web_workers: usize,
    pub service_workers: usize,
    pub terminated: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_worker_creation() {
        let mut workers = WorkerIntegration::new();
        
        let id = workers.create_worker("worker.js", "console.log('hello')");
        assert_eq!(id, 0);
        assert_eq!(workers.active_workers(), 1);
    }
    
    #[test]
    fn test_worker_messaging() {
        let mut workers = WorkerIntegration::new();
        let id = workers.create_worker("worker.js", "");
        
        workers.post_message(id, "hello");
        
        // Worker would process and respond...
    }
    
    #[test]
    fn test_worker_termination() {
        let mut workers = WorkerIntegration::new();
        let id = workers.create_worker("worker.js", "");
        
        workers.terminate_worker(id);
        assert_eq!(workers.active_workers(), 0);
    }
    
    #[test]
    fn test_service_worker_registration() {
        let mut workers = WorkerIntegration::new();
        
        workers.register_service_worker("/", "sw.js").unwrap();
        
        let sws = workers.get_service_workers("https://example.com/page");
        // Would match if scope matches
    }
}
