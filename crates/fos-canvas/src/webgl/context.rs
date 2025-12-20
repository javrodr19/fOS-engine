//! WebGL Rendering Context
//!
//! WebGL 1.0 implementation.

use std::collections::HashMap;

/// WebGL rendering context
#[derive(Debug)]
pub struct WebGLRenderingContext {
    /// Canvas width
    width: u32,
    /// Canvas height
    height: u32,
    /// Current program
    current_program: Option<WebGLProgram>,
    /// Active texture unit
    active_texture: u32,
    /// Viewport
    viewport: (i32, i32, i32, i32),
    /// Clear color
    clear_color: (f32, f32, f32, f32),
    /// Clear depth
    clear_depth: f32,
    /// Enabled capabilities
    enabled: HashMap<u32, bool>,
}

/// WebGL program
#[derive(Debug, Clone)]
pub struct WebGLProgram {
    pub id: u32,
    pub vertex_shader: Option<WebGLShader>,
    pub fragment_shader: Option<WebGLShader>,
    pub linked: bool,
}

/// WebGL shader
#[derive(Debug, Clone)]
pub struct WebGLShader {
    pub id: u32,
    pub shader_type: ShaderType,
    pub source: String,
    pub compiled: bool,
}

/// Shader type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderType {
    Vertex,
    Fragment,
}

/// WebGL buffer
#[derive(Debug, Clone)]
pub struct WebGLBuffer {
    pub id: u32,
    pub target: BufferTarget,
    pub data: Vec<u8>,
    pub usage: BufferUsage,
}

/// Buffer target
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferTarget {
    ArrayBuffer,
    ElementArrayBuffer,
}

/// Buffer usage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferUsage {
    StaticDraw,
    DynamicDraw,
    StreamDraw,
}

/// WebGL texture
#[derive(Debug, Clone)]
pub struct WebGLTexture {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub data: Vec<u8>,
}

/// Texture format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    Rgba,
    Rgb,
    Alpha,
    Luminance,
    LuminanceAlpha,
}

/// Uniform location
#[derive(Debug, Clone)]
pub struct WebGLUniformLocation {
    pub program_id: u32,
    pub location: i32,
    pub name: String,
}

/// Attribute location
pub type GLint = i32;
pub type GLuint = u32;
pub type GLfloat = f32;
pub type GLsizei = i32;
pub type GLenum = u32;

// WebGL constants
pub const GL_VERTEX_SHADER: u32 = 0x8B31;
pub const GL_FRAGMENT_SHADER: u32 = 0x8B30;
pub const GL_ARRAY_BUFFER: u32 = 0x8892;
pub const GL_ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
pub const GL_STATIC_DRAW: u32 = 0x88E4;
pub const GL_DYNAMIC_DRAW: u32 = 0x88E8;
pub const GL_STREAM_DRAW: u32 = 0x88E0;
pub const GL_TEXTURE_2D: u32 = 0x0DE1;
pub const GL_RGBA: u32 = 0x1908;
pub const GL_RGB: u32 = 0x1907;
pub const GL_DEPTH_TEST: u32 = 0x0B71;
pub const GL_BLEND: u32 = 0x0BE2;
pub const GL_CULL_FACE: u32 = 0x0B44;
pub const GL_TRIANGLES: u32 = 0x0004;
pub const GL_LINES: u32 = 0x0001;
pub const GL_POINTS: u32 = 0x0000;

