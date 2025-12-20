//! WebGL Module
//!
//! WebGL 1.0 implementation.

pub mod context;
pub mod framebuffer;
pub mod extensions;

pub use context::{
    WebGLRenderingContext, WebGLProgram, WebGLShader, WebGLBuffer,
    WebGLTexture, WebGLUniformLocation, ShaderType, BufferTarget,
    BufferUsage, TextureFormat,
    GL_VERTEX_SHADER, GL_FRAGMENT_SHADER, GL_ARRAY_BUFFER,
    GL_ELEMENT_ARRAY_BUFFER, GL_STATIC_DRAW, GL_TRIANGLES,
};
pub use framebuffer::{
    WebGLFramebuffer, WebGLRenderbuffer, RenderbufferFormat,
    GL_FRAMEBUFFER, GL_RENDERBUFFER, GL_FRAMEBUFFER_COMPLETE,
};
pub use extensions::{
    ExtensionRegistry, OesVertexArrayObject, AngleInstancedArrays,
};
