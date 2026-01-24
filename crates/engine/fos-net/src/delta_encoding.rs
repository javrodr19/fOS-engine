//! Delta Encoding
//!
//! Diff-based response handling for incremental updates.

use std::collections::HashMap;

/// Delta operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaOp {
    Copy { offset: u32, length: u32 },
    Insert(Vec<u8>),
}

/// Delta patch
#[derive(Debug, Clone)]
pub struct DeltaPatch {
    pub base_hash: [u8; 32],
    pub target_size: u32,
    pub ops: Vec<DeltaOp>,
}

impl DeltaPatch {
    pub fn new(base_hash: [u8; 32], target_size: u32) -> Self {
        Self { base_hash, target_size, ops: Vec::new() }
    }
    
    pub fn copy(&mut self, offset: u32, length: u32) {
        self.ops.push(DeltaOp::Copy { offset, length });
    }
    
    pub fn insert(&mut self, data: Vec<u8>) {
        if !data.is_empty() {
            self.ops.push(DeltaOp::Insert(data));
        }
    }
    
    pub fn estimated_size(&self) -> usize {
        44 + self.ops.iter().map(|op| match op {
            DeltaOp::Copy { .. } => 8,
            DeltaOp::Insert(d) => 4 + d.len(),
        }).sum::<usize>()
    }
}

/// Delta encoder
#[derive(Debug, Default)]
pub struct DeltaEncoder {
    chunk_index: HashMap<u32, Vec<usize>>,
}

impl DeltaEncoder {
    pub fn new() -> Self { Self::default() }
    
    pub fn encode(&mut self, base: &[u8], target: &[u8]) -> DeltaPatch {
        let base_hash = Self::hash(base);
        let mut patch = DeltaPatch::new(base_hash, target.len() as u32);
        self.build_index(base);
        
        let mut pos = 0;
        let mut insert_buf = Vec::new();
        
        while pos < target.len() {
            if let Some((off, len)) = self.find_match(base, &target[pos..]) {
                if !insert_buf.is_empty() {
                    patch.insert(std::mem::take(&mut insert_buf));
                }
                patch.copy(off as u32, len as u32);
                pos += len;
            } else {
                insert_buf.push(target[pos]);
                pos += 1;
            }
        }
        if !insert_buf.is_empty() { patch.insert(insert_buf); }
        self.chunk_index.clear();
        patch
    }
    
    fn build_index(&mut self, data: &[u8]) {
        self.chunk_index.clear();
        for i in 0..data.len().saturating_sub(4) {
            let h = Self::roll_hash(&data[i..i+4]);
            self.chunk_index.entry(h).or_default().push(i);
        }
    }
    
    fn find_match(&self, base: &[u8], target: &[u8]) -> Option<(usize, usize)> {
        if target.len() < 4 { return None; }
        let h = Self::roll_hash(&target[..4]);
        let mut best = (0, 0);
        for &off in self.chunk_index.get(&h)? {
            let len = base[off..].iter().zip(target).take_while(|(a,b)| a==b).count();
            if len > best.1 && len >= 4 { best = (off, len); }
        }
        if best.1 >= 4 { Some(best) } else { None }
    }
    
    fn roll_hash(d: &[u8]) -> u32 {
        d.iter().fold(0u32, |h, &b| h.wrapping_mul(31).wrapping_add(b as u32))
    }
    
    fn hash(data: &[u8]) -> [u8; 32] {
        let mut h = [0u8; 32];
        let mut s = 0u64;
        for (i, &b) in data.iter().enumerate() {
            s = s.wrapping_mul(31).wrapping_add(b as u64);
            if i % 8 == 7 { 
                let idx = (i/8) % 4;
                for (j, byte) in s.to_le_bytes().iter().enumerate() { h[idx*8+j] ^= byte; }
            }
        }
        h
    }
}

/// Delta decoder
#[derive(Debug, Default)]
pub struct DeltaDecoder;

#[derive(Debug)]
pub enum DecodeError { HashMismatch, InvalidOffset, SizeMismatch }

impl DeltaDecoder {
    pub fn new() -> Self { Self }
    
    pub fn decode(&self, base: &[u8], patch: &DeltaPatch) -> Result<Vec<u8>, DecodeError> {
        if DeltaEncoder::hash(base) != patch.base_hash { return Err(DecodeError::HashMismatch); }
        let mut out = Vec::with_capacity(patch.target_size as usize);
        for op in &patch.ops {
            match op {
                DeltaOp::Copy { offset, length } => {
                    let (s, e) = (*offset as usize, *offset as usize + *length as usize);
                    if e > base.len() { return Err(DecodeError::InvalidOffset); }
                    out.extend_from_slice(&base[s..e]);
                }
                DeltaOp::Insert(d) => out.extend_from_slice(d),
            }
        }
        if out.len() != patch.target_size as usize { return Err(DecodeError::SizeMismatch); }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_delta_roundtrip() {
        let mut enc = DeltaEncoder::new();
        let dec = DeltaDecoder::new();
        let base = b"Hello, World!";
        let target = b"Hello, Universe!";
        let patch = enc.encode(base, target);
        assert_eq!(dec.decode(base, &patch).unwrap(), target);
    }
}
