//! Browser extensions API
//!
//! Minimal extension support for browser customization.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Extension manifest
#[derive(Debug, Clone)]
pub struct ExtensionManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: Vec<Permission>,
    pub content_scripts: Vec<ContentScript>,
    pub background: Option<BackgroundScript>,
    pub browser_action: Option<BrowserAction>,
    pub page_action: Option<PageAction>,
}

/// Extension permission
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Permission {
    Tabs,
    ActiveTab,
    Storage,
    Cookies,
    WebRequest,
    WebRequestBlocking,
    Downloads,
    Bookmarks,
    History,
    Notifications,
    ContextMenus,
    Host(String), // e.g., "*://*.example.com/*"
}

impl Permission {
    pub fn from_str(s: &str) -> Self {
        match s {
            "tabs" => Self::Tabs,
            "activeTab" => Self::ActiveTab,
            "storage" => Self::Storage,
            "cookies" => Self::Cookies,
            "webRequest" => Self::WebRequest,
            "webRequestBlocking" => Self::WebRequestBlocking,
            "downloads" => Self::Downloads,
            "bookmarks" => Self::Bookmarks,
            "history" => Self::History,
            "notifications" => Self::Notifications,
            "contextMenus" => Self::ContextMenus,
            host => Self::Host(host.to_string()),
        }
    }
}

/// Content script definition
#[derive(Debug, Clone)]
pub struct ContentScript {
    pub matches: Vec<String>,
    pub js: Vec<String>,
    pub css: Vec<String>,
    pub run_at: RunAt,
}

/// When to run content scripts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunAt {
    DocumentStart,
    #[default]
    DocumentEnd,
    DocumentIdle,
}

/// Background script
#[derive(Debug, Clone)]
pub struct BackgroundScript {
    pub scripts: Vec<String>,
    pub persistent: bool,
}

/// Browser action (toolbar button)
#[derive(Debug, Clone)]
pub struct BrowserAction {
    pub default_icon: Option<String>,
    pub default_title: Option<String>,
    pub default_popup: Option<String>,
}

/// Page action (address bar button)
#[derive(Debug, Clone)]
pub struct PageAction {
    pub default_icon: Option<String>,
    pub default_title: Option<String>,
    pub default_popup: Option<String>,
}

/// Loaded extension
#[derive(Debug)]
pub struct Extension {
    pub id: String,
    pub manifest: ExtensionManifest,
    pub enabled: bool,
    pub storage: HashMap<String, String>,
}

impl Extension {
    pub fn new(id: &str, manifest: ExtensionManifest) -> Self {
        Self {
            id: id.to_string(),
            manifest,
            enabled: true,
            storage: HashMap::new(),
        }
    }
    
    /// Check if extension has permission
    pub fn has_permission(&self, perm: &Permission) -> bool {
        self.manifest.permissions.contains(perm)
    }
    
    /// Check if content script should run on URL
    pub fn matches_url(&self, url: &str) -> Vec<&ContentScript> {
        self.manifest.content_scripts.iter()
            .filter(|cs| cs.matches.iter().any(|pattern| Self::url_matches(url, pattern)))
            .collect()
    }
    
    fn url_matches(url: &str, pattern: &str) -> bool {
        // Simple glob matching
        if pattern == "<all_urls>" {
            return true;
        }
        
        // Convert pattern to regex-like matching
        let pattern = pattern
            .replace(".", r"\.")
            .replace("*", ".*");
        
        url.contains(&pattern.replace(".*", ""))
    }
}

/// Extension manager
#[derive(Debug, Default)]
pub struct ExtensionManager {
    extensions: HashMap<String, Arc<Mutex<Extension>>>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Load an extension
    pub fn load(&mut self, id: &str, manifest: ExtensionManifest) -> Result<(), ExtensionError> {
        if self.extensions.contains_key(id) {
            return Err(ExtensionError::AlreadyLoaded);
        }
        
        let extension = Extension::new(id, manifest);
        self.extensions.insert(id.to_string(), Arc::new(Mutex::new(extension)));
        Ok(())
    }
    
    /// Unload an extension
    pub fn unload(&mut self, id: &str) -> bool {
        self.extensions.remove(id).is_some()
    }
    
    /// Enable/disable extension
    pub fn set_enabled(&self, id: &str, enabled: bool) -> bool {
        if let Some(ext) = self.extensions.get(id) {
            ext.lock().unwrap().enabled = enabled;
            true
        } else {
            false
        }
    }
    
    /// Get extension
    pub fn get(&self, id: &str) -> Option<Arc<Mutex<Extension>>> {
        self.extensions.get(id).cloned()
    }
    
    /// Get all extension IDs
    pub fn list(&self) -> Vec<String> {
        self.extensions.keys().cloned().collect()
    }
    
    /// Get enabled extensions
    pub fn enabled(&self) -> Vec<Arc<Mutex<Extension>>> {
        self.extensions.values()
            .filter(|ext| ext.lock().unwrap().enabled)
            .cloned()
            .collect()
    }
    
    /// Get content scripts for URL
    pub fn get_content_scripts(&self, url: &str) -> Vec<(String, ContentScript)> {
        let mut scripts = Vec::new();
        
        for (_id, ext) in &self.extensions {
            let ext = ext.lock().unwrap();
            if !ext.enabled {
                continue;
            }
            
            for cs in &ext.manifest.content_scripts {
                if cs.matches.iter().any(|p| Extension::url_matches(url, p)) {
                    scripts.push((ext.id.clone(), cs.clone()));
                }
            }
        }
        
        scripts
    }
    
    /// Store data for extension
    pub fn storage_set(&self, ext_id: &str, key: &str, value: &str) -> bool {
        if let Some(ext) = self.extensions.get(ext_id) {
            ext.lock().unwrap().storage.insert(key.to_string(), value.to_string());
            true
        } else {
            false
        }
    }
    
    /// Get stored data
    pub fn storage_get(&self, ext_id: &str, key: &str) -> Option<String> {
        self.extensions.get(ext_id)
            .and_then(|ext| ext.lock().unwrap().storage.get(key).cloned())
    }
}

/// Extension errors
#[derive(Debug)]
pub enum ExtensionError {
    AlreadyLoaded,
    NotFound,
    InvalidManifest(String),
    PermissionDenied,
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyLoaded => write!(f, "Extension already loaded"),
            Self::NotFound => write!(f, "Extension not found"),
            Self::InvalidManifest(msg) => write!(f, "Invalid manifest: {}", msg),
            Self::PermissionDenied => write!(f, "Permission denied"),
        }
    }
}

impl std::error::Error for ExtensionError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extension_loading() {
        let mut mgr = ExtensionManager::new();
        
        let manifest = ExtensionManifest {
            name: "Test Extension".to_string(),
            version: "1.0.0".to_string(),
            description: "A test extension".to_string(),
            permissions: vec![Permission::Tabs, Permission::Storage],
            content_scripts: vec![],
            background: None,
            browser_action: None,
            page_action: None,
        };
        
        mgr.load("test-ext", manifest).unwrap();
        
        assert!(mgr.get("test-ext").is_some());
    }
    
    #[test]
    fn test_url_matching() {
        assert!(Extension::url_matches("https://example.com/page", "*://example.com/*"));
        assert!(Extension::url_matches("https://test.example.com/", "*://*.example.com/*"));
    }
}
