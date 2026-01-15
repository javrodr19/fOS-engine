//! Process Architecture
//!
//! Main orchestrator for multi-process browser architecture.

use std::collections::HashMap;
use std::io::{self, Error, ErrorKind};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use super::{
    ProcessId, ProcessState, ProcessType, TabId,
    BrowserProcess, RendererProcess, NetworkProcess, GpuProcess, StorageProcess,
};

/// Counter for generating unique process IDs
static NEXT_PROCESS_ID: AtomicU32 = AtomicU32::new(1);

fn next_process_id() -> ProcessId {
    ProcessId::new(NEXT_PROCESS_ID.fetch_add(1, Ordering::SeqCst))
}

/// Multi-process browser architecture
#[derive(Debug)]
pub struct ProcessArchitecture {
    /// Main browser process state
    browser: BrowserProcess,
    /// Renderer processes (one per tab)
    renderers: HashMap<TabId, RendererProcess>,
    /// Network process (single, optional)
    network: Option<NetworkProcess>,
    /// GPU process (single, optional)
    gpu: Option<GpuProcess>,
    /// Storage process (single, optional)
    storage: Option<StorageProcess>,
    /// Whether we're in single-process mode (for debugging)
    single_process_mode: bool,
}

impl ProcessArchitecture {
    /// Create a new process architecture
    pub fn new() -> Self {
        Self {
            browser: BrowserProcess::new(),
            renderers: HashMap::new(),
            network: None,
            gpu: None,
            storage: None,
            single_process_mode: false,
        }
    }
    
    /// Create in single-process mode (all in-process, for debugging)
    pub fn single_process() -> Self {
        Self {
            browser: BrowserProcess::new(),
            renderers: HashMap::new(),
            network: None,
            gpu: None,
            storage: None,
            single_process_mode: true,
        }
    }
    
    /// Check if running in single-process mode
    pub fn is_single_process(&self) -> bool {
        self.single_process_mode
    }
    
    /// Get browser process
    pub fn browser(&self) -> &BrowserProcess {
        &self.browser
    }
    
    /// Get mutable browser process
    pub fn browser_mut(&mut self) -> &mut BrowserProcess {
        &mut self.browser
    }
    
    /// Spawn a new renderer process for a tab
    pub fn spawn_renderer(&mut self, tab: TabId) -> io::Result<()> {
        if self.single_process_mode {
            // In single-process mode, create in-process renderer
            let renderer = RendererProcess::in_process(next_process_id(), tab);
            self.renderers.insert(tab, renderer);
            return Ok(());
        }
        
        // Spawn actual process
        let exe = std::env::current_exe()?;
        let ipc_path = format!("/tmp/fos-renderer-{}.sock", tab.0);
        
        let child = Command::new(exe)
            .arg("--type=renderer")
            .arg(format!("--tab={}", tab.0))
            .arg(format!("--ipc={}", ipc_path))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let process_id = ProcessId::new(child.id());
        let renderer = RendererProcess::from_child(process_id, tab, child, ipc_path);
        self.renderers.insert(tab, renderer);
        
        Ok(())
    }
    
    /// Get renderer for a tab
    pub fn get_renderer(&self, tab: TabId) -> Option<&RendererProcess> {
        self.renderers.get(&tab)
    }
    
    /// Get mutable renderer for a tab
    pub fn get_renderer_mut(&mut self, tab: TabId) -> Option<&mut RendererProcess> {
        self.renderers.get_mut(&tab)
    }
    
    /// Terminate renderer for a tab
    pub fn terminate_renderer(&mut self, tab: TabId) -> io::Result<()> {
        if let Some(mut renderer) = self.renderers.remove(&tab) {
            renderer.terminate()?;
        }
        Ok(())
    }
    
    /// Spawn network process
    pub fn spawn_network(&mut self) -> io::Result<()> {
        if self.single_process_mode {
            self.network = Some(NetworkProcess::in_process(next_process_id()));
            return Ok(());
        }
        
        let exe = std::env::current_exe()?;
        let ipc_path = "/tmp/fos-network.sock";
        
        let child = Command::new(exe)
            .arg("--type=network")
            .arg(format!("--ipc={}", ipc_path))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let process_id = ProcessId::new(child.id());
        self.network = Some(NetworkProcess::from_child(process_id, child, ipc_path.to_string()));
        
        Ok(())
    }
    
