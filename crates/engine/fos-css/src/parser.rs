//! CSS Parser using lightningcss
//!
//! Parses CSS stylesheets into our internal representation.

use crate::{Stylesheet, Rule, Selector, Declaration, Specificity, CssError};
use crate::properties::{PropertyId, PropertyValue, Keyword, Length, LengthUnit, Color};

/// CSS Parser
pub struct CssParser;

impl CssParser {
    pub fn new() -> Self {
        Self
    }
    
    /// Parse a CSS stylesheet
    pub fn parse(&self, css: &str) -> Result<Stylesheet, CssError> {
        use lightningcss::stylesheet::{StyleSheet, ParserOptions};
        
        let options = ParserOptions::default();
        
        let stylesheet = StyleSheet::parse(css, options)
            .map_err(|e| CssError::ParseError {
                line: 0,
                message: format!("{:?}", e),
            })?;
        
        let mut result = Stylesheet::new();
        
        // Convert lightningcss rules to our format
        for rule in stylesheet.rules.0.iter() {
            if let Some(converted) = self.convert_rule(rule) {
                result.rules.push(converted);
            }
        }
        
        Ok(result)
    }
    
    fn convert_rule(&self, rule: &lightningcss::rules::CssRule) -> Option<Rule> {
        use lightningcss::rules::CssRule;
        
        match rule {
            CssRule::Style(style_rule) => {
                let selectors = self.convert_selectors(&style_rule.selectors);
                let declarations = self.convert_declarations(&style_rule.declarations);
                
                Some(Rule { selectors, declarations })
            }
            // Skip other rule types for now (media queries, keyframes, etc.)
            _ => None,
        }
    }
    
    fn convert_selectors(&self, selectors: &lightningcss::selector::SelectorList) -> Vec<Selector> {
        selectors.0.iter().map(|sel| {
            let text = format!("{:?}", sel); // Use debug for now
            let specificity = Specificity::default();
            let parts = Vec::new(); // Simplified - would parse selector components
            
            Selector { text, specificity, parts }
        }).collect()
    }
    
    fn convert_declarations(&self, declarations: &lightningcss::declaration::DeclarationBlock) -> Vec<Declaration> {
        let mut result = Vec::new();
        
        // Process declarations
        for decl in declarations.declarations.iter() {
            if let Some(converted) = self.convert_declaration(decl, false) {
                result.push(converted);
            }
        }
        
        // Process important declarations
        for decl in declarations.important_declarations.iter() {
            if let Some(converted) = self.convert_declaration(decl, true) {
                result.push(converted);
            }
        }
        
        result
    }
    
    fn convert_declaration(&self, decl: &lightningcss::properties::Property, important: bool) -> Option<Declaration> {
        use lightningcss::properties::Property;
        
        match decl {
            Property::Display(display) => {
                // Handle Display which can have complex values
                let keyword = self.display_to_keyword(display)?;
                Some(Declaration {
                    property: PropertyId::Display,
                    value: PropertyValue::Keyword(keyword),
                    important,
                })
            }
            Property::Color(color) => {
                if let Some(converted) = self.convert_color(color) {
                    Some(Declaration {
                        property: PropertyId::Color,
                        value: PropertyValue::Color(converted),
                        important,
                    })
                } else {
                    None
                }
            }
            Property::BackgroundColor(color) => {
                if let Some(converted) = self.convert_color(color) {
                    Some(Declaration {
                        property: PropertyId::BackgroundColor,
                        value: PropertyValue::Color(converted),
                        important,
                    })
                } else {
                    None
                }
            }
            Property::Width(size) => {
                if let Some(length) = self.convert_size(size) {
                    Some(Declaration {
                        property: PropertyId::Width,
                        value: length,
                        important,
                    })
                } else {
                    None
                }
            }
            Property::Height(size) => {
                if let Some(length) = self.convert_size(size) {
                    Some(Declaration {
                        property: PropertyId::Height,
                        value: length,
                        important,
                    })
                } else {
                    None
                }
            }
            Property::Unparsed(unparsed) => {
                let property_name = unparsed.property_id.name();
                if let Some(property_id) = PropertyId::from_name(property_name) {
                    Some(Declaration {
                        property: property_id,
                        value: PropertyValue::Raw(format!("{:?}", unparsed.value)),
                        important,
                    })
                } else {
                    None
                }
            }
            _ => None, // Skip unsupported properties for now
        }
    }
    
