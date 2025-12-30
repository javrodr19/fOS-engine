//! Responsive Images (srcset) Support
//!
//! Implementation of srcset and sizes attributes for responsive images.

use std::cmp::Ordering;

/// Parsed srcset entry
#[derive(Debug, Clone)]
pub struct SrcsetEntry {
    /// Image URL
    pub url: String,
    /// Width descriptor (e.g., 800w)
    pub width: Option<u32>,
    /// Pixel density descriptor (e.g., 2x)
    pub density: Option<f32>,
}

impl SrcsetEntry {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            width: None,
            density: None,
        }
    }
    
    pub fn with_width(mut self, w: u32) -> Self {
        self.width = Some(w);
        self
    }
    
    pub fn with_density(mut self, d: f32) -> Self {
        self.density = Some(d);
        self
    }
}

/// Parsed sizes entry
#[derive(Debug, Clone)]
pub struct SizesEntry {
    /// Media condition (e.g., "(max-width: 600px)")
    pub media_condition: Option<String>,
    /// Size value (e.g., "100vw", "50vw", "300px")
    pub size: String,
}

/// Responsive image resolver
#[derive(Debug)]
pub struct ResponsiveImageResolver {
    /// Parsed srcset entries
    srcset: Vec<SrcsetEntry>,
    /// Parsed sizes entries
    sizes: Vec<SizesEntry>,
}

impl ResponsiveImageResolver {
    /// Parse srcset and sizes attributes
    pub fn new(srcset: &str, sizes: Option<&str>) -> Self {
        Self {
            srcset: parse_srcset(srcset),
            sizes: sizes.map(parse_sizes).unwrap_or_default(),
        }
    }
    
    /// Select best image for given viewport and device pixel ratio
    pub fn select(&self, viewport_width: u32, device_pixel_ratio: f32) -> Option<&SrcsetEntry> {
        // Calculate effective slot width
        let slot_width = self.calculate_slot_width(viewport_width);
        let target_width = (slot_width * device_pixel_ratio) as u32;
        
        // Find best match based on width descriptors
        if self.srcset.iter().any(|e| e.width.is_some()) {
            return self.select_by_width(target_width);
        }
        
        // Fall back to density descriptors
        self.select_by_density(device_pixel_ratio)
    }
    
    fn calculate_slot_width(&self, viewport_width: u32) -> f32 {
        for entry in &self.sizes {
            // Check media condition
            if let Some(ref cond) = entry.media_condition {
                if !self.matches_media(cond, viewport_width) {
                    continue;
                }
            }
            
            // Parse size value
            return self.parse_size_value(&entry.size, viewport_width);
        }
        
        // Default to 100vw
        viewport_width as f32
    }
    
    fn matches_media(&self, condition: &str, viewport_width: u32) -> bool {
        // Simple media query matching
        if let Some(max_width) = parse_max_width(condition) {
            return viewport_width <= max_width;
        }
        if let Some(min_width) = parse_min_width(condition) {
            return viewport_width >= min_width;
        }
        true
    }
    
    fn parse_size_value(&self, size: &str, viewport_width: u32) -> f32 {
        let size = size.trim();
        
        if size.ends_with("vw") {
            let vw: f32 = size.trim_end_matches("vw").parse().unwrap_or(100.0);
            return (viewport_width as f32 * vw) / 100.0;
        }
        
        if size.ends_with("px") {
            return size.trim_end_matches("px").parse().unwrap_or(viewport_width as f32);
        }
        
        // calc() not fully supported, return viewport width
        viewport_width as f32
    }
    
    fn select_by_width(&self, target_width: u32) -> Option<&SrcsetEntry> {
        let mut candidates: Vec<_> = self.srcset.iter()
            .filter(|e| e.width.is_some())
            .collect();
        
        // Sort by width
        candidates.sort_by(|a, b| {
            a.width.cmp(&b.width)
        });
        
        // Find smallest image that's >= target width
        for entry in &candidates {
            if let Some(w) = entry.width {
                if w >= target_width {
                    return Some(entry);
                }
            }
        }
        
        // Fall back to largest
        candidates.last().copied()
    }
    
