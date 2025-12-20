//! WebGL Implementation
//!
//! WebGL 1.0 and 2.0 context with shader compilation, textures, and framebuffers.

use std::collections::HashMap;

/// WebGL version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebGLVersion {
    WebGL1,
    WebGL2,
}

/// WebGL context
#[derive(Debug)]
pub struct WebGLRenderingContext {
    /// Version
    pub version: WebGLVersion,
    /// Canvas width
    pub width: u32,
    /// Canvas height
    pub height: u32,
    /// Shaders
    shaders: HashMap<u32, Shader>,
    /// Programs
    programs: HashMap<u32, Program>,
    /// Textures
    textures: HashMap<u32, Texture>,
    /// Framebuffers
    framebuffers: HashMap<u32, Framebuffer>,
    /// Buffers
    buffers: HashMap<u32, Buffer>,
    /// Next ID
    next_id: u32,
    /// Current program
    current_program: Option<u32>,
    /// Clear color
    clear_color: [f32; 4],
    /// Viewport
    viewport: [i32; 4],
}

/// Shader type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderType {
    Vertex,
    Fragment,
}

/// Shader
#[derive(Debug)]
pub struct Shader {
    pub id: u32,
    pub shader_type: ShaderType,
    pub source: String,
    pub compiled: bool,
    pub error: Option<String>,
}

/// Shader program
#[derive(Debug)]
pub struct Program {
    pub id: u32,
    pub vertex_shader: Option<u32>,
    pub fragment_shader: Option<u32>,
    pub linked: bool,
    pub uniforms: HashMap<String, UniformLocation>,
    pub attributes: HashMap<String, u32>,
}

/// Uniform location
#[derive(Debug, Clone)]
pub struct UniformLocation {
    pub id: u32,
    pub name: String,
}

/// Texture
#[derive(Debug)]
pub struct Texture {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub data: Vec<u8>,
    pub mipmaps: bool,
}

/// Texture format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    RGBA,
    RGB,
    LuminanceAlpha,
    Luminance,
    Alpha,
    Depth,
    DepthStencil,
}

/// Framebuffer
#[derive(Debug)]
pub struct Framebuffer {
    pub id: u32,
    pub color_attachment: Option<u32>,
    pub depth_attachment: Option<u32>,
    pub stencil_attachment: Option<u32>,
    pub complete: bool,
}

/// Buffer
#[derive(Debug)]
pub struct Buffer {
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
    UniformBuffer, // WebGL2
    TransformFeedbackBuffer, // WebGL2
}

/// Buffer usage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferUsage {
    StaticDraw,
    DynamicDraw,
    StreamDraw,
}

impl WebGLRenderingContext {
    pub fn new(version: WebGLVersion, width: u32, height: u32) -> Self {
        Self {
            version,
            width,
            height,
            shaders: HashMap::new(),
            programs: HashMap::new(),
            textures: HashMap::new(),
            framebuffers: HashMap::new(),
            buffers: HashMap::new(),
            next_id: 1,
            current_program: None,
            clear_color: [0.0, 0.0, 0.0, 0.0],
            viewport: [0, 0, width as i32, height as i32],
        }
    }
    
    fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    
    // Shader operations
    pub fn create_shader(&mut self, shader_type: ShaderType) -> u32 {
        let id = self.next_id();
        self.shaders.insert(id, Shader {
            id,
            shader_type,
            source: String::new(),
            compiled: false,
            error: None,
        });
        id
    }
    
    pub fn shader_source(&mut self, shader: u32, source: &str) {
        if let Some(s) = self.shaders.get_mut(&shader) {
            s.source = source.to_string();
        }
    }
    
    pub fn compile_shader(&mut self, shader: u32) -> bool {
        if let Some(s) = self.shaders.get_mut(&shader) {
            // Simplified validation
            if s.source.contains("void main") {
                s.compiled = true;
                s.error = None;
                true
            } else {
                s.compiled = false;
                s.error = Some("Missing main function".to_string());
                false
            }
        } else {
            false
        }
    }
    
    pub fn get_shader_info_log(&self, shader: u32) -> Option<String> {
        self.shaders.get(&shader).and_then(|s| s.error.clone())
    }
    
