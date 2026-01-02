//! Service Worker Integration
//!
//! Offline support, background sync, and fetch interception via Service Workers.
//!
//! ## Lifecycle
//! 1. **Install**: SW downloads and caches assets
//! 2. **Activate**: SW takes control, cleans old caches
//! 3. **Fetch**: SW intercepts network requests
//!
//! ## Events
//! - `install`: Cache assets for offline use
//! - `activate`: Clean up old caches, claim clients
//! - `fetch`: Intercept and serve cached/network responses
//! - `push`: Handle push notifications
//! - `sync`: Background sync when online

use std::collections::HashMap;

/// Service worker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceWorkerState {
    Parsed,
    Installing,
    Installed,
    Activating,
    Activated,
    Redundant,
}

/// A registered service worker
#[derive(Debug, Clone)]
pub struct ServiceWorker {
    pub id: u64,
    pub scope: String,
    pub script_url: String,
    pub state: ServiceWorkerState,
}

impl ServiceWorker {
    /// Transition to next state
    pub fn advance_state(&mut self) {
        self.state = match self.state {
            ServiceWorkerState::Parsed => ServiceWorkerState::Installing,
            ServiceWorkerState::Installing => ServiceWorkerState::Installed,
            ServiceWorkerState::Installed => ServiceWorkerState::Activating,
            ServiceWorkerState::Activating => ServiceWorkerState::Activated,
            _ => self.state,
        };
    }
}

/// Service worker registration
#[derive(Debug)]
pub struct ServiceWorkerRegistration {
    pub scope: String,
    pub installing: Option<ServiceWorker>,
    pub waiting: Option<ServiceWorker>,
    pub active: Option<ServiceWorker>,
}

impl ServiceWorkerRegistration {
    /// Update registration state (move installing -> waiting -> active)
    pub fn update(&mut self) {
        // Move installed worker to waiting
        if let Some(ref sw) = self.installing {
            if sw.state == ServiceWorkerState::Installed {
                self.waiting = self.installing.take();
            }
        }

        // Move activated worker to active
        if let Some(ref sw) = self.waiting {
            if sw.state == ServiceWorkerState::Activated {
                self.active = self.waiting.take();
            }
        }
    }
}

/// Service worker events
#[derive(Debug, Clone)]
pub enum ServiceWorkerEvent {
    /// Install event - cache assets
    Install { worker_id: u64 },
    /// Activate event - claim clients, cleanup
    Activate { worker_id: u64 },
    /// Fetch event - intercept request
    Fetch { worker_id: u64, request: FetchRequest },
    /// Push notification received
    Push { worker_id: u64, data: Vec<u8> },
    /// Background sync
    Sync { worker_id: u64, tag: String },
    /// Message from client
    Message { worker_id: u64, data: String },
}