    fn select_by_density(&self, target_dpr: f32) -> Option<&SrcsetEntry> {
        let mut candidates: Vec<_> = self.srcset.iter().collect();
        
        // Sort by density
        candidates.sort_by(|a, b| {
            let da = a.density.unwrap_or(1.0);
            let db = b.density.unwrap_or(1.0);
            da.partial_cmp(&db).unwrap_or(Ordering::Equal)
        });
        
        // Find smallest density >= target
        for entry in &candidates {
            let d = entry.density.unwrap_or(1.0);
            if d >= target_dpr {
                return Some(entry);
            }
        }
        
        // Fall back to highest density
        candidates.last().copied()
    }
    
    /// Get all srcset entries
    pub fn entries(&self) -> &[SrcsetEntry] {
        &self.srcset
    }
}

/// Parse srcset attribute
fn parse_srcset(srcset: &str) -> Vec<SrcsetEntry> {
    let mut entries = Vec::new();
    
    for candidate in srcset.split(',') {
        let parts: Vec<&str> = candidate.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        
        let url = parts[0];
        let mut entry = SrcsetEntry::new(url);
        
        if parts.len() > 1 {
            let descriptor = parts[1];
            if descriptor.ends_with('w') {
                if let Ok(w) = descriptor.trim_end_matches('w').parse() {
                    entry.width = Some(w);
                }
            } else if descriptor.ends_with('x') {
                if let Ok(d) = descriptor.trim_end_matches('x').parse() {
                    entry.density = Some(d);
                }
            }
        }
        
        entries.push(entry);
    }
    
    entries
}

/// Parse sizes attribute
fn parse_sizes(sizes: &str) -> Vec<SizesEntry> {
    let mut entries = Vec::new();
    
    for size_entry in sizes.split(',') {
        let trimmed = size_entry.trim();
        
        // Check for media condition
        if let Some(paren_start) = trimmed.find('(') {
            if let Some(paren_end) = trimmed.find(')') {
                let condition = &trimmed[paren_start..=paren_end];
                let size = trimmed[paren_end + 1..].trim();
                entries.push(SizesEntry {
                    media_condition: Some(condition.to_string()),
                    size: size.to_string(),
                });
                continue;
            }
        }
        
        // No condition, just size
        entries.push(SizesEntry {
            media_condition: None,
            size: trimmed.to_string(),
        });
    }
    
    entries
}

fn parse_max_width(condition: &str) -> Option<u32> {
    if condition.contains("max-width") {
        let start = condition.find(':')?;
        let end = condition.rfind("px")?;
        condition[start + 1..end].trim().parse().ok()
    } else {
        None
    }
}

fn parse_min_width(condition: &str) -> Option<u32> {
    if condition.contains("min-width") {
        let start = condition.find(':')?;
        let end = condition.rfind("px")?;
        condition[start + 1..end].trim().parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_srcset() {
        let srcset = "small.jpg 300w, medium.jpg 600w, large.jpg 1200w";
        let entries = parse_srcset(srcset);
        
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].url, "small.jpg");
        assert_eq!(entries[0].width, Some(300));
    }
    
    #[test]
    fn test_parse_srcset_density() {
        let srcset = "image.jpg, image@2x.jpg 2x, image@3x.jpg 3x";
        let entries = parse_srcset(srcset);
        
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[1].density, Some(2.0));
    }
    
    #[test]
    fn test_responsive_resolver() {
        let resolver = ResponsiveImageResolver::new(
            "small.jpg 300w, medium.jpg 600w, large.jpg 1200w",
            Some("(max-width: 600px) 100vw, 50vw"),
        );
        
        // Small viewport
        let selected = resolver.select(400, 1.0).unwrap();
        assert_eq!(selected.url, "medium.jpg"); // 400px slot, need 400w
        
        // Large viewport
        let selected = resolver.select(1200, 1.0).unwrap();
        assert_eq!(selected.url, "medium.jpg"); // 600px slot (50vw), need 600w
    }
}
