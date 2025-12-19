//! Page - Represents a loaded web page

use fos_dom::Document;
use fos_render::Canvas;

/// A loaded web page
pub struct Page {
    pub url: String,
    pub document: Document,
}

impl Page {
    /// Create a new page
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            document: Document::new(url),
        }
    }
    
    /// Render the page to a pixel buffer
    pub fn render(&self, width: u32, height: u32) -> Canvas {
        tracing::info!("Rendering page {}x{}", width, height);
        
        // TODO: Implement full rendering pipeline
        // 1. Layout the document
        // 2. Paint to canvas
        
        Canvas::new(width, height)
    }
    
    /// Get the page title
    pub fn title(&self) -> &str {
        &self.document.title
    }
}
