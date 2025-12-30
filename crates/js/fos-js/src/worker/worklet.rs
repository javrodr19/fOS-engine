//! Worklets
//!
//! Paint and CSS worklets.

use std::collections::HashMap;

/// Worklet type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkletType {
    Paint,
    Animation,
    Layout,
    Audio,
}

/// Worklet
#[derive(Debug)]
pub struct Worklet {
    pub worklet_type: WorkletType,
    pub modules: Vec<WorkletModule>,
}

/// Worklet module
#[derive(Debug, Clone)]
pub struct WorkletModule {
    pub url: String,
    pub code: String,
}

impl Worklet {
    pub fn new(worklet_type: WorkletType) -> Self {
        Self {
            worklet_type,
            modules: Vec::new(),
        }
    }
    
    /// Add module
    pub fn add_module(&mut self, url: &str, code: &str) {
        self.modules.push(WorkletModule {
            url: url.to_string(),
            code: code.to_string(),
        });
    }
}

/// Paint worklet
#[derive(Debug)]
pub struct PaintWorklet {
    pub name: String,
    pub input_properties: Vec<String>,
    pub context_options: PaintContextOptions,
}

/// Paint context options
#[derive(Debug, Clone, Default)]
pub struct PaintContextOptions {
    pub alpha: bool,
}

/// Paint rendering context
#[derive(Debug)]
pub struct PaintRenderingContext2D {
    pub width: f64,
    pub height: f64,
    fill_style: String,
    stroke_style: String,
}

impl PaintRenderingContext2D {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            fill_style: "#000000".to_string(),
            stroke_style: "#000000".to_string(),
        }
    }
    
    pub fn set_fill_style(&mut self, style: &str) {
        self.fill_style = style.to_string();
    }
    
    pub fn set_stroke_style(&mut self, style: &str) {
        self.stroke_style = style.to_string();
    }
    
    pub fn fill_rect(&self, _x: f64, _y: f64, _width: f64, _height: f64) {
        // Would fill rectangle
    }
    
    pub fn stroke_rect(&self, _x: f64, _y: f64, _width: f64, _height: f64) {
        // Would stroke rectangle
    }
    
    pub fn begin_path(&self) {}
    pub fn move_to(&self, _x: f64, _y: f64) {}
    pub fn line_to(&self, _x: f64, _y: f64) {}
    pub fn arc(&self, _x: f64, _y: f64, _radius: f64, _start: f64, _end: f64) {}
    pub fn fill(&self) {}
    pub fn stroke(&self) {}
}

/// Paint size
#[derive(Debug, Clone)]
pub struct PaintSize {
    pub width: f64,
    pub height: f64,
}

/// CSS Typed OM value for worklets
#[derive(Debug, Clone)]
pub enum CSSStyleValue {
    Number(f64),
    Percentage(f64),
    Length { value: f64, unit: String },
    Color(String),
    String(String),
}

/// Paint worklet global scope
#[derive(Debug, Default)]
pub struct PaintWorkletGlobalScope {
    registered_painters: HashMap<String, PaintWorklet>,
}

impl PaintWorkletGlobalScope {
    pub fn new() -> Self { Self::default() }
    
    /// Register paint
    pub fn register_paint(&mut self, name: &str, input_properties: Vec<String>) {
        let worklet = PaintWorklet {
            name: name.to_string(),
            input_properties,
            context_options: PaintContextOptions::default(),
        };
        self.registered_painters.insert(name.to_string(), worklet);
    }
    
    /// Get registered painters
    pub fn get_painters(&self) -> Vec<&str> {
        self.registered_painters.keys().map(String::as_str).collect()
    }
}

/// Animation worklet
#[derive(Debug)]
pub struct AnimationWorklet {
    pub name: String,
}

/// Animation worklet global scope
#[derive(Debug, Default)]
pub struct AnimationWorkletGlobalScope {
    registered_animators: HashMap<String, AnimationWorklet>,
}

impl AnimationWorkletGlobalScope {
    pub fn new() -> Self { Self::default() }
    
    /// Register animator
    pub fn register_animator(&mut self, name: &str) {
        self.registered_animators.insert(name.to_string(), AnimationWorklet {
            name: name.to_string(),
        });
    }
}

/// Layout worklet
#[derive(Debug)]
pub struct LayoutWorklet {
    pub name: String,
    pub input_properties: Vec<String>,
    pub child_input_properties: Vec<String>,
}

/// Layout worklet global scope
#[derive(Debug, Default)]
pub struct LayoutWorkletGlobalScope {
    registered_layouts: HashMap<String, LayoutWorklet>,
}

impl LayoutWorkletGlobalScope {
    pub fn new() -> Self { Self::default() }
    
    /// Register layout
    pub fn register_layout(&mut self, name: &str, input_props: Vec<String>, child_props: Vec<String>) {
        self.registered_layouts.insert(name.to_string(), LayoutWorklet {
            name: name.to_string(),
            input_properties: input_props,
            child_input_properties: child_props,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_paint_worklet() {
        let mut scope = PaintWorkletGlobalScope::new();
        scope.register_paint("checkerboard", vec!["--size".into()]);
        
        assert!(scope.get_painters().contains(&"checkerboard"));
    }
    
    #[test]
    fn test_paint_context() {
        let ctx = PaintRenderingContext2D::new(100.0, 100.0);
        ctx.fill_rect(0.0, 0.0, 50.0, 50.0);
    }
}
