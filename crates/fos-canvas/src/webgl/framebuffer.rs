//! WebGL Framebuffers
//!
//! Render targets for WebGL.

/// WebGL Framebuffer
#[derive(Debug, Clone)]
pub struct WebGLFramebuffer {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub color_attachment: Option<u32>,
    pub depth_attachment: Option<u32>,
    pub stencil_attachment: Option<u32>,
}

/// WebGL Renderbuffer
#[derive(Debug, Clone)]
pub struct WebGLRenderbuffer {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub format: RenderbufferFormat,
}

/// Renderbuffer format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderbufferFormat {
    Rgba4,
    Rgb565,
    Rgb5A1,
    DepthComponent16,
    StencilIndex8,
    DepthStencil,
}

// Constants
pub const GL_FRAMEBUFFER: u32 = 0x8D40;
pub const GL_RENDERBUFFER: u32 = 0x8D41;
pub const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
pub const GL_DEPTH_ATTACHMENT: u32 = 0x8D00;
pub const GL_STENCIL_ATTACHMENT: u32 = 0x8D20;
pub const GL_DEPTH_STENCIL_ATTACHMENT: u32 = 0x821A;
pub const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;

impl WebGLFramebuffer {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            width: 0,
            height: 0,
            color_attachment: None,
            depth_attachment: None,
            stencil_attachment: None,
        }
    }
    
    /// Check if complete
    pub fn check_status(&self) -> u32 {
        if self.color_attachment.is_some() {
            GL_FRAMEBUFFER_COMPLETE
        } else {
            0
        }
    }
}

impl WebGLRenderbuffer {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            width: 0,
            height: 0,
            format: RenderbufferFormat::Rgba4,
        }
    }
    
    /// Set storage
    pub fn storage(&mut self, format: RenderbufferFormat, width: u32, height: u32) {
        self.format = format;
        self.width = width;
        self.height = height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_framebuffer() {
        let mut fb = WebGLFramebuffer::new(1);
        assert_ne!(fb.check_status(), GL_FRAMEBUFFER_COMPLETE);
        
        fb.color_attachment = Some(1);
        assert_eq!(fb.check_status(), GL_FRAMEBUFFER_COMPLETE);
    }
}
