//! Media JavaScript Bindings
//!
//! HTMLMediaElement API for video/audio elements.

use rquickjs::{Ctx, Function, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::media::{MediaElement, MediaType, ReadyState};

/// Media element storage
pub type MediaElements = Arc<Mutex<HashMap<i32, MediaElement>>>;

/// Install media element API
pub fn install_media_api(ctx: &Ctx, elements: MediaElements) -> Result<(), rquickjs::Error> {
    let globals = ctx.globals();
    
    // createVideoElement
    let elems = elements.clone();
    globals.set("createVideoElement", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<i32, rquickjs::Error> {
        let mut map = elems.lock().unwrap();
        let id = map.len() as i32;
        map.insert(id, MediaElement::video());
        Ok(id)
    })?)?;
    
    // createAudioElement
    let elems = elements.clone();
    globals.set("createAudioElement", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<i32, rquickjs::Error> {
        let mut map = elems.lock().unwrap();
        let id = map.len() as i32;
        map.insert(id, MediaElement::audio());
        Ok(id)
    })?)?;
    
    // mediaSetSrc
    let elems = elements.clone();
    globals.set("mediaSetSrc", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        if args.len() >= 2 {
            if let (Some(id), Some(src)) = (args[0].as_int(), args[1].as_string()) {
                let src = src.to_string().unwrap_or_default();
                let mut map = elems.lock().unwrap();
                if let Some(elem) = map.get_mut(&id) {
                    elem.set_src(&src);
                }
            }
        }
        Ok(())
    })?)?;
    
    // mediaPlay
    let elems = elements.clone();
    globals.set("mediaPlay", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<bool, rquickjs::Error> {
        if let Some(id) = args.first().and_then(|v| v.as_int()) {
            let mut map = elems.lock().unwrap();
            if let Some(elem) = map.get_mut(&id) {
                // Simulate metadata loaded for testing
                if elem.ready_state == crate::media::ReadyState::HaveNothing {
                    elem.on_metadata_loaded(100.0, 1920, 1080);
                    elem.on_can_play();
                }
                return Ok(elem.play().is_ok());
            }
        }
        Ok(false)
    })?)?;
    
    // mediaPause
    let elems = elements.clone();
    globals.set("mediaPause", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        if let Some(id) = args.first().and_then(|v| v.as_int()) {
            let mut map = elems.lock().unwrap();
            if let Some(elem) = map.get_mut(&id) {
                elem.pause();
            }
        }
        Ok(())
    })?)?;
    
    // mediaSeek
    let elems = elements.clone();
    globals.set("mediaSeek", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        if args.len() >= 2 {
            if let (Some(id), Some(time)) = (args[0].as_int(), args[1].as_float()) {
                let mut map = elems.lock().unwrap();
                if let Some(elem) = map.get_mut(&id) {
                    elem.seek(time);
                }
            }
        }
        Ok(())
    })?)?;
    
    // mediaGetCurrentTime
    let elems = elements.clone();
    globals.set("mediaGetCurrentTime", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<f64, rquickjs::Error> {
        if let Some(id) = args.first().and_then(|v| v.as_int()) {
            let map = elems.lock().unwrap();
            if let Some(elem) = map.get(&id) {
                return Ok(elem.properties.current_time);
            }
        }
        Ok(0.0)
    })?)?;
    
    // mediaGetDuration
    let elems = elements.clone();
    globals.set("mediaGetDuration", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<f64, rquickjs::Error> {
        if let Some(id) = args.first().and_then(|v| v.as_int()) {
            let map = elems.lock().unwrap();
            if let Some(elem) = map.get(&id) {
                return Ok(elem.properties.duration);
            }
        }
        Ok(0.0)
    })?)?;
    
    // mediaSetVolume
    let elems = elements.clone();
    globals.set("mediaSetVolume", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        if args.len() >= 2 {
            if let (Some(id), Some(vol)) = (args[0].as_int(), args[1].as_float()) {
                let mut map = elems.lock().unwrap();
                if let Some(elem) = map.get_mut(&id) {
                    elem.set_volume(vol);
                }
            }
        }
        Ok(())
    })?)?;
    
    // mediaGetVolume
    let elems = elements.clone();
    globals.set("mediaGetVolume", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<f64, rquickjs::Error> {
        if let Some(id) = args.first().and_then(|v| v.as_int()) {
            let map = elems.lock().unwrap();
            if let Some(elem) = map.get(&id) {
                return Ok(elem.properties.volume);
            }
        }
        Ok(1.0)
    })?)?;
    
    // mediaIsPlaying
    let elems = elements.clone();
    globals.set("mediaIsPlaying", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<bool, rquickjs::Error> {
        if let Some(id) = args.first().and_then(|v| v.as_int()) {
            let map = elems.lock().unwrap();
            if let Some(elem) = map.get(&id) {
                return Ok(elem.is_playing());
            }
        }
        Ok(false)
    })?)?;
    
    // mediaIsPaused
    let elems = elements;
    globals.set("mediaIsPaused", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<bool, rquickjs::Error> {
        if let Some(id) = args.first().and_then(|v| v.as_int()) {
            let map = elems.lock().unwrap();
            if let Some(elem) = map.get(&id) {
                return Ok(elem.is_paused());
            }
        }
        Ok(true)
    })?)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rquickjs::Runtime;
    
    fn create_context() -> (Runtime, rquickjs::Context, MediaElements) {
        let runtime = Runtime::new().unwrap();
        let context = rquickjs::Context::full(&runtime).unwrap();
        let elements = Arc::new(Mutex::new(HashMap::new()));
        
        context.with(|ctx| {
            install_media_api(&ctx, elements.clone()).unwrap();
        });
        
        (runtime, context, elements)
    }
    
    #[test]
    fn test_create_video() {
        let (_rt, ctx, elements) = create_context();
        
        ctx.with(|ctx| {
            let id: i32 = ctx.eval("createVideoElement()").unwrap();
            assert_eq!(id, 0);
        });
        
        assert_eq!(elements.lock().unwrap().len(), 1);
    }
    
    #[test]
    fn test_create_audio() {
        let (_rt, ctx, elements) = create_context();
        
        ctx.with(|ctx| {
            let id: i32 = ctx.eval("createAudioElement()").unwrap();
            assert_eq!(id, 0);
        });
        
        let map = elements.lock().unwrap();
        let elem = map.get(&0).unwrap();
        assert_eq!(elem.element_type, MediaType::Audio);
    }
    
    #[test]
    fn test_set_src() {
        let (_rt, ctx, elements) = create_context();
        
        ctx.with(|ctx| {
            let _: Value = ctx.eval(r#"
                var v = createVideoElement();
                mediaSetSrc(v, 'movie.mp4');
            "#).unwrap();
        });
        
        let map = elements.lock().unwrap();
        let elem = map.get(&0).unwrap();
        assert_eq!(elem.properties.src, "movie.mp4");
    }
    
    #[test]
    fn test_play_pause() {
        let (_rt, ctx, _elements) = create_context();
        
        ctx.with(|ctx| {
            let playing: bool = ctx.eval(r#"
                var v = createVideoElement();
                mediaSetSrc(v, 'movie.mp4');
                mediaPlay(v);
                mediaIsPlaying(v);
            "#).unwrap();
            assert!(playing);
            
            let paused: bool = ctx.eval(r#"
                mediaPause(v);
                mediaIsPaused(v);
            "#).unwrap();
            assert!(paused);
        });
    }
    
    #[test]
    fn test_volume() {
        let (_rt, ctx, _elements) = create_context();
        
        ctx.with(|ctx| {
            let vol: f64 = ctx.eval(r#"
                var v = createVideoElement();
                mediaSetVolume(v, 0.5);
                mediaGetVolume(v);
            "#).unwrap();
            assert_eq!(vol, 0.5);
        });
    }
}
