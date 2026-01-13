//! Live Region Support
//!
//! ARIA live regions for dynamic content announcements.
//! Custom implementation with no external dependencies.

use std::collections::VecDeque;

use crate::aria::{LiveRegionMode, LiveRelevant};

/// Live region configuration
#[derive(Debug, Clone)]
pub struct LiveRegionConfig {
    /// Politeness level: off, polite, assertive
    pub politeness: LiveRegionMode,
    /// Whether to announce entire region or just changes
    pub atomic: bool,
    /// What types of changes to announce
    pub relevant: RelevantFlags,
    /// Whether content is being updated
    pub busy: bool,
}

impl Default for LiveRegionConfig {
    fn default() -> Self {
        Self {
            politeness: LiveRegionMode::Off,
            atomic: false,
            // ARIA default for aria-relevant is "additions text"
            relevant: RelevantFlags::additions_text(),
            busy: false,
        }
    }
}

impl LiveRegionConfig {
    /// Create polite live region
    pub fn polite() -> Self {
        Self {
            politeness: LiveRegionMode::Polite,
            ..Default::default()
        }
    }
    
    /// Create assertive live region
    pub fn assertive() -> Self {
        Self {
            politeness: LiveRegionMode::Assertive,
            ..Default::default()
        }
    }
    
    /// Set atomic mode
    pub fn with_atomic(mut self, atomic: bool) -> Self {
        self.atomic = atomic;
        self
    }
}

/// Relevant change flags
#[derive(Debug, Clone, Copy, Default)]
pub struct RelevantFlags {
    pub additions: bool,
    pub removals: bool,
    pub text: bool,
}

impl RelevantFlags {
    pub fn all() -> Self {
        Self { additions: true, removals: true, text: true }
    }
    
    pub fn additions_text() -> Self {
        Self { additions: true, removals: false, text: true }
    }
    
    pub fn from_aria(relevant: &[LiveRelevant]) -> Self {
        let mut flags = Self::default();
        for r in relevant {
            match r {
                LiveRelevant::Additions => flags.additions = true,
                LiveRelevant::Removals => flags.removals = true,
                LiveRelevant::Text => flags.text = true,
                LiveRelevant::All => return Self::all(),
            }
        }
        if !flags.additions && !flags.removals && !flags.text {
            // Default is additions text
            Self::additions_text()
        } else {
            flags
        }
    }
}

/// Type of change detected in live region
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeType {
    /// Node added
    Addition,
    /// Node removed
    Removal,
    /// Text content changed
    TextChange,
}

/// A detected change in a live region
#[derive(Debug, Clone)]
pub struct LiveRegionChange {
    /// ID of the live region
    pub region_id: u64,
    /// Type of change
    pub change_type: ChangeType,
    /// Text to announce
    pub text: String,
    /// Politeness level
    pub politeness: LiveRegionMode,
    /// Whether to interrupt current announcements
    pub interrupt: bool,
    /// Timestamp
    pub timestamp_ms: u64,
}

/// Live region tracker
#[derive(Debug, Default)]
pub struct LiveRegionTracker {
    /// Registered live regions by node ID
    regions: Vec<RegisteredRegion>,
    /// Pending changes to announce
    pending: VecDeque<LiveRegionChange>,
    /// Maximum pending queue size
    max_pending: usize,
}

#[derive(Debug)]
struct RegisteredRegion {
    node_id: u64,
    config: LiveRegionConfig,
    /// Last known text content for change detection
    last_content: String,
    /// Last known child count
    last_child_count: usize,
}

impl LiveRegionTracker {
    pub fn new() -> Self {
        Self {
            regions: Vec::new(),
            pending: VecDeque::new(),
            max_pending: 100,
        }
    }
    
    /// Register a live region
    pub fn register(&mut self, node_id: u64, config: LiveRegionConfig) {
        // Remove existing registration if any
        self.regions.retain(|r| r.node_id != node_id);
        
        if config.politeness != LiveRegionMode::Off {
            self.regions.push(RegisteredRegion {
                node_id,
                config,
                last_content: String::new(),
                last_child_count: 0,
            });
        }
    }
    
    /// Unregister a live region
    pub fn unregister(&mut self, node_id: u64) {
        self.regions.retain(|r| r.node_id != node_id);
    }
    
