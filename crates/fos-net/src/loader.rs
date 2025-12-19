//! Resource Loader

use crate::{Response, NetError};

/// Load resources from network
pub struct ResourceLoader;

impl ResourceLoader {
    pub fn new() -> Self {
        Self
    }
    
    /// Fetch a URL
    pub async fn fetch(&self, url: &str) -> Result<Response, NetError> {
        tracing::info!("Fetching: {}", url);
        
        // TODO: Implement using reqwest
        Err(NetError::Network("Not implemented".into()))
    }
}

impl Default for ResourceLoader {
    fn default() -> Self {
        Self::new()
    }
}
