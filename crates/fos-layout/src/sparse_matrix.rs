//! Sparse Matrix for Layout (Phase 24.3)
//!
//! Only store non-zero flex basis values. Sparse grid track definitions.
//! Skip empty table cells. CSR format for constraint matrices.

use std::collections::HashMap;

/// Sparse matrix in CSR (Compressed Sparse Row) format
#[derive(Debug, Clone)]
pub struct SparseMatrix {
    /// Non-zero values
    values: Vec<f32>,
    /// Column indices for each value
    col_indices: Vec<u32>,
    /// Row pointer (start of each row in values/col_indices)
    row_ptr: Vec<u32>,
    /// Number of rows
    rows: usize,
    /// Number of columns
    cols: usize,
}

impl SparseMatrix {
    /// Create empty sparse matrix
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            values: Vec::new(),
            col_indices: Vec::new(),
            row_ptr: vec![0; rows + 1],
            rows,
            cols,
        }
    }
    
    /// Create from dense matrix
    pub fn from_dense(dense: &[Vec<f32>]) -> Self {
        let rows = dense.len();
        let cols = dense.get(0).map(|r| r.len()).unwrap_or(0);
        
        let mut values = Vec::new();
        let mut col_indices = Vec::new();
        let mut row_ptr = vec![0u32];
        
        for row in dense {
            for (col, &val) in row.iter().enumerate() {
                if val.abs() > f32::EPSILON {
                    values.push(val);
                    col_indices.push(col as u32);
                }
            }
            row_ptr.push(values.len() as u32);
        }
        
        Self {
            values,
            col_indices,
            row_ptr,
            rows,
            cols,
        }
    }
    
    /// Get value at position
    pub fn get(&self, row: usize, col: usize) -> f32 {
        if row >= self.rows || col >= self.cols {
            return 0.0;
        }
        
        let start = self.row_ptr[row] as usize;
        let end = self.row_ptr[row + 1] as usize;
        
        for i in start..end {
            if self.col_indices[i] as usize == col {
                return self.values[i];
            }
        }
        
        0.0
    }
    
    /// Number of non-zero elements
    pub fn nnz(&self) -> usize {
        self.values.len()
    }
    
    /// Dimensions
    pub fn dims(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }
    
    /// Memory usage vs dense
    pub fn compression_ratio(&self) -> f64 {
        let dense_size = self.rows * self.cols * 4; // f32
        let sparse_size = self.values.len() * 4 + self.col_indices.len() * 4 + self.row_ptr.len() * 4;
        
        if sparse_size == 0 {
            1.0
        } else {
            dense_size as f64 / sparse_size as f64
        }
    }
    
    /// Iterate over row
    pub fn row_iter(&self, row: usize) -> impl Iterator<Item = (usize, f32)> + '_ {
        let start = self.row_ptr.get(row).copied().unwrap_or(0) as usize;
        let end = self.row_ptr.get(row + 1).copied().unwrap_or(0) as usize;
        
        (start..end).map(move |i| (self.col_indices[i] as usize, self.values[i]))
    }
    
    /// Matrix-vector multiply
    pub fn mul_vec(&self, vec: &[f32]) -> Vec<f32> {
        let mut result = vec![0.0; self.rows];
        
        for row in 0..self.rows {
            let start = self.row_ptr[row] as usize;
            let end = self.row_ptr[row + 1] as usize;
            
            for i in start..end {
                let col = self.col_indices[i] as usize;
                if col < vec.len() {
                    result[row] += self.values[i] * vec[col];
                }
            }
        }
        
        result
    }
}

/// Sparse flex values (most are 0)
#[derive(Debug, Clone, Default)]
pub struct SparseFlexValues {
    /// Non-zero grow values
    grow: HashMap<u32, f32>,
    /// Non-zero shrink values (default is 1)
    shrink: HashMap<u32, f32>,
    /// Non-zero basis values
    basis: HashMap<u32, f32>,
}

impl SparseFlexValues {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set flex-grow
    pub fn set_grow(&mut self, item: u32, value: f32) {
        if value.abs() < f32::EPSILON {
            self.grow.remove(&item);
        } else {
            self.grow.insert(item, value);
        }
    }
    
    /// Get flex-grow
    pub fn get_grow(&self, item: u32) -> f32 {
        self.grow.get(&item).copied().unwrap_or(0.0)
    }
    
    /// Set flex-shrink
    pub fn set_shrink(&mut self, item: u32, value: f32) {
        if (value - 1.0).abs() < f32::EPSILON {
            self.shrink.remove(&item); // Default is 1
        } else {
            self.shrink.insert(item, value);
        }
    }
    
    /// Get flex-shrink
    pub fn get_shrink(&self, item: u32) -> f32 {
        self.shrink.get(&item).copied().unwrap_or(1.0)
    }
    
    /// Set flex-basis
    pub fn set_basis(&mut self, item: u32, value: f32) {
        if value.abs() < f32::EPSILON {
            self.basis.remove(&item);
        } else {
            self.basis.insert(item, value);
        }
    }
    
    /// Get flex-basis
    pub fn get_basis(&self, item: u32) -> f32 {
        self.basis.get(&item).copied().unwrap_or(0.0)
    }
    
    /// Storage count
    pub fn storage_count(&self) -> usize {
        self.grow.len() + self.shrink.len() + self.basis.len()
    }
    
