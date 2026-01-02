//! WebGL Resource Pool
//!
//! Object pooling for WebGL resources to reduce allocation overhead.
//!
//! Uses local Pool for efficient recycling of:
//! - Buffers
//! - Textures
//! - Shaders
//! - Programs

use crate::pool::Pool;
use super::context::{WebGLBuffer, WebGLTexture, WebGLShader, WebGLProgram, ShaderType, BufferTarget, BufferUsage, TextureFormat};
use std::sync::Mutex;

/// WebGL resource pool for efficient allocation/deallocation
#[derive(Default)]
pub struct WebGLResourcePool {
    /// Buffer pool
    buffers: Mutex<Pool<WebGLBuffer>>,
    /// Texture pool
    textures: Mutex<Pool<WebGLTexture>>,
    /// Shader pool
    shaders: Mutex<Pool<WebGLShader>>,
    /// Program pool
    programs: Mutex<Pool<WebGLProgram>>,
    /// Next IDs
    next_buffer_id: Mutex<u32>,
    next_texture_id: Mutex<u32>,
    next_shader_id: Mutex<u32>,
    next_program_id: Mutex<u32>,
}

impl WebGLResourcePool {
    /// Create a new resource pool
    pub fn new() -> Self {
        Self::default()
    }

    /// Create pool with initial capacities
    pub fn with_capacity(buffers: usize, textures: usize, shaders: usize, programs: usize) -> Self {
        Self {
            buffers: Mutex::new(Pool::with_capacity(buffers)),
            textures: Mutex::new(Pool::with_capacity(textures)),
            shaders: Mutex::new(Pool::with_capacity(shaders)),
            programs: Mutex::new(Pool::with_capacity(programs)),
            next_buffer_id: Mutex::new(1),
            next_texture_id: Mutex::new(1),
            next_shader_id: Mutex::new(1),
            next_program_id: Mutex::new(1),
        }
    }

    // Buffer management

    /// Acquire a buffer from the pool
    pub fn acquire_buffer(&self) -> WebGLBuffer {
        let mut pool = self.buffers.lock().unwrap();
        if let Some(mut buffer) = pool.get() {
            // Reset buffer state
            buffer.data.clear();
            buffer.target = BufferTarget::ArrayBuffer;
            buffer.usage = BufferUsage::StaticDraw;
            buffer
        } else {
            let mut id = self.next_buffer_id.lock().unwrap();
            let buffer = WebGLBuffer {
                id: *id,
                target: BufferTarget::ArrayBuffer,
                data: Vec::new(),
                usage: BufferUsage::StaticDraw,
            };
            *id += 1;
            buffer
        }
    }

    /// Release a buffer back to the pool
    pub fn release_buffer(&self, mut buffer: WebGLBuffer) {
        buffer.data.clear(); // Free memory
        self.buffers.lock().unwrap().put(buffer);
    }

    // Texture management

    /// Acquire a texture from the pool
    pub fn acquire_texture(&self) -> WebGLTexture {
        let mut pool = self.textures.lock().unwrap();
        if let Some(mut texture) = pool.get() {
            texture.data.clear();
            texture.width = 0;
            texture.height = 0;
            texture.format = TextureFormat::Rgba;
            texture
        } else {
            let mut id = self.next_texture_id.lock().unwrap();
            let texture = WebGLTexture {
                id: *id,
                width: 0,
                height: 0,
                format: TextureFormat::Rgba,
                data: Vec::new(),
            };
            *id += 1;
            texture
        }
    }

    /// Release a texture back to the pool
    pub fn release_texture(&self, mut texture: WebGLTexture) {
        texture.data.clear();
        self.textures.lock().unwrap().put(texture);
    }

    // Shader management

    /// Acquire a shader from the pool
    pub fn acquire_shader(&self, shader_type: ShaderType) -> WebGLShader {
        let mut pool = self.shaders.lock().unwrap();
        if let Some(mut shader) = pool.get() {
            shader.shader_type = shader_type;
            shader.source.clear();
            shader.compiled = false;
            shader
        } else {
            let mut id = self.next_shader_id.lock().unwrap();
            let shader = WebGLShader {
                id: *id,
                shader_type,
                source: String::new(),
                compiled: false,
            };
            *id += 1;
            shader
        }
    }

    /// Release a shader back to the pool
    pub fn release_shader(&self, mut shader: WebGLShader) {
        shader.source.clear();
        self.shaders.lock().unwrap().put(shader);
    }

    // Program management