    fn display_to_keyword(&self, display: &lightningcss::properties::display::Display) -> Option<Keyword> {
        // Convert display to string and match
        let display_str = format!("{:?}", display).to_lowercase();
        
        if display_str.contains("none") {
            Some(Keyword::None)
        } else if display_str.contains("flex") {
            Some(Keyword::Flex)
        } else if display_str.contains("grid") {
            Some(Keyword::Grid)
        } else if display_str.contains("inline-block") || display_str.contains("inlineblock") {
            Some(Keyword::InlineBlock)
        } else if display_str.contains("inline") {
            Some(Keyword::Inline)
        } else if display_str.contains("block") {
            Some(Keyword::Block)
        } else if display_str.contains("contents") {
            Some(Keyword::Contents)
        } else {
            None
        }
    }
    
    fn convert_color(&self, color: &lightningcss::values::color::CssColor) -> Option<Color> {
        use lightningcss::values::color::CssColor;
        
        match color {
            CssColor::RGBA(rgba) => {
                Some(Color::rgba(rgba.red, rgba.green, rgba.blue, rgba.alpha))
            }
            CssColor::CurrentColor => {
                // Would need to inherit from parent
                None
            }
            _ => None,
        }
    }
    
    fn convert_size(&self, size: &lightningcss::properties::size::Size) -> Option<PropertyValue> {
        use lightningcss::properties::size::Size;
        
        match size {
            Size::Auto => Some(PropertyValue::Keyword(Keyword::Auto)),
            Size::LengthPercentage(lp) => {
                self.convert_length_percentage(lp)
            }
            _ => None,
        }
    }
    
    fn convert_length_percentage(&self, lp: &lightningcss::values::length::LengthPercentage) -> Option<PropertyValue> {
        use lightningcss::values::length::LengthPercentage;
        
        match lp {
            LengthPercentage::Dimension(dim) => {
                // The Dimension type has a value() method or direct access
                let debug_str = format!("{:?}", dim);
                // Parse the debug string to get value - hacky but works for now
                if let Some(value) = self.parse_dimension_value(&debug_str) {
                    let unit = self.parse_dimension_unit(&debug_str);
                    Some(PropertyValue::Length(Length { value, unit }))
                } else {
                    None
                }
            }
            LengthPercentage::Percentage(p) => {
                Some(PropertyValue::Length(Length { value: p.0 * 100.0, unit: LengthUnit::Percent }))
            }
            _ => None,
        }
    }
    
    fn parse_dimension_value(&self, s: &str) -> Option<f32> {
        // Extract numeric value from debug string like "Dimension { value: 10.0, unit: Px }"
        let num_str: String = s.chars()
            .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .collect();
        num_str.parse().ok()
    }
    
    fn parse_dimension_unit(&self, s: &str) -> LengthUnit {
        let s_lower = s.to_lowercase();
        if s_lower.contains("px") {
            LengthUnit::Px
        } else if s_lower.contains("em") {
            LengthUnit::Em
        } else if s_lower.contains("rem") {
            LengthUnit::Rem
        } else if s_lower.contains("percent") || s_lower.contains("%") {
            LengthUnit::Percent
        } else if s_lower.contains("vw") {
            LengthUnit::Vw
        } else if s_lower.contains("vh") {
            LengthUnit::Vh
        } else {
            LengthUnit::Px
        }
    }
}

impl Default for CssParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple() {
        let css = r#"
            .foo { display: block; }
            #bar { color: red; }
        "#;
        
        let result = CssParser::new().parse(css);
        assert!(result.is_ok(), "Parse error: {:?}", result.err());
        
        let stylesheet = result.unwrap();
        assert_eq!(stylesheet.len(), 2);
    }
    
    #[test]
    fn test_parse_colors() {
        let css = r#"
            div { 
                color: #ff0000;
                background-color: blue;
            }
        "#;
        
        let result = CssParser::new().parse(css);
        assert!(result.is_ok(), "Parse error: {:?}", result.err());
    }
}
