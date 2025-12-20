//! Page Representation
//!
//! A loaded web page with DOM, styles, and layout.

/// A loaded web page
#[derive(Debug)]
pub struct Page {
    /// Page URL
    pub url: String,
    /// Page title
    pub title: Option<String>,
    /// HTML source
    pub html: String,
    /// Rendered content (pixel buffer)
    pub rendered: Option<RenderedContent>,
    /// Scroll position
    pub scroll_x: f32,
    pub scroll_y: f32,
    /// Content dimensions
    pub content_width: f32,
    pub content_height: f32,
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
            html: String::new(),
            rendered: None,
            scroll_x: 0.0,
            scroll_y: 0.0,
            content_width: 0.0,
            content_height: 0.0,
        }
    }
    
    /// Create from HTML content
    pub fn from_html(url: &str, html: String) -> Self {
        let mut page = Self::new(url);
        page.html = html;
        
        // Extract title from HTML (simple extraction)
        page.title = extract_title(&page.html);
        
        page
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
}

/// Extract title from HTML (simple regex-free extraction)
fn extract_title(html: &str) -> Option<String> {
    let html_lower = html.to_lowercase();
    
    let start = html_lower.find("<title>")?;
    let end = html_lower.find("</title>")?;
    
    if end > start + 7 {
        let title = &html[start + 7..end];
        Some(title.trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_title() {
        let html = r#"<!DOCTYPE html>
            <html>
            <head><title>Test Page</title></head>
            <body></body>
            </html>"#;
        
        assert_eq!(extract_title(html), Some("Test Page".to_string()));
    }
}
