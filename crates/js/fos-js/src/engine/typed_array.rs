//! TypedArrays Implementation
//!
//! ArrayBuffer, DataView, and TypedArray implementations.

use std::mem;

/// JavaScript ArrayBuffer
#[derive(Debug, Clone)]
pub struct ArrayBuffer {
    data: Vec<u8>,
}

impl ArrayBuffer {
    pub fn new(byte_length: usize) -> Self {
        Self { data: vec![0; byte_length] }
    }
    
    pub fn byte_length(&self) -> usize { self.data.len() }
    pub fn as_slice(&self) -> &[u8] { &self.data }
    pub fn as_mut_slice(&mut self) -> &mut [u8] { &mut self.data }
    
    pub fn slice(&self, begin: usize, end: usize) -> Self {
        let end = end.min(self.data.len());
        let begin = begin.min(end);
        Self { data: self.data[begin..end].to_vec() }
    }
}

/// JavaScript DataView
#[derive(Debug)]
pub struct DataView {
    buffer_id: u32,
    byte_offset: usize,
    byte_length: usize,
}

impl DataView {
    pub fn new(buffer_id: u32, byte_offset: usize, byte_length: usize) -> Self {
        Self { buffer_id, byte_offset, byte_length }
    }
    
    pub fn buffer_id(&self) -> u32 { self.buffer_id }
    pub fn byte_offset(&self) -> usize { self.byte_offset }
    pub fn byte_length(&self) -> usize { self.byte_length }
}

/// TypedArray element type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypedArrayKind {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Float32,
    Float64,
    BigInt64,
    BigUint64,
}

impl TypedArrayKind {
    pub fn byte_size(&self) -> usize {
        match self {
            TypedArrayKind::Int8 | TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => 1,
            TypedArrayKind::Int16 | TypedArrayKind::Uint16 => 2,
            TypedArrayKind::Int32 | TypedArrayKind::Uint32 | TypedArrayKind::Float32 => 4,
            TypedArrayKind::Float64 | TypedArrayKind::BigInt64 | TypedArrayKind::BigUint64 => 8,
        }
    }
}

/// JavaScript TypedArray
#[derive(Debug, Clone)]
pub struct TypedArray {
    kind: TypedArrayKind,
    buffer_id: u32,
    byte_offset: usize,
    length: usize,
}

impl TypedArray {
    pub fn new(kind: TypedArrayKind, buffer_id: u32, byte_offset: usize, length: usize) -> Self {
        Self { kind, buffer_id, byte_offset, length }
    }
    
    pub fn kind(&self) -> TypedArrayKind { self.kind }
    pub fn buffer_id(&self) -> u32 { self.buffer_id }
    pub fn byte_offset(&self) -> usize { self.byte_offset }
    pub fn length(&self) -> usize { self.length }
    pub fn byte_length(&self) -> usize { self.length * self.kind.byte_size() }
    
    /// Get element (returns f64 for all types)
    pub fn get(&self, buffer: &ArrayBuffer, index: usize) -> Option<f64> {
        if index >= self.length { return None; }
        let offset = self.byte_offset + index * self.kind.byte_size();
        let slice = buffer.as_slice();
        
        match self.kind {
            TypedArrayKind::Int8 => Some(slice.get(offset).map(|&b| b as i8 as f64)?),
            TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => Some(slice.get(offset).map(|&b| b as f64)?),
            TypedArrayKind::Int16 => {
                if offset + 2 > slice.len() { return None; }
                Some(i16::from_le_bytes([slice[offset], slice[offset + 1]]) as f64)
            }
            TypedArrayKind::Uint16 => {
                if offset + 2 > slice.len() { return None; }
                Some(u16::from_le_bytes([slice[offset], slice[offset + 1]]) as f64)
            }
            TypedArrayKind::Int32 => {
                if offset + 4 > slice.len() { return None; }
                Some(i32::from_le_bytes([slice[offset], slice[offset + 1], slice[offset + 2], slice[offset + 3]]) as f64)
            }
            TypedArrayKind::Uint32 => {
                if offset + 4 > slice.len() { return None; }
                Some(u32::from_le_bytes([slice[offset], slice[offset + 1], slice[offset + 2], slice[offset + 3]]) as f64)
            }
            TypedArrayKind::Float32 => {
                if offset + 4 > slice.len() { return None; }
                Some(f32::from_le_bytes([slice[offset], slice[offset + 1], slice[offset + 2], slice[offset + 3]]) as f64)
            }
            TypedArrayKind::Float64 => {
                if offset + 8 > slice.len() { return None; }
                Some(f64::from_le_bytes([
                    slice[offset], slice[offset + 1], slice[offset + 2], slice[offset + 3],
                    slice[offset + 4], slice[offset + 5], slice[offset + 6], slice[offset + 7]
                ]))
            }
            _ => None,
        }
    }
    
