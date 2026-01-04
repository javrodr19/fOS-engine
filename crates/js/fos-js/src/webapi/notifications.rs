//! Notifications API
//!
//! System notifications and permissions with platform backend support.

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};

/// Platform backend for notifications
pub trait NotificationBackend: Send + Sync {
    /// Request permission from user
    fn request_permission(&self) -> NotificationPermission;
    
    /// Get current permission state
    fn get_permission(&self) -> NotificationPermission;
    
    /// Show a notification, returns notification ID
    fn show(&self, id: u32, title: &str, options: &NotificationOptions);
    
    /// Close a notification
    fn close(&self, id: u32);
}

/// Simulated backend for testing
#[derive(Debug)]
pub struct SimulatedNotificationBackend {
    permission: Mutex<NotificationPermission>,
    shown_notifications: Mutex<Vec<u32>>,
}

impl Default for SimulatedNotificationBackend {
    fn default() -> Self {
        Self {
            permission: Mutex::new(NotificationPermission::Default),
            shown_notifications: Mutex::new(Vec::new()),
        }
    }
}

impl SimulatedNotificationBackend {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set permission for testing
    pub fn set_permission(&self, perm: NotificationPermission) {
        *self.permission.lock().unwrap() = perm;
    }
    
    /// Grant permission (simulates user clicking "Allow")
    pub fn grant_permission(&self) {
        *self.permission.lock().unwrap() = NotificationPermission::Granted;
    }
    
    /// Get list of shown notification IDs (for testing)
    pub fn shown_ids(&self) -> Vec<u32> {
        self.shown_notifications.lock().unwrap().clone()
    }
}

impl NotificationBackend for SimulatedNotificationBackend {
    fn request_permission(&self) -> NotificationPermission {
        // In simulation, auto-grant if Default
        let mut perm = self.permission.lock().unwrap();
        if *perm == NotificationPermission::Default {
            *perm = NotificationPermission::Granted;
        }
        *perm
    }
    
    fn get_permission(&self) -> NotificationPermission {
        *self.permission.lock().unwrap()
    }
    
    fn show(&self, id: u32, _title: &str, _options: &NotificationOptions) {
        self.shown_notifications.lock().unwrap().push(id);
    }
    
    fn close(&self, id: u32) {
        self.shown_notifications.lock().unwrap().retain(|&i| i != id);
    }
}

// Global backend and ID counter
static NEXT_NOTIFICATION_ID: AtomicU32 = AtomicU32::new(1);

use std::sync::OnceLock;

static NOTIFICATION_BACKEND: OnceLock<Mutex<Arc<dyn NotificationBackend>>> = OnceLock::new();

fn get_backend() -> &'static Mutex<Arc<dyn NotificationBackend>> {
    NOTIFICATION_BACKEND.get_or_init(|| {
        Mutex::new(Arc::new(SimulatedNotificationBackend::default()))
    })
}

/// Set the global notification backend
pub fn set_backend(backend: Arc<dyn NotificationBackend>) {
    *get_backend().lock().unwrap() = backend;
}

/// Notification permission state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationPermission {
    Default,
    Granted,
    Denied,
}

/// Notification
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub title: String,
    pub options: NotificationOptions,
    pub permission: NotificationPermission,
}

/// Notification options
#[derive(Debug, Clone, Default)]
pub struct NotificationOptions {
    pub body: Option<String>,
    pub icon: Option<String>,
    pub badge: Option<String>,
    pub tag: Option<String>,
    pub data: Option<String>,
    pub require_interaction: bool,
    pub silent: bool,
    pub vibrate: Vec<u32>,
    pub actions: Vec<NotificationAction>,
}

/// Notification action
#[derive(Debug, Clone)]
pub struct NotificationAction {
    pub action: String,
    pub title: String,
    pub icon: Option<String>,
}

impl Notification {
    /// Request permission
    pub fn request_permission() -> NotificationPermission {
        get_backend().lock().unwrap().request_permission()
    }
    
    /// Get current permission
    pub fn permission() -> NotificationPermission {
        get_backend().lock().unwrap().get_permission()
    }
    
    /// Create and show a notification
    pub fn new(title: &str, options: NotificationOptions) -> Self {
        let perm = Self::permission();
        let id = NEXT_NOTIFICATION_ID.fetch_add(1, Ordering::SeqCst);
        
        let notif = Self {
            id,
            title: title.to_string(),
            options: options.clone(),
            permission: perm,
        };
        
        if perm == NotificationPermission::Granted {
            get_backend().lock().unwrap().show(id, title, &options);
        }
        
        notif
    }
    
    /// Close the notification
    pub fn close(&self) {
        get_backend().lock().unwrap().close(self.id);
    }
}

/// Vibration API
pub fn vibrate(pattern: &[u32]) -> bool {
    // Returns true if vibration is supported and pattern is valid
    !pattern.is_empty()
}

/// Cancel vibration
pub fn cancel_vibration() {
    // Would cancel via platform API
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_notification() {
        let notif = Notification::new("Hello", NotificationOptions::default());
        assert_eq!(notif.title, "Hello");
    }
    
    #[test]
    fn test_permission_flow() {
        let backend = Arc::new(SimulatedNotificationBackend::new());
        set_backend(backend.clone());
        
        // Initially default
        assert_eq!(Notification::permission(), NotificationPermission::Default);
        
        // Request permission (auto-grants in simulation)
        let perm = Notification::request_permission();
        assert_eq!(perm, NotificationPermission::Granted);
        
        // Create notification - should be shown
        let notif = Notification::new("Test", NotificationOptions::default());
        assert!(backend.shown_ids().contains(&notif.id));
        
        // Close notification
        notif.close();
        assert!(!backend.shown_ids().contains(&notif.id));
    }
}
