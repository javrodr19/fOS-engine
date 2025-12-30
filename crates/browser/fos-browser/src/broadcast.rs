//! Broadcast Channel API
//!
//! Cross-tab communication.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Message type for broadcast
#[derive(Debug, Clone)]
pub struct BroadcastMessage {
    pub channel: String,
    pub data: String,
    pub origin: String,
    pub timestamp: u64,
}

/// Broadcast channel
#[derive(Debug)]
pub struct BroadcastChannel {
    pub name: String,
    pub id: u64,
    closed: bool,
}

impl BroadcastChannel {
    pub fn new(name: &str, id: u64) -> Self {
        Self {
            name: name.to_string(),
            id,
            closed: false,
        }
    }
    
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    
    pub fn close(&mut self) {
        self.closed = true;
    }
}

/// Broadcast channel manager
#[derive(Debug, Default)]
pub struct BroadcastChannelManager {
    channels: HashMap<String, Vec<u64>>,
    channel_instances: HashMap<u64, BroadcastChannel>,
    pending_messages: Vec<(u64, BroadcastMessage)>,
    next_id: u64,
}

impl BroadcastChannelManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create a broadcast channel
    pub fn create(&mut self, name: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let channel = BroadcastChannel::new(name, id);
        self.channel_instances.insert(id, channel);
        
        self.channels
            .entry(name.to_string())
            .or_default()
            .push(id);
        
        id
    }
    
    /// Post message to channel
    pub fn post_message(&mut self, channel_id: u64, data: &str, origin: &str) {
        let Some(channel) = self.channel_instances.get(&channel_id) else {
            return;
        };
        
        if channel.is_closed() {
            return;
        }
        
        let channel_name = channel.name.clone();
        
        let message = BroadcastMessage {
            channel: channel_name.clone(),
            data: data.to_string(),
            origin: origin.to_string(),
            timestamp: Self::now(),
        };
        
        // Send to all other channels with same name
        if let Some(subscribers) = self.channels.get(&channel_name) {
            for &sub_id in subscribers {
                if sub_id != channel_id {
                    if let Some(sub) = self.channel_instances.get(&sub_id) {
                        if !sub.is_closed() {
                            self.pending_messages.push((sub_id, message.clone()));
                        }
                    }
                }
            }
        }
    }
    
    /// Get pending messages for a channel
    pub fn get_messages(&mut self, channel_id: u64) -> Vec<BroadcastMessage> {
        let messages: Vec<_> = self.pending_messages
            .iter()
            .filter(|(id, _)| *id == channel_id)
            .map(|(_, msg)| msg.clone())
            .collect();
        
        self.pending_messages.retain(|(id, _)| *id != channel_id);
        
        messages
    }
    
    /// Close a channel
    pub fn close(&mut self, channel_id: u64) {
        if let Some(channel) = self.channel_instances.get_mut(&channel_id) {
            let name = channel.name.clone();
            channel.close();
            
            if let Some(subs) = self.channels.get_mut(&name) {
                subs.retain(|&id| id != channel_id);
            }
        }
    }
    
    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_broadcast() {
        let mut mgr = BroadcastChannelManager::new();
        
        let ch1 = mgr.create("test");
        let ch2 = mgr.create("test");
        
        mgr.post_message(ch1, "hello", "https://example.com");
        
        let messages = mgr.get_messages(ch2);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].data, "hello");
    }
}
