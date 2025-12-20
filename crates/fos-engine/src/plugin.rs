//! Plugin architecture for optional engine components.
//!
//! This allows WebGL, media codecs, and other heavy components to be
//! loaded as external shared libraries, reducing the base binary size.
//!
//! # Design
//!
//! The plugin system is designed with future extensibility in mind:
//! - Plugins can provide WebGL, media decoding, font rendering, etc.
//! - Plugins are loaded dynamically at runtime (when the `dynamic-plugins` feature is enabled)
//! - Each plugin declares its capabilities via `PluginCapabilities`
//!
//! # Example
//! ```rust,ignore
//! use fos_engine::{PluginLoader, PluginCapabilities};
//!
//! let mut loader = PluginLoader::new();
//! loader.load(Path::new("/usr/lib/fos/webgl.so"))?;
//!
//! if let Some(webgl) = loader.get_by_capability(PluginCapabilities::WEBGL) {
//!     println!("WebGL plugin loaded: {}", webgl.info().name);
//! }
//! ```

use std::any::Any;

/// Plugin capability flags indicating what a plugin can provide.
///
/// Multiple capabilities can be combined using the `|` operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginCapabilities(u32);

impl PluginCapabilities {
    /// WebGL rendering capability
    pub const WEBGL: Self = Self(1 << 0);
    /// Media decoding capability (audio/video)
    pub const MEDIA_DECODE: Self = Self(1 << 1);
    /// Media encoding capability
    pub const MEDIA_ENCODE: Self = Self(1 << 2);
    /// Font rasterization capability
    pub const FONT_RASTERIZER: Self = Self(1 << 3);
    /// GPU compositing capability
    pub const GPU_COMPOSITOR: Self = Self(1 << 4);
    /// No capabilities
    pub const NONE: Self = Self(0);

    /// Check if this capability set contains a specific capability.
    #[inline]
    pub const fn has(self, cap: Self) -> bool {
        (self.0 & cap.0) != 0
    }

    /// Combine two capability sets.
    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for PluginCapabilities {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl Default for PluginCapabilities {
    fn default() -> Self {
        Self::NONE
    }
}

/// Plugin metadata returned by `Plugin::info()`.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Human-readable plugin name
    pub name: &'static str,
    /// Plugin version (semver)
    pub version: &'static str,
    /// Capabilities this plugin provides
    pub capabilities: PluginCapabilities,
}

/// Plugin trait for external components.
///
/// Implement this trait to create a plugin that can be loaded by the engine.
pub trait Plugin: Send + Sync {
    /// Return plugin metadata including name, version, and capabilities.
    fn info(&self) -> PluginInfo;

    /// Initialize the plugin. Called once after loading.
    fn init(&mut self) -> Result<(), PluginError>;

    /// Shutdown the plugin. Called before unloading.
    fn shutdown(&mut self);

    /// Get plugin-specific APIs as `Any` for downcasting.
    ///
    /// Plugins expose their specific APIs through this method.
    /// Callers downcast to the expected API type.
    fn api(&self) -> &dyn Any;
}

/// Plugin loading errors.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// Failed to load the plugin shared library
    #[error("Failed to load plugin: {0}")]
    LoadError(String),

    /// Plugin file not found
    #[error("Plugin not found: {0}")]
    NotFound(String),

    /// Plugin initialization failed
    #[error("Plugin init failed: {0}")]
    InitError(String),

    /// Plugin doesn't have the required capability
    #[error("Missing capability: {0:?}")]
    MissingCapability(PluginCapabilities),

    /// Plugin version mismatch
    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch {
        expected: String,
        actual: String,
    },
}

/// Plugin loader for managing dynamic plugins.
///
/// # Example
/// ```rust
/// use fos_engine::PluginLoader;
///
/// let loader = PluginLoader::new();
/// assert!(loader.list().is_empty());
/// ```
pub struct PluginLoader {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginLoader {
    /// Create a new empty plugin loader.
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    /// Load a plugin from a shared library path.
    ///
    /// This is only available with the `dynamic-plugins` feature.
    #[cfg(feature = "dynamic-plugins")]
    pub fn load(&mut self, _path: &std::path::Path) -> Result<(), PluginError> {
        // Future: Use libloading to load .so/.dll
        // let lib = unsafe { libloading::Library::new(path)? };
        // let create_fn = unsafe { lib.get::<fn() -> Box<dyn Plugin>>(b"create_plugin")? };
        // let plugin = create_fn();
        // self.plugins.push(plugin);
        Err(PluginError::LoadError("Dynamic plugins not yet implemented".into()))
    }

    /// Register a plugin directly (for statically linked plugins).
    pub fn register(&mut self, mut plugin: Box<dyn Plugin>) -> Result<(), PluginError> {
        plugin.init()?;
        self.plugins.push(plugin);
        Ok(())
    }

    /// Get a plugin by its capability.
    pub fn get_by_capability(&self, cap: PluginCapabilities) -> Option<&dyn Plugin> {
        self.plugins
            .iter()
            .map(|p| p.as_ref())
            .find(|p| p.info().capabilities.has(cap))
    }

    /// Get all loaded plugins.
    pub fn list(&self) -> &[Box<dyn Plugin>] {
        &self.plugins
    }

    /// Unload all plugins.
    pub fn unload_all(&mut self) {
        for plugin in &mut self.plugins {
            plugin.shutdown();
        }
        self.plugins.clear();
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PluginLoader {
    fn drop(&mut self) {
        self.unload_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPlugin {
        initialized: bool,
    }

    impl MockPlugin {
        fn new() -> Self {
            Self { initialized: false }
        }
    }

    impl Plugin for MockPlugin {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                name: "MockPlugin",
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

    #[test]
    fn test_plugin_loader() {
        let mut loader = PluginLoader::new();
        assert!(loader.list().is_empty());

        loader.register(Box::new(MockPlugin::new())).unwrap();
        assert_eq!(loader.list().len(), 1);
    }

    #[test]
    fn test_get_by_capability() {
        let mut loader = PluginLoader::new();
        loader.register(Box::new(MockPlugin::new())).unwrap();

        let plugin = loader.get_by_capability(PluginCapabilities::WEBGL);
        assert!(plugin.is_some());
        assert_eq!(plugin.unwrap().info().name, "MockPlugin");

        let plugin = loader.get_by_capability(PluginCapabilities::MEDIA_DECODE);
        assert!(plugin.is_none());
    }

    #[test]
    fn test_capability_operations() {
        let caps = PluginCapabilities::WEBGL | PluginCapabilities::MEDIA_DECODE;
        assert!(caps.has(PluginCapabilities::WEBGL));
        assert!(caps.has(PluginCapabilities::MEDIA_DECODE));
        assert!(!caps.has(PluginCapabilities::FONT_RASTERIZER));
    }
}