/// Fetch request for interception
#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub mode: RequestMode,
    pub destination: RequestDestination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestMode {
    Navigate,
    SameOrigin,
    Cors,
    NoCors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestDestination {
    Document,
    Script,
    Style,
    Image,
    Font,
    Fetch,
    Worker,
    Unknown,
}

/// Fetch response from service worker
#[derive(Debug, Clone)]
pub enum FetchResponse {
    /// Use network response
    Network,
    /// Serve from cache
    Cache(CachedResponse),
    /// Generate custom response
    Synthetic { status: u16, body: Vec<u8>, headers: HashMap<String, String> },
    /// Failed to respond
    Error(String),
}

/// Service worker container (navigator.serviceWorker)
#[derive(Debug, Default)]
pub struct ServiceWorkerContainer {
    registrations: HashMap<String, ServiceWorkerRegistration>,
    next_id: u64,
    /// Pending events
    event_queue: Vec<ServiceWorkerEvent>,
}

impl ServiceWorkerContainer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a service worker
    pub fn register(&mut self, script_url: &str, scope: Option<&str>) -> Result<u64, ServiceWorkerError> {
        let scope = scope
            .map(String::from)
            .unwrap_or_else(|| Self::default_scope(script_url));

        let id = self.next_id;
        self.next_id += 1;

        let worker = ServiceWorker {
            id,
            scope: scope.clone(),
            script_url: script_url.to_string(),
            state: ServiceWorkerState::Parsed,
        };

        let registration = ServiceWorkerRegistration {
            scope: scope.clone(),
            installing: Some(worker),
            waiting: None,
            active: None,
        };

        self.registrations.insert(scope, registration);

        // Queue install event
        self.event_queue.push(ServiceWorkerEvent::Install { worker_id: id });

        Ok(id)
    }

    /// Get registration for a URL
    pub fn get_registration(&self, url: &str) -> Option<&ServiceWorkerRegistration> {
        let mut best_match: Option<&ServiceWorkerRegistration> = None;
        let mut best_len = 0;

        for reg in self.registrations.values() {
            if url.starts_with(&reg.scope) && reg.scope.len() > best_len {
                best_match = Some(reg);
                best_len = reg.scope.len();
            }
        }

        best_match
    }

    /// Get active worker for URL
    pub fn get_controller(&self, url: &str) -> Option<&ServiceWorker> {
        self.get_registration(url)
            .and_then(|reg| reg.active.as_ref())
    }

    /// Process lifecycle events
    pub fn process_events(&mut self) -> Vec<ServiceWorkerEvent> {
        std::mem::take(&mut self.event_queue)
    }

    /// Handle install completion
    pub fn on_install_complete(&mut self, worker_id: u64) {
        for reg in self.registrations.values_mut() {
            if let Some(ref mut sw) = reg.installing {
                if sw.id == worker_id {
                    sw.state = ServiceWorkerState::Installed;
                    reg.update();
                    // Queue activate
                    self.event_queue.push(ServiceWorkerEvent::Activate { worker_id });
                }
            }
        }
    }

    /// Handle activate completion
    pub fn on_activate_complete(&mut self, worker_id: u64) {
        for reg in self.registrations.values_mut() {
            if let Some(ref mut sw) = reg.waiting {
                if sw.id == worker_id {
                    sw.state = ServiceWorkerState::Activated;
                    reg.update();
                }
            }
        }
    }

    /// Intercept fetch request
    pub fn intercept_fetch(&mut self, url: &str, method: &str) -> Option<ServiceWorkerEvent> {
        if let Some(reg) = self.get_registration(url) {
            if let Some(ref sw) = reg.active {
                return Some(ServiceWorkerEvent::Fetch {
                    worker_id: sw.id,
                    request: FetchRequest {
                        url: url.to_string(),
                        method: method.to_string(),
                        headers: HashMap::new(),
                        mode: RequestMode::Navigate,
                        destination: RequestDestination::Document,
                    },
                });
            }
        }
        None
    }

    /// Unregister a service worker
    pub fn unregister(&mut self, scope: &str) -> bool {
        self.registrations.remove(scope).is_some()
    }

    /// Get all registrations
    pub fn get_registrations(&self) -> Vec<&ServiceWorkerRegistration> {
        self.registrations.values().collect()
    }

    fn default_scope(script_url: &str) -> String {
        if let Some(pos) = script_url.rfind('/') {
            script_url[..=pos].to_string()
        } else {
            "/".to_string()
        }
    }
}

/// Cache storage for service workers
#[derive(Debug, Default)]
pub struct CacheStorage {
    caches: HashMap<String, Cache>,
}

impl CacheStorage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open or create a cache
    pub fn open(&mut self, name: &str) -> &mut Cache {
        self.caches.entry(name.to_string()).or_insert_with(Cache::new)
    }

    /// Delete a cache
    pub fn delete(&mut self, name: &str) -> bool {
        self.caches.remove(name).is_some()
    }

    /// Check if cache exists
    pub fn has(&self, name: &str) -> bool {
        self.caches.contains_key(name)
    }

    /// Get all cache names
    pub fn keys(&self) -> Vec<&str> {
        self.caches.keys().map(|s| s.as_str()).collect()
    }

    /// Match request across all caches
    pub fn match_all(&self, url: &str) -> Option<&CachedResponse> {
        for cache in self.caches.values() {
            if let Some(resp) = cache.match_url(url) {
                return Some(resp);
            }
        }
        None
    }
}

/// A cache for storing request/response pairs
#[derive(Debug, Default)]
pub struct Cache {
    entries: HashMap<String, CachedResponse>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a response to the cache
    pub fn put(&mut self, url: &str, response: CachedResponse) {
        self.entries.insert(url.to_string(), response);
    }

    /// Add multiple URLs to cache
    pub fn add_all(&mut self, urls: &[&str], responses: Vec<CachedResponse>) {
        for (url, response) in urls.iter().zip(responses) {
            self.entries.insert(url.to_string(), response);
        }
    }

    /// Get a cached response
    pub fn match_url(&self, url: &str) -> Option<&CachedResponse> {
        self.entries.get(url)
    }

    /// Delete a cached response
    pub fn delete(&mut self, url: &str) -> bool {
        self.entries.remove(url).is_some()
    }