impl WebGLRenderingContext {
    /// Create new WebGL context
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            current_program: None,
            active_texture: 0,
            viewport: (0, 0, width as i32, height as i32),
            clear_color: (0.0, 0.0, 0.0, 0.0),
            clear_depth: 1.0,
            enabled: HashMap::new(),
        }
    }
    
    // Viewport and clear
    
    pub fn viewport(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.viewport = (x, y, width, height);
    }
    
    pub fn clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.clear_color = (r, g, b, a);
    }
    
    pub fn clear_depth(&mut self, depth: f32) {
        self.clear_depth = depth;
    }
    
    pub fn clear(&mut self, _mask: u32) {
        // Would clear buffers
    }
    
    // Shaders
    
    pub fn create_shader(&self, shader_type: u32) -> WebGLShader {
        static mut SHADER_ID: u32 = 0;
        let id = unsafe { SHADER_ID += 1; SHADER_ID };
        
        WebGLShader {
            id,
            shader_type: if shader_type == GL_VERTEX_SHADER {
                ShaderType::Vertex
            } else {
                ShaderType::Fragment
            },
            source: String::new(),
            compiled: false,
        }
    }
    
    pub fn shader_source(&self, shader: &mut WebGLShader, source: &str) {
        shader.source = source.to_string();
    }
    
    pub fn compile_shader(&self, shader: &mut WebGLShader) {
        // Would compile shader
        shader.compiled = true;
    }
    
    pub fn get_shader_parameter(&self, shader: &WebGLShader, _pname: u32) -> bool {
        shader.compiled
    }
    
    // Programs
    
    pub fn create_program(&self) -> WebGLProgram {
        static mut PROGRAM_ID: u32 = 0;
        let id = unsafe { PROGRAM_ID += 1; PROGRAM_ID };
        
        WebGLProgram {
            id,
            vertex_shader: None,
            fragment_shader: None,
            linked: false,
        }
    }
    
    pub fn attach_shader(&self, program: &mut WebGLProgram, shader: &WebGLShader) {
        match shader.shader_type {
            ShaderType::Vertex => program.vertex_shader = Some(shader.clone()),
            ShaderType::Fragment => program.fragment_shader = Some(shader.clone()),
        }
    }
    
    pub fn link_program(&self, program: &mut WebGLProgram) {
        program.linked = program.vertex_shader.is_some() && program.fragment_shader.is_some();
    }
    
    pub fn use_program(&mut self, program: Option<WebGLProgram>) {
        self.current_program = program;
    }
    
    pub fn get_attrib_location(&self, _program: &WebGLProgram, name: &str) -> GLint {
        // Would look up attribute
        name.len() as i32 % 16
    }
    
    pub fn get_uniform_location(&self, program: &WebGLProgram, name: &str) -> WebGLUniformLocation {
        WebGLUniformLocation {
            program_id: program.id,
            location: name.len() as i32 % 16,
            name: name.to_string(),
        }
    }
    
    // Buffers
    
    pub fn create_buffer(&self) -> WebGLBuffer {
        static mut BUFFER_ID: u32 = 0;
        let id = unsafe { BUFFER_ID += 1; BUFFER_ID };
        
        WebGLBuffer {
            id,
            target: BufferTarget::ArrayBuffer,
            data: Vec::new(),
            usage: BufferUsage::StaticDraw,
        }
    }
    
    pub fn bind_buffer(&self, _target: u32, _buffer: &WebGLBuffer) {
        // Would bind buffer
    }
    
    pub fn buffer_data(&self, buffer: &mut WebGLBuffer, data: &[u8], usage: u32) {
        buffer.data = data.to_vec();
        buffer.usage = match usage {
            GL_STATIC_DRAW => BufferUsage::StaticDraw,
            GL_DYNAMIC_DRAW => BufferUsage::DynamicDraw,
            _ => BufferUsage::StreamDraw,
        };
    }
    
    // Textures
    
    pub fn create_texture(&self) -> WebGLTexture {
        static mut TEXTURE_ID: u32 = 0;
        let id = unsafe { TEXTURE_ID += 1; TEXTURE_ID };
        
        WebGLTexture {
            id,
            width: 0,
            height: 0,
            format: TextureFormat::Rgba,
            data: Vec::new(),
        }
    }
    
    pub fn bind_texture(&self, _target: u32, _texture: &WebGLTexture) {
        // Would bind texture
    }
    
    pub fn tex_image_2d(&self, texture: &mut WebGLTexture, width: u32, height: u32, data: &[u8]) {
        texture.width = width;
        texture.height = height;
        texture.data = data.to_vec();
    }
    
    pub fn active_texture(&mut self, unit: u32) {
        self.active_texture = unit;
    }
    
    // Enable/Disable
    
    pub fn enable(&mut self, cap: u32) {
        self.enabled.insert(cap, true);
    }
    
    pub fn disable(&mut self, cap: u32) {
        self.enabled.insert(cap, false);
    }
    
    pub fn is_enabled(&self, cap: u32) -> bool {
        *self.enabled.get(&cap).unwrap_or(&false)
    }
    
    // Drawing
    
    pub fn draw_arrays(&self, _mode: u32, _first: i32, _count: i32) {
        // Would draw primitives
    }
    
    pub fn draw_elements(&self, _mode: u32, _count: i32, _type: u32, _offset: i32) {
        // Would draw indexed primitives
    }
    
    // Vertex attributes
    
    pub fn vertex_attrib_pointer(&self, _index: u32, _size: i32, _type: u32, _normalized: bool, _stride: i32, _offset: i32) {
        // Would set vertex attribute pointer
    }
    
    pub fn enable_vertex_attrib_array(&self, _index: u32) {
        // Would enable vertex attrib
    }
    
    // Uniforms
    
    pub fn uniform1f(&self, _location: &WebGLUniformLocation, _x: f32) {}
    pub fn uniform2f(&self, _location: &WebGLUniformLocation, _x: f32, _y: f32) {}
    pub fn uniform3f(&self, _location: &WebGLUniformLocation, _x: f32, _y: f32, _z: f32) {}
    pub fn uniform4f(&self, _location: &WebGLUniformLocation, _x: f32, _y: f32, _z: f32, _w: f32) {}
    pub fn uniform1i(&self, _location: &WebGLUniformLocation, _x: i32) {}
    pub fn uniform_matrix4fv(&self, _location: &WebGLUniformLocation, _transpose: bool, _value: &[f32; 16]) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_context() {
        let ctx = WebGLRenderingContext::new(800, 600);
        assert_eq!(ctx.viewport, (0, 0, 800, 600));
    }
    
    #[test]
    fn test_create_program() {
        let ctx = WebGLRenderingContext::new(800, 600);
        let mut program = ctx.create_program();
        
        let mut vs = ctx.create_shader(GL_VERTEX_SHADER);
        ctx.shader_source(&mut vs, "void main() {}");
        ctx.compile_shader(&mut vs);
        
        let mut fs = ctx.create_shader(GL_FRAGMENT_SHADER);
        ctx.shader_source(&mut fs, "void main() {}");
        ctx.compile_shader(&mut fs);
        
        ctx.attach_shader(&mut program, &vs);
        ctx.attach_shader(&mut program, &fs);
        ctx.link_program(&mut program);
        
        assert!(program.linked);
    }
}
