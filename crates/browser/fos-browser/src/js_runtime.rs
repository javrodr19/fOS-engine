//! JavaScript Runtime Integration
//!
//! Integrates fos-js into the browser for script execution.

use std::sync::{Arc, Mutex};
use fos_dom::{Document, DomTree, NodeId};
use fos_js::{JsContext, JsValue, JsError};
use fos_devtools::{Console, ConsoleMessage};

/// Script to execute
#[derive(Debug, Clone)]
pub struct Script {
    /// Script source code
    pub source: String,
    /// Source URL (for error reporting)
    pub source_url: Option<String>,
    /// Whether this is an external script
    pub is_external: bool,
    /// Script type (text/javascript, module, etc.)
    pub script_type: ScriptType,
}

/// Script type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptType {
    /// Classic JavaScript
    Classic,
    /// ES Module
    Module,
}

impl Default for ScriptType {
    fn default() -> Self {
        ScriptType::Classic
    }
}

/// JavaScript runtime for a page
pub struct PageJsRuntime {
    /// JavaScript context with DOM bindings
    context: Option<JsContext>,
    /// Pending scripts to execute
    pending_scripts: Vec<Script>,
    /// Console for log output
    console: Arc<Mutex<Console>>,
    /// Whether scripts are enabled
    scripts_enabled: bool,
    /// Page URL for security context
    page_url: String,
}

impl PageJsRuntime {
    /// Create a new JavaScript runtime for a page
    pub fn new(page_url: &str) -> Self {
        Self {
            context: None,
            pending_scripts: Vec::new(),
            console: Arc::new(Mutex::new(Console::new())),
            scripts_enabled: true,
            page_url: page_url.to_string(),
        }
    }
    
    /// Initialize the JavaScript context with the document
    pub fn initialize(&mut self, document: Arc<Mutex<Document>>) -> Result<(), JsError> {
        if !self.scripts_enabled {
            return Ok(());
        }
        
        let context = JsContext::with_url(document, &self.page_url)?;
        self.context = Some(context);
        
        log::info!("JavaScript context initialized for {}", self.page_url);
        Ok(())
    }
    
    /// Extract scripts from the document
    pub fn extract_scripts(&mut self, document: &Document) {
        let tree = document.tree();
        let mut scripts = Vec::new();
        
        // Walk the DOM to find all <script> tags
        self.collect_scripts(tree, tree.root(), &mut scripts);
        
        log::info!("Found {} scripts in document", scripts.len());
        self.pending_scripts = scripts;
    }
    
    /// Recursively collect scripts from DOM
    fn collect_scripts(&self, tree: &DomTree, node_id: NodeId, scripts: &mut Vec<Script>) {
        if !node_id.is_valid() {
            return;
        }
        
        if let Some(node) = tree.get(node_id) {
            if let Some(element) = node.as_element() {
                let tag = tree.resolve(element.name.local);
                
                if tag.eq_ignore_ascii_case("script") {
                    // Check script type
                    let mut script_type = ScriptType::Classic;
                    let mut src: Option<String> = None;
                    
                    for attr in element.attrs.iter() {
                        let attr_name = tree.resolve(attr.name.local);
                        match attr_name {
                            "type" => {
                                if attr.value.contains("module") {
                                    script_type = ScriptType::Module;
                                }
                            }
                            "src" => {
                                src = Some(attr.value.clone());
                            }
                            _ => {}
                        }
                    }
                    
                    if let Some(src_url) = src {
                        // External script - will need to fetch
                        scripts.push(Script {
                            source: String::new(), // To be fetched
                            source_url: Some(src_url),
                            is_external: true,
                            script_type,
                        });
                    } else {
                        // Inline script - collect text content
                        let mut source = String::new();
                        for (_, child) in tree.children(node_id) {
                            if let Some(text) = child.as_text() {
                                source.push_str(text);
                            }
                        }
                        
                        if !source.trim().is_empty() {
                            scripts.push(Script {
                                source,
                                source_url: None,
                                is_external: false,
                                script_type,
                            });
                        }
                    }
                }
            }
        }
        
        // Recurse into children
        for (child_id, _) in tree.children(node_id) {
            self.collect_scripts(tree, child_id, scripts);
        }
    }
    