    /// Get network process
    pub fn network(&self) -> Option<&NetworkProcess> {
        self.network.as_ref()
    }
    
    /// Spawn GPU process
    pub fn spawn_gpu(&mut self) -> io::Result<()> {
        if self.single_process_mode {
            self.gpu = Some(GpuProcess::in_process(next_process_id()));
            return Ok(());
        }
        
        let exe = std::env::current_exe()?;
        let ipc_path = "/tmp/fos-gpu.sock";
        
        let child = Command::new(exe)
            .arg("--type=gpu")
            .arg(format!("--ipc={}", ipc_path))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let process_id = ProcessId::new(child.id());
        self.gpu = Some(GpuProcess::from_child(process_id, child, ipc_path.to_string()));
        
        Ok(())
    }
    
    /// Get GPU process
    pub fn gpu(&self) -> Option<&GpuProcess> {
        self.gpu.as_ref()
    }
    
    /// Spawn storage process
    pub fn spawn_storage(&mut self) -> io::Result<()> {
        if self.single_process_mode {
            self.storage = Some(StorageProcess::in_process(next_process_id()));
            return Ok(());
        }
        
        let exe = std::env::current_exe()?;
        let ipc_path = "/tmp/fos-storage.sock";
        
        let child = Command::new(exe)
            .arg("--type=storage")
            .arg(format!("--ipc={}", ipc_path))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let process_id = ProcessId::new(child.id());
        self.storage = Some(StorageProcess::from_child(process_id, child, ipc_path.to_string()));
        
        Ok(())
    }
    
    /// Get storage process
    pub fn storage(&self) -> Option<&StorageProcess> {
        self.storage.as_ref()
    }
    
    /// Get all renderer tab IDs
    pub fn renderer_tabs(&self) -> impl Iterator<Item = TabId> + '_ {
        self.renderers.keys().copied()
    }
    
    /// Count active renderers
    pub fn renderer_count(&self) -> usize {
        self.renderers.len()
    }
    
    /// Shutdown all processes
    pub fn shutdown(&mut self) -> io::Result<()> {
        // Terminate all renderers
        let tabs: Vec<TabId> = self.renderers.keys().copied().collect();
        for tab in tabs {
            let _ = self.terminate_renderer(tab);
        }
        
        // Terminate service processes
        if let Some(mut network) = self.network.take() {
            let _ = network.terminate();
        }
        if let Some(mut gpu) = self.gpu.take() {
            let _ = gpu.terminate();
        }
        if let Some(mut storage) = self.storage.take() {
            let _ = storage.terminate();
        }
        
        Ok(())
    }
}

impl Default for ProcessArchitecture {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ProcessArchitecture {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_single_process_mode() {
        let mut arch = ProcessArchitecture::single_process();
        assert!(arch.is_single_process());
        
        // Should be able to create renderers without spawning processes
        arch.spawn_renderer(TabId::new(1)).unwrap();
        arch.spawn_renderer(TabId::new(2)).unwrap();
        
        assert_eq!(arch.renderer_count(), 2);
        assert!(arch.get_renderer(TabId::new(1)).is_some());
        assert!(arch.get_renderer(TabId::new(2)).is_some());
    }
    
    #[test]
    fn test_service_processes() {
        let mut arch = ProcessArchitecture::single_process();
        
        arch.spawn_network().unwrap();
        arch.spawn_gpu().unwrap();
        arch.spawn_storage().unwrap();
        
        assert!(arch.network().is_some());
        assert!(arch.gpu().is_some());
        assert!(arch.storage().is_some());
    }
    
    #[test]
    fn test_terminate_renderer() {
        let mut arch = ProcessArchitecture::single_process();
        
        arch.spawn_renderer(TabId::new(1)).unwrap();
        assert_eq!(arch.renderer_count(), 1);
        
        arch.terminate_renderer(TabId::new(1)).unwrap();
        assert_eq!(arch.renderer_count(), 0);
    }
}