    /// Items with non-default grow
    pub fn items_with_grow(&self) -> impl Iterator<Item = (u32, f32)> + '_ {
        self.grow.iter().map(|(&k, &v)| (k, v))
    }
}

/// Sparse grid tracks
#[derive(Debug, Clone, Default)]
pub struct SparseGridTracks {
    /// Explicit track sizes (gaps are implicit)
    tracks: HashMap<u32, TrackSize>,
    /// Track count
    track_count: u32,
}

/// Grid track size
#[derive(Debug, Clone, Copy)]
pub enum TrackSize {
    Fixed(f32),
    FlexFr(f32),
    MinMax(f32, f32),
    Auto,
}

impl SparseGridTracks {
    pub fn new(count: u32) -> Self {
        Self {
            tracks: HashMap::new(),
            track_count: count,
        }
    }
    
    /// Set track size
    pub fn set(&mut self, track: u32, size: TrackSize) {
        self.tracks.insert(track, size);
    }
    
    /// Get track size
    pub fn get(&self, track: u32) -> TrackSize {
        self.tracks.get(&track).copied().unwrap_or(TrackSize::Auto)
    }
    
    /// Count of explicit tracks
    pub fn explicit_count(&self) -> usize {
        self.tracks.len()
    }
    
    /// Total tracks
    pub fn total_count(&self) -> u32 {
        self.track_count
    }
    
    /// Compression ratio
    pub fn compression_ratio(&self) -> f64 {
        if self.track_count == 0 {
            1.0
        } else {
            self.track_count as f64 / self.tracks.len().max(1) as f64
        }
    }
}

/// Sparse table cells (for tables with many empty cells)
#[derive(Debug, Clone, Default)]
pub struct SparseTableCells {
    /// Non-empty cells: (row, col) -> content
    cells: HashMap<(u32, u32), u32>,
    /// Row count
    rows: u32,
    /// Column count
    cols: u32,
}

impl SparseTableCells {
    pub fn new(rows: u32, cols: u32) -> Self {
        Self {
            cells: HashMap::new(),
            rows,
            cols,
        }
    }
    
    /// Set cell content
    pub fn set(&mut self, row: u32, col: u32, content: u32) {
        if content == 0 {
            self.cells.remove(&(row, col));
        } else {
            self.cells.insert((row, col), content);
        }
    }
    
    /// Get cell content
    pub fn get(&self, row: u32, col: u32) -> Option<u32> {
        self.cells.get(&(row, col)).copied()
    }
    
    /// Check if cell is non-empty
    pub fn has_content(&self, row: u32, col: u32) -> bool {
        self.cells.contains_key(&(row, col))
    }
    
    /// Non-empty cell count
    pub fn count(&self) -> usize {
        self.cells.len()
    }
    
    /// Iterate non-empty cells
    pub fn iter(&self) -> impl Iterator<Item = ((u32, u32), u32)> + '_ {
        self.cells.iter().map(|(&pos, &content)| (pos, content))
    }
    
    /// Memory savings ratio
    pub fn savings_ratio(&self) -> f64 {
        let total = self.rows as usize * self.cols as usize;
        if total == 0 {
            0.0
        } else {
            1.0 - (self.cells.len() as f64 / total as f64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sparse_matrix() {
        let dense = vec![
            vec![1.0, 0.0, 0.0, 2.0],
            vec![0.0, 3.0, 0.0, 0.0],
            vec![0.0, 0.0, 4.0, 0.0],
        ];
        
        let sparse = SparseMatrix::from_dense(&dense);
        
        assert_eq!(sparse.nnz(), 4);
        assert_eq!(sparse.get(0, 0), 1.0);
        assert_eq!(sparse.get(0, 3), 2.0);
        assert_eq!(sparse.get(1, 1), 3.0);
        assert_eq!(sparse.get(1, 0), 0.0); // Zero
        
        // Compression ratio should be > 1 for sparse data
        assert!(sparse.compression_ratio() > 1.0);
    }
    
    #[test]
    fn test_sparse_flex() {
        let mut flex = SparseFlexValues::new();
        
        // Most items have default values
        assert_eq!(flex.get_grow(0), 0.0);
        assert_eq!(flex.get_shrink(0), 1.0);
        
        // Set non-default
        flex.set_grow(5, 2.0);
        flex.set_basis(5, 100.0);
        
        assert_eq!(flex.get_grow(5), 2.0);
        assert_eq!(flex.get_basis(5), 100.0);
        
        // Only 2 stored values
        assert_eq!(flex.storage_count(), 2);
    }
    
    #[test]
    fn test_sparse_table() {
        let mut table = SparseTableCells::new(100, 50);
        
        // Mostly empty table
        table.set(5, 10, 1);
        table.set(20, 30, 2);
        table.set(99, 49, 3);
        
        assert_eq!(table.count(), 3);
        assert!(table.savings_ratio() > 0.99); // 99%+ savings
    }
    
    #[test]
    fn test_matrix_mul() {
        let dense = vec![
            vec![1.0, 2.0],
            vec![3.0, 4.0],
        ];
        
        let sparse = SparseMatrix::from_dense(&dense);
        let vec = vec![1.0, 2.0];
        
        let result = sparse.mul_vec(&vec);
        
        assert!((result[0] - 5.0).abs() < 0.001); // 1*1 + 2*2 = 5
        assert!((result[1] - 11.0).abs() < 0.001); // 3*1 + 4*2 = 11
    }
}
