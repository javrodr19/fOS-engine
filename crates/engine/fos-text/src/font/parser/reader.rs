//! Binary reader for font data

use super::ParseError;

/// Binary reader with bounds checking
pub struct FontReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> FontReader<'a> {
    /// Create a new reader
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    
    /// Get current position
    pub fn pos(&self) -> usize {
        self.pos
    }
    
    /// Set position
    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }
    
    /// Skip bytes
    pub fn skip(&mut self, n: usize) -> Result<(), ParseError> {
        if self.pos + n > self.data.len() {
            return Err(ParseError::InvalidData);
        }
        self.pos += n;
        Ok(())
    }
    
    /// Read u8
    pub fn read_u8(&mut self) -> Result<u8, ParseError> {
        if self.pos >= self.data.len() {
            return Err(ParseError::InvalidData);
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }
    
    /// Read big-endian u16
    pub fn read_u16(&mut self) -> Result<u16, ParseError> {
        if self.pos + 2 > self.data.len() {
            return Err(ParseError::InvalidData);
        }
        let v = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }
    
    /// Read big-endian i16
    pub fn read_i16(&mut self) -> Result<i16, ParseError> {
        Ok(self.read_u16()? as i16)
    }
    
    /// Read big-endian u32
    pub fn read_u32(&mut self) -> Result<u32, ParseError> {
        if self.pos + 4 > self.data.len() {
            return Err(ParseError::InvalidData);
        }
        let v = u32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }
    
    /// Read big-endian i32 (Fixed 16.16)
    pub fn read_fixed(&mut self) -> Result<f32, ParseError> {
        let raw = self.read_u32()? as i32;
        Ok(raw as f32 / 65536.0)
    }
    
    /// Read 4-byte tag
    pub fn read_tag(&mut self) -> Result<[u8; 4], ParseError> {
        if self.pos + 4 > self.data.len() {
            return Err(ParseError::InvalidData);
        }
        let tag = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ];
        self.pos += 4;
        Ok(tag)
    }
    
    /// Read bytes
    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        if self.pos + n > self.data.len() {
            return Err(ParseError::InvalidData);
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }
    
    /// Remaining bytes
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    
    /// Get slice at current position
    pub fn slice_from_here(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_read_u16() {
        let data = [0x12, 0x34, 0x56, 0x78];
        let mut reader = FontReader::new(&data);
        assert_eq!(reader.read_u16().unwrap(), 0x1234);
        assert_eq!(reader.read_u16().unwrap(), 0x5678);
    }
    
    #[test]
    fn test_read_tag() {
        let data = b"headtest";
        let mut reader = FontReader::new(data);
        assert_eq!(reader.read_tag().unwrap(), *b"head");
    }
}
