//! SVG Image Support
//!
//! SVG rendering using resvg-like approach for vector graphics.

use std::collections::HashMap;

/// SVG image container
#[derive(Debug, Clone)]
pub struct SvgImage {
    /// Original SVG source
    pub source: String,
    /// Parsed viewBox
    pub view_box: Option<ViewBox>,
    /// Intrinsic width (if specified)
    pub width: Option<f32>,
    /// Intrinsic height (if specified)
    pub height: Option<f32>,
    /// Parsed elements (simplified)
    pub elements: Vec<SvgElement>,
}

/// SVG viewBox
#[derive(Debug, Clone, Copy)]
pub struct ViewBox {
    pub min_x: f32,
    pub min_y: f32,
    pub width: f32,
    pub height: f32,
}

impl ViewBox {
    pub fn new(min_x: f32, min_y: f32, width: f32, height: f32) -> Self {
        Self { min_x, min_y, width, height }
    }
    
    /// Get aspect ratio
    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0.0 { 1.0 } else { self.width / self.height }
    }
}

/// SVG element (simplified representation)
#[derive(Debug, Clone)]
pub enum SvgElement {
    Rect { x: f32, y: f32, width: f32, height: f32, fill: Option<String>, stroke: Option<String> },
    Circle { cx: f32, cy: f32, r: f32, fill: Option<String>, stroke: Option<String> },
    Ellipse { cx: f32, cy: f32, rx: f32, ry: f32, fill: Option<String>, stroke: Option<String> },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, stroke: Option<String> },
    Polyline { points: Vec<(f32, f32)>, stroke: Option<String> },
    Polygon { points: Vec<(f32, f32)>, fill: Option<String>, stroke: Option<String> },
    Path { d: String, fill: Option<String>, stroke: Option<String> },
    Text { x: f32, y: f32, content: String, font_size: f32, fill: Option<String> },
    Group { transform: Option<String>, elements: Vec<SvgElement> },
    Image { href: String, x: f32, y: f32, width: f32, height: f32 },
    Use { href: String, x: f32, y: f32 },
}

/// SVG decoder/parser
#[derive(Debug, Default)]
pub struct SvgDecoder {
    /// Cached definitions (for <use> elements)
    definitions: HashMap<String, SvgElement>,
}

impl SvgDecoder {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Parse SVG from string
    pub fn parse(&mut self, svg: &str) -> Result<SvgImage, SvgError> {
        // In a real implementation, use an XML parser like quick-xml
        // and parse the full SVG spec
        
        // Basic parsing for now
        let view_box = self.parse_view_box(svg);
        let width = self.parse_dimension(svg, "width");
        let height = self.parse_dimension(svg, "height");
        
        Ok(SvgImage {
            source: svg.to_string(),
            view_box,
            width,
            height,
            elements: Vec::new(), // Would be populated by full parser
        })
    }
    
    fn parse_view_box(&self, svg: &str) -> Option<ViewBox> {
        // Simple regex-like extraction
        if let Some(start) = svg.find("viewBox=\"") {
            let rest = &svg[start + 9..];
            if let Some(end) = rest.find('"') {
                let vb = &rest[..end];
                let parts: Vec<f32> = vb.split_whitespace()
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if parts.len() >= 4 {
                    return Some(ViewBox::new(parts[0], parts[1], parts[2], parts[3]));
                }
            }
        }
        None
    }
    
    fn parse_dimension(&self, svg: &str, attr: &str) -> Option<f32> {
        let pattern = format!("{}=\"", attr);
        if let Some(start) = svg.find(&pattern) {
            let rest = &svg[start + pattern.len()..];
            if let Some(end) = rest.find('"') {
                let value = &rest[..end];
                // Remove units
                let num: String = value.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                return num.parse().ok();
            }
        }
        None
    }
    
    /// Render SVG to pixel buffer
    pub fn render(&self, svg: &SvgImage, width: u32, height: u32) -> Vec<u8> {
        // In a real implementation, use tiny-skia to rasterize
        // For now, return empty RGBA buffer
        vec![0u8; (width * height * 4) as usize]
    }
}

/// Check if data is SVG
pub fn is_svg(data: &[u8]) -> bool {
    // Check for XML declaration or <svg tag
    let text = std::str::from_utf8(&data[..data.len().min(100)]).unwrap_or("");
    text.contains("<svg") || text.contains("<?xml")
}

/// SVG errors
#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Invalid element: {0}")]
    InvalidElement(String),
    #[error("Render error: {0}")]
    Render(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_svg_decoder() {
        let mut decoder = SvgDecoder::new();
        let svg = r#"<svg viewBox="0 0 100 100" width="200" height="200"></svg>"#;
        let result = decoder.parse(svg).unwrap();
        
        assert!(result.view_box.is_some());
        assert_eq!(result.view_box.unwrap().width, 100.0);
        assert_eq!(result.width, Some(200.0));
    }
    
    #[test]
    fn test_is_svg() {
        assert!(is_svg(b"<svg></svg>"));
        assert!(is_svg(b"<?xml version=\"1.0\"?>"));
        assert!(!is_svg(b"\x89PNG"));
    }
}
