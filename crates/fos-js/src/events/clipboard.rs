//! Clipboard Events
//!
//! Copy, cut, paste events and Clipboard API.

/// Clipboard event
#[derive(Debug, Clone)]
pub struct ClipboardEvent {
    pub event_type: ClipboardEventType,
    pub data: ClipboardData,
    
    // Event state
    pub bubbles: bool,
    pub cancelable: bool,
    default_prevented: bool,
    pub timestamp: f64,
}

/// Clipboard event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardEventType {
    Copy,
    Cut,
    Paste,
}

/// Clipboard data
#[derive(Debug, Clone, Default)]
pub struct ClipboardData {
    items: Vec<ClipboardItem>,
}

/// Single clipboard item
#[derive(Debug, Clone)]
pub struct ClipboardItem {
    pub mime_type: String,
    pub data: Vec<u8>,
}

impl ClipboardItem {
    /// Create a text item
    pub fn text(s: &str) -> Self {
        Self {
            mime_type: "text/plain".to_string(),
            data: s.as_bytes().to_vec(),
        }
    }
    
    /// Create an HTML item
    pub fn html(s: &str) -> Self {
        Self {
            mime_type: "text/html".to_string(),
            data: s.as_bytes().to_vec(),
        }
    }
    
    /// Get as string (if text)
    pub fn as_string(&self) -> Option<String> {
        if self.mime_type.starts_with("text/") {
            String::from_utf8(self.data.clone()).ok()
        } else {
            None
        }
    }
}

impl ClipboardData {
    /// Create empty clipboard data
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get data for mime type
    pub fn get_data(&self, mime_type: &str) -> Option<&ClipboardItem> {
        self.items.iter().find(|i| i.mime_type == mime_type)
    }
    
    /// Set data for mime type
    pub fn set_data(&mut self, item: ClipboardItem) {
        // Remove existing item with same type
        self.items.retain(|i| i.mime_type != item.mime_type);
        self.items.push(item);
    }
    
    /// Clear all data
    pub fn clear(&mut self) {
        self.items.clear();
    }
    
    /// Get plain text
    pub fn get_text(&self) -> Option<String> {
        self.get_data("text/plain")?.as_string()
    }
    
    /// Set plain text
    pub fn set_text(&mut self, text: &str) {
        self.set_data(ClipboardItem::text(text));
    }
    
    /// Get available types
    pub fn types(&self) -> Vec<&str> {
        self.items.iter().map(|i| i.mime_type.as_str()).collect()
    }
}

impl ClipboardEvent {
    /// Create a copy event
    pub fn copy() -> Self {
        Self {
            event_type: ClipboardEventType::Copy,
            data: ClipboardData::new(),
            bubbles: true,
            cancelable: true,
            default_prevented: false,
            timestamp: 0.0,
        }
    }
    
    /// Create a paste event with data
    pub fn paste(data: ClipboardData) -> Self {
        Self {
            event_type: ClipboardEventType::Paste,
            data,
            bubbles: true,
            cancelable: true,
            default_prevented: false,
            timestamp: 0.0,
        }
    }
    
    /// Prevent default
    pub fn prevent_default(&mut self) {
        self.default_prevented = true;
    }
}

/// System clipboard access
#[derive(Debug, Default)]
pub struct Clipboard {
    data: ClipboardData,
}

impl Clipboard {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Read text from clipboard
    pub fn read_text(&self) -> Option<String> {
        self.data.get_text()
    }
    
    /// Write text to clipboard
    pub fn write_text(&mut self, text: &str) {
        self.data.set_text(text);
    }
    
    /// Read clipboard data
    pub fn read(&self) -> &ClipboardData {
        &self.data
    }
    
    /// Write clipboard data
    pub fn write(&mut self, data: ClipboardData) {
        self.data = data;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_clipboard_text() {
        let mut clipboard = Clipboard::new();
        clipboard.write_text("Hello, World!");
        
        assert_eq!(clipboard.read_text(), Some("Hello, World!".to_string()));
    }
    
    #[test]
    fn test_clipboard_data() {
        let mut data = ClipboardData::new();
        data.set_data(ClipboardItem::text("plain text"));
        data.set_data(ClipboardItem::html("<b>rich</b>"));
        
        assert_eq!(data.types().len(), 2);
    }
}
