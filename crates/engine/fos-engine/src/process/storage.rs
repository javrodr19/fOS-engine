//! Storage Process
//!
//! Storage process for IndexedDB, cache, and persistent storage.

use std::io;
use std::process::Child;

use super::{ProcessId, ProcessState};

/// Storage process - handles IndexedDB, cache, persistent storage
#[derive(Debug)]
pub struct StorageProcess {
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
    /// Total storage used (bytes)
    storage_used: u64,
    /// Storage quota (bytes)
    storage_quota: u64,
}

impl StorageProcess {
    /// Create an in-process storage handler
    pub fn in_process(id: ProcessId) -> Self {
        Self {
            id,
            state: ProcessState::Running,
            child: None,
            ipc_path: None,
            in_process: true,
            storage_used: 0,
            storage_quota: 100 * 1024 * 1024, // 100MB default
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
            storage_used: 0,
            storage_quota: 100 * 1024 * 1024,
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
    
    /// Get storage used
    pub fn storage_used(&self) -> u64 {
        self.storage_used
    }
    
    /// Set storage used
    pub fn set_storage_used(&mut self, bytes: u64) {
        self.storage_used = bytes;
    }
    
    /// Get storage quota
    pub fn storage_quota(&self) -> u64 {
        self.storage_quota
    }
    
    /// Set storage quota
    pub fn set_storage_quota(&mut self, bytes: u64) {
        self.storage_quota = bytes;
    }
    
    /// Get remaining storage
    pub fn storage_remaining(&self) -> u64 {
        self.storage_quota.saturating_sub(self.storage_used)
    }
    
    /// Get storage usage ratio (0.0-1.0)
    pub fn usage_ratio(&self) -> f64 {
        if self.storage_quota == 0 {
            return 1.0;
        }
        self.storage_used as f64 / self.storage_quota as f64
    }
    
    /// Terminate the storage process
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

/// Storage process statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct StorageStats {
    /// Total storage used (bytes)
    pub storage_used: u64,
    /// Storage quota (bytes)
    pub storage_quota: u64,
    /// IndexedDB databases
    pub idb_databases: usize,
    /// Cache entries
    pub cache_entries: usize,
    /// Cookie count
    pub cookies: usize,
    /// LocalStorage items
    pub local_storage_items: usize,
    /// SessionStorage items
    pub session_storage_items: usize,
}

/// Storage origin info
#[derive(Debug, Clone)]
pub struct StorageOrigin {
    /// Origin (e.g., "https://example.com")
    pub origin: String,
    /// Storage used by this origin
    pub storage_used: u64,
    /// Origin quota
    pub quota: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_in_process_storage() {
        let storage = StorageProcess::in_process(ProcessId::new(1));
        
        assert!(storage.is_in_process());
        assert_eq!(storage.state(), ProcessState::Running);
    }
    
    #[test]
    fn test_storage_quota() {
        let mut storage = StorageProcess::in_process(ProcessId::new(1));
        
        storage.set_storage_quota(1024 * 1024 * 1024); // 1GB
        storage.set_storage_used(512 * 1024 * 1024); // 512MB
        
        assert_eq!(storage.storage_remaining(), 512 * 1024 * 1024);
        assert!((storage.usage_ratio() - 0.5).abs() < 0.001);
    }
}