    /// Set element
    pub fn set(&self, buffer: &mut ArrayBuffer, index: usize, value: f64) -> bool {
        if index >= self.length { return false; }
        let offset = self.byte_offset + index * self.kind.byte_size();
        let slice = buffer.as_mut_slice();
        
        match self.kind {
            TypedArrayKind::Int8 => {
                if offset >= slice.len() { return false; }
                slice[offset] = value as i8 as u8;
            }
            TypedArrayKind::Uint8 => {
                if offset >= slice.len() { return false; }
                slice[offset] = value as u8;
            }
            TypedArrayKind::Uint8Clamped => {
                if offset >= slice.len() { return false; }
                slice[offset] = value.clamp(0.0, 255.0) as u8;
            }
            TypedArrayKind::Int16 => {
                if offset + 2 > slice.len() { return false; }
                let bytes = (value as i16).to_le_bytes();
                slice[offset..offset + 2].copy_from_slice(&bytes);
            }
            TypedArrayKind::Uint16 => {
                if offset + 2 > slice.len() { return false; }
                let bytes = (value as u16).to_le_bytes();
                slice[offset..offset + 2].copy_from_slice(&bytes);
            }
            TypedArrayKind::Int32 => {
                if offset + 4 > slice.len() { return false; }
                let bytes = (value as i32).to_le_bytes();
                slice[offset..offset + 4].copy_from_slice(&bytes);
            }
            TypedArrayKind::Uint32 => {
                if offset + 4 > slice.len() { return false; }
                let bytes = (value as u32).to_le_bytes();
                slice[offset..offset + 4].copy_from_slice(&bytes);
            }
            TypedArrayKind::Float32 => {
                if offset + 4 > slice.len() { return false; }
                let bytes = (value as f32).to_le_bytes();
                slice[offset..offset + 4].copy_from_slice(&bytes);
            }
            TypedArrayKind::Float64 => {
                if offset + 8 > slice.len() { return false; }
                let bytes = value.to_le_bytes();
                slice[offset..offset + 8].copy_from_slice(&bytes);
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_array_buffer() {
        let buf = ArrayBuffer::new(16);
        assert_eq!(buf.byte_length(), 16);
    }
    
    #[test]
    fn test_typed_array_uint8() {
        let mut buf = ArrayBuffer::new(4);
        let arr = TypedArray::new(TypedArrayKind::Uint8, 0, 0, 4);
        arr.set(&mut buf, 0, 42.0);
        assert_eq!(arr.get(&buf, 0), Some(42.0));
    }
    
    #[test]
    fn test_typed_array_float64() {
        let mut buf = ArrayBuffer::new(16);
        let arr = TypedArray::new(TypedArrayKind::Float64, 0, 0, 2);
        arr.set(&mut buf, 0, 3.14159);
        assert!((arr.get(&buf, 0).unwrap() - 3.14159).abs() < 0.0001);
    }
}
