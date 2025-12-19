//! Fetch API
//!
//! JavaScript-style fetch() implementation.

use crate::{Response, NetError, ResourceLoader, loader::Request, loader::Method};

/// Fetch options
#[derive(Debug, Default)]
pub struct FetchOptions {
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl FetchOptions {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn method(mut self, method: &str) -> Self {
        self.method = method.to_string();
        self
    }
    
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }
    
    pub fn body(mut self, body: &str) -> Self {
        self.body = Some(body.to_string());
        self
    }
}

/// Fetch a URL (async)
pub async fn fetch(url: &str) -> Result<FetchResponse, NetError> {
    fetch_with_options(url, FetchOptions::default()).await
}

/// Fetch with options
pub async fn fetch_with_options(url: &str, options: FetchOptions) -> Result<FetchResponse, NetError> {
    let loader = ResourceLoader::new();
    
    let method = match options.method.to_uppercase().as_str() {
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "DELETE" => Method::Delete,
        "HEAD" => Method::Head,
        "OPTIONS" => Method::Options,
        "PATCH" => Method::Patch,
        _ => Method::Get,
    };
    
    let mut req = Request {
        method,
        url: url.to_string(),
        headers: options.headers.into_iter().collect(),
        body: options.body.map(|b| b.into_bytes()),
    };
    
    let response = loader.request(req).await?;
    Ok(FetchResponse::from(response))
}

/// Fetch response with convenience methods
#[derive(Debug)]
pub struct FetchResponse {
    inner: Response,
}

impl FetchResponse {
    /// HTTP status code
    pub fn status(&self) -> u16 {
        self.inner.status
    }
    
    /// Check if response is OK (2xx)
    pub fn ok(&self) -> bool {
        self.inner.status >= 200 && self.inner.status < 300
    }
    
    /// Get header value
    pub fn header(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_lowercase();
        self.inner.headers.iter()
            .find(|(k, _)| k.to_lowercase() == name_lower)
            .map(|(_, v)| v.as_str())
    }
    
    /// Get all headers
    pub fn headers(&self) -> &[(String, String)] {
        &self.inner.headers
    }
    
    /// Get body as text
    pub fn text(&self) -> Result<String, NetError> {
        String::from_utf8(self.inner.body.clone())
            .map_err(|e| NetError::Network(e.to_string()))
    }
    
    /// Get body as JSON (requires serde)
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, NetError> {
        serde_json::from_slice(&self.inner.body)
            .map_err(|e| NetError::Network(e.to_string()))
    }
    
    /// Get raw body bytes
    pub fn bytes(&self) -> &[u8] {
        &self.inner.body
    }
}

impl From<Response> for FetchResponse {
    fn from(inner: Response) -> Self {
        Self { inner }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fetch_options() {
        let opts = FetchOptions::new()
            .method("POST")
            .header("Content-Type", "application/json")
            .body(r#"{"key": "value"}"#);
        
        assert_eq!(opts.method, "POST");
        assert!(opts.body.is_some());
    }
}