    pub fn delete_shader(&mut self, shader: u32) {
        self.shaders.remove(&shader);
    }
    
    // Program operations
    pub fn create_program(&mut self) -> u32 {
        let id = self.next_id();
        self.programs.insert(id, Program {
            id,
            vertex_shader: None,
            fragment_shader: None,
            linked: false,
            uniforms: HashMap::new(),
            attributes: HashMap::new(),
        });
        id
    }
    
    pub fn attach_shader(&mut self, program: u32, shader: u32) {
        if let (Some(p), Some(s)) = (self.programs.get_mut(&program), self.shaders.get(&shader)) {
            match s.shader_type {
                ShaderType::Vertex => p.vertex_shader = Some(shader),
                ShaderType::Fragment => p.fragment_shader = Some(shader),
            }
        }
    }
    
    pub fn link_program(&mut self, program: u32) -> bool {
        if let Some(p) = self.programs.get_mut(&program) {
            p.linked = p.vertex_shader.is_some() && p.fragment_shader.is_some();
            p.linked
        } else {
            false
        }
    }
    
    pub fn use_program(&mut self, program: Option<u32>) {
        self.current_program = program;
    }
    
    // Texture operations
    pub fn create_texture(&mut self) -> u32 {
        let id = self.next_id();
        self.textures.insert(id, Texture {
            id,
            width: 0,
            height: 0,
            format: TextureFormat::RGBA,
            data: Vec::new(),
            mipmaps: false,
        });
        id
    }
    
    pub fn tex_image_2d(&mut self, texture: u32, width: u32, height: u32, format: TextureFormat, data: Vec<u8>) {
        if let Some(t) = self.textures.get_mut(&texture) {
            t.width = width;
            t.height = height;
            t.format = format;
            t.data = data;
        }
    }
    
    pub fn generate_mipmap(&mut self, texture: u32) {
        if let Some(t) = self.textures.get_mut(&texture) {
            t.mipmaps = true;
        }
    }
    
    pub fn delete_texture(&mut self, texture: u32) {
        self.textures.remove(&texture);
    }
    
    // Framebuffer operations
    pub fn create_framebuffer(&mut self) -> u32 {
        let id = self.next_id();
        self.framebuffers.insert(id, Framebuffer {
            id,
            color_attachment: None,
            depth_attachment: None,
            stencil_attachment: None,
            complete: false,
        });
        id
    }
    
    pub fn framebuffer_texture_2d(&mut self, framebuffer: u32, attachment: FramebufferAttachment, texture: u32) {
        if let Some(fb) = self.framebuffers.get_mut(&framebuffer) {
            match attachment {
                FramebufferAttachment::Color => fb.color_attachment = Some(texture),
                FramebufferAttachment::Depth => fb.depth_attachment = Some(texture),
                FramebufferAttachment::Stencil => fb.stencil_attachment = Some(texture),
                FramebufferAttachment::DepthStencil => {
                    fb.depth_attachment = Some(texture);
                    fb.stencil_attachment = Some(texture);
                }
            }
            fb.complete = fb.color_attachment.is_some();
        }
    }
    
    pub fn check_framebuffer_status(&self, framebuffer: u32) -> FramebufferStatus {
        if let Some(fb) = self.framebuffers.get(&framebuffer) {
            if fb.complete {
                FramebufferStatus::Complete
            } else {
                FramebufferStatus::IncompleteAttachment
            }
        } else {
            FramebufferStatus::Undefined
        }
    }
    
    // Buffer operations
    pub fn create_buffer(&mut self) -> u32 {
        let id = self.next_id();
        self.buffers.insert(id, Buffer {
            id,
            target: BufferTarget::ArrayBuffer,
            data: Vec::new(),
            usage: BufferUsage::StaticDraw,
        });
        id
    }
    
    pub fn buffer_data(&mut self, buffer: u32, target: BufferTarget, data: Vec<u8>, usage: BufferUsage) {
        if let Some(b) = self.buffers.get_mut(&buffer) {
            b.target = target;
            b.data = data;
            b.usage = usage;
        }
    }
    
