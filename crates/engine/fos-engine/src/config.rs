//! Engine Configuration

/// Engine configuration options
#[derive(Debug, Clone)]
pub struct Config {
    /// Enable JavaScript execution
    pub enable_javascript: bool,
    
    /// Enable image loading
    pub enable_images: bool,
    
    /// User agent string
    pub user_agent: String,
    
    /// Maximum memory per page (bytes)
    pub max_memory: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enable_javascript: true,
            enable_images: true,
            user_agent: format!("fOS-Engine/{}", crate::VERSION),
            max_memory: 100 * 1024 * 1024, // 100MB
        }
    }
}
