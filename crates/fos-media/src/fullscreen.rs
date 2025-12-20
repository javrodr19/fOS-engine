//! Fullscreen API
//!
//! Fullscreen and Picture-in-Picture.

/// Fullscreen options
#[derive(Debug, Clone, Default)]
pub struct FullscreenOptions {
    pub navigation_ui: NavigationUI,
}

/// Navigation UI visibility
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NavigationUI {
    #[default]
    Auto,
    Show,
    Hide,
}

/// Fullscreen manager
#[derive(Debug, Default)]
pub struct FullscreenManager {
    pub fullscreen_element: Option<u64>, // Element ID
    pub fullscreen_enabled: bool,
}

impl FullscreenManager {
    pub fn new() -> Self {
        Self {
            fullscreen_element: None,
            fullscreen_enabled: true,
        }
    }
    
    /// Request fullscreen
    pub fn request_fullscreen(&mut self, element_id: u64, _options: FullscreenOptions) -> Result<(), FullscreenError> {
        if !self.fullscreen_enabled {
            return Err(FullscreenError::NotAllowed);
        }
        self.fullscreen_element = Some(element_id);
        Ok(())
    }
    
    /// Exit fullscreen
    pub fn exit_fullscreen(&mut self) -> Result<(), FullscreenError> {
        self.fullscreen_element = None;
        Ok(())
    }
    
    /// Check if in fullscreen
    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen_element.is_some()
    }
}

/// Fullscreen error
#[derive(Debug, Clone)]
pub enum FullscreenError {
    NotAllowed,
    NotSupported,
}

/// Picture-in-Picture window
#[derive(Debug)]
pub struct PictureInPictureWindow {
    pub width: u32,
    pub height: u32,
}

/// Picture-in-Picture manager
#[derive(Debug, Default)]
pub struct PipManager {
    pub pip_element: Option<u64>,
    pub pip_window: Option<PictureInPictureWindow>,
}

impl PipManager {
    pub fn new() -> Self { Self::default() }
    
    /// Request PiP
    pub fn request_pip(&mut self, element_id: u64, width: u32, height: u32) -> Result<&PictureInPictureWindow, PipError> {
        self.pip_element = Some(element_id);
        self.pip_window = Some(PictureInPictureWindow { width, height });
        self.pip_window.as_ref().ok_or(PipError::NotAllowed)
    }
    
    /// Exit PiP
    pub fn exit_pip(&mut self) {
        self.pip_element = None;
        self.pip_window = None;
    }
}

/// PiP error
#[derive(Debug, Clone)]
pub enum PipError {
    NotAllowed,
    NotSupported,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fullscreen() {
        let mut fs = FullscreenManager::new();
        fs.request_fullscreen(1, FullscreenOptions::default()).unwrap();
        assert!(fs.is_fullscreen());
        
        fs.exit_fullscreen().unwrap();
        assert!(!fs.is_fullscreen());
    }
}
