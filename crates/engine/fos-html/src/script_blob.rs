//! Script Content as Blob (Phase 24.4)
//!
//! Don't parse inside <script>. Keep as raw byte slice.
//! Pass to JS engine as-is. 30% less parsing work.

use std::collections::HashMap;

/// Script blob ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptBlobId(pub u32);

/// Script type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptType {
    /// Classic JavaScript
    JavaScript,
    /// ES Module
    Module,
    /// JSON (for <script type="application/json">)
    Json,
    /// Unknown/custom type
    Unknown,
}

impl ScriptType {
    pub fn from_type_attr(type_attr: Option<&str>) -> Self {
        match type_attr {
            None => Self::JavaScript, // Default
            Some("") => Self::JavaScript,
            Some("text/javascript") => Self::JavaScript,
            Some("application/javascript") => Self::JavaScript,
            Some("module") => Self::Module,
            Some("application/json") => Self::Json,
            _ => Self::Unknown,
        }
    }
}

/// Script blob - raw unparsed script content
#[derive(Debug, Clone)]
pub struct ScriptBlob {
    /// Unique ID
    pub id: ScriptBlobId,
    /// Script type
    pub script_type: ScriptType,
    /// Raw content (unparsed)
    content: Vec<u8>,
    /// Source URL (if external)
    pub source_url: Option<Box<str>>,
    /// Is deferred
    pub defer: bool,
    /// Is async
    pub is_async: bool,
    /// Byte offset in source HTML
    pub source_offset: u32,
    /// Has been parsed
    parsed: bool,
}

impl ScriptBlob {
    /// Create a new script blob
    pub fn new(id: ScriptBlobId, content: Vec<u8>, script_type: ScriptType) -> Self {
        Self {
            id,
            script_type,
            content,
            source_url: None,
            defer: false,
            is_async: false,
            source_offset: 0,
            parsed: false,
        }
    }
    
    /// Create from inline script content
    pub fn inline(id: ScriptBlobId, content: &str) -> Self {
        Self::new(id, content.as_bytes().to_vec(), ScriptType::JavaScript)
    }
    
    /// Create from external source
    pub fn external(id: ScriptBlobId, url: &str) -> Self {
        let mut blob = Self::new(id, Vec::new(), ScriptType::JavaScript);
        blob.source_url = Some(url.into());
        blob
    }
    
    /// Set content (when external script loads)
    pub fn set_content(&mut self, content: Vec<u8>) {
        self.content = content;
    }
    
    /// Get raw content
    pub fn content(&self) -> &[u8] {
        &self.content
    }
    
    /// Get content as string (if valid UTF-8)
    pub fn content_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.content).ok()
    }
    
    /// Content length
    pub fn len(&self) -> usize {
        self.content.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
    
    /// Check if external
    pub fn is_external(&self) -> bool {
        self.source_url.is_some()
    }
    
    /// Mark as parsed
    pub fn mark_parsed(&mut self) {
        self.parsed = true;
    }
    
    /// Was parsed
    pub fn was_parsed(&self) -> bool {
        self.parsed
    }
    
    /// Should execute immediately
    pub fn is_blocking(&self) -> bool {
        !self.defer && !self.is_async && self.script_type == ScriptType::JavaScript
    }
}

/// Script blob store
#[derive(Debug)]
pub struct ScriptBlobStore {
    /// All blobs
    blobs: HashMap<ScriptBlobId, ScriptBlob>,
    /// Next ID
    next_id: u32,
    /// Execution queue
    queue: Vec<ScriptBlobId>,
    /// Deferred scripts
    deferred: Vec<ScriptBlobId>,
    /// Async scripts ready
    async_ready: Vec<ScriptBlobId>,
    /// Statistics
    stats: ScriptStats,
}

/// Script statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct ScriptStats {
    pub scripts_registered: u64,
    pub bytes_stored: u64,
    pub scripts_parsed: u64,
    pub scripts_never_parsed: u64,
    pub external_scripts: u64,
    pub inline_scripts: u64,
}

impl ScriptStats {
    pub fn never_parsed_ratio(&self) -> f64 {
        if self.scripts_registered == 0 {
            0.0
        } else {
            self.scripts_never_parsed as f64 / self.scripts_registered as f64
        }
    }
}

impl Default for ScriptBlobStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptBlobStore {
    pub fn new() -> Self {
        Self {
            blobs: HashMap::new(),
            next_id: 0,
            queue: Vec::new(),
            deferred: Vec::new(),
            async_ready: Vec::new(),
            stats: ScriptStats::default(),
        }
    }
    
