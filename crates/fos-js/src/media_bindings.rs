//! Media Bindings
//!
//! JavaScript bindings for audio/video media elements.

use crate::{JsValue, JsError};
use crate::engine_trait::JsContextApi;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};

static MEDIA_ID: AtomicI32 = AtomicI32::new(1);

/// Media element type
#[derive(Clone, Debug)]
pub enum MediaType {
    Video,
    Audio,
}

/// Media element state
#[derive(Clone, Debug)]
pub struct MediaElement {
    pub id: i32,
    pub media_type: MediaType,
    pub src: String,
    pub current_time: f64,
    pub duration: f64,
    pub volume: f64,
    pub is_playing: bool,
    pub is_paused: bool,
}

impl MediaElement {
    pub fn new(media_type: MediaType) -> Self {
        Self {
            id: MEDIA_ID.fetch_add(1, Ordering::SeqCst),
            media_type,
            src: String::new(),
            current_time: 0.0,
            duration: 0.0,
            volume: 1.0,
            is_playing: false,
            is_paused: true,
        }
    }
}

/// Collection of media elements
#[derive(Default)]
pub struct MediaElements {
    elements: HashMap<i32, MediaElement>,
}

impl MediaElements {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn create_video(&mut self) -> i32 {
        let elem = MediaElement::new(MediaType::Video);
        let id = elem.id;
        self.elements.insert(id, elem);
        id
    }
    
    pub fn create_audio(&mut self) -> i32 {
        let elem = MediaElement::new(MediaType::Audio);
        let id = elem.id;
        self.elements.insert(id, elem);
        id
    }
    
    pub fn get(&self, id: i32) -> Option<&MediaElement> {
        self.elements.get(&id)
    }
    
    pub fn get_mut(&mut self, id: i32) -> Option<&mut MediaElement> {
        self.elements.get_mut(&id)
    }
}

