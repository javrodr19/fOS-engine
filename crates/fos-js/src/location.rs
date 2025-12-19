//! Location API
//!
//! Implements window.location object.

use rquickjs::{Ctx, Function, Object, Value};
use std::sync::{Arc, Mutex};
use url::Url;

/// Location state
pub struct LocationManager {
    url: Url,
}

impl LocationManager {
    pub fn new(url_str: &str) -> Result<Self, url::ParseError> {
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
    pub fn href(&self) -> &str {
        self.url.as_str()
    }
    
    /// Set href (navigate)
    pub fn set_href(&mut self, url: &str) -> Result<(), url::ParseError> {
        self.url = Url::parse(url)?;
        Ok(())
    }
    
    /// Protocol (e.g., "https:")
    pub fn protocol(&self) -> String {
        format!("{}:", self.url.scheme())
    }
    
    /// Host (hostname:port)
    pub fn host(&self) -> String {
        match self.url.port() {
            Some(port) => format!("{}:{}", self.url.host_str().unwrap_or(""), port),
            None => self.url.host_str().unwrap_or("").to_string(),
        }
    }
    
    /// Hostname only
    pub fn hostname(&self) -> &str {
        self.url.host_str().unwrap_or("")
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
        self.url.query().map(|q| format!("?{}", q)).unwrap_or_default()
    }
    
    /// Hash/fragment (including #)
    pub fn hash(&self) -> String {
        self.url.fragment().map(|f| format!("#{}", f)).unwrap_or_default()
    }
    
    /// Origin
    pub fn origin(&self) -> String {
        self.url.origin().ascii_serialization()
    }
}

/// Install location API into global
pub fn install_location(ctx: &Ctx, location: Arc<Mutex<LocationManager>>) -> Result<(), rquickjs::Error> {
    let globals = ctx.globals();
    let obj = Object::new(ctx.clone())?;
    
    // href getter
    let l = location.clone();
    obj.set("getHref", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<String, rquickjs::Error> {
        Ok(l.lock().unwrap().href().to_string())
    })?)?;
    
    // protocol
    let l = location.clone();
    obj.set("getProtocol", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<String, rquickjs::Error> {
        Ok(l.lock().unwrap().protocol())
    })?)?;
    
    // host
    let l = location.clone();
    obj.set("getHost", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<String, rquickjs::Error> {
        Ok(l.lock().unwrap().host())
    })?)?;
    
    // hostname
    let l = location.clone();
    obj.set("getHostname", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<String, rquickjs::Error> {
        Ok(l.lock().unwrap().hostname().to_string())
    })?)?;
    
    // pathname
    let l = location.clone();
    obj.set("getPathname", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<String, rquickjs::Error> {
        Ok(l.lock().unwrap().pathname().to_string())
    })?)?;
    
    // search
    let l = location.clone();
    obj.set("getSearch", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<String, rquickjs::Error> {
        Ok(l.lock().unwrap().search())
    })?)?;
    
    // hash
    let l = location.clone();
    obj.set("getHash", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<String, rquickjs::Error> {
        Ok(l.lock().unwrap().hash())
    })?)?;
    
    // origin
    let l = location.clone();
    obj.set("getOrigin", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<String, rquickjs::Error> {
        Ok(l.lock().unwrap().origin())
    })?)?;
    
    // assign (navigate)
    let l = location.clone();
    obj.set("assign", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        if let Some(url) = args.first().and_then(|v| v.as_string()) {
            let url = url.to_string().unwrap_or_default();
            let _ = l.lock().unwrap().set_href(&url);
        }
        Ok(())
    })?)?;
    
    // replace (navigate without history)
    let l = location.clone();
    obj.set("replace", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        if let Some(url) = args.first().and_then(|v| v.as_string()) {
            let url = url.to_string().unwrap_or_default();
            let _ = l.lock().unwrap().set_href(&url);
        }
        Ok(())
    })?)?;
    
    // reload
    obj.set("reload", Function::new(ctx.clone(), |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        // Would trigger page reload
        tracing::info!("location.reload() called");
        Ok(())
    })?)?;
    
    globals.set("location", obj)?;
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
