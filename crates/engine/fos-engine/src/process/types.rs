//! Process Types
//!
//! Core types for multi-process architecture.

use std::fmt;

/// Unique identifier for a tab
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub u32);

impl TabId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tab({})", self.0)
    }
}

/// Unique identifier for a process
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId(pub u32);

impl ProcessId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
    
    /// Get the OS process ID (if this is a real process)
    pub fn as_raw(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PID({})", self.0)
    }
}

/// Type of process in the browser
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessType {
    /// Main browser process (UI, navigation)
    Browser,
    /// Renderer process (DOM, layout, JS)
    Renderer,
    /// Network process (all I/O)
    Network,
    /// GPU process (compositing, WebGL)
    Gpu,
    /// Storage process (IndexedDB, cache)
    Storage,
}

impl ProcessType {
    /// Parse from command-line argument
    pub fn from_arg(arg: &str) -> Option<Self> {
        match arg {
            "browser" => Some(Self::Browser),
            "renderer" => Some(Self::Renderer),
            "network" => Some(Self::Network),
            "gpu" => Some(Self::Gpu),
            "storage" => Some(Self::Storage),
            _ => None,
        }
    }
    
    /// Convert to command-line argument
    pub fn as_arg(&self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Renderer => "renderer",
            Self::Network => "network",
            Self::Gpu => "gpu",
            Self::Storage => "storage",
        }
    }
}

impl fmt::Display for ProcessType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_arg())
    }
}

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is starting up
    Starting,
    /// Process is running normally
    Running,
    /// Process is shutting down gracefully
    ShuttingDown,
    /// Process has terminated
    Terminated,
    /// Process crashed
    Crashed,
}

/// Command-line arguments for child processes
#[derive(Debug, Clone)]
pub struct ProcessArgs {
    /// Process type
    pub process_type: ProcessType,
    /// Tab ID (for renderer processes)
    pub tab_id: Option<TabId>,
    /// IPC channel identifier
    pub ipc_channel: Option<String>,
}

impl ProcessArgs {
    /// Parse from command-line arguments
    pub fn parse() -> Option<Self> {
        let args: Vec<String> = std::env::args().collect();
        Self::parse_from(&args)
    }
    
    /// Parse from argument slice
    pub fn parse_from(args: &[String]) -> Option<Self> {
        let mut process_type = None;
        let mut tab_id = None;
        let mut ipc_channel = None;
        
        for arg in args.iter().skip(1) {
            if let Some(type_str) = arg.strip_prefix("--type=") {
                process_type = ProcessType::from_arg(type_str);
            } else if let Some(tab_str) = arg.strip_prefix("--tab=") {
                if let Ok(id) = tab_str.parse::<u32>() {
                    tab_id = Some(TabId::new(id));
                }
            } else if let Some(channel) = arg.strip_prefix("--ipc=") {
                ipc_channel = Some(channel.to_string());
            }
        }
        
        process_type.map(|pt| Self {
            process_type: pt,
            tab_id,
            ipc_channel,
        })
    }
    
    /// Build command-line arguments for spawning
    pub fn to_args(&self) -> Vec<String> {
        let mut args = vec![format!("--type={}", self.process_type.as_arg())];
        
        if let Some(tab) = self.tab_id {
            args.push(format!("--tab={}", tab.0));
        }
        
        if let Some(ref channel) = self.ipc_channel {
            args.push(format!("--ipc={}", channel));
        }
        
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_process_type_round_trip() {
        for pt in [
            ProcessType::Browser,
            ProcessType::Renderer,
            ProcessType::Network,
            ProcessType::Gpu,
            ProcessType::Storage,
        ] {
            let arg = pt.as_arg();
            let parsed = ProcessType::from_arg(arg);
            assert_eq!(parsed, Some(pt));
        }
    }
    
    #[test]
    fn test_process_args_parse() {
        let args = vec![
            "fos".to_string(),
            "--type=renderer".to_string(),
            "--tab=42".to_string(),
            "--ipc=/tmp/ipc.sock".to_string(),
        ];
        
        let parsed = ProcessArgs::parse_from(&args).unwrap();
        assert_eq!(parsed.process_type, ProcessType::Renderer);
        assert_eq!(parsed.tab_id, Some(TabId::new(42)));
        assert_eq!(parsed.ipc_channel, Some("/tmp/ipc.sock".to_string()));
    }
    
    #[test]
    fn test_process_args_to_args() {
        let args = ProcessArgs {
            process_type: ProcessType::Renderer,
            tab_id: Some(TabId::new(1)),
            ipc_channel: Some("/tmp/test".to_string()),
        };
        
        let result = args.to_args();
        assert!(result.contains(&"--type=renderer".to_string()));
        assert!(result.contains(&"--tab=1".to_string()));
        assert!(result.contains(&"--ipc=/tmp/test".to_string()));
    }
}
