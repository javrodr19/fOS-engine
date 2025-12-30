//! Drag and Drop API
//!
//! HTML5 drag and drop functionality.

use std::collections::HashMap;

/// Drag operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DragEffectAllowed {
    None,
    Copy,
    CopyLink,
    CopyMove,
    Link,
    LinkMove,
    Move,
    All,
    #[default]
    Uninitialized,
}

/// Current drop effect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DropEffect {
    #[default]
    None,
    Copy,
    Link,
    Move,
}

/// Data transfer object
#[derive(Debug, Clone, Default)]
pub struct DataTransfer {
    /// Data by MIME type
    data: HashMap<String, String>,
    /// Files being dragged
    files: Vec<DragFile>,
    /// Allowed effect
    pub effect_allowed: DragEffectAllowed,
    /// Drop effect
    pub drop_effect: DropEffect,
}

impl DataTransfer {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            files: Vec::new(),
            effect_allowed: DragEffectAllowed::Uninitialized,
            drop_effect: DropEffect::None,
        }
    }
    
    /// Set data for a type
    pub fn set_data(&mut self, format: &str, data: &str) {
        self.data.insert(format.to_string(), data.to_string());
    }
    
    /// Get data for a type
    pub fn get_data(&self, format: &str) -> String {
        self.data.get(format).cloned().unwrap_or_default()
    }
    
    /// Clear all data
    pub fn clear_data(&mut self) {
        self.data.clear();
    }
    
    /// Get available types
    pub fn types(&self) -> Vec<&str> {
        self.data.keys().map(|s| s.as_str()).collect()
    }
    
    /// Add a file
    pub fn add_file(&mut self, file: DragFile) {
        self.files.push(file);
    }
    
    /// Get files
    pub fn files(&self) -> &[DragFile] {
        &self.files
    }
}

/// File in drag operation
#[derive(Debug, Clone)]
pub struct DragFile {
    pub name: String,
    pub size: u64,
    pub mime_type: String,
    pub path: Option<String>,
}

/// Drag event
#[derive(Debug, Clone)]
pub struct DragEvent {
    pub event_type: DragEventType,
    pub x: f32,
    pub y: f32,
    pub data_transfer: DataTransfer,
}

/// Drag event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragEventType {
    DragStart,
    Drag,
    DragEnter,
    DragOver,
    DragLeave,
    Drop,
    DragEnd,
}

/// Drag and drop manager
#[derive(Debug)]
pub struct DragDropManager {
    /// Current active drag operation
    active_drag: Option<ActiveDrag>,
    /// Drop targets (element IDs)
    drop_targets: Vec<u64>,
}

/// Active drag state
#[derive(Debug)]
struct ActiveDrag {
    source_element: u64,
    data_transfer: DataTransfer,
    start_x: f32,
    start_y: f32,
}

impl Default for DragDropManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DragDropManager {
    pub fn new() -> Self {
        Self {
            active_drag: None,
            drop_targets: Vec::new(),
        }
    }
    
    /// Start a drag operation
    pub fn start_drag(&mut self, element_id: u64, x: f32, y: f32, data: DataTransfer) {
        self.active_drag = Some(ActiveDrag {
            source_element: element_id,
            data_transfer: data,
            start_x: x,
            start_y: y,
        });
    }
    
    /// Update drag position
    pub fn drag(&mut self, x: f32, y: f32) -> Option<DragEvent> {
        self.active_drag.as_ref().map(|drag| DragEvent {
            event_type: DragEventType::Drag,
            x,
            y,
            data_transfer: drag.data_transfer.clone(),
        })
    }
    
    /// Enter a drop target
    pub fn enter_target(&mut self, target_id: u64, x: f32, y: f32) -> Option<DragEvent> {
        if !self.drop_targets.contains(&target_id) {
            self.drop_targets.push(target_id);
        }
        
        self.active_drag.as_ref().map(|drag| DragEvent {
            event_type: DragEventType::DragEnter,
            x,
            y,
            data_transfer: drag.data_transfer.clone(),
        })
    }
    
    /// Leave a drop target
    pub fn leave_target(&mut self, target_id: u64, x: f32, y: f32) -> Option<DragEvent> {
        self.drop_targets.retain(|&id| id != target_id);
        
        self.active_drag.as_ref().map(|drag| DragEvent {
            event_type: DragEventType::DragLeave,
            x,
            y,
            data_transfer: drag.data_transfer.clone(),
        })
    }
    
    /// Drop on a target
    pub fn drop(&mut self, x: f32, y: f32) -> Option<DragEvent> {
        let drag = self.active_drag.take()?;
        self.drop_targets.clear();
        
        Some(DragEvent {
            event_type: DragEventType::Drop,
            x,
            y,
            data_transfer: drag.data_transfer,
        })
    }
    
    /// Cancel drag operation
    pub fn cancel(&mut self) {
        self.active_drag = None;
        self.drop_targets.clear();
    }
    
    /// Check if drag is active
    pub fn is_dragging(&self) -> bool {
        self.active_drag.is_some()
    }
    
    /// Get current data transfer
    pub fn get_data_transfer(&self) -> Option<&DataTransfer> {
        self.active_drag.as_ref().map(|d| &d.data_transfer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_drag_drop() {
        let mut mgr = DragDropManager::new();
        
        let mut data = DataTransfer::new();
        data.set_data("text/plain", "Hello, World!");
        
        mgr.start_drag(1, 100.0, 100.0, data);
        assert!(mgr.is_dragging());
        
        let event = mgr.drop(200.0, 200.0).unwrap();
        assert_eq!(event.event_type, DragEventType::Drop);
        assert_eq!(event.data_transfer.get_data("text/plain"), "Hello, World!");
    }
}
