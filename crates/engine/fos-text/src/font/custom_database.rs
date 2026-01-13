use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// Use local StringInterner to avoid cyclic dependency with fos-engine
use super::intern::{StringInterner, InternedString};

use super::parser::FontParser;
use super::{FontStyle, FontWeight, FontQuery};
use crate::{Result, TextError};

/// Unique font identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

/// Font entry in database
#[derive(Debug)]
pub struct FontEntry {
    /// Font ID
    pub id: FontId,
    /// Font family name (interned)
    pub family: InternedString,
    /// Full font name
    pub full_name: String,
    /// PostScript name
    pub postscript_name: Option<String>,
    /// Font style
    pub style: FontStyle,
    /// Font weight
    pub weight: FontWeight,
    /// Font data source
    pub source: FontSource,
    /// Index in font file (for TTC)
    pub index: u32,
}

/// Font data source
#[derive(Debug, Clone)]
pub enum FontSource {
    /// File path
    File(PathBuf),
    /// Embedded data
    Memory(Arc<Vec<u8>>),
}

/// Font database with custom implementation
pub struct CustomFontDatabase {
    /// All font entries
    fonts: Vec<FontEntry>,
    /// Family name interner
    interner: StringInterner,
    /// Index by family name
    by_family: HashMap<InternedString, Vec<FontId>>,
    /// Next font ID
    next_id: u32,
}

impl CustomFontDatabase {
    /// Create a new empty database
    pub fn new() -> Self {
        Self {
            fonts: Vec::new(),
            interner: StringInterner::new(),
            by_family: HashMap::new(),
            next_id: 0,
        }
    }
    
    /// Create with system fonts loaded
    pub fn with_system_fonts() -> Self {
        let mut db = Self::new();
        db.load_system_fonts();
        db
    }
    
    /// Load system fonts
    pub fn load_system_fonts(&mut self) {
        // Linux font directories
        #[cfg(target_os = "linux")]
        {
            let dirs = [
                "/usr/share/fonts",
                "/usr/local/share/fonts",
                "/home/*/.fonts",
                "/home/*/.local/share/fonts",
            ];
            
            for dir in dirs {
                if let Ok(expanded) = glob::glob(dir) {
                    for path in expanded.flatten() {
                        self.scan_directory(&path);
                    }
                } else if Path::new(dir).exists() {
                    self.scan_directory(Path::new(dir));
                }
            }
        }
        
        // macOS font directories
        #[cfg(target_os = "macos")]
        {
            let dirs = [
                "/System/Library/Fonts",
                "/Library/Fonts",
                "~/Library/Fonts",
            ];
            
            for dir in dirs {
                let path = if dir.starts_with("~") {
                    if let Ok(home) = std::env::var("HOME") {
                        PathBuf::from(dir.replacen("~", &home, 1))
                    } else {
                        continue;
                    }
                } else {
                    PathBuf::from(dir)
                };
                
                if path.exists() {
                    self.scan_directory(&path);
                }
            }
        }
        
        // Windows font directories
        #[cfg(target_os = "windows")]
        {
            if let Ok(windir) = std::env::var("WINDIR") {
                let fonts_dir = PathBuf::from(windir).join("Fonts");
                if fonts_dir.exists() {
                    self.scan_directory(&fonts_dir);
                }
            }
        }
    }
    
