//! Plugin System Integration
//!
//! Integrates fos-engine plugin system for extensible browser capabilities.

use fos_engine::{
    Plugin, PluginInfo, PluginCapabilities, PluginError, PluginLoader,
};
use std::any::Any;

/// Plugin manager for browser extensions
pub struct PluginManager {
    /// Plugin loader
    loader: PluginLoader,
}

impl PluginManager {
    /// Create new plugin manager
    pub fn new() -> Self {
        Self {
            loader: PluginLoader::new(),
        }
    }
    
    /// Register a plugin
    pub fn register<P: Plugin + 'static>(&mut self, plugin: P) -> Result<(), PluginError> {
        self.loader.register(Box::new(plugin))
    }
    
    /// Get plugin by capability
    pub fn get_by_capability(&self, cap: PluginCapabilities) -> Option<&dyn Plugin> {
        self.loader.get_by_capability(cap)
    }
    
    /// Check if WebGL is available
    pub fn has_webgl(&self) -> bool {
        self.loader.get_by_capability(PluginCapabilities::WEBGL).is_some()
    }
    
    /// Check if media decoding is available
    pub fn has_media_decode(&self) -> bool {
        self.loader.get_by_capability(PluginCapabilities::MEDIA_DECODE).is_some()
    }
    
    /// Check if GPU compositor is available
    pub fn has_gpu_compositor(&self) -> bool {
        self.loader.get_by_capability(PluginCapabilities::GPU_COMPOSITOR).is_some()
    }
    
    /// Get all loaded plugins
    pub fn plugins(&self) -> &[Box<dyn Plugin>] {
        self.loader.list()
    }
    
    /// Get plugin count
    pub fn count(&self) -> usize {
        self.loader.list().len()
    }
    
    /// Unload all plugins
    pub fn unload_all(&mut self) {
        self.loader.unload_all();
    }
    
    /// Get summary
    pub fn summary(&self) -> PluginSummary {
        PluginSummary {
            plugin_count: self.count(),
            has_webgl: self.has_webgl(),
            has_media: self.has_media_decode(),
            has_gpu: self.has_gpu_compositor(),
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin summary
#[derive(Debug, Clone)]
pub struct PluginSummary {
    pub plugin_count: usize,
    pub has_webgl: bool,
    pub has_media: bool,
    pub has_gpu: bool,
}

/// Mock WebGL plugin for testing
pub struct MockWebGLPlugin {
    initialized: bool,
}

impl MockWebGLPlugin {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl Default for MockWebGLPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for MockWebGLPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "MockWebGL",
            version: "1.0.0",
            capabilities: PluginCapabilities::WEBGL,
        }
    }
    
    fn init(&mut self) -> Result<(), PluginError> {
        self.initialized = true;
        Ok(())
    }
    
    fn shutdown(&mut self) {
        self.initialized = false;
    }
    
    fn api(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_plugin_manager_creation() {
        let manager = PluginManager::new();
        assert_eq!(manager.count(), 0);
    }
    
    #[test]
    fn test_plugin_registration() {
        let mut manager = PluginManager::new();
        manager.register(MockWebGLPlugin::new()).unwrap();
        
        assert_eq!(manager.count(), 1);
        assert!(manager.has_webgl());
    }
    
    #[test]
    fn test_plugin_summary() {
        let mut manager = PluginManager::new();
        manager.register(MockWebGLPlugin::new()).unwrap();
        
        let summary = manager.summary();
        assert!(summary.has_webgl);
        assert!(!summary.has_media);
    }
}
