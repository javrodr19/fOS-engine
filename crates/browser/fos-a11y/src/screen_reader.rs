//! Screen Reader Integration
//!
//! Virtual buffer and announcement queue for assistive technology.

use std::collections::VecDeque;

/// Screen reader announcement
#[derive(Debug, Clone)]
pub struct Announcement {
    pub text: String,
    pub priority: AnnouncePriority,
    pub interrupt: bool,
    pub timestamp: u64,
}

/// Announcement priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnnouncePriority { Low, Normal, High, Critical }

/// Live region politeness
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Politeness { Off, Polite, Assertive }

/// Announcement queue
#[derive(Debug, Default)]
pub struct AnnouncementQueue {
    queue: VecDeque<Announcement>,
    max_size: usize,
}

impl AnnouncementQueue {
    pub fn new(max_size: usize) -> Self { Self { queue: VecDeque::new(), max_size } }
    
    pub fn enqueue(&mut self, announcement: Announcement) {
        if announcement.interrupt { self.queue.clear(); }
        self.queue.push_back(announcement);
        while self.queue.len() > self.max_size { self.queue.pop_front(); }
    }
    
    pub fn dequeue(&mut self) -> Option<Announcement> { self.queue.pop_front() }
    pub fn peek(&self) -> Option<&Announcement> { self.queue.front() }
    pub fn clear(&mut self) { self.queue.clear(); }
    pub fn is_empty(&self) -> bool { self.queue.is_empty() }
}

/// Virtual buffer content
#[derive(Debug, Clone)]
pub struct VirtualBufferItem {
    pub node_id: u64,
    pub item_type: BufferItemType,
    pub text: String,
    pub level: Option<u8>,
    pub accessible_name: String,
    pub accessible_description: Option<String>,
    pub focusable: bool,
}

/// Buffer item type for rotor navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferItemType { Text, Heading, Link, Button, FormControl, Landmark, Image, List, ListItem, Table, TableCell }

/// Virtual buffer for non-visual navigation
#[derive(Debug, Default)]
pub struct VirtualBuffer {
    items: Vec<VirtualBufferItem>,
    cursor: usize,
    landmarks: Vec<usize>,
    headings: Vec<usize>,
    links: Vec<usize>,
    form_controls: Vec<usize>,
}

impl VirtualBuffer {
    pub fn new() -> Self { Self::default() }
    
    pub fn add_item(&mut self, item: VirtualBufferItem) {
        let idx = self.items.len();
        match item.item_type {
            BufferItemType::Landmark => self.landmarks.push(idx),
            BufferItemType::Heading => self.headings.push(idx),
            BufferItemType::Link => self.links.push(idx),
            BufferItemType::FormControl | BufferItemType::Button => self.form_controls.push(idx),
            _ => {}
        }
        self.items.push(item);
    }
    
    pub fn current(&self) -> Option<&VirtualBufferItem> { self.items.get(self.cursor) }
    
    pub fn move_next(&mut self) -> Option<&VirtualBufferItem> {
        if self.cursor + 1 < self.items.len() { self.cursor += 1; }
        self.current()
    }
    
    pub fn move_prev(&mut self) -> Option<&VirtualBufferItem> {
        if self.cursor > 0 { self.cursor -= 1; }
        self.current()
    }
    
    pub fn next_heading(&mut self) -> Option<&VirtualBufferItem> {
        if let Some(&idx) = self.headings.iter().find(|&&i| i > self.cursor) {
            self.cursor = idx;
            return self.current();
        }
        None
    }
    
    pub fn next_landmark(&mut self) -> Option<&VirtualBufferItem> {
        if let Some(&idx) = self.landmarks.iter().find(|&&i| i > self.cursor) {
            self.cursor = idx;
            return self.current();
        }
        None
    }
    
    pub fn next_link(&mut self) -> Option<&VirtualBufferItem> {
        if let Some(&idx) = self.links.iter().find(|&&i| i > self.cursor) {
            self.cursor = idx;
            return self.current();
        }
        None
    }
    
    pub fn next_form_control(&mut self) -> Option<&VirtualBufferItem> {
        if let Some(&idx) = self.form_controls.iter().find(|&&i| i > self.cursor) {
            self.cursor = idx;
            return self.current();
        }
        None
    }
    
    pub fn clear(&mut self) {
        self.items.clear();
        self.landmarks.clear(); self.headings.clear(); self.links.clear(); self.form_controls.clear();
        self.cursor = 0;
    }
}

/// Screen reader integration
#[derive(Debug, Default)]
pub struct ScreenReaderBridge {
    queue: AnnouncementQueue,
    buffer: VirtualBuffer,
    enabled: bool,
}

impl ScreenReaderBridge {
    pub fn new() -> Self { Self { queue: AnnouncementQueue::new(100), buffer: VirtualBuffer::new(), enabled: true } }
    
    pub fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }
    pub fn is_enabled(&self) -> bool { self.enabled }
    
    pub fn announce(&mut self, text: &str, priority: AnnouncePriority) {
        if !self.enabled { return; }
        self.queue.enqueue(Announcement { text: text.into(), priority, interrupt: priority == AnnouncePriority::Critical,
            timestamp: current_time_ms() });
    }
    
    pub fn announce_live_region(&mut self, text: &str, politeness: Politeness) {
        let priority = match politeness {
            Politeness::Off => return,
            Politeness::Polite => AnnouncePriority::Normal,
            Politeness::Assertive => AnnouncePriority::High,
        };
        self.announce(text, priority);
    }
    
    pub fn next_announcement(&mut self) -> Option<Announcement> { self.queue.dequeue() }
    pub fn buffer(&mut self) -> &mut VirtualBuffer { &mut self.buffer }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_announcement_queue() {
        let mut queue = AnnouncementQueue::new(10);
        queue.enqueue(Announcement { text: "Hello".into(), priority: AnnouncePriority::Normal, interrupt: false, timestamp: 0 });
        assert!(!queue.is_empty());
        assert_eq!(queue.dequeue().unwrap().text, "Hello");
    }
    
    #[test]
    fn test_virtual_buffer() {
        let mut buffer = VirtualBuffer::new();
        buffer.add_item(VirtualBufferItem { node_id: 1, item_type: BufferItemType::Heading, text: "Title".into(),
            level: Some(1), accessible_name: "Title".into(), accessible_description: None, focusable: false });
        buffer.add_item(VirtualBufferItem { node_id: 2, item_type: BufferItemType::Link, text: "Link".into(),
            level: None, accessible_name: "Link".into(), accessible_description: None, focusable: true });
        
        assert_eq!(buffer.current().unwrap().text, "Title");
        buffer.move_next();
        assert_eq!(buffer.current().unwrap().text, "Link");
    }
}
