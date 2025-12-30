//! Bookmarks storage
//!
//! Save and manage browser bookmarks.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// A bookmark
#[derive(Debug, Clone)]
pub struct Bookmark {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub folder_id: Option<u64>,
    pub created: u64,
    pub favicon: Option<Vec<u8>>,
}

/// A bookmark folder
#[derive(Debug, Clone)]
pub struct BookmarkFolder {
    pub id: u64,
    pub name: String,
    pub parent_id: Option<u64>,
}

/// Bookmark manager
#[derive(Debug)]
pub struct BookmarkManager {
    bookmarks: HashMap<u64, Bookmark>,
    folders: HashMap<u64, BookmarkFolder>,
    next_id: u64,
    storage_path: Option<PathBuf>,
}

impl Default for BookmarkManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BookmarkManager {
    pub fn new() -> Self {
        Self {
            bookmarks: HashMap::new(),
            folders: HashMap::new(),
            next_id: 1,
            storage_path: None,
        }
    }
    
    /// Create with persistence
    pub fn with_storage(path: PathBuf) -> Self {
        let mut mgr = Self::new();
        mgr.storage_path = Some(path);
        mgr.load();
        mgr
    }
    
    /// Add a bookmark
    pub fn add(&mut self, url: &str, title: &str, folder_id: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        let bookmark = Bookmark {
            id,
            url: url.to_string(),
            title: title.to_string(),
            folder_id,
            created: now,
            favicon: None,
        };
        
        self.bookmarks.insert(id, bookmark);
        id
    }
    
    /// Remove a bookmark
    pub fn remove(&mut self, id: u64) -> bool {
        self.bookmarks.remove(&id).is_some()
    }
    
    /// Create a folder
    pub fn create_folder(&mut self, name: &str, parent_id: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let folder = BookmarkFolder {
            id,
            name: name.to_string(),
            parent_id,
        };
        
        self.folders.insert(id, folder);
        id
    }
    
    /// Get all bookmarks in a folder (None for root)
    pub fn get_in_folder(&self, folder_id: Option<u64>) -> Vec<&Bookmark> {
        self.bookmarks.values()
            .filter(|b| b.folder_id == folder_id)
            .collect()
    }
    
    /// Get all folders in a parent (None for root)
    pub fn get_folders_in(&self, parent_id: Option<u64>) -> Vec<&BookmarkFolder> {
        self.folders.values()
            .filter(|f| f.parent_id == parent_id)
            .collect()
    }
    
    /// Check if URL is bookmarked
    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.bookmarks.values().any(|b| b.url == url)
    }
    
    /// Find bookmark by URL
    pub fn find_by_url(&self, url: &str) -> Option<&Bookmark> {
        self.bookmarks.values().find(|b| b.url == url)
    }
    
    /// Toggle bookmark (add if not exists, remove if exists)
    pub fn toggle(&mut self, url: &str, title: &str) -> Option<u64> {
        if let Some(bookmark) = self.bookmarks.values().find(|b| b.url == url).cloned() {
            self.remove(bookmark.id);
            None
        } else {
            Some(self.add(url, title, None))
        }
    }
    
    /// Get all bookmarks
    pub fn all(&self) -> Vec<&Bookmark> {
        self.bookmarks.values().collect()
    }
    
    /// Search bookmarks
    pub fn search(&self, query: &str) -> Vec<&Bookmark> {
        let query = query.to_lowercase();
        self.bookmarks.values()
            .filter(|b| {
                b.title.to_lowercase().contains(&query) ||
                b.url.to_lowercase().contains(&query)
            })
            .collect()
    }
    
    /// Save to disk
    pub fn save(&self) {
        let Some(path) = &self.storage_path else { return };
        
        let mut data = String::new();
        
        // Save folders
        for folder in self.folders.values() {
            data.push_str(&format!(
                "F\t{}\t{}\t{}\n",
                folder.id,
                folder.name,
                folder.parent_id.map(|i| i.to_string()).unwrap_or_default()
            ));
        }
        
        // Save bookmarks
        for bookmark in self.bookmarks.values() {
            data.push_str(&format!(
                "B\t{}\t{}\t{}\t{}\t{}\n",
                bookmark.id,
                bookmark.url,
                bookmark.title.replace('\t', " "),
                bookmark.folder_id.map(|i| i.to_string()).unwrap_or_default(),
                bookmark.created
            ));
        }
        
        let _ = fs::write(path, data);
    }
    
    /// Load from disk
    pub fn load(&mut self) {
        let Some(path) = &self.storage_path else { return };
        
        let data = match fs::read_to_string(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        
        for line in data.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.is_empty() {
                continue;
            }
            
            match parts[0] {
                "F" if parts.len() >= 3 => {
                    let id: u64 = parts[1].parse().unwrap_or(0);
                    let name = parts[2].to_string();
                    let parent_id = parts.get(3).and_then(|s| s.parse().ok());
                    
                    self.folders.insert(id, BookmarkFolder { id, name, parent_id });
                    self.next_id = self.next_id.max(id + 1);
                }
                "B" if parts.len() >= 5 => {
                    let id: u64 = parts[1].parse().unwrap_or(0);
                    let url = parts[2].to_string();
                    let title = parts[3].to_string();
                    let folder_id = parts.get(4).and_then(|s| s.parse().ok());
                    let created = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
                    
                    self.bookmarks.insert(id, Bookmark {
                        id, url, title, folder_id, created, favicon: None
                    });
                    self.next_id = self.next_id.max(id + 1);
                }
                _ => {}
            }
        }
    }
    
    /// Count bookmarks
    pub fn len(&self) -> usize {
        self.bookmarks.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.bookmarks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bookmarks() {
        let mut mgr = BookmarkManager::new();
        
        let id = mgr.add("https://rust-lang.org", "Rust", None);
        assert!(mgr.is_bookmarked("https://rust-lang.org"));
        
        mgr.remove(id);
        assert!(!mgr.is_bookmarked("https://rust-lang.org"));
    }
    
    #[test]
    fn test_toggle() {
        let mut mgr = BookmarkManager::new();
        
        // Add
        let id = mgr.toggle("https://example.com", "Example");
        assert!(id.is_some());
        assert!(mgr.is_bookmarked("https://example.com"));
        
        // Remove
        let id = mgr.toggle("https://example.com", "Example");
        assert!(id.is_none());
        assert!(!mgr.is_bookmarked("https://example.com"));
    }
}
