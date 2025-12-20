//! Font database for loading and managing fonts

use std::sync::Arc;
use fontdb::{Database, FaceInfo, Source};
use super::{FontId, FontQuery};
use crate::{Result, TextError};

/// Font database for loading and matching fonts
pub struct FontDatabase {
    db: Database,
}

impl FontDatabase {
    /// Create a new empty font database
    pub fn new() -> Self {
        Self {
            db: Database::new(),
        }
    }
    
    /// Create a font database with system fonts loaded
    pub fn with_system_fonts() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        Self { db }
    }
    
    /// Load system fonts into the database
    pub fn load_system_fonts(&mut self) {
        self.db.load_system_fonts();
    }
    
    /// Load a font from file
    pub fn load_font_file(&mut self, path: &std::path::Path) -> Result<()> {
        self.db.load_font_file(path)
            .map_err(|e| TextError::FontParsing(e.to_string()))
    }
    
    /// Load a font from memory
    pub fn load_font_data(&mut self, data: Arc<dyn AsRef<[u8]> + Send + Sync>) {
        self.db.load_font_source(Source::Binary(data));
    }
    
    /// Find a font matching the query
    pub fn query(&self, query: &FontQuery) -> Option<FontId> {
        let families: Vec<fontdb::Family> = query.families
            .iter()
            .map(|f| fontdb::Family::Name(f.as_str()))
            .collect();
        
        self.db.query(&fontdb::Query {
            families: &families,
            weight: fontdb::Weight(query.weight.0),
            stretch: fontdb::Stretch::Normal,
            style: query.style.into(),
        }).map(FontId)
    }
    
    /// Get font info by ID
    pub fn face_info(&self, id: FontId) -> Option<&FaceInfo> {
        self.db.face(id.0)
    }
    
    /// Get font data by ID (for shaping)
    pub fn with_face_data<R>(&self, id: FontId, f: impl FnOnce(&[u8], u32) -> R) -> Option<R> {
        self.db.with_face_data(id.0, f)
    }
    
    /// List all loaded font families
    pub fn families(&self) -> impl Iterator<Item = &str> {
        self.db.faces().filter_map(|f| f.families.first().map(|(name, _)| name.as_str()))
    }
    
    /// Number of loaded fonts
    pub fn len(&self) -> usize {
        self.db.len()
    }
    
    /// Check if database is empty
    pub fn is_empty(&self) -> bool {
        self.db.len() == 0
    }
}

impl Default for FontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_system_fonts() {
        let db = FontDatabase::with_system_fonts();
        // Should have loaded some fonts
        assert!(db.len() > 0, "No system fonts found");
    }
    
    #[test]
    fn test_query_sans_serif() {
        let db = FontDatabase::with_system_fonts();
        if db.len() == 0 {
            // Skip on systems without fonts
            return;
        }
        let query = FontQuery::new(&["sans-serif", "Arial", "DejaVu Sans", "Liberation Sans"]);
        // Just check that query doesn't panic - result depends on installed fonts
        let _ = db.query(&query);
    }
}
