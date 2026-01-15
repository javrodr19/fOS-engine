//! GPU Process
//!
//! GPU process for compositing and WebGL.

use std::io;
use std::process::Child;

use super::{ProcessId, ProcessState};

/// GPU process - handles compositing and WebGL
#[derive(Debug)]
pub struct GpuProcess {
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
    /// GPU memory usage (bytes)
    gpu_memory: usize,
    /// Active WebGL contexts
    webgl_contexts: usize,
}

impl GpuProcess {
    /// Create an in-process GPU handler
    pub fn in_process(id: ProcessId) -> Self {
        Self {
            id,
            state: ProcessState::Running,
            child: None,
            ipc_path: None,
            in_process: true,
            gpu_memory: 0,
            webgl_contexts: 0,
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
            gpu_memory: 0,
            webgl_contexts: 0,
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
    
    /// Get GPU memory usage
    pub fn gpu_memory(&self) -> usize {
        self.gpu_memory
    }
    
    /// Set GPU memory usage
    pub fn set_gpu_memory(&mut self, bytes: usize) {
        self.gpu_memory = bytes;
    }
    
    /// Get WebGL context count
    pub fn webgl_contexts(&self) -> usize {
        self.webgl_contexts
    }
    
    /// Set WebGL context count
    pub fn set_webgl_contexts(&mut self, count: usize) {
        self.webgl_contexts = count;
    }
    
    /// Terminate the GPU process
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

/// GPU process statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct GpuStats {
    /// GPU memory used (bytes)
    pub gpu_memory: usize,
    /// GPU memory limit (bytes)
    pub gpu_memory_limit: usize,
    /// Active textures
    pub active_textures: usize,
    /// Active WebGL contexts
    pub webgl_contexts: usize,
    /// Frames composited
    pub frames_composited: u64,
    /// Average composite time (ms)
    pub avg_composite_ms: f64,
}

/// Compositing layer info
#[derive(Debug, Clone, Copy)]
pub struct LayerInfo {
    /// Layer ID
    pub id: u32,
    /// Width
    pub width: u32,
    /// Height
    pub height: u32,
    /// Texture memory (bytes)
    pub texture_bytes: usize,
    /// Is visible
    pub visible: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_in_process_gpu() {
        let gpu = GpuProcess::in_process(ProcessId::new(1));
        
        assert!(gpu.is_in_process());
        assert_eq!(gpu.state(), ProcessState::Running);
    }
    
    #[test]
    fn test_memory_tracking() {
        let mut gpu = GpuProcess::in_process(ProcessId::new(1));
        
        gpu.set_gpu_memory(100_000_000); // 100MB
        assert_eq!(gpu.gpu_memory(), 100_000_000);
    }
    
    #[test]
    fn test_webgl_contexts() {
        let mut gpu = GpuProcess::in_process(ProcessId::new(1));
        
        gpu.set_webgl_contexts(3);
        assert_eq!(gpu.webgl_contexts(), 3);
    }
}
