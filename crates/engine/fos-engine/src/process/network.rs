//! Network Process
//!
//! Centralized network process for all I/O operations.

use std::io;
use std::process::Child;

use super::{ProcessId, ProcessState};

/// Network process - handles all network I/O
#[derive(Debug)]
pub struct NetworkProcess {
    /// Process ID
    id: ProcessId,
    /// Current state
    state: ProcessState,
    /// Child process (None if in-process)
    child: Option<Child>,
    /// IPC channel path
    ipc_path: Option<String>,
    /// Is in-process mode
    in_process: bool,
    /// Active connections count
    active_connections: usize,
    /// Total bytes transferred
    bytes_transferred: u64,
}

impl NetworkProcess {
    /// Create an in-process network handler
    pub fn in_process(id: ProcessId) -> Self {
        Self {
            id,
            state: ProcessState::Running,
            child: None,
            ipc_path: None,
            in_process: true,
            active_connections: 0,
            bytes_transferred: 0,
        }
    }
    
    /// Create from spawned child process
    pub fn from_child(id: ProcessId, child: Child, ipc_path: String) -> Self {
        Self {
            id,
            state: ProcessState::Starting,
            child: Some(child),
            ipc_path: Some(ipc_path),
            in_process: false,
            active_connections: 0,
            bytes_transferred: 0,
        }
    }
    
    /// Get process ID
    pub fn id(&self) -> ProcessId {
        self.id
    }
    
    /// Get current state
    pub fn state(&self) -> ProcessState {
        self.state
    }
    
    /// Set state
    pub fn set_state(&mut self, state: ProcessState) {
        self.state = state;
    }
    
    /// Is this in-process?
    pub fn is_in_process(&self) -> bool {
        self.in_process
    }
    
    /// Get IPC path
    pub fn ipc_path(&self) -> Option<&str> {
        self.ipc_path.as_deref()
    }
    
    /// Get active connection count
    pub fn active_connections(&self) -> usize {
        self.active_connections
    }
    
    /// Update active connection count
    pub fn set_active_connections(&mut self, count: usize) {
        self.active_connections = count;
    }
    
    /// Get bytes transferred
    pub fn bytes_transferred(&self) -> u64 {
        self.bytes_transferred
    }
    
    /// Add to bytes transferred
    pub fn add_bytes_transferred(&mut self, bytes: u64) {
        self.bytes_transferred = self.bytes_transferred.saturating_add(bytes);
    }
    
    /// Terminate the network process
    pub fn terminate(&mut self) -> io::Result<()> {
        if self.in_process {
            self.state = ProcessState::Terminated;
            return Ok(());
        }
        
        if let Some(ref mut child) = self.child {
            self.state = ProcessState::ShuttingDown;
            child.kill()?;
            child.wait()?;
            self.state = ProcessState::Terminated;
        }
        
        Ok(())
    }
}

/// Network process statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct NetworkStats {
    /// Active connections
    pub active_connections: usize,
    /// Total requests made
    pub total_requests: u64,
    /// Cache hit ratio (0.0-1.0)
    pub cache_hit_ratio: f32,
    /// Bytes downloaded
    pub bytes_downloaded: u64,
    /// Bytes uploaded
    pub bytes_uploaded: u64,
    /// DNS queries
    pub dns_queries: u64,
    /// DNS cache hits
    pub dns_cache_hits: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_in_process_network() {
        let network = NetworkProcess::in_process(ProcessId::new(1));
        
        assert!(network.is_in_process());
        assert_eq!(network.state(), ProcessState::Running);
    }
    
    #[test]
    fn test_connection_tracking() {
        let mut network = NetworkProcess::in_process(ProcessId::new(1));
        
        network.set_active_connections(10);
        assert_eq!(network.active_connections(), 10);
    }
    
    #[test]
    fn test_bytes_tracking() {
        let mut network = NetworkProcess::in_process(ProcessId::new(1));
        
        network.add_bytes_transferred(1000);
        network.add_bytes_transferred(500);
        assert_eq!(network.bytes_transferred(), 1500);
    }
}
