//! Location API
//!
//! Implements window.location object using custom URL parser.

use crate::{JsValue, JsError};
use crate::engine_trait::JsContextApi;
use fos_dom::url::{Url, ParseError};
use std::sync::{Arc, Mutex};

/// Location state
pub struct LocationManager {
    url: Url,
}

impl LocationManager {
    pub fn new(url_str: &str) -> Result<Self, ParseError> {
        let url = Url::parse(url_str)?;
        Ok(Self { url })
    }
    
    pub fn from_parts(protocol: &str, host: &str, pathname: &str) -> Self {
        let url_str = format!("{}://{}{}", protocol.trim_end_matches(':'), host, pathname);
        Self {
            url: Url::parse(&url_str).unwrap_or_else(|_| Url::parse("about:blank").unwrap()),
        }
    }
    
    /// Full URL
    pub fn href(&self) -> String {
        self.url.to_string()
    }
    
    /// Set href (navigate)
    pub fn set_href(&mut self, url: &str) -> Result<(), ParseError> {
        self.url = Url::parse(url)?;
        Ok(())
    }
    
    /// Protocol (e.g., "https:")
    pub fn protocol(&self) -> String {
        format!("{}:", self.url.scheme())
    }
    
    /// Host (hostname:port)
    pub fn host(&self) -> String {
        self.url.host_with_port()
    }
    
    /// Hostname only
    pub fn hostname(&self) -> String {
        self.url.host_str().unwrap_or("").to_string()
    }
    
    /// Port
    pub fn port(&self) -> String {
        self.url.port().map(|p| p.to_string()).unwrap_or_default()
    }
    
    /// Pathname
    pub fn pathname(&self) -> &str {
        self.url.path()
    }
    
    /// Search/query string (including ?)
    pub fn search(&self) -> String {
        self.url.query_params()
            .map(|q| format!("?{}", q.to_string()))
            .unwrap_or_default()
    }
    
    /// Hash/fragment (including #)
    pub fn hash(&self) -> String {
        self.url.fragment()
            .map(|f| format!("#{}", f))
            .unwrap_or_default()
    }
    
    /// Origin
    pub fn origin(&self) -> String {
        self.url.origin()
    }
}

/// Install location API into global
pub fn install_location<C: JsContextApi>(ctx: &C, location: Arc<Mutex<LocationManager>>) -> Result<(), JsError> {
    let obj = ctx.create_object()?;
    
    // getHref
    let l = location.clone();
    ctx.set_function(&obj, "getHref", move |_args| {
        Ok(JsValue::String(l.lock().unwrap().href()))
    })?;
    
    // getProtocol
    let l = location.clone();
    ctx.set_function(&obj, "getProtocol", move |_args| {
        Ok(JsValue::String(l.lock().unwrap().protocol()))
    })?;
    
    // getHost
    let l = location.clone();
    ctx.set_function(&obj, "getHost", move |_args| {
        Ok(JsValue::String(l.lock().unwrap().host()))
    })?;
    
    // getHostname
    let l = location.clone();
    ctx.set_function(&obj, "getHostname", move |_args| {
        Ok(JsValue::String(l.lock().unwrap().hostname()))
    })?;
    
    // getPort
    let l = location.clone();
    ctx.set_function(&obj, "getPort", move |_args| {
        Ok(JsValue::String(l.lock().unwrap().port()))
    })?;
    
    // getPathname
    let l = location.clone();
    ctx.set_function(&obj, "getPathname", move |_args| {
        Ok(JsValue::String(l.lock().unwrap().pathname().to_string()))
    })?;
    
    // getSearch
    let l = location.clone();
    ctx.set_function(&obj, "getSearch", move |_args| {
        Ok(JsValue::String(l.lock().unwrap().search()))
    })?;
    
    // getHash
    let l = location.clone();
    ctx.set_function(&obj, "getHash", move |_args| {
        Ok(JsValue::String(l.lock().unwrap().hash()))
    })?;
    
    // getOrigin
    let l = location.clone();
    ctx.set_function(&obj, "getOrigin", move |_args| {
        Ok(JsValue::String(l.lock().unwrap().origin()))
    })?;
    
    // assign
    let l = location.clone();
    ctx.set_function(&obj, "assign", move |args| {
        if let Some(url) = args.first().and_then(|v| v.as_string()) {
            let _ = l.lock().unwrap().set_href(url);
        }
        Ok(JsValue::Undefined)
    })?;
    
    // replace
    let l = location.clone();
    ctx.set_function(&obj, "replace", move |args| {
        if let Some(url) = args.first().and_then(|v| v.as_string()) {
            let _ = l.lock().unwrap().set_href(url);
        }
        Ok(JsValue::Undefined)
    })?;
    
    // reload
    ctx.set_function(&obj, "reload", |_args| {
        tracing::info!("location.reload() called");
        Ok(JsValue::Undefined)
    })?;
    
    ctx.set_global("location", JsValue::Object)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_location_parts() {
        let loc = LocationManager::new("https://example.com:8080/path/to/page?query=1#section").unwrap();
        
        assert_eq!(loc.protocol(), "https:");
        assert_eq!(loc.host(), "example.com:8080");
        assert_eq!(loc.hostname(), "example.com");
        assert_eq!(loc.port(), "8080");
        assert_eq!(loc.pathname(), "/path/to/page");
        assert_eq!(loc.search(), "?query=1");
        assert_eq!(loc.hash(), "#section");
    }
    
    #[test]
    fn test_location_simple() {
        let loc = LocationManager::new("https://example.com/").unwrap();
        
        assert_eq!(loc.protocol(), "https:");
        assert_eq!(loc.hostname(), "example.com");
        assert_eq!(loc.port(), "");
        assert_eq!(loc.pathname(), "/");
    }
    
    #[test]
    fn test_location_set_href() {
        let mut loc = LocationManager::new("https://example.com/").unwrap();
        loc.set_href("https://other.com/page").unwrap();
        
        assert_eq!(loc.hostname(), "other.com");
        assert_eq!(loc.pathname(), "/page");
    }
}