    /// Scan a directory for fonts
    fn scan_directory(&mut self, dir: &Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.scan_directory(&path);
                } else if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc" | "woff" | "woff2") {
                        let _ = self.load_font_file(&path);
                    }
                }
            }
        }
    }
    
    /// Load a font file
    pub fn load_font_file(&mut self, path: &Path) -> Result<Vec<FontId>> {
        let data = std::fs::read(path)
            .map_err(|e| TextError::FontParsing(e.to_string()))?;
        
        self.load_font_data_with_source(data, FontSource::File(path.to_path_buf()))
    }
    
    /// Load font from memory
    pub fn load_font_data(&mut self, data: Vec<u8>) -> Result<Vec<FontId>> {
        let arc = Arc::new(data);
        self.load_font_data_with_source((*arc).clone(), FontSource::Memory(arc))
    }
    
    /// Load font data with source tracking
    fn load_font_data_with_source(&mut self, data: Vec<u8>, source: FontSource) -> Result<Vec<FontId>> {
        let mut ids = Vec::new();
        
        // Check for WOFF2 or WOFF1 and decode
        let data = if super::woff2::is_woff2(&data) {
            super::woff2::decode_woff2(&data)
                .ok_or_else(|| TextError::FontParsing("Failed to decode WOFF2".into()))?
        } else if super::woff::is_woff(&data) {
            super::woff::decode_woff(&data)
                .ok_or_else(|| TextError::FontParsing("Failed to decode WOFF1".into()))?
        } else {
            data
        };
        
        // Check for TTC (multiple faces)
        let num_faces = if data.len() >= 12 && &data[0..4] == b"ttcf" {
            u32::from_be_bytes([data[8], data[9], data[10], data[11]])
        } else {
            1
        };
        
        for index in 0..num_faces {
            if let Ok(parser) = FontParser::parse_index(&data, index) {
                let id = self.add_font_from_parser(&parser, source.clone(), index)?;
                ids.push(id);
            }
        }
        
        Ok(ids)
    }
    
    /// Add font from parsed data
    fn add_font_from_parser(
        &mut self,
        parser: &FontParser,
        source: FontSource,
        index: u32,
    ) -> Result<FontId> {
        // Extract font info from name table
        let (family, full_name, postscript_name, style, weight) = 
            self.extract_font_info(parser)?;
        
        let id = FontId(self.next_id);
        self.next_id += 1;
        
        let family_interned = self.interner.intern(&family);
        
        let entry = FontEntry {
            id,
            family: family_interned.clone(),
            full_name,
            postscript_name,
            style,
            weight,
            source,
            index,
        };
        
        self.by_family.entry(family_interned).or_default().push(id);
        self.fonts.push(entry);
        
        Ok(id)
    }
    
    /// Extract font info from parser
    fn extract_font_info(&self, _parser: &FontParser) -> Result<(String, String, Option<String>, FontStyle, FontWeight)> {
        // Default values - in a full implementation, parse name table
        let family = "Unknown".to_string();
        let full_name = "Unknown Font".to_string();
        let postscript_name = None;
        let style = FontStyle::Normal;
        let weight = FontWeight::NORMAL;
        
        // Try to get OS/2 table for weight
        // This is simplified - full implementation would parse name table
        
        Ok((family, full_name, postscript_name, style, weight))
    }
    
    /// Query for a matching font
    pub fn query(&self, query: &FontQuery) -> Option<FontId> {
        // Try each family in order
        for family_name in &query.families {
            let family_interned = {
                // Look up in interner - need to check if it exists
                let mut temp_interner = StringInterner::new();
                let _id = temp_interner.intern(family_name);
                
                // Search for matching family by string comparison
                self.fonts.iter()
                    .find(|f| self.interner.get(&f.family) == Some(family_name.as_str()))
                    .map(|f| f.family.clone())
            };
            
            if let Some(family) = family_interned {
                if let Some(fonts) = self.by_family.get(&family) {
                    // Find best match for weight and style
                    let best = fonts.iter()
                        .filter_map(|id| self.font(*id))
                        .min_by_key(|f| {
                            let weight_diff = (f.weight.0 as i32 - query.weight.0 as i32).abs();
                            let style_match = if f.style == query.style { 0 } else { 1000 };
                            weight_diff + style_match
                        });
                    
                    if let Some(font) = best {
                        return Some(font.id);
                    }
                }
            }
        }
        
        // Return first available font as fallback
        self.fonts.first().map(|f| f.id)
    }
    
    /// Get font by ID
    pub fn font(&self, id: FontId) -> Option<&FontEntry> {
        self.fonts.iter().find(|f| f.id == id)
    }
    
    /// Get font data by ID
    pub fn with_face_data<R>(&self, id: FontId, f: impl FnOnce(&[u8], u32) -> R) -> Option<R> {
        let font = self.font(id)?;
        
        match &font.source {
            FontSource::File(path) => {
                let data = std::fs::read(path).ok()?;
                Some(f(&data, font.index))
            }
            FontSource::Memory(data) => {
                Some(f(data, font.index))
            }
        }
    }
    
    /// List all font families
    pub fn families(&self) -> impl Iterator<Item = &str> {
        self.by_family.keys()
            .filter_map(|id| self.interner.get(id))
    }
    
    /// Number of fonts
    pub fn len(&self) -> usize {
        self.fonts.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl Default for CustomFontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_empty_database() {
        let db = CustomFontDatabase::new();
        assert!(db.is_empty());
    }
    
    #[test]
    fn test_query_empty() {
        let db = CustomFontDatabase::new();
        let query = FontQuery::new(&["Arial"]);
        assert!(db.query(&query).is_none());
    }
}