    /// Check for changes in a region's content
    pub fn update_content(&mut self, node_id: u64, new_content: &str, child_count: usize) {
        // Find region index to avoid multiple mutable borrows
        let region_idx = self.regions.iter().position(|r| r.node_id == node_id);
        
        let change = if let Some(idx) = region_idx {
            let region = &self.regions[idx];
            
            if region.config.busy {
                return; // Don't announce while busy
            }
            
            let mut change_type = None;
            
            // Check for text changes
            if region.config.relevant.text && new_content != region.last_content {
                change_type = Some(ChangeType::TextChange);
            }
            
            // Check for additions/removals
            if change_type.is_none() {
                if child_count > region.last_child_count && region.config.relevant.additions {
                    change_type = Some(ChangeType::Addition);
                } else if child_count < region.last_child_count && region.config.relevant.removals {
                    change_type = Some(ChangeType::Removal);
                }
            }
            
            // Build change if needed
            change_type.map(|ct| {
                let text = if region.config.atomic {
                    new_content.to_string()
                } else {
                    new_content.to_string()
                };
                
                LiveRegionChange {
                    region_id: node_id,
                    change_type: ct,
                    text,
                    politeness: region.config.politeness,
                    interrupt: region.config.politeness == LiveRegionMode::Assertive,
                    timestamp_ms: current_time_ms(),
                }
            })
        } else {
            None
        };
        
        // Queue change outside mutable region borrow
        if let Some(c) = change {
            self.queue_change(c);
        }
        
        // Update region state
        if let Some(idx) = region_idx {
            self.regions[idx].last_content = new_content.to_string();
            self.regions[idx].last_child_count = child_count;
        }
    }
    
    /// Set busy state for a region
    pub fn set_busy(&mut self, node_id: u64, busy: bool) {
        // Find region and extract needed data
        let change = {
            let region_idx = self.regions.iter().position(|r| r.node_id == node_id);
            
            if let Some(idx) = region_idx {
                let region = &mut self.regions[idx];
                let was_busy = region.config.busy;
                region.config.busy = busy;
                
                // When transitioning from busy to not busy, announce current content
                if was_busy && !busy && region.config.politeness != LiveRegionMode::Off {
                    Some(LiveRegionChange {
                        region_id: node_id,
                        change_type: ChangeType::TextChange,
                        text: region.last_content.clone(),
                        politeness: region.config.politeness,
                        interrupt: false,
                        timestamp_ms: current_time_ms(),
                    })
                } else {
                    None
                }
            } else {
                None
            }
        };
        
        // Queue change outside borrow
        if let Some(c) = change {
            self.queue_change(c);
        }
    }
    
    fn queue_change(&mut self, change: LiveRegionChange) {
        // Assertive interrupts existing queue
        if change.interrupt {
            self.pending.retain(|c| c.politeness == LiveRegionMode::Assertive);
        }
        
        self.pending.push_back(change);
        
        // Trim queue if too large
        while self.pending.len() > self.max_pending {
            self.pending.pop_front();
        }
    }
    
    /// Get next change to announce
    pub fn next_change(&mut self) -> Option<LiveRegionChange> {
        // Prioritize assertive announcements
        if let Some(pos) = self.pending.iter().position(|c| c.politeness == LiveRegionMode::Assertive) {
            return self.pending.remove(pos);
        }
        self.pending.pop_front()
    }
    
    /// Check if there are pending announcements
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    
    /// Get number of pending announcements
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
    
    /// Clear all pending announcements
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }
    
    /// Check if a node is a live region
    pub fn is_live_region(&self, node_id: u64) -> bool {
        self.regions.iter().any(|r| r.node_id == node_id)
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_live_region_registration() {
        let mut tracker = LiveRegionTracker::new();
        tracker.register(1, LiveRegionConfig::polite());
        assert!(tracker.is_live_region(1));
        
        tracker.unregister(1);
        assert!(!tracker.is_live_region(1));
    }
    
    #[test]
    fn test_content_change_detection() {
        let mut tracker = LiveRegionTracker::new();
        tracker.register(1, LiveRegionConfig::polite());
        
        // First update from empty - creates a change
        tracker.update_content(1, "Hello", 1);
        // Second update - creates another change
        tracker.update_content(1, "Hello World", 1);
        
        assert!(tracker.has_pending());
        
        // First change is from initial content
        let change1 = tracker.next_change().unwrap();
        assert_eq!(change1.text, "Hello");
        
        // Second change is the update
        let change2 = tracker.next_change().unwrap();
        assert_eq!(change2.text, "Hello World");
        assert_eq!(change2.change_type, ChangeType::TextChange);
    }
    
    #[test]
    fn test_assertive_priority() {
        let mut tracker = LiveRegionTracker::new();
        tracker.register(1, LiveRegionConfig::polite());
        tracker.register(2, LiveRegionConfig::assertive());
        
        // Set initial content first
        tracker.update_content(1, "", 0);
        tracker.update_content(2, "", 0);
        tracker.clear_pending(); // Clear initial changes
        
        // Now test priority
        tracker.update_content(1, "Polite message", 1);
        tracker.update_content(2, "Assertive message", 1);
        
        let change = tracker.next_change().unwrap();
        assert_eq!(change.politeness, LiveRegionMode::Assertive);
    }
    
    #[test]
    fn test_busy_state() {
        let mut tracker = LiveRegionTracker::new();
        tracker.register(1, LiveRegionConfig::polite());
        
        tracker.set_busy(1, true);
        tracker.update_content(1, "During busy", 1);
        assert!(!tracker.has_pending()); // No announcement while busy
        
        tracker.set_busy(1, false);
        assert!(tracker.has_pending()); // Announces when busy clears
    }
}