    // Drawing
    pub fn clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.clear_color = [r, g, b, a];
    }
    
    pub fn clear(&mut self, _mask: u32) {
        // Clear implementation
    }
    
    pub fn viewport(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.viewport = [x, y, width, height];
    }
    
    pub fn draw_arrays(&self, _mode: DrawMode, _first: i32, _count: i32) {
        // Draw implementation
    }
    
    pub fn draw_elements(&self, _mode: DrawMode, _count: i32, _type_: ElementType, _offset: i32) {
        // Draw implementation
    }
}

/// Framebuffer attachment
#[derive(Debug, Clone, Copy)]
pub enum FramebufferAttachment {
    Color,
    Depth,
    Stencil,
    DepthStencil,
}

/// Framebuffer status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramebufferStatus {
    Complete,
    IncompleteAttachment,
    IncompleteMissingAttachment,
    IncompleteDimensions,
    Unsupported,
    Undefined,
}

/// Draw mode
#[derive(Debug, Clone, Copy)]
pub enum DrawMode {
    Points,
    Lines,
    LineStrip,
    LineLoop,
    Triangles,
    TriangleStrip,
    TriangleFan,
}

/// Element type
#[derive(Debug, Clone, Copy)]
pub enum ElementType {
    UnsignedByte,
    UnsignedShort,
    UnsignedInt,
}

/// WebGL2 specific extensions
impl WebGLRenderingContext {
    /// Create transform feedback (WebGL2)
    pub fn create_transform_feedback(&mut self) -> Option<u32> {
        if self.version == WebGLVersion::WebGL2 {
            Some(self.next_id())
        } else {
            None
        }
    }
    
    /// Create uniform buffer (WebGL2)
    pub fn create_uniform_buffer(&mut self) -> Option<u32> {
        if self.version == WebGLVersion::WebGL2 {
            Some(self.create_buffer())
        } else {
            None
        }
    }
    
    /// Bind buffer base (WebGL2)
    pub fn bind_buffer_base(&mut self, _target: BufferTarget, _index: u32, _buffer: u32) {
        // WebGL2 implementation
    }
}

/// WebGL extensions
#[derive(Debug, Clone)]
pub struct WebGLExtensions {
    pub anisotropic_filter: bool,
    pub vertex_array_objects: bool,
    pub instanced_arrays: bool,
    pub float_textures: bool,
    pub depth_textures: bool,
    pub compressed_textures: Vec<String>,
}

impl Default for WebGLExtensions {
    fn default() -> Self {
        Self {
            anisotropic_filter: true,
            vertex_array_objects: true,
            instanced_arrays: true,
            float_textures: true,
            depth_textures: true,
            compressed_textures: vec!["WEBGL_compressed_texture_s3tc".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_webgl_context() {
        let ctx = WebGLRenderingContext::new(WebGLVersion::WebGL1, 800, 600);
        assert_eq!(ctx.version, WebGLVersion::WebGL1);
    }
    
    #[test]
    fn test_shader_compilation() {
        let mut ctx = WebGLRenderingContext::new(WebGLVersion::WebGL1, 800, 600);
        
        let vs = ctx.create_shader(ShaderType::Vertex);
        ctx.shader_source(vs, "void main() { gl_Position = vec4(0.0); }");
        assert!(ctx.compile_shader(vs));
    }
    
    #[test]
    fn test_program_linking() {
        let mut ctx = WebGLRenderingContext::new(WebGLVersion::WebGL1, 800, 600);
        
        let vs = ctx.create_shader(ShaderType::Vertex);
        ctx.shader_source(vs, "void main() {}");
        ctx.compile_shader(vs);
        
        let fs = ctx.create_shader(ShaderType::Fragment);
        ctx.shader_source(fs, "void main() {}");
        ctx.compile_shader(fs);
        
        let program = ctx.create_program();
        ctx.attach_shader(program, vs);
        ctx.attach_shader(program, fs);
        assert!(ctx.link_program(program));
    }
    
    #[test]
    fn test_texture() {
        let mut ctx = WebGLRenderingContext::new(WebGLVersion::WebGL1, 800, 600);
        
        let tex = ctx.create_texture();
        ctx.tex_image_2d(tex, 256, 256, TextureFormat::RGBA, vec![0u8; 256*256*4]);
        ctx.generate_mipmap(tex);
        
        let t = ctx.textures.get(&tex).unwrap();
        assert!(t.mipmaps);
    }
}
