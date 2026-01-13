//! Bit-level Reader for AV1 Parsing
//!
//! Provides bit-level reading operations required for OBU parsing.

use super::AvifError;

/// Bit-level reader for parsing AV1 bitstreams
#[derive(Debug)]
pub struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }
    
    /// Get current byte position
    pub fn position(&self) -> usize {
        self.byte_pos
    }
    
    /// Get current bit offset within byte
    pub fn bit_offset(&self) -> u8 {
        self.bit_pos
    }
    
    /// Remaining bytes available
    pub fn remaining(&self) -> usize {
        if self.byte_pos >= self.data.len() {
            0
        } else {
            self.data.len() - self.byte_pos
        }
    }
    
    /// Check if we've reached end of data
    pub fn is_empty(&self) -> bool {
        self.byte_pos >= self.data.len()
    }
    
    /// Read n bits (up to 32)
    pub fn read_bits(&mut self, n: u8) -> Result<u32, AvifError> {
        if n == 0 {
            return Ok(0);
        }
        if n > 32 {
            return Err(AvifError::InvalidData);
        }
        
        let mut value: u32 = 0;
        let mut bits_remaining = n;
        
        while bits_remaining > 0 {
            if self.byte_pos >= self.data.len() {
                return Err(AvifError::InvalidData);
            }
            
            let bits_in_byte = 8 - self.bit_pos;
            let bits_to_read = bits_remaining.min(bits_in_byte);
            
            // Avoid overflow when bits_to_read is 8: use u16 for mask calculation
            let mask = ((1u16 << bits_to_read) - 1) as u8;
            let shift = bits_in_byte - bits_to_read;
            let bits = (self.data[self.byte_pos] >> shift) & mask;
            
            value = (value << bits_to_read) | (bits as u32);
            
            self.bit_pos += bits_to_read;
            bits_remaining -= bits_to_read;
            
            if self.bit_pos >= 8 {
                self.bit_pos = 0;
                self.byte_pos += 1;
            }
        }
        
        Ok(value)
    }
    
    /// Read a single bit as boolean
    pub fn read_bit(&mut self) -> Result<bool, AvifError> {
        Ok(self.read_bits(1)? != 0)
    }
    
    /// Read unsigned variable-length code (uvlc)
    /// Format: leading zeros followed by a 1, then that many data bits
    pub fn read_uvlc(&mut self) -> Result<u32, AvifError> {
        let mut leading_zeros = 0u8;
        
        loop {
            if self.byte_pos >= self.data.len() {
                return Err(AvifError::InvalidData);
            }
            
            if self.read_bit()? {
                break;
            }
            
            leading_zeros += 1;
            if leading_zeros > 32 {
                return Err(AvifError::InvalidData);
            }
        }
        
        if leading_zeros == 0 {
            return Ok(0);
        }
        
        let value = self.read_bits(leading_zeros)?;
        Ok((1 << leading_zeros) - 1 + value)
    }
    
    /// Read LEB128 encoded unsigned integer
    pub fn read_leb128(&mut self) -> Result<u64, AvifError> {
        let mut value: u64 = 0;
        
        for i in 0..8 {
            let byte = self.read_bits(8)? as u8;
            value |= ((byte & 0x7f) as u64) << (i * 7);
            
            if byte & 0x80 == 0 {
                break;
            }
        }
        
        Ok(value)
    }
    
    /// Read signed integer with n bits
    pub fn read_su(&mut self, n: u8) -> Result<i32, AvifError> {
        let value = self.read_bits(n)?;
        let sign_bit = 1u32 << (n - 1);
        
        if value & sign_bit != 0 {
            Ok((value as i32) - (1 << n))
        } else {
            Ok(value as i32)
        }
    }
    
    /// Read ns (non-symmetric) encoded value
    pub fn read_ns(&mut self, n: u32) -> Result<u32, AvifError> {
        if n <= 1 {
            return Ok(0);
        }
        
        let w = 32 - (n - 1).leading_zeros();
        let m = (1 << w) - n;
        
        let v = self.read_bits(w as u8 - 1)?;
        
        if v < m {
            Ok(v)
        } else {
            let extra = self.read_bits(1)?;
            Ok((v << 1) - m + extra)
        }
    }
    
    /// Read literal n bytes
    pub fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, AvifError> {
        // Align to byte boundary first
        if self.bit_pos != 0 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
        
        if self.byte_pos + n > self.data.len() {
            return Err(AvifError::InvalidData);
        }
        
        let bytes = self.data[self.byte_pos..self.byte_pos + n].to_vec();
        self.byte_pos += n;
        
        Ok(bytes)
    }
    
    /// Skip n bits
    pub fn skip_bits(&mut self, n: u32) -> Result<(), AvifError> {
        let total_bits = (self.byte_pos * 8) as u32 + self.bit_pos as u32 + n;
        let new_byte = total_bits / 8;
        let new_bit = (total_bits % 8) as u8;
        
        if new_byte as usize > self.data.len() {
            return Err(AvifError::InvalidData);
        }
        
        self.byte_pos = new_byte as usize;
        self.bit_pos = new_bit;
        
        Ok(())
    }
    
    /// Align to byte boundary
    pub fn byte_align(&mut self) {
        if self.bit_pos != 0 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
    }
    
    /// Get remaining data slice from current position
    pub fn remaining_data(&self) -> &'a [u8] {
        if self.bit_pos == 0 && self.byte_pos < self.data.len() {
            &self.data[self.byte_pos..]
        } else if self.byte_pos + 1 < self.data.len() {
            &self.data[self.byte_pos + 1..]
        } else {
            &[]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_read_bits() {
        let data = [0b10110100, 0b11001010];
        let mut reader = BitReader::new(&data);
        
        assert_eq!(reader.read_bits(1).unwrap(), 1);
        assert_eq!(reader.read_bits(2).unwrap(), 0b01);
        assert_eq!(reader.read_bits(5).unwrap(), 0b10100);
        assert_eq!(reader.read_bits(4).unwrap(), 0b1100);
    }
    
    #[test]
    fn test_read_bit() {
        let data = [0b10101010];
        let mut reader = BitReader::new(&data);
        
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
    }
    
    #[test]
    fn test_read_uvlc() {
        // UVLC: leading zeros, then 1, then that many data bits
        // 0 encoded as: 1 (just the stop bit)
        let data = [0b10000000];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_uvlc().unwrap(), 0);
        
        // 1 encoded as: 01 0 (one zero, stop, one data bit = 0)
        let data = [0b01000000];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_uvlc().unwrap(), 1);
        
        // 2 encoded as: 01 1 (one zero, stop, one data bit = 1)
        let data = [0b01100000];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_uvlc().unwrap(), 2);
    }
    
    #[test]
    fn test_read_leb128() {
        // Single byte
        let data = [0x42];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_leb128().unwrap(), 0x42);
        
        // Multi-byte: 300 = 0x12C = 10101100 00000010
        let data = [0xAC, 0x02];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_leb128().unwrap(), 300);
    }
    
    #[test]
    fn test_read_su() {
        // Positive value
        let data = [0b01100000];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_su(4).unwrap(), 6);
        
        // Negative value (-2 in 4 bits = 1110)
        let data = [0b11100000];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_su(4).unwrap(), -2);
    }
    
    #[test]
    fn test_skip_and_align() {
        let data = [0xFF, 0xFF, 0xFF];
        let mut reader = BitReader::new(&data);
        
        reader.read_bits(3).unwrap();
        assert_eq!(reader.bit_offset(), 3);
        
        reader.byte_align();
        assert_eq!(reader.bit_offset(), 0);
        assert_eq!(reader.position(), 1);
        
        reader.skip_bits(5).unwrap();
        assert_eq!(reader.position(), 1);
        assert_eq!(reader.bit_offset(), 5);
    }
}
