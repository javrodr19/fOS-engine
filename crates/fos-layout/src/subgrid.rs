//! CSS Subgrid
//!
//! Subgrid layout for nested grid alignment.

/// Subgrid configuration
#[derive(Debug, Clone)]
pub struct Subgrid {
    /// Whether columns are subgrid
    pub columns: bool,
    /// Whether rows are subgrid  
    pub rows: bool,
    /// Parent grid reference
    pub parent_tracks: Option<ParentTracks>,
}

/// Parent track information
#[derive(Debug, Clone)]
pub struct ParentTracks {
    pub column_count: usize,
    pub row_count: usize,
    pub column_sizes: Vec<f32>,
    pub row_sizes: Vec<f32>,
    pub column_gaps: Vec<f32>,
    pub row_gaps: Vec<f32>,
}

/// Subgrid context for layout
#[derive(Debug, Clone)]
pub struct SubgridContext {
    /// Start column in parent
    pub column_start: usize,
    /// End column in parent
    pub column_end: usize,
    /// Start row in parent
    pub row_start: usize,
    /// End row in parent
    pub row_end: usize,
    /// Inherited column tracks
    pub inherited_columns: Vec<f32>,
    /// Inherited row tracks
    pub inherited_rows: Vec<f32>,
}

impl Subgrid {
    pub fn new() -> Self {
        Self {
            columns: false,
            rows: false,
            parent_tracks: None,
        }
    }
    
    /// Enable column subgrid
    pub fn with_columns(mut self) -> Self {
        self.columns = true;
        self
    }
    
    /// Enable row subgrid
    pub fn with_rows(mut self) -> Self {
        self.rows = true;
        self
    }
    
    /// Check if any subgrid is enabled
    pub fn is_subgrid(&self) -> bool {
        self.columns || self.rows
    }
}

impl SubgridContext {
    /// Create context from parent placement
    pub fn from_placement(
        column_start: usize,
        column_end: usize,
        row_start: usize,
        row_end: usize,
        parent: &ParentTracks,
    ) -> Self {
        let inherited_columns: Vec<f32> = parent.column_sizes
            .get(column_start..column_end)
            .map(|s| s.to_vec())
            .unwrap_or_default();
            
        let inherited_rows: Vec<f32> = parent.row_sizes
            .get(row_start..row_end)
            .map(|s| s.to_vec())
            .unwrap_or_default();
        
        Self {
            column_start,
            column_end,
            row_start,
            row_end,
            inherited_columns,
            inherited_rows,
        }
    }
    
    /// Get inherited column count
    pub fn column_count(&self) -> usize {
        self.column_end - self.column_start
    }
    
    /// Get inherited row count
    pub fn row_count(&self) -> usize {
        self.row_end - self.row_start
    }
}

impl Default for Subgrid {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_subgrid() {
        let subgrid = Subgrid::new().with_columns();
        
        assert!(subgrid.columns);
        assert!(!subgrid.rows);
        assert!(subgrid.is_subgrid());
    }
    
    #[test]
    fn test_subgrid_context() {
        let parent = ParentTracks {
            column_count: 4,
            row_count: 3,
            column_sizes: vec![100.0, 200.0, 100.0, 150.0],
            row_sizes: vec![50.0, 100.0, 50.0],
            column_gaps: vec![10.0; 3],
            row_gaps: vec![10.0; 2],
        };
        
        let ctx = SubgridContext::from_placement(1, 3, 0, 2, &parent);
        
        assert_eq!(ctx.column_count(), 2);
        assert_eq!(ctx.row_count(), 2);
    }
}
