//! Page Representation
//!
//! A loaded web page with DOM, styles, layout, and JavaScript.

use std::sync::{Arc, Mutex};
use fos_dom::Document;
use crate::js_runtime::PageJsRuntime;

/// A loaded web page
pub struct Page {
    /// Page URL
    pub url: String,
    /// Page title
    pub title: Option<String>,
    /// HTML source (shared, e.g. with the tab's cache)
    pub html: Arc<str>,
    /// Parsed DOM document
    pub document: Option<Arc<Mutex<Document>>>,
    /// JavaScript runtime for this page
    pub js_runtime: Option<PageJsRuntime>,
    /// Rendered content (pixel buffer)
    pub rendered: Option<RenderedContent>,
    /// Scroll position
    pub scroll_x: f32,
    pub scroll_y: f32,
    /// Content dimensions
    pub content_width: f32,
    pub content_height: f32,
    /// Whether JavaScript has been initialized
    js_initialized: bool,
    /// Whether scripts have been executed
    scripts_executed: bool,
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("url", &self.url)
            .field("title", &self.title)
            .field("html_len", &self.html.len())
            .field("has_document", &self.document.is_some())
            .field("has_js_runtime", &self.js_runtime.is_some())
            .field("js_initialized", &self.js_initialized)
            .field("scripts_executed", &self.scripts_executed)
            .finish()
    }
}

/// Rendered page content
#[derive(Debug)]
pub struct RenderedContent {
    /// Pixel buffer (ARGB)
    pub pixels: Vec<u32>,
    /// Width
    pub width: u32,
    /// Height
    pub height: u32,
}

impl Page {
    /// Create a new empty page
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            title: None,
            html: Arc::from(""),
            document: None,
            js_runtime: Some(PageJsRuntime::new(url)),
            rendered: None,
            scroll_x: 0.0,
            scroll_y: 0.0,
            content_width: 0.0,
            content_height: 0.0,
            js_initialized: false,
            scripts_executed: false,
        }
    }
    
    /// Create from HTML content, parsing it into the page's DOM. The DOM
    /// is the page's single parsed form: scripts and rendering share it.
    pub fn from_html(url: &str, html: impl Into<Arc<str>>) -> Self {
        let mut page = Self::new(url);
        page.html = html.into();

        let document = fos_html::parse_with_url(&page.html, url);
        let title = document.title();
        page.title = (!title.is_empty()).then_some(title);
        page.document = Some(Arc::new(Mutex::new(document)));

        page
    }

    /// Initialize JavaScript runtime and execute scripts
    pub fn initialize_javascript(&mut self) -> Result<(), String> {
        if self.js_initialized {
            return Ok(());
        }
        
        let Some(ref document) = self.document else {
            return Err("No document loaded".to_string());
        };
        
        let Some(ref mut js_runtime) = self.js_runtime else {
            return Err("No JavaScript runtime".to_string());
        };
        
        // Initialize JS context with document
        js_runtime.initialize(document.clone())
            .map_err(|e| format!("Failed to initialize JavaScript: {}", e))?;
        
        // Extract scripts from DOM
        {
            let doc = document.lock().unwrap();
            js_runtime.extract_scripts(&doc);
        }
        
        self.js_initialized = true;
        log::info!("JavaScript initialized for {}", self.url);
        
        Ok(())
    }
    
    /// Execute pending inline scripts
    pub fn execute_scripts(&mut self) -> Result<(), String> {
        if self.scripts_executed {
            return Ok(());
        }
        
        if !self.js_initialized {
            self.initialize_javascript()?;
        }
        
        let Some(ref mut js_runtime) = self.js_runtime else {
            return Ok(());
        };
        
        js_runtime.execute_inline_scripts()
            .map_err(|e| format!("Script execution error: {}", e))?;
        
        self.scripts_executed = true;
        log::info!("Inline scripts executed for {}", self.url);
        
        Ok(())
    }
    
    /// Process JavaScript timers (call periodically)
    pub fn process_timers(&mut self) -> Result<(), String> {
        let Some(ref js_runtime) = self.js_runtime else {
            return Ok(());
        };
        
        js_runtime.process_timers()
            .map_err(|e| format!("Timer error: {}", e))
    }
    
    /// Check if there are pending timers
    pub fn has_pending_timers(&self) -> bool {
        self.js_runtime.as_ref().map(|r| r.has_pending_timers()).unwrap_or(false)
    }
    
    /// Get pending external script URLs
    pub fn pending_external_scripts(&self) -> Vec<String> {
        self.js_runtime.as_ref()
            .map(|r| r.pending_external_scripts())
            .unwrap_or_default()
    }
    
    /// Execute an external script after fetching
    pub fn execute_external_script(&mut self, url: &str, source: &str) -> Result<(), String> {
        let Some(ref mut js_runtime) = self.js_runtime else {
            return Ok(());
        };
        
        js_runtime.execute_external_script(url, source)
            .map_err(|e| format!("External script error: {}", e))
    }
    
    /// Scroll by delta
    pub fn scroll(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        self.scroll_x = (self.scroll_x + dx).max(0.0);
        self.scroll_y = (self.scroll_y + dy)
            .max(0.0)
            .min((self.content_height - viewport_height).max(0.0));
    }
    
    /// Set scroll position
    pub fn set_scroll(&mut self, x: f32, y: f32) {
        self.scroll_x = x.max(0.0);
        self.scroll_y = y.max(0.0);
    }
    
    /// Get the document (if parsed)
    pub fn document(&self) -> Option<Arc<Mutex<Document>>> {
        self.document.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_title_whitespace_is_collapsed() {
        let page = Page::from_html("https://example.com", "<title>\n  Two\t words  </title><p>x</p>");
        assert_eq!(page.title.as_deref(), Some("Two words"));
        assert_eq!(Page::from_html("https://example.com", "<p>untitled</p>").title, None);
    }

    #[test]
    fn test_page_from_html() {
        let html = r#"<!DOCTYPE html>
            <html>
            <head><title>Hello</title></head>
            <body><p>Content</p></body>
            </html>"#;
        
        let page = Page::from_html("https://example.com", html);
        assert_eq!(page.title, Some("Hello".to_string()));
        assert_eq!(&*page.html, html);
        assert!(page.document.is_some());
        assert!(page.js_runtime.is_some());
    }
}