    /// Get all cached URLs
    pub fn keys(&self) -> Vec<&str> {
        self.entries.keys().map(|s| s.as_str()).collect()
    }
}

/// A cached response
#[derive(Debug, Clone)]
pub struct CachedResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl CachedResponse {
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            body,
        }
    }

    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }
}

/// Service worker errors
#[derive(Debug)]
pub enum ServiceWorkerError {
    SecurityError(String),
    NetworkError(String),
    NotFound,
    InvalidScope,
}

impl std::fmt::Display for ServiceWorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SecurityError(msg) => write!(f, "Security error: {}", msg),
            Self::NetworkError(msg) => write!(f, "Network error: {}", msg),
            Self::NotFound => write!(f, "Service worker not found"),
            Self::InvalidScope => write!(f, "Invalid scope"),
        }
    }
}

impl std::error::Error for ServiceWorkerError {}

/// Service worker manager - coordinates all service worker functionality
#[derive(Debug, Default)]
pub struct ServiceWorkerManager {
    container: ServiceWorkerContainer,
    cache_storage: CacheStorage,
}

impl ServiceWorkerManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn container(&self) -> &ServiceWorkerContainer {
        &self.container
    }

    pub fn container_mut(&mut self) -> &mut ServiceWorkerContainer {
        &mut self.container
    }

    pub fn cache_storage(&self) -> &CacheStorage {
        &self.cache_storage
    }

    pub fn cache_storage_mut(&mut self) -> &mut CacheStorage {
        &mut self.cache_storage
    }

    /// Register a service worker
    pub fn register(&mut self, script_url: &str, scope: Option<&str>) -> Result<u64, ServiceWorkerError> {
        self.container.register(script_url, scope)
    }

    /// Process pending lifecycle events
    pub fn tick(&mut self) -> Vec<ServiceWorkerEvent> {
        self.container.process_events()
    }

    /// Handle install complete
    pub fn complete_install(&mut self, worker_id: u64) {
        self.container.on_install_complete(worker_id);
    }

    /// Handle activate complete
    pub fn complete_activate(&mut self, worker_id: u64) {
        self.container.on_activate_complete(worker_id);
    }

    /// Intercept a fetch request
    pub fn intercept(&mut self, url: &str, method: &str) -> Option<FetchResponse> {
        // Check cache first
        if let Some(cached) = self.cache_storage.match_all(url) {
            return Some(FetchResponse::Cache(cached.clone()));
        }
        None
    }

    /// Check if URL can be served offline
    pub fn can_serve_offline(&self, url: &str) -> bool {
        self.cache_storage.match_all(url).is_some()
    }

    /// Get cached response for URL
    pub fn get_cached(&self, url: &str) -> Option<&CachedResponse> {
        self.cache_storage.match_all(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_worker_registration() {
        let mut container = ServiceWorkerContainer::new();
        let id = container.register("/sw.js", Some("/app/")).unwrap();

        assert!(container.get_registration("/app/page.html").is_some());
        assert!(container.get_registration("/other/page.html").is_none());
    }

    #[test]
    fn test_service_worker_lifecycle() {
        let mut mgr = ServiceWorkerManager::new();
        let id = mgr.register("/sw.js", Some("/")).unwrap();

        // Process install event
        let events = mgr.tick();
        assert!(events.iter().any(|e| matches!(e, ServiceWorkerEvent::Install { .. })));

        // Complete install
        mgr.complete_install(id);

        // Process activate event
        let events = mgr.tick();
        assert!(events.iter().any(|e| matches!(e, ServiceWorkerEvent::Activate { .. })));

        // Complete activate
        mgr.complete_activate(id);

        // Should now have active controller
        assert!(mgr.container().get_controller("/index.html").is_some());
    }

    #[test]
    fn test_cache_storage() {
        let mut storage = CacheStorage::new();
        let cache = storage.open("v1");

        cache.put("/index.html", CachedResponse::new(200, b"<html>".to_vec()));
        assert!(cache.match_url("/index.html").is_some());

        // Match all
        assert!(storage.match_all("/index.html").is_some());
    }

    #[test]
    fn test_fetch_interception() {
        let mut mgr = ServiceWorkerManager::new();

        // Cache a response
        mgr.cache_storage_mut()
            .open("v1")
            .put("/app.js", CachedResponse::new(200, b"// js".to_vec()));

        // Should intercept
        let resp = mgr.intercept("/app.js", "GET");
        assert!(matches!(resp, Some(FetchResponse::Cache(_))));

        // Unknown URL
        let resp = mgr.intercept("/unknown.js", "GET");
        assert!(resp.is_none());
    }
}