    /// Acquire a program from the pool
    pub fn acquire_program(&self) -> WebGLProgram {
        let mut pool = self.programs.lock().unwrap();
        if let Some(mut program) = pool.get() {
            program.vertex_shader = None;
            program.fragment_shader = None;
            program.linked = false;
            program
        } else {
            let mut id = self.next_program_id.lock().unwrap();
            let program = WebGLProgram {
                id: *id,
                vertex_shader: None,
                fragment_shader: None,
                linked: false,
            };
            *id += 1;
            program
        }
    }

    /// Release a program back to the pool
    pub fn release_program(&self, mut program: WebGLProgram) {
        // Release attached shaders
        if let Some(vs) = program.vertex_shader.take() {
            self.release_shader(vs);
        }
        if let Some(fs) = program.fragment_shader.take() {
            self.release_shader(fs);
        }
        self.programs.lock().unwrap().put(program);
    }

    // Pool statistics

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            buffers_available: self.buffers.lock().unwrap().available(),
            textures_available: self.textures.lock().unwrap().available(),
            shaders_available: self.shaders.lock().unwrap().available(),
            programs_available: self.programs.lock().unwrap().available(),
        }
    }

    /// Clear all pools
    pub fn clear(&self) {
        self.buffers.lock().unwrap().clear();
        self.textures.lock().unwrap().clear();
        self.shaders.lock().unwrap().clear();
        self.programs.lock().unwrap().clear();
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub buffers_available: usize,
    pub textures_available: usize,
    pub shaders_available: usize,
    pub programs_available: usize,
}

/// Pooled WebGL context wrapping standard context with pooling
pub struct PooledWebGLContext {
    pool: WebGLResourcePool,
}

impl PooledWebGLContext {
    pub fn new() -> Self {
        Self {
            pool: WebGLResourcePool::new(),
        }
    }

    /// Create buffer from pool
    pub fn create_buffer(&self) -> WebGLBuffer {
        self.pool.acquire_buffer()
    }

    /// Delete buffer (returns to pool)
    pub fn delete_buffer(&self, buffer: WebGLBuffer) {
        self.pool.release_buffer(buffer);
    }

    /// Create texture from pool
    pub fn create_texture(&self) -> WebGLTexture {
        self.pool.acquire_texture()
    }

    /// Delete texture (returns to pool)
    pub fn delete_texture(&self, texture: WebGLTexture) {
        self.pool.release_texture(texture);
    }

    /// Create shader from pool
    pub fn create_shader(&self, shader_type: ShaderType) -> WebGLShader {
        self.pool.acquire_shader(shader_type)
    }

    /// Delete shader (returns to pool)
    pub fn delete_shader(&self, shader: WebGLShader) {
        self.pool.release_shader(shader);
    }

    /// Create program from pool
    pub fn create_program(&self) -> WebGLProgram {
        self.pool.acquire_program()
    }

    /// Delete program (returns to pool)
    pub fn delete_program(&self, program: WebGLProgram) {
        self.pool.release_program(program);
    }

    /// Get pool stats
    pub fn pool_stats(&self) -> PoolStats {
        self.pool.stats()
    }
}

impl Default for PooledWebGLContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pooling() {
        let pool = WebGLResourcePool::new();

        // Acquire buffer
        let buf1 = pool.acquire_buffer();
        let id1 = buf1.id;

        // Release buffer
        pool.release_buffer(buf1);

        // Acquire again - should get recycled buffer
        let buf2 = pool.acquire_buffer();
        assert_eq!(buf2.id, id1);
    }

    #[test]
    fn test_texture_pooling() {
        let pool = WebGLResourcePool::new();

        let tex = pool.acquire_texture();
        assert_eq!(tex.width, 0);
        assert_eq!(tex.height, 0);

        pool.release_texture(tex);
        assert_eq!(pool.stats().textures_available, 1);
    }

    #[test]
    fn test_shader_pooling() {
        let pool = WebGLResourcePool::new();

        let vs = pool.acquire_shader(ShaderType::Vertex);
        let fs = pool.acquire_shader(ShaderType::Fragment);

        assert_eq!(vs.shader_type, ShaderType::Vertex);
        assert_eq!(fs.shader_type, ShaderType::Fragment);

        pool.release_shader(vs);
        pool.release_shader(fs);

        assert_eq!(pool.stats().shaders_available, 2);
    }

    #[test]
    fn test_pooled_context() {
        let ctx = PooledWebGLContext::new();

        let buf = ctx.create_buffer();
        ctx.delete_buffer(buf);

        let tex = ctx.create_texture();
        ctx.delete_texture(tex);

        let stats = ctx.pool_stats();
        assert_eq!(stats.buffers_available, 1);
        assert_eq!(stats.textures_available, 1);
    }
}
