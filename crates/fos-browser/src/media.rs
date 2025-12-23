//! Media Integration
//!
//! Integrates fos-media for video and audio element support.

use std::collections::HashMap;
use fos_dom::{Document, DomTree, NodeId};
use fos_media::{
    HTMLVideoElement, HTMLAudioElement,
};

/// Media manager for the browser
pub struct MediaManager {
    /// Video elements by node ID
    videos: HashMap<u64, VideoInstance>,
    /// Audio elements by node ID
    audios: HashMap<u64, AudioInstance>,
    /// Next media ID
    next_id: u64,
}

/// Video element instance
#[derive(Debug)]
pub struct VideoInstance {
    pub id: u64,
    pub element: HTMLVideoElement,
    pub src: String,
    pub bounds: MediaBounds,
    pub loaded: bool,
}

/// Audio element instance
#[derive(Debug)]
pub struct AudioInstance {
    pub id: u64,
    pub element: HTMLAudioElement,
    pub src: String,
    pub loaded: bool,
}

/// Media element bounds for rendering
#[derive(Debug, Clone, Default)]
pub struct MediaBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl MediaManager {
    /// Create new media manager
    pub fn new() -> Self {
        Self {
            videos: HashMap::new(),
            audios: HashMap::new(),
            next_id: 1,
        }
    }
    
    /// Extract media elements from DOM
    pub fn extract_from_document(&mut self, document: &Document) {
        self.videos.clear();
        self.audios.clear();
        
        let tree = document.tree();
        self.scan_tree(tree, tree.root());
        
        log::debug!("Found {} videos, {} audios", self.videos.len(), self.audios.len());
    }
    
    /// Recursively scan DOM tree for media elements
    fn scan_tree(&mut self, tree: &DomTree, node_id: NodeId) {
        if !node_id.is_valid() {
            return;
        }
        
        if let Some(node) = tree.get(node_id) {
            if let Some(element) = node.as_element() {
                let tag = tree.resolve(element.name.local).to_lowercase();
                
                match tag.as_str() {
                    "video" => {
                        let mut video_el = HTMLVideoElement::new();
                        let mut src = String::new();
                        let mut width = 320u32;
                        let mut height = 240u32;
                        
                        // Parse attributes
                        for attr in element.attrs.iter() {
                            let name = tree.resolve(attr.name.local);
                            match name {
                                "src" => {
                                    src = attr.value.clone();
                                    video_el.base.src = src.clone();
                                }
                                "width" => {
                                    width = attr.value.parse().unwrap_or(320);
                                    video_el.width = width;
                                }
                                "height" => {
                                    height = attr.value.parse().unwrap_or(240);
                                    video_el.height = height;
                                }
                                "poster" => {
                                    video_el.poster = attr.value.clone();
                                }
                                "controls" => {
                                    video_el.base.controls = true;
                                }
                                "autoplay" => {
                                    video_el.base.autoplay = true;
                                }
                                "muted" => {
                                    video_el.base.muted = true;
                                    video_el.base.default_muted = true;
                                }
                                "loop" => {
                                    video_el.base.loop_ = true;
                                }
                                "playsinline" => {
                                    video_el.plays_inline = true;
                                }
                                _ => {}
                            }
                        }
                        
                        // Look for <source> children if no src
                        if src.is_empty() {
                            for (child_id, _) in tree.children(node_id) {
                                if let Some(child) = tree.get(child_id) {
                                    if let Some(child_el) = child.as_element() {
                                        let child_tag = tree.resolve(child_el.name.local);
                                        if child_tag == "source" {
                                            for attr in child_el.attrs.iter() {
                                                if tree.resolve(attr.name.local) == "src" {
                                                    src = attr.value.clone();
                                                    video_el.base.src = src.clone();
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                if !src.is_empty() { break; }
                            }
                        }
                        
                        let id = self.next_id;
                        self.next_id += 1;
                        
                        self.videos.insert(id, VideoInstance {
                            id,
                            element: video_el,
                            src,
                            bounds: MediaBounds {
                                x: 0.0,
                                y: 0.0,
                                width: width as f32,
                                height: height as f32,
                            },
                            loaded: false,
                        });
                    }
                    
                    "audio" => {
                        let mut audio_el = HTMLAudioElement::new();
                        let mut src = String::new();
                        
                        // Parse attributes
                        for attr in element.attrs.iter() {
                            let name = tree.resolve(attr.name.local);
                            match name {
                                "src" => {
                                    src = attr.value.clone();
                                    audio_el.base.src = src.clone();
                                }
                                "controls" => {
                                    audio_el.base.controls = true;
                                }
                                "autoplay" => {
                                    audio_el.base.autoplay = true;
                                }
                                "muted" => {
                                    audio_el.base.muted = true;
                                    audio_el.base.default_muted = true;
                                }
                                "loop" => {
                                    audio_el.base.loop_ = true;
                                }
                                _ => {}
                            }
                        }
                        
                        // Look for <source> children if no src
                        if src.is_empty() {
                            for (child_id, _) in tree.children(node_id) {
                                if let Some(child) = tree.get(child_id) {
                                    if let Some(child_el) = child.as_element() {
                                        let child_tag = tree.resolve(child_el.name.local);
                                        if child_tag == "source" {
                                            for attr in child_el.attrs.iter() {
                                                if tree.resolve(attr.name.local) == "src" {
                                                    src = attr.value.clone();
                                                    audio_el.base.src = src.clone();
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                if !src.is_empty() { break; }
                            }
                        }
                        
                        let id = self.next_id;
                        self.next_id += 1;
                        
                        self.audios.insert(id, AudioInstance {
                            id,
                            element: audio_el,
                            src,
                            loaded: false,
                        });
                    }
                    
                    _ => {}
                }
            }
        }
        
        // Recurse into children
        for (child_id, _) in tree.children(node_id) {
            self.scan_tree(tree, child_id);
        }
    }
    
    /// Get all video elements
    pub fn get_videos(&self) -> impl Iterator<Item = &VideoInstance> {
        self.videos.values()
    }
    
    /// Get all audio elements
    pub fn get_audios(&self) -> impl Iterator<Item = &AudioInstance> {
        self.audios.values()
    }
    
    /// Get video by ID
    pub fn get_video(&self, id: u64) -> Option<&VideoInstance> {
        self.videos.get(&id)
    }
    
    /// Get mutable video by ID
    pub fn get_video_mut(&mut self, id: u64) -> Option<&mut VideoInstance> {
        self.videos.get_mut(&id)
    }
    
    /// Get audio by ID
    pub fn get_audio(&self, id: u64) -> Option<&AudioInstance> {
        self.audios.get(&id)
    }
    
    /// Get mutable audio by ID
    pub fn get_audio_mut(&mut self, id: u64) -> Option<&mut AudioInstance> {
        self.audios.get_mut(&id)
    }
    
    /// Play video
    pub fn play_video(&mut self, id: u64) -> Result<(), &'static str> {
        if let Some(video) = self.videos.get_mut(&id) {
            video.element.base.play().map_err(|_| "Cannot play")?;
            Ok(())
        } else {
            Err("Video not found")
        }
    }
    
    /// Pause video
    pub fn pause_video(&mut self, id: u64) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.element.base.pause();
        }
    }
    
    /// Play audio
    pub fn play_audio(&mut self, id: u64) -> Result<(), &'static str> {
        if let Some(audio) = self.audios.get_mut(&id) {
            audio.element.base.play().map_err(|_| "Cannot play")?;
            Ok(())
        } else {
            Err("Audio not found")
        }
    }
    
    /// Pause audio
    pub fn pause_audio(&mut self, id: u64) {
        if let Some(audio) = self.audios.get_mut(&id) {
            audio.element.base.pause();
        }
    }
    
    /// Set video volume
    pub fn set_video_volume(&mut self, id: u64, volume: f64) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.element.base.volume = volume.clamp(0.0, 1.0);
        }
    }
    
    /// Set audio volume
    pub fn set_audio_volume(&mut self, id: u64, volume: f64) {
        if let Some(audio) = self.audios.get_mut(&id) {
            audio.element.base.volume = volume.clamp(0.0, 1.0);
        }
    }
    
    /// Seek video
    pub fn seek_video(&mut self, id: u64, time: f64) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.element.base.seek(time);
        }
    }
    
    /// Seek audio
    pub fn seek_audio(&mut self, id: u64, time: f64) {
        if let Some(audio) = self.audios.get_mut(&id) {
            audio.element.base.seek(time);
        }
    }
    
    /// Toggle mute for video
    pub fn toggle_video_mute(&mut self, id: u64) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.element.base.muted = !video.element.base.muted;
        }
    }
    