    /// Register an inline script
    pub fn register_inline(&mut self, content: &str, script_type: ScriptType) -> ScriptBlobId {
        let id = ScriptBlobId(self.next_id);
        self.next_id += 1;
        
        let mut blob = ScriptBlob::new(id, content.as_bytes().to_vec(), script_type);
        
        self.stats.scripts_registered += 1;
        self.stats.inline_scripts += 1;
        self.stats.bytes_stored += blob.len() as u64;
        
        self.blobs.insert(id, blob);
        
        // Add to execution queue if blocking
        if self.blobs.get(&id).map(|b| b.is_blocking()).unwrap_or(false) {
            self.queue.push(id);
        }
        
        id
    }
    
    /// Register an external script
    pub fn register_external(&mut self, url: &str, defer: bool, is_async: bool) -> ScriptBlobId {
        let id = ScriptBlobId(self.next_id);
        self.next_id += 1;
        
        let mut blob = ScriptBlob::external(id, url);
        blob.defer = defer;
        blob.is_async = is_async;
        
        self.stats.scripts_registered += 1;
        self.stats.external_scripts += 1;
        
        self.blobs.insert(id, blob);
        
        if defer {
            self.deferred.push(id);
        } else if !is_async {
            self.queue.push(id);
        }
        
        id
    }
    
    /// Set content for external script
    pub fn set_content(&mut self, id: ScriptBlobId, content: Vec<u8>) {
        if let Some(blob) = self.blobs.get_mut(&id) {
            self.stats.bytes_stored += content.len() as u64;
            blob.set_content(content);
            
            if blob.is_async {
                self.async_ready.push(id);
            }
        }
    }
    
    /// Get blob by ID
    pub fn get(&self, id: ScriptBlobId) -> Option<&ScriptBlob> {
        self.blobs.get(&id)
    }
    
    /// Get mutable blob
    pub fn get_mut(&mut self, id: ScriptBlobId) -> Option<&mut ScriptBlob> {
        self.blobs.get_mut(&id)
    }
    
    /// Get next script to execute
    pub fn next_to_execute(&mut self) -> Option<ScriptBlobId> {
        // First check main queue
        if let Some(id) = self.queue.pop() {
            if let Some(blob) = self.blobs.get(&id) {
                if !blob.is_external() || !blob.is_empty() {
                    return Some(id);
                }
                // External script not loaded yet - put back
                self.queue.push(id);
            }
        }
        
        // Check async ready
        self.async_ready.pop()
    }
    
    /// Get deferred scripts (call at DOMContentLoaded)
    pub fn get_deferred(&mut self) -> Vec<ScriptBlobId> {
        std::mem::take(&mut self.deferred)
    }
    
    /// Mark script as parsed/executed
    pub fn mark_executed(&mut self, id: ScriptBlobId) {
        if let Some(blob) = self.blobs.get_mut(&id) {
            blob.mark_parsed();
            self.stats.scripts_parsed += 1;
        }
    }
    
    /// Calculate never-parsed scripts
    pub fn finalize(&mut self) {
        for blob in self.blobs.values() {
            if !blob.was_parsed() {
                self.stats.scripts_never_parsed += 1;
            }
        }
    }
    
    /// Get statistics
    pub fn stats(&self) -> &ScriptStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_script_type_detection() {
        assert_eq!(ScriptType::from_type_attr(None), ScriptType::JavaScript);
        assert_eq!(ScriptType::from_type_attr(Some("module")), ScriptType::Module);
        assert_eq!(ScriptType::from_type_attr(Some("application/json")), ScriptType::Json);
    }
    
    #[test]
    fn test_inline_script() {
        let mut store = ScriptBlobStore::new();
        
        let id = store.register_inline("console.log('hello')", ScriptType::JavaScript);
        
        let blob = store.get(id).unwrap();
        assert!(!blob.is_external());
        assert!(blob.is_blocking());
        assert_eq!(blob.content_str(), Some("console.log('hello')"));
    }
    
    #[test]
    fn test_external_script() {
        let mut store = ScriptBlobStore::new();
        
        let id = store.register_external("https://example.com/app.js", false, false);
        
        let blob = store.get(id).unwrap();
        assert!(blob.is_external());
        assert!(blob.is_empty());
        
        // Load content
        store.set_content(id, b"console.log('loaded')".to_vec());
        
        let blob = store.get(id).unwrap();
        assert!(!blob.is_empty());
    }
    
    #[test]
    fn test_deferred_scripts() {
        let mut store = ScriptBlobStore::new();
        
        store.register_external("a.js", true, false);
        store.register_external("b.js", true, false);
        
        let deferred = store.get_deferred();
        assert_eq!(deferred.len(), 2);
    }
}