/// Install media API into global object
pub fn install_media_api<C: JsContextApi>(ctx: &C, elements: Arc<Mutex<MediaElements>>) -> Result<(), JsError> {
    // createVideoElement
    let e = elements.clone();
    ctx.set_global_function("createVideoElement", move |_args| {
        let id = e.lock().unwrap().create_video();
        Ok(JsValue::Number(id as f64))
    })?;
    
    // createAudioElement
    let e = elements.clone();
    ctx.set_global_function("createAudioElement", move |_args| {
        let id = e.lock().unwrap().create_audio();
        Ok(JsValue::Number(id as f64))
    })?;
    
    // mediaSetSrc
    let e = elements.clone();
    ctx.set_global_function("mediaSetSrc", move |args| {
        if args.len() < 2 {
            return Ok(JsValue::Undefined);
        }
        
        let id = args[0].as_number().unwrap_or(0.0) as i32;
        let src = args[1].as_string().unwrap_or("").to_string();
        
        if let Some(elem) = e.lock().unwrap().get_mut(id) {
            elem.src = src;
        }
        Ok(JsValue::Undefined)
    })?;
    
    // mediaPlay
    let e = elements.clone();
    ctx.set_global_function("mediaPlay", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Bool(false));
        }
        
        let id = args[0].as_number().unwrap_or(0.0) as i32;
        let mut elems = e.lock().unwrap();
        
        if let Some(elem) = elems.get_mut(id) {
            elem.is_playing = true;
            elem.is_paused = false;
            Ok(JsValue::Bool(true))
        } else {
            Ok(JsValue::Bool(false))
        }
    })?;
    
    // mediaPause
    let e = elements.clone();
    ctx.set_global_function("mediaPause", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Undefined);
        }
        
        let id = args[0].as_number().unwrap_or(0.0) as i32;
        let mut elems = e.lock().unwrap();
        
        if let Some(elem) = elems.get_mut(id) {
            elem.is_playing = false;
            elem.is_paused = true;
        }
        Ok(JsValue::Undefined)
    })?;
    
    // mediaSeek
    let e = elements.clone();
    ctx.set_global_function("mediaSeek", move |args| {
        if args.len() < 2 {
            return Ok(JsValue::Undefined);
        }
        
        let id = args[0].as_number().unwrap_or(0.0) as i32;
        let time = args[1].as_number().unwrap_or(0.0);
        
        if let Some(elem) = e.lock().unwrap().get_mut(id) {
            elem.current_time = time.max(0.0).min(elem.duration);
        }
        Ok(JsValue::Undefined)
    })?;
    
    // mediaGetCurrentTime
    let e = elements.clone();
    ctx.set_global_function("mediaGetCurrentTime", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Number(0.0));
        }
        
        let id = args[0].as_number().unwrap_or(0.0) as i32;
        let time = e.lock().unwrap()
            .get(id)
            .map(|elem| elem.current_time)
            .unwrap_or(0.0);
        Ok(JsValue::Number(time))
    })?;
    
    // mediaGetDuration
    let e = elements.clone();
    ctx.set_global_function("mediaGetDuration", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Number(0.0));
        }
        
        let id = args[0].as_number().unwrap_or(0.0) as i32;
        let duration = e.lock().unwrap()
            .get(id)
            .map(|elem| elem.duration)
            .unwrap_or(0.0);
        Ok(JsValue::Number(duration))
    })?;
    
    // mediaSetVolume
    let e = elements.clone();
    ctx.set_global_function("mediaSetVolume", move |args| {
        if args.len() < 2 {
            return Ok(JsValue::Undefined);
        }
        
        let id = args[0].as_number().unwrap_or(0.0) as i32;
        let volume = args[1].as_number().unwrap_or(1.0).max(0.0).min(1.0);
        
        if let Some(elem) = e.lock().unwrap().get_mut(id) {
            elem.volume = volume;
        }
        Ok(JsValue::Undefined)
    })?;
    
    // mediaGetVolume
    let e = elements.clone();
    ctx.set_global_function("mediaGetVolume", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Number(1.0));
        }
        
        let id = args[0].as_number().unwrap_or(0.0) as i32;
        let volume = e.lock().unwrap()
            .get(id)
            .map(|elem| elem.volume)
            .unwrap_or(1.0);
        Ok(JsValue::Number(volume))
    })?;
    
    // mediaIsPlaying
    let e = elements.clone();
    ctx.set_global_function("mediaIsPlaying", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Bool(false));
        }
        
        let id = args[0].as_number().unwrap_or(0.0) as i32;
        let is_playing = e.lock().unwrap()
            .get(id)
            .map(|elem| elem.is_playing)
            .unwrap_or(false);
        Ok(JsValue::Bool(is_playing))
    })?;
    
    // mediaIsPaused
    let e = elements;
    ctx.set_global_function("mediaIsPaused", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Bool(true));
        }
        
        let id = args[0].as_number().unwrap_or(0.0) as i32;
        let is_paused = e.lock().unwrap()
            .get(id)
            .map(|elem| elem.is_paused)
            .unwrap_or(true);
        Ok(JsValue::Bool(is_paused))
    })?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_media_elements() {
        let mut elements = MediaElements::new();
        
        let video_id = elements.create_video();
        let audio_id = elements.create_audio();
        
        assert!(elements.get(video_id).is_some());
        assert!(elements.get(audio_id).is_some());
        
        if let Some(elem) = elements.get_mut(video_id) {
            elem.src = "test.mp4".to_string();
            assert_eq!(elem.src, "test.mp4");
        }
    }
    
    #[test]
    fn test_media_playback() {
        let mut elements = MediaElements::new();
        let id = elements.create_video();
        
        if let Some(elem) = elements.get_mut(id) {
            elem.duration = 100.0;
            elem.is_playing = true;
            elem.is_paused = false;
            elem.current_time = 50.0;
            
            assert!(elem.is_playing);
            assert!(!elem.is_paused);
            assert_eq!(elem.current_time, 50.0);
        }
    }
}
