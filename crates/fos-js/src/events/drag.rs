//! Drag and Drop Events
//!
//! HTML5 drag and drop API implementation.

/// Drag event
#[derive(Debug, Clone)]
pub struct DragEvent {
    pub event_type: DragEventType,
    pub data_transfer: DataTransfer,
    
    // Coordinates
    pub client_x: f64,
    pub client_y: f64,
    pub page_x: f64,
    pub page_y: f64,
    
    // Modifiers
    pub shift_key: bool,
    pub ctrl_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
    
    // Event state
    pub bubbles: bool,
    pub cancelable: bool,
    default_prevented: bool,
    pub timestamp: f64,
}

/// Drag event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragEventType {
    DragStart,
    Drag,
    DragEnd,
    DragEnter,
    DragOver,
    DragLeave,
    Drop,
}

/// Data transfer for drag operations
#[derive(Debug, Clone, Default)]
pub struct DataTransfer {
    /// Drop effect (none, copy, link, move)
    pub drop_effect: DropEffect,
    /// Allowed effects
    pub effect_allowed: EffectAllowed,
    /// Transferred items
    items: Vec<DataTransferItem>,
    /// Files being dragged
    pub files: Vec<FileInfo>,
}

/// Drop effect
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DropEffect {
    #[default]
    None,
    Copy,
    Link,
    Move,
}

/// Allowed effects
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EffectAllowed {
    None,
    Copy,
    CopyLink,
    CopyMove,
    Link,
    LinkMove,
    Move,
    #[default]
    All,
    Uninitialized,
}

/// Single transfer item
#[derive(Debug, Clone)]
pub struct DataTransferItem {
    pub kind: DataTransferKind,
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataTransferKind {
    String,
    File,
}

/// File info for drag operations
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub mime_type: String,
    pub last_modified: u64,
}

impl DataTransfer {
    /// Create empty data transfer
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set data for a type
    pub fn set_data(&mut self, mime_type: &str, data: &str) {
        self.items.retain(|i| i.mime_type != mime_type);
        self.items.push(DataTransferItem {
            kind: DataTransferKind::String,
            mime_type: mime_type.to_string(),
            data: data.to_string(),
        });
    }
    
    /// Get data for a type
    pub fn get_data(&self, mime_type: &str) -> Option<&str> {
        self.items.iter()
            .find(|i| i.mime_type == mime_type)
            .map(|i| i.data.as_str())
    }
    
    /// Clear data
    pub fn clear_data(&mut self, mime_type: Option<&str>) {
        if let Some(mt) = mime_type {
            self.items.retain(|i| i.mime_type != mt);
        } else {
            self.items.clear();
        }
    }
    
    /// Get types
    pub fn types(&self) -> Vec<&str> {
        self.items.iter().map(|i| i.mime_type.as_str()).collect()
    }
}

impl Default for DragEvent {
    fn default() -> Self {
        Self {
            event_type: DragEventType::Drag,
            data_transfer: DataTransfer::new(),
            client_x: 0.0,
            client_y: 0.0,
            page_x: 0.0,
            page_y: 0.0,
            shift_key: false,
            ctrl_key: false,
            alt_key: false,
            meta_key: false,
            bubbles: true,
            cancelable: true,
            default_prevented: false,
            timestamp: 0.0,
        }
    }
}

impl DragEvent {
    /// Create a drag start event
    pub fn drag_start(x: f64, y: f64) -> Self {
        Self {
            event_type: DragEventType::DragStart,
            client_x: x,
            client_y: y,
            page_x: x,
            page_y: y,
            ..Default::default()
        }
    }
    
    /// Create a drop event
    pub fn drop(x: f64, y: f64, data: DataTransfer) -> Self {
        Self {
            event_type: DragEventType::Drop,
            data_transfer: data,
            client_x: x,
            client_y: y,
            page_x: x,
            page_y: y,
            ..Default::default()
        }
    }
    
    /// Prevent default
    pub fn prevent_default(&mut self) {
        self.default_prevented = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_data_transfer() {
        let mut dt = DataTransfer::new();
        dt.set_data("text/plain", "Hello");
        dt.set_data("text/html", "<b>Hello</b>");
        
        assert_eq!(dt.get_data("text/plain"), Some("Hello"));
        assert_eq!(dt.types().len(), 2);
    }
    
    #[test]
    fn test_drag_event() {
        let event = DragEvent::drag_start(100.0, 200.0);
        assert_eq!(event.event_type, DragEventType::DragStart);
    }
}
