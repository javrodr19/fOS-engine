//! File Upload Handling
//!
//! Multiple file selection, directory upload, and drag-drop file handling.

use std::path::PathBuf;

/// File entry from file input
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub mime_type: String,
    pub last_modified: u64,
    pub path: Option<PathBuf>,
    pub content: Option<Vec<u8>>,
    pub relative_path: Option<String>,
}

impl FileEntry {
    pub fn new(name: &str, size: u64, mime_type: &str) -> Self {
        Self { name: name.into(), size, mime_type: mime_type.into(),
               last_modified: 0, path: None, content: None, relative_path: None }
    }
    
    pub fn extension(&self) -> Option<&str> {
        self.name.rsplit('.').next()
    }
}

/// File list from input
#[derive(Debug, Clone, Default)]
pub struct FileList {
    pub files: Vec<FileEntry>,
}

impl FileList {
    pub fn new() -> Self { Self::default() }
    pub fn add(&mut self, file: FileEntry) { self.files.push(file); }
    pub fn len(&self) -> usize { self.files.len() }
    pub fn is_empty(&self) -> bool { self.files.is_empty() }
    pub fn get(&self, index: usize) -> Option<&FileEntry> { self.files.get(index) }
    pub fn total_size(&self) -> u64 { self.files.iter().map(|f| f.size).sum() }
    pub fn iter(&self) -> impl Iterator<Item = &FileEntry> { self.files.iter() }
}

/// Accept attribute parser
#[derive(Debug, Clone, Default)]
pub struct AcceptFilter {
    pub extensions: Vec<String>,
    pub mime_types: Vec<String>,
    pub mime_wildcards: Vec<String>,
}

impl AcceptFilter {
    pub fn parse(accept: &str) -> Self {
        let mut filter = Self::default();
        for part in accept.split(',').map(|s| s.trim().to_lowercase()) {
            if part.starts_with('.') {
                filter.extensions.push(part[1..].to_string());
            } else if part.ends_with("/*") {
                filter.mime_wildcards.push(part[..part.len()-2].to_string());
            } else if part.contains('/') {
                filter.mime_types.push(part);
            }
        }
        filter
    }
    
    pub fn matches(&self, file: &FileEntry) -> bool {
        if self.extensions.is_empty() && self.mime_types.is_empty() && self.mime_wildcards.is_empty() {
            return true;
        }
        if let Some(ext) = file.extension() {
            if self.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)) { return true; }
        }
        if self.mime_types.iter().any(|m| m.eq_ignore_ascii_case(&file.mime_type)) { return true; }
        let mime_prefix = file.mime_type.split('/').next().unwrap_or("");
        self.mime_wildcards.iter().any(|w| w.eq_ignore_ascii_case(mime_prefix))
    }
}

/// File input configuration
#[derive(Debug, Clone, Default)]
pub struct FileInputConfig {
    pub multiple: bool,
    pub directory: bool,
    pub accept: Option<AcceptFilter>,
    pub capture: Option<CaptureMode>,
}

/// Capture mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode { User, Environment }

/// Upload progress
#[derive(Debug, Clone)]
pub struct UploadProgress {
    pub file_name: String,
    pub loaded: u64,
    pub total: u64,
}

impl UploadProgress {
    pub fn percentage(&self) -> f64 {
        if self.total == 0 { 0.0 } else { (self.loaded as f64 / self.total as f64) * 100.0 }
    }
}

/// Drop zone for drag-and-drop
#[derive(Debug, Default)]
pub struct DropZone {
    pub element_id: u64,
    pub active: bool,
    pub accept: Option<AcceptFilter>,
}

impl DropZone {
    pub fn new(element_id: u64) -> Self { Self { element_id, active: false, accept: None } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}

/// File upload manager
#[derive(Debug, Default)]
pub struct FileUploadManager {
    configs: std::collections::HashMap<u64, FileInputConfig>,
    drop_zones: std::collections::HashMap<u64, DropZone>,
    pending_files: std::collections::HashMap<u64, FileList>,
}

impl FileUploadManager {
    pub fn new() -> Self { Self::default() }
    
    pub fn register_input(&mut self, id: u64, config: FileInputConfig) {
        self.configs.insert(id, config);
    }
    
    pub fn register_drop_zone(&mut self, id: u64, accept: Option<AcceptFilter>) {
        self.drop_zones.insert(id, DropZone { element_id: id, active: false, accept });
    }
    
    pub fn handle_files(&mut self, input_id: u64, files: FileList) -> FileList {
        let config = self.configs.get(&input_id);
        let mut accepted = FileList::new();
        
        for file in files.files {
            let matches = config.as_ref()
                .and_then(|c| c.accept.as_ref())
                .map(|a| a.matches(&file))
                .unwrap_or(true);
            if matches { accepted.add(file); }
        }
        
        if let Some(config) = config {
            if !config.multiple && accepted.len() > 1 {
                accepted.files = accepted.files.into_iter().take(1).collect();
            }
        }
        
        self.pending_files.insert(input_id, accepted.clone());
        accepted
    }
    
    pub fn get_files(&self, input_id: u64) -> Option<&FileList> { self.pending_files.get(&input_id) }
    pub fn clear_files(&mut self, input_id: u64) { self.pending_files.remove(&input_id); }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_accept_filter() {
        let filter = AcceptFilter::parse(".jpg,.png,image/*");
        assert!(filter.extensions.contains(&"jpg".to_string()));
        assert!(filter.mime_wildcards.contains(&"image".to_string()));
    }
    
    #[test]
    fn test_file_filter() {
        let filter = AcceptFilter::parse("image/*");
        let jpg = FileEntry::new("test.jpg", 100, "image/jpeg");
        let txt = FileEntry::new("test.txt", 100, "text/plain");
        assert!(filter.matches(&jpg));
        assert!(!filter.matches(&txt));
    }
}
