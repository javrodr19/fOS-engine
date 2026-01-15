//! Renderer Process
//!
//! Renderer process responsible for DOM, layout, paint, and JS execution.

use std::io;
use std::process::Child;

use super::{ProcessId, ProcessState, TabId};

/// Renderer process - handles DOM, layout, paint, JS for a tab
#[derive(Debug)]
pub struct RendererProcess {
    /// Process ID
    id: ProcessId,
    /// Associated tab
    tab: TabId,
    /// Current state
    state: ProcessState,
    /// Child process (None if in-process)
    child: Option<Child>,
    /// IPC channel path
    ipc_path: Option<String>,
    /// Memory usage estimate (bytes)
    memory_usage: usize,
    /// Is in-process (single-process mode)
    in_process: bool,
}

impl RendererProcess {
    /// Create an in-process renderer (for single-process mode)
    pub fn in_process(id: ProcessId, tab: TabId) -> Self {
        Self {
            id,
            tab,
            state: ProcessState::Running,
            child: None,
            ipc_path: None,
            memory_usage: 0,
            in_process: true,
        }
    }
    
    /// Create from spawned child process
    pub fn from_child(id: ProcessId, tab: TabId, child: Child, ipc_path: String) -> Self {
        Self {
            id,
            tab,
            state: ProcessState::Starting,
            child: Some(child),
            ipc_path: Some(ipc_path),
            memory_usage: 0,
            in_process: false,
        }
    }
    
    /// Get process ID
    pub fn id(&self) -> ProcessId {
        self.id
    }
    
    /// Get associated tab
    pub fn tab(&self) -> TabId {
        self.tab
    }
    
    /// Get current state
    pub fn state(&self) -> ProcessState {
        self.state
    }
    
    /// Set state
    pub fn set_state(&mut self, state: ProcessState) {
        self.state = state;
    }
    
    /// Is this an in-process renderer?
    pub fn is_in_process(&self) -> bool {
        self.in_process
    }
    
    /// Get IPC path
    pub fn ipc_path(&self) -> Option<&str> {
        self.ipc_path.as_deref()
    }
    
    /// Get memory usage
    pub fn memory_usage(&self) -> usize {
        self.memory_usage
    }
    
    /// Set memory usage
    pub fn set_memory_usage(&mut self, bytes: usize) {
        self.memory_usage = bytes;
    }
    
    /// Check if process is still running
    pub fn is_running(&self) -> bool {
        if self.in_process {
            return self.state == ProcessState::Running;
        }
        
        if let Some(ref child) = self.child {
            // Check by trying to get exit status
            matches!(self.state, ProcessState::Running | ProcessState::Starting)
        } else {
            false
        }
    }
    
    /// Try to get exit status (non-blocking)
    pub fn try_wait(&mut self) -> io::Result<Option<i32>> {
        if let Some(ref mut child) = self.child {
            match child.try_wait()? {
                Some(status) => {
                    self.state = if status.success() {
                        ProcessState::Terminated
                    } else {
                        ProcessState::Crashed
                    };
                    Ok(status.code())
                }
                None => Ok(None),
            }
        } else {
            Ok(None)
        }
    }
    
    /// Terminate the renderer process
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

/// Renderer process statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct RendererStats {
    /// DOM node count
    pub dom_nodes: usize,
    /// Layout box count
    pub layout_boxes: usize,
    /// Paint operations
    pub paint_ops: usize,
    /// JS heap size
    pub js_heap_bytes: usize,
    /// Frame time (ms)
    pub frame_time_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_in_process_renderer() {
        let renderer = RendererProcess::in_process(ProcessId::new(1), TabId::new(1));
        
        assert!(renderer.is_in_process());
        assert!(renderer.is_running());
        assert_eq!(renderer.state(), ProcessState::Running);
    }
    
    #[test]
    fn test_terminate_in_process() {
        let mut renderer = RendererProcess::in_process(ProcessId::new(1), TabId::new(1));
        
        renderer.terminate().unwrap();
        assert_eq!(renderer.state(), ProcessState::Terminated);
        assert!(!renderer.is_running());
    }
    
    #[test]
    fn test_memory_tracking() {
        let mut renderer = RendererProcess::in_process(ProcessId::new(1), TabId::new(1));
        
        renderer.set_memory_usage(50_000_000); // 50MB
        assert_eq!(renderer.memory_usage(), 50_000_000);
    }
}
