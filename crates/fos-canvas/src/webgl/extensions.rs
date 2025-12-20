//! WebGL Extensions
//!
//! Common WebGL extensions.

use std::collections::HashMap;

/// Extension registry
#[derive(Debug, Default)]
pub struct ExtensionRegistry {
    extensions: HashMap<String, Extension>,
}

/// WebGL Extension
#[derive(Debug, Clone)]
pub struct Extension {
    pub name: String,
    pub supported: bool,
}

// Common WebGL 1.0 extensions
pub const OES_TEXTURE_FLOAT: &str = "OES_texture_float";
pub const OES_TEXTURE_HALF_FLOAT: &str = "OES_texture_half_float";
pub const OES_STANDARD_DERIVATIVES: &str = "OES_standard_derivatives";
pub const OES_VERTEX_ARRAY_OBJECT: &str = "OES_vertex_array_object";
pub const OES_ELEMENT_INDEX_UINT: &str = "OES_element_index_uint";
pub const WEBGL_DEPTH_TEXTURE: &str = "WEBGL_depth_texture";
pub const WEBGL_LOSE_CONTEXT: &str = "WEBGL_lose_context";
pub const WEBGL_COMPRESSED_TEXTURE_S3TC: &str = "WEBGL_compressed_texture_s3tc";
pub const EXT_TEXTURE_FILTER_ANISOTROPIC: &str = "EXT_texture_filter_anisotropic";
pub const ANGLE_INSTANCED_ARRAYS: &str = "ANGLE_instanced_arrays";

impl ExtensionRegistry {
    pub fn new() -> Self {
        let mut registry = Self::default();
        
        // Register supported extensions
        registry.register(OES_TEXTURE_FLOAT, true);
        registry.register(OES_TEXTURE_HALF_FLOAT, true);
        registry.register(OES_STANDARD_DERIVATIVES, true);
        registry.register(OES_VERTEX_ARRAY_OBJECT, true);
        registry.register(OES_ELEMENT_INDEX_UINT, true);
        registry.register(WEBGL_DEPTH_TEXTURE, true);
        registry.register(WEBGL_LOSE_CONTEXT, true);
        registry.register(ANGLE_INSTANCED_ARRAYS, true);
        
        registry
    }
    
    /// Register an extension
    pub fn register(&mut self, name: &str, supported: bool) {
        self.extensions.insert(name.to_string(), Extension {
            name: name.to_string(),
            supported,
        });
    }
    
    /// Get supported extensions
    pub fn get_supported_extensions(&self) -> Vec<&str> {
        self.extensions.iter()
            .filter(|(_, ext)| ext.supported)
            .map(|(name, _)| name.as_str())
            .collect()
    }
    
    /// Check if extension is supported
    pub fn is_supported(&self, name: &str) -> bool {
        self.extensions.get(name)
            .map(|ext| ext.supported)
            .unwrap_or(false)
    }
    
    /// Get extension
    pub fn get_extension(&self, name: &str) -> Option<&Extension> {
        self.extensions.get(name).filter(|ext| ext.supported)
    }
}

/// OES_vertex_array_object extension
#[derive(Debug)]
pub struct OesVertexArrayObject {
    next_id: u32,
}

/// Vertex Array Object
#[derive(Debug, Clone)]
pub struct WebGLVertexArrayObject {
    pub id: u32,
}

impl OesVertexArrayObject {
    pub fn new() -> Self {
        Self { next_id: 1 }
    }
    
    pub fn create_vertex_array(&mut self) -> WebGLVertexArrayObject {
        let id = self.next_id;
        self.next_id += 1;
        WebGLVertexArrayObject { id }
    }
    
    pub fn delete_vertex_array(&self, _vao: WebGLVertexArrayObject) {}
    pub fn bind_vertex_array(&self, _vao: Option<&WebGLVertexArrayObject>) {}
    pub fn is_vertex_array(&self, _vao: &WebGLVertexArrayObject) -> bool { true }
}

impl Default for OesVertexArrayObject {
    fn default() -> Self {
        Self::new()
    }
}

/// ANGLE_instanced_arrays extension
#[derive(Debug, Default)]
pub struct AngleInstancedArrays;

impl AngleInstancedArrays {
    pub fn new() -> Self { Self }
    
    pub fn draw_arrays_instanced(&self, _mode: u32, _first: i32, _count: i32, _primcount: i32) {}
    pub fn draw_elements_instanced(&self, _mode: u32, _count: i32, _type: u32, _offset: i32, _primcount: i32) {}
    pub fn vertex_attrib_divisor(&self, _index: u32, _divisor: u32) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extension_registry() {
        let registry = ExtensionRegistry::new();
        
        assert!(registry.is_supported(OES_TEXTURE_FLOAT));
        assert!(registry.is_supported(OES_VERTEX_ARRAY_OBJECT));
        assert!(!registry.is_supported("UNSUPPORTED_EXT"));
    }
    
    #[test]
    fn test_vao_extension() {
        let mut vao_ext = OesVertexArrayObject::new();
        let vao = vao_ext.create_vertex_array();
        
        assert!(vao_ext.is_vertex_array(&vao));
    }
}
