//! fOS Networking
//!
//! HTTP client and resource loading.

mod loader;
mod cache;

pub use loader::ResourceLoader;
pub use url::Url;

/// Fetch a URL
pub async fn fetch(url: &str) -> Result<Response, NetError> {
    ResourceLoader::new().fetch(url).await
}

/// HTTP Response
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Network error
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("HTTP error: {status}")]
    HttpError { status: u16 },
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}
