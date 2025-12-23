//! Web Notifications API
//!
//! Push notifications for the browser.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

static NOTIFICATION_ID: AtomicU64 = AtomicU64::new(1);

/// Notification permission state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationPermission {
    Default,
    Granted,
    Denied,
}

/// Notification options
#[derive(Debug, Clone)]
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

impl Default for NotificationOptions {
    fn default() -> Self {
        Self {
            body: None,
            icon: None,
            badge: None,
            tag: None,
            data: None,
            require_interaction: false,
            silent: false,
            vibrate: Vec::new(),
            actions: Vec::new(),
        }
    }
}

/// Notification action button
#[derive(Debug, Clone)]
pub struct NotificationAction {
    pub action: String,
    pub title: String,
    pub icon: Option<String>,
}

/// A notification instance
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u64,
    pub title: String,
    pub options: NotificationOptions,
    pub origin: String,
    pub timestamp: u64,
}

impl Notification {
    pub fn new(title: &str, options: NotificationOptions, origin: &str) -> Self {
        Self {
            id: NOTIFICATION_ID.fetch_add(1, Ordering::SeqCst),
            title: title.to_string(),
            options,
            origin: origin.to_string(),
            timestamp: Self::now(),
        }
    }
    
    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    
    /// Close the notification
    pub fn close(&self) {
        // In a real implementation, remove from system tray
        log::debug!("Closing notification {}", self.id);
    }
}

/// Notification manager
#[derive(Debug, Default)]
pub struct NotificationManager {
    /// Permission per origin
    permissions: HashMap<String, NotificationPermission>,
    /// Active notifications
    active: HashMap<u64, Notification>,
}

impl NotificationManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Request permission for an origin
    pub fn request_permission(&mut self, origin: &str) -> NotificationPermission {
        // In a real browser, show UI prompt
        // For now, auto-grant
        let perm = NotificationPermission::Granted;
        self.permissions.insert(origin.to_string(), perm);
        perm
    }
    
    /// Check permission for an origin
    pub fn get_permission(&self, origin: &str) -> NotificationPermission {
        self.permissions.get(origin).copied().unwrap_or(NotificationPermission::Default)
    }
    
    /// Show a notification
    pub fn show(&mut self, notification: Notification) -> Result<u64, NotificationError> {
        let perm = self.get_permission(&notification.origin);
        if perm != NotificationPermission::Granted {
            return Err(NotificationError::PermissionDenied);
        }
        
        // Check for tag replacement
        if let Some(ref tag) = notification.options.tag {
            self.active.retain(|_, n| {
                n.origin != notification.origin || n.options.tag.as_ref() != Some(tag)
            });
        }
        
        let id = notification.id;
        
        // Show via system notifications (platform-specific)
        #[cfg(target_os = "linux")]
        {
            self.show_linux(&notification);
        }
        
        self.active.insert(id, notification);
        Ok(id)
    }
    
    #[cfg(target_os = "linux")]
    fn show_linux(&self, notification: &Notification) {
        use std::process::Command;
        
        let mut cmd = Command::new("notify-send");
        cmd.arg(&notification.title);
        
        if let Some(ref body) = notification.options.body {
            cmd.arg(body);
        }
        
        if let Some(ref icon) = notification.options.icon {
            cmd.args(["-i", icon]);
        }
        
        let _ = cmd.spawn();
    }
    
    /// Close a notification
    pub fn close(&mut self, id: u64) -> bool {
        if let Some(notification) = self.active.remove(&id) {
            notification.close();
            true
        } else {
            false
        }
    }
    
    /// Get active notifications for origin
    pub fn get_notifications(&self, origin: &str) -> Vec<&Notification> {
        self.active.values()
            .filter(|n| n.origin == origin)
            .collect()
    }
}

/// Notification errors
#[derive(Debug)]
pub enum NotificationError {
    PermissionDenied,
    InvalidOptions(String),
}

impl std::fmt::Display for NotificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermissionDenied => write!(f, "Notification permission denied"),
            Self::InvalidOptions(msg) => write!(f, "Invalid options: {}", msg),
        }
    }
}

impl std::error::Error for NotificationError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_notifications() {
        let mut mgr = NotificationManager::new();
        
        // Should fail without permission
        let n = Notification::new("Test", NotificationOptions::default(), "https://example.com");
        assert!(mgr.show(n).is_err());
        
        // Grant permission
        mgr.request_permission("https://example.com");
        
        // Should succeed
        let n = Notification::new("Test", NotificationOptions::default(), "https://example.com");
        let id = mgr.show(n).unwrap();
        assert!(mgr.close(id));
    }
}
