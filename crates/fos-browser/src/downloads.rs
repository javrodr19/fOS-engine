//! File download manager
//!
//! Handles file downloads with progress tracking.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

/// Download state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

/// A file download
#[derive(Debug, Clone)]
pub struct Download {
    pub id: u64,
    pub url: String,
    pub filename: String,
    pub save_path: PathBuf,
    pub state: DownloadState,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub mime_type: String,
    pub error: Option<String>,
}

impl Download {
    /// Progress as percentage (0-100)
    pub fn progress(&self) -> f32 {
        match self.total_bytes {
            Some(total) if total > 0 => (self.bytes_downloaded as f32 / total as f32) * 100.0,
            _ => 0.0,
        }
    }
    
    /// Human readable size
    pub fn size_text(&self) -> String {
        let bytes = self.total_bytes.unwrap_or(self.bytes_downloaded);
        format_bytes(bytes)
    }
    
    /// Progress text
    pub fn progress_text(&self) -> String {
        match self.total_bytes {
            Some(total) => format!("{} / {}", format_bytes(self.bytes_downloaded), format_bytes(total)),
            None => format_bytes(self.bytes_downloaded),
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Download manager
pub struct DownloadManager {
    downloads: Arc<Mutex<HashMap<u64, Download>>>,
    next_id: u64,
    download_dir: PathBuf,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadManager {
    pub fn new() -> Self {
        // Default to ~/Downloads
        let download_dir = dirs::download_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        
        Self {
            downloads: Arc::new(Mutex::new(HashMap::new())),
            next_id: 1,
            download_dir,
        }
    }
    
    /// Set download directory
    pub fn set_download_dir(&mut self, path: PathBuf) {
        self.download_dir = path;
    }
    
    /// Start a new download
    pub fn start(&mut self, url: &str, suggested_filename: Option<&str>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        // Extract filename from URL or use suggested
        let filename = suggested_filename
            .map(String::from)
            .or_else(|| Self::filename_from_url(url))
            .unwrap_or_else(|| format!("download_{}", id));
        
        let save_path = self.download_dir.join(&filename);
        
        let download = Download {
            id,
            url: url.to_string(),
            filename,
            save_path: save_path.clone(),
            state: DownloadState::Pending,
            bytes_downloaded: 0,
            total_bytes: None,
            mime_type: String::new(),
            error: None,
        };
        
        {
            let mut downloads = self.downloads.lock().unwrap();
            downloads.insert(id, download);
        }
        
        // Start download in background thread
        let downloads = Arc::clone(&self.downloads);
        let url = url.to_string();
        
        thread::spawn(move || {
            Self::download_file(downloads, id, &url, save_path);
        });
        
        id
    }
    
    fn filename_from_url(url: &str) -> Option<String> {
        let parsed = url::Url::parse(url).ok()?;
        let path = parsed.path();
        let filename = path.rsplit('/').next()?;
        if filename.is_empty() {
            None
        } else {
            Some(urlencoding::decode(filename).unwrap_or_else(|_| filename.into()).to_string())
        }
    }
    
    fn download_file(downloads: Arc<Mutex<HashMap<u64, Download>>>, id: u64, url: &str, save_path: PathBuf) {
        // Update state to in progress
        {
            let mut dl = downloads.lock().unwrap();
            if let Some(d) = dl.get_mut(&id) {
                d.state = DownloadState::InProgress;
            }
        }
        
        // Perform download (blocking)
        let result = reqwest::blocking::Client::new()
            .get(url)
            .send();
        
        match result {
            Ok(response) => {
                // Get content length
                let total = response.content_length();
                
                // Get content type
                let mime = response.headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                
                {
                    let mut dl = downloads.lock().unwrap();
                    if let Some(d) = dl.get_mut(&id) {
                        d.total_bytes = total;
                        d.mime_type = mime;
                    }
                }
                
                // Create parent directory if needed
                if let Some(parent) = save_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                
                // Write to file
                match File::create(&save_path) {
                    Ok(mut file) => {
                        let bytes = match response.bytes() {
                            Ok(b) => b,
                            Err(e) => {
                                let mut dl = downloads.lock().unwrap();
                                if let Some(d) = dl.get_mut(&id) {
                                    d.state = DownloadState::Failed;
                                    d.error = Some(e.to_string());
                                }
                                return;
                            }
                        };
                        
                        if let Err(e) = file.write_all(&bytes) {
                            let mut dl = downloads.lock().unwrap();
                            if let Some(d) = dl.get_mut(&id) {
                                d.state = DownloadState::Failed;
                                d.error = Some(e.to_string());
                            }
                            return;
                        }
                        
                        // Success
                        let mut dl = downloads.lock().unwrap();
                        if let Some(d) = dl.get_mut(&id) {
                            d.bytes_downloaded = bytes.len() as u64;
                            d.state = DownloadState::Completed;
                        }
                    }
                    Err(e) => {
                        let mut dl = downloads.lock().unwrap();
                        if let Some(d) = dl.get_mut(&id) {
                            d.state = DownloadState::Failed;
                            d.error = Some(e.to_string());
                        }
                    }
                }
            }
            Err(e) => {
                let mut dl = downloads.lock().unwrap();
                if let Some(d) = dl.get_mut(&id) {
                    d.state = DownloadState::Failed;
                    d.error = Some(e.to_string());
                }
            }
        }
    }
    
    /// Cancel a download
    pub fn cancel(&self, id: u64) {
        let mut dl = self.downloads.lock().unwrap();
        if let Some(d) = dl.get_mut(&id) {
            if d.state == DownloadState::InProgress || d.state == DownloadState::Pending {
                d.state = DownloadState::Cancelled;
            }
        }
    }
    
    /// Get download by ID
    pub fn get(&self, id: u64) -> Option<Download> {
        self.downloads.lock().unwrap().get(&id).cloned()
    }
    
    /// Get all downloads
    pub fn all(&self) -> Vec<Download> {
        self.downloads.lock().unwrap().values().cloned().collect()
    }
    
    /// Get active downloads
    pub fn active(&self) -> Vec<Download> {
        self.downloads.lock().unwrap()
            .values()
            .filter(|d| d.state == DownloadState::InProgress || d.state == DownloadState::Pending)
            .cloned()
            .collect()
    }
    
    /// Clear completed/failed downloads
    pub fn clear_finished(&self) {
        let mut dl = self.downloads.lock().unwrap();
        dl.retain(|_, d| {
            d.state == DownloadState::InProgress || d.state == DownloadState::Pending
        });
    }
}

// Simple URL decoding
mod urlencoding {
    use std::borrow::Cow;
    
    pub fn decode(input: &str) -> Result<Cow<str>, ()> {
        let mut result = String::new();
        let mut chars = input.chars().peekable();
        let mut has_changes = false;
        
        while let Some(c) = chars.next() {
            if c == '%' {
                has_changes = true;
                let hex: String = chars.by_ref().take(2).collect();
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                }
            } else if c == '+' {
                has_changes = true;
                result.push(' ');
            } else {
                result.push(c);
            }
        }
        
        if has_changes {
            Ok(Cow::Owned(result))
        } else {
            Ok(Cow::Borrowed(input))
        }
    }
}

// Placeholder for dirs crate functionality
mod dirs {
    use std::path::PathBuf;
    
    pub fn download_dir() -> Option<PathBuf> {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join("Downloads"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1500), "1.5 KB");
        assert_eq!(format_bytes(1_500_000), "1.4 MB");
    }
    
    #[test]
    fn test_filename_from_url() {
        let url = "https://example.com/files/document.pdf";
        assert_eq!(DownloadManager::filename_from_url(url), Some("document.pdf".to_string()));
    }
}