    /// Execute all pending inline scripts (external scripts need fetching first)
    pub fn execute_inline_scripts(&mut self) -> Result<(), JsError> {
        let Some(ref context) = self.context else {
            return Ok(());
        };
        
        let scripts: Vec<_> = self.pending_scripts
            .iter()
            .filter(|s| !s.is_external && !s.source.is_empty())
            .cloned()
            .collect();
        
        for script in scripts {
            log::debug!("Executing inline script ({} bytes)", script.source.len());
            
            match context.exec(&script.source) {
                Ok(()) => {
                    log::debug!("Script executed successfully");
                }
                Err(e) => {
                    log::error!("Script error: {}", e);
                    self.console.lock().unwrap().error(
                        &format!("Script error: {}", e),
                        Vec::new(),
                    );
                }
            }
        }
        
        // Remove executed inline scripts
        self.pending_scripts.retain(|s| s.is_external);
        
        Ok(())
    }
    
    /// Execute an external script (after fetching its source)
    pub fn execute_external_script(&mut self, url: &str, source: &str) -> Result<(), JsError> {
        let Some(ref context) = self.context else {
            return Ok(());
        };
        
        log::debug!("Executing external script from {} ({} bytes)", url, source.len());
        
        match context.exec(source) {
            Ok(()) => {
                log::debug!("External script executed successfully");
            }
            Err(e) => {
                log::error!("External script error ({}): {}", url, e);
                self.console.lock().unwrap().error(
                    &format!("Script error ({}): {}", url, e),
                    Vec::new(),
                );
            }
        }
        
        // Remove from pending
        self.pending_scripts.retain(|s| s.source_url.as_deref() != Some(url));
        
        Ok(())
    }
    
    /// Execute arbitrary JavaScript code
    pub fn eval(&self, code: &str) -> Result<JsValue, JsError> {
        let Some(ref context) = self.context else {
            return Err(JsError::Runtime("No JavaScript context".to_string()));
        };
        
        context.eval(code)
    }
    
    /// Process pending timers (setTimeout, setInterval)
    pub fn process_timers(&self) -> Result<(), JsError> {
        let Some(ref context) = self.context else {
            return Ok(());
        };
        
        context.process_timers()
    }
    
    /// Check if there are pending timers
    pub fn has_pending_timers(&self) -> bool {
        self.context.as_ref().map(|c| c.has_pending_timers()).unwrap_or(false)
    }
    
    /// Get pending external script URLs
    pub fn pending_external_scripts(&self) -> Vec<String> {
        self.pending_scripts
            .iter()
            .filter(|s| s.is_external)
            .filter_map(|s| s.source_url.clone())
            .collect()
    }
    
    /// Get console messages
    pub fn console_messages(&self) -> Vec<ConsoleMessage> {
        self.console.lock().unwrap().get_messages().iter().cloned().collect()
    }
    
    /// Clear console
    pub fn clear_console(&self) {
        self.console.lock().unwrap().clear();
    }
    
    /// Enable/disable scripts
    pub fn set_scripts_enabled(&mut self, enabled: bool) {
        self.scripts_enabled = enabled;
    }
    
    /// Check if scripts are enabled
    pub fn scripts_enabled(&self) -> bool {
        self.scripts_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_runtime_creation() {
        let runtime = PageJsRuntime::new("https://example.com");
        assert!(runtime.scripts_enabled());
        assert!(runtime.pending_external_scripts().is_empty());
    }
    
    #[test]
    fn test_disable_scripts() {
        let mut runtime = PageJsRuntime::new("https://example.com");
        runtime.set_scripts_enabled(false);
        assert!(!runtime.scripts_enabled());
    }
}