    /// Toggle mute for audio
    pub fn toggle_audio_mute(&mut self, id: u64) {
        if let Some(audio) = self.audios.get_mut(&id) {
            audio.element.base.muted = !audio.element.base.muted;
        }
    }
    
    /// Get media statistics
    pub fn stats(&self) -> MediaStats {
        MediaStats {
            video_count: self.videos.len(),
            audio_count: self.audios.len(),
            playing_videos: self.videos.values()
                .filter(|v| !v.element.base.paused)
                .count(),
            playing_audios: self.audios.values()
                .filter(|a| !a.element.base.paused)
                .count(),
        }
    }
    
    /// Check if any media is playing
    pub fn is_any_playing(&self) -> bool {
        self.videos.values().any(|v| !v.element.base.paused) ||
        self.audios.values().any(|a| !a.element.base.paused)
    }
    
    /// Pause all media
    pub fn pause_all(&mut self) {
        for video in self.videos.values_mut() {
            video.element.base.pause();
        }
        for audio in self.audios.values_mut() {
            audio.element.base.pause();
        }
    }
}

impl Default for MediaManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Media statistics
#[derive(Debug, Clone)]
pub struct MediaStats {
    pub video_count: usize,
    pub audio_count: usize,
    pub playing_videos: usize,
    pub playing_audios: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_media_manager_creation() {
        let manager = MediaManager::new();
        assert_eq!(manager.videos.len(), 0);
        assert_eq!(manager.audios.len(), 0);
    }
    
    #[test]
    fn test_media_stats() {
        let manager = MediaManager::new();
        let stats = manager.stats();
        assert_eq!(stats.video_count, 0);
        assert_eq!(stats.audio_count, 0);
    }
}
