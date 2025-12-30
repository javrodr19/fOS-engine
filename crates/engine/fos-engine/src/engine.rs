//! Engine - Main entry point

use crate::{Config, Page};

/// The fOS browser engine
pub struct Engine {
    config: Config,
}

impl Engine {
    /// Create a new engine with the given configuration
    pub fn new(config: Config) -> Self {
        tracing::info!("fOS Engine {} initialized", crate::VERSION);
        Self { config }
    }
    
    /// Load a URL and return a Page
    pub async fn load_url(&self, url: &str) -> Result<Page, EngineError> {
        tracing::info!("Loading: {}", url);
        
        // TODO: Implement full loading pipeline
        // 1. Fetch HTML from network
        // 2. Parse HTML into DOM
        // 3. Fetch and parse CSS
        // 4. Compute styles
        // 5. Execute JavaScript
        // 6. Ready for layout/render
        
        Ok(Page::new(url))
    }
    
    /// Get engine configuration
    pub fn config(&self) -> &Config {
        &self.config
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

/// Engine error
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Network error: {0}")]
    Network(#[from] fos_net::NetError),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("JavaScript error: {0}")]
    JavaScript(#[from] fos_js::JsError),
}
