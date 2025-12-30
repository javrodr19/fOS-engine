//! Ruby Annotations
//!
//! CJK ruby text support for furigana and similar annotations.

/// Ruby annotation
#[derive(Debug, Clone)]
pub struct RubyAnnotation {
    /// Base text
    pub base: String,
    /// Ruby text (annotation)
    pub text: String,
    /// Position
    pub position: RubyPosition,
}

/// Ruby position
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RubyPosition {
    #[default]
    Over,
    Under,
    InterCharacter,
}

/// Ruby style
#[derive(Debug, Clone, Default)]
pub struct RubyStyle {
    pub position: RubyPosition,
    pub align: RubyAlign,
    pub merge: bool,
}

/// Ruby alignment
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RubyAlign {
    #[default]
    SpaceAround,
    Start,
    Center,
    SpaceBetween,
}

/// Ruby container for layout
#[derive(Debug, Clone)]
pub struct RubyContainer {
    pub bases: Vec<RubyBase>,
    pub style: RubyStyle,
}

/// Ruby base element
#[derive(Debug, Clone)]
pub struct RubyBase {
    pub text: String,
    pub annotation: Option<String>,
    pub width: f32,
    pub annotation_width: f32,
}

impl RubyContainer {
    pub fn new(style: RubyStyle) -> Self {
        Self {
            bases: Vec::new(),
            style,
        }
    }
    
    /// Add a base with annotation
    pub fn add(&mut self, base: &str, annotation: Option<&str>) {
        self.bases.push(RubyBase {
            text: base.to_string(),
            annotation: annotation.map(|s| s.to_string()),
            width: 0.0,
            annotation_width: 0.0,
        });
    }
    
    /// Calculate total width
    pub fn total_width(&self) -> f32 {
        self.bases.iter()
            .map(|b| b.width.max(b.annotation_width))
            .sum()
    }
    
    /// Layout ruby annotations
    pub fn layout(&mut self, base_widths: &[f32], annotation_widths: &[f32]) {
        for (i, base) in self.bases.iter_mut().enumerate() {
            base.width = base_widths.get(i).copied().unwrap_or(0.0);
            base.annotation_width = annotation_widths.get(i).copied().unwrap_or(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ruby_container() {
        let mut container = RubyContainer::new(RubyStyle::default());
        container.add("東", Some("とう"));
        container.add("京", Some("きょう"));
        
        assert_eq!(container.bases.len(), 2);
    }
}
