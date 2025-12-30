//! Page Loader
//!
//! Fetches and processes web pages.

use crate::page::Page;
use std::error::Error;

/// Page loader
pub struct Loader {
    /// User agent string
    user_agent: String,
}

impl Loader {
    /// Create a new loader
    pub fn new() -> Self {
        Self {
            user_agent: format!(
                "fOS-Browser/0.1 (compatible; fOS Engine; +https://github.com/fosproject)"
            ),
        }
    }
    
    /// Load a page from URL (blocking)
    pub fn load_sync(&self, url: &str) -> Result<Page, Box<dyn Error>> {
        // Handle about: URLs
        if url.starts_with("about:") {
            return Ok(self.load_about_page(url));
        }
        
        // Fetch HTML
        let html = self.fetch_html(url)?;
        
        // Create page
        let page = Page::from_html(url, html);
        
        Ok(page)
    }
    
    /// Fetch HTML content
    fn fetch_html(&self, url: &str) -> Result<String, Box<dyn Error>> {
        // Use custom blocking client from fos-net
        let mut client = fos_net::client::blocking::Client::new();
        
        let response = client.get(url)?;
        
        if !response.is_success() {
            return Err(format!("HTTP error: {}", response.status).into());
        }
        
        let html = response.text().unwrap_or_default();
        Ok(html)
    }
    
    /// Load an about: page
    fn load_about_page(&self, url: &str) -> Page {
        let html = match url {
            "about:blank" => r#"
                <!DOCTYPE html>
                <html>
                <head><title>New Tab</title></head>
                <body style="background: #0d0d0d; color: #e0e0e0; font-family: sans-serif;">
                </body>
                </html>
            "#.to_string(),
            
            "about:version" => format!(r#"
                <!DOCTYPE html>
                <html>
                <head><title>fOS Browser</title></head>
                <body style="background: #0d0d0d; color: #e0e0e0; font-family: sans-serif; padding: 20px;">
                    <h1>fOS Browser</h1>
                    <p>Version: 0.1.0</p>
                    <p>Engine: fOS Engine</p>
                    <p>Built with Rust</p>
                </body>
                </html>
            "#),
            
            _ => format!(r#"
                <!DOCTYPE html>
                <html>
                <head><title>Unknown Page</title></head>
                <body style="background: #0d0d0d; color: #e0e0e0; font-family: sans-serif; padding: 20px;">
                    <h1>Unknown about: page</h1>
                    <p>The page <code>{}</code> was not found.</p>
                </body>
                </html>
            "#, url),
        };
        
        Page::from_html(url, html)
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_about_blank() {
        let loader = Loader::new();
        let page = loader.load_about_page("about:blank");
        
        assert_eq!(page.title, Some("New Tab".to_string()));
    }
    
    #[test]
    fn test_about_version() {
        let loader = Loader::new();
        let page = loader.load_about_page("about:version");
        
        assert_eq!(page.title, Some("fOS Browser".to_string()));
    }
}
