//! AV1 Entropy Coding (Arithmetic Decoder)
//!
//! Multi-symbol arithmetic decoder for AV1 coefficient and mode decoding.

use super::transform::{TxSize, TxType};
use super::predict::IntraMode;
use super::AvifError;

/// Partition types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Partition {
    None,
    Horizontal,
    Vertical,
    Split,
}

/// Arithmetic decoder for AV1 entropy coding
#[derive(Debug)]
pub struct ArithmeticDecoder<'a> {
    data: &'a [u8],
    pos: usize,
    range: u32,
    value: u32,
    bits_left: i32,
}

impl<'a> ArithmeticDecoder<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, AvifError> {
        if data.is_empty() {
            return Err(AvifError::ArithmeticError);
        }
        
        let mut decoder = Self {
            data,
            pos: 0,
            range: 0x8000, // Initial range
            value: 0,
            bits_left: 0,
        };
        
        // Initialize value from first bytes
        for _ in 0..2 {
            decoder.value = (decoder.value << 8) | decoder.read_byte()? as u32;
        }
        decoder.bits_left = 8;
        
        Ok(decoder)
    }
    
    fn read_byte(&mut self) -> Result<u8, AvifError> {
        if self.pos >= self.data.len() {
            return Ok(0); // Pad with zeros at end
        }
        let byte = self.data[self.pos];
        self.pos += 1;
        Ok(byte)
    }
    
    /// Read a single bit using specified probability
    pub fn decode_bool(&mut self, prob: u8) -> Result<bool, AvifError> {
        // Scale probability to range
        let split = 1 + (((self.range - 1) * (prob as u32)) >> 8);
        
        let bit = self.value >= split;
        
        if bit {
            self.value -= split;
            self.range -= split;
        } else {
            self.range = split;
        }
        
        // Renormalize
        while self.range < 0x80 {
            self.range <<= 1;
            self.value = (self.value << 1) | self.read_bit_raw()?;
        }
        
        Ok(bit)
    }
    
    fn read_bit_raw(&mut self) -> Result<u32, AvifError> {
        if self.bits_left == 0 {
            self.value = (self.value << 8) | self.read_byte()? as u32;
            self.bits_left = 8;
        }
        self.bits_left -= 1;
        Ok((self.value >> 15) & 1)
    }
    
    /// Decode a symbol using CDF (cumulative distribution function)
    pub fn decode_symbol(&mut self, cdf: &mut [u16]) -> Result<u32, AvifError> {
        let n = cdf.len() - 1; // Last element is count
        let range = self.range;
        let value = self.value;
        
        let mut symbol = 0u32;
        let mut low = 0u32;
        let mut high;
        
        // Binary search through CDF
        for i in 0..n {
            high = ((range as u64 * cdf[i] as u64) >> 15) as u32;
            if value < high {
                symbol = i as u32;
                self.range = high - low;
                self.value = value - low;
                break;
            }
            low = high;
            if i == n - 1 {
                symbol = n as u32;
                self.range = range - low;
                self.value = value - low;
            }
        }
        
        // Renormalize
        while self.range < 0x80 {
            self.range <<= 1;
            self.value = (self.value << 1) | self.read_bit_raw()?;
        }
        
        // Adapt CDF
        self.adapt_cdf(cdf, symbol as usize);
        
        Ok(symbol)
    }
    
    fn adapt_cdf(&self, cdf: &mut [u16], symbol: usize) {
        let n = cdf.len() - 1;
        let count = cdf[n] as usize;
        let rate = 4 + (count >> 4).min(2);
        
        // Update CDF probabilities
        for i in 0..n {
            if i < symbol {
                cdf[i] -= (cdf[i] >> rate) as u16;
            } else {
                cdf[i] += ((32768 - cdf[i]) >> rate) as u16;
            }
        }
        
        // Update count
        if count < 32 {
            cdf[n] += 1;
        }
    }
    
    /// Decode literal bits
    pub fn decode_literal(&mut self, n: u8) -> Result<u32, AvifError> {
        let mut value = 0u32;
        for _ in 0..n {
            value = (value << 1) | if self.decode_bool(128)? { 1 } else { 0 };
        }
        Ok(value)
    }
    
    /// Decode partition type
    pub fn decode_partition(&mut self, width: u32, height: u32) -> Result<Partition, AvifError> {
        // Simplified partition decoding
        if width <= 4 || height <= 4 {
            return Ok(Partition::None);
        }
        
        // Use fixed probabilities for partition
        let has_partition = self.decode_bool(180)?;
        
        if !has_partition {
            return Ok(Partition::None);
        }
        
        let partition_type = self.decode_literal(2)?;
        
        Ok(match partition_type {
            0 => Partition::Horizontal,
            1 => Partition::Vertical,
            2 => Partition::Split,
            _ => Partition::None,
        })
    }
    
    /// Decode intra prediction mode
    pub fn decode_intra_mode(&mut self) -> Result<IntraMode, AvifError> {
        // Use CDF for intra mode (13 modes)
        let mut cdf = INTRA_MODE_CDF.clone();
        let symbol = self.decode_symbol(&mut cdf)?;
        Ok(IntraMode::from_u8(symbol as u8))
    }
    
    /// Decode chroma mode based on luma mode
    pub fn decode_chroma_mode(&mut self, luma_mode: IntraMode) -> Result<IntraMode, AvifError> {
        // CfL or match luma
        let use_cfl = self.decode_bool(128)?;
        if use_cfl {
            Ok(IntraMode::CflPred)
        } else {
            Ok(luma_mode)
        }
    }
    
    /// Decode CfL alpha parameters
    pub fn decode_cfl_alpha(&mut self) -> Result<(i16, i16), AvifError> {
        let sign_u = self.decode_bool(128)?;
        let mag_u = self.decode_literal(4)? as i16;
        let alpha_u = if sign_u { -mag_u } else { mag_u };
        
        let sign_v = self.decode_bool(128)?;
        let mag_v = self.decode_literal(4)? as i16;
        let alpha_v = if sign_v { -mag_v } else { mag_v };
        
        Ok((alpha_u, alpha_v))
    }
    
    /// Decode transform size
    pub fn decode_tx_size(&mut self, width: u32, height: u32) -> Result<TxSize, AvifError> {
        // For intra-only AVIF, use the block size as transform size
        TxSize::from_dimensions(width, height)
            .ok_or(AvifError::TransformError)
    }
    
    /// Decode transform type
    pub fn decode_tx_type(&mut self) -> Result<TxType, AvifError> {
        // Simplified: mostly DCT_DCT for AVIF
        let tx_type = self.decode_literal(4)?;
        
        Ok(match tx_type {
            0 => TxType::DctDct,
            1 => TxType::AdstDct,
            2 => TxType::DctAdst,
            3 => TxType::AdstAdst,
            4 => TxType::IdentityIdentity,
            5 => TxType::IdentityDct,
            6 => TxType::DctIdentity,
            _ => TxType::DctDct,
        })
    }
    
    /// Decode transform coefficients
    pub fn decode_coefficients(&mut self, tx_size: TxSize, bit_depth: u8) -> Result<Vec<i16>, AvifError> {
        let width = tx_size.width();
        let height = tx_size.height();
        let size = width * height;
        
        let mut coeffs = vec![0i16; size];
        
        // Check if block has any non-zero coefficients
        let has_coeffs = self.decode_bool(200)?;
        
        if !has_coeffs {
            return Ok(coeffs);
        }
        
        // Decode end of block position
        let eob = self.decode_eob(size)?;
        
        if eob == 0 {
            return Ok(coeffs);
        }
        
        // Decode coefficients up to EOB
        let max_value = (1 << (bit_depth + 2)) - 1;
        
        for i in 0..eob {
            let scan_idx = SCAN_ORDER_4X4.get(i % 16).copied().unwrap_or(i);
            let actual_idx = if size <= 16 {
                scan_idx
            } else {
                // For larger blocks, use sequential scan
                i
            };
            
            if actual_idx >= size {
                break;
            }
            
            let level = self.decode_coeff_level()?;
            
            if level > 0 {
                let sign = self.decode_bool(128)?;
                let coeff = if sign { -(level as i16) } else { level as i16 };
                coeffs[actual_idx] = coeff.clamp(-max_value, max_value);
            }
        }
        
        Ok(coeffs)
    }
    
    fn decode_eob(&mut self, max_eob: usize) -> Result<usize, AvifError> {
        if max_eob <= 1 {
            return Ok(if self.decode_bool(128)? { 1 } else { 0 });
        }
        
        // Decode EOB class
        let eob_class = if max_eob <= 4 {
            self.decode_literal(2)? as usize
        } else if max_eob <= 16 {
            self.decode_literal(3)? as usize
        } else if max_eob <= 64 {
            self.decode_literal(4)? as usize
        } else {
            self.decode_literal(5)? as usize
        };
        
        // EOB is 2^class + extra bits
        let base = 1 << eob_class;
        let extra = if eob_class > 0 {
            self.decode_literal(eob_class as u8)? as usize
        } else {
            0
        };
        
        Ok((base + extra).min(max_eob))
    }
    
    fn decode_coeff_level(&mut self) -> Result<i32, AvifError> {
        // Decode absolute coefficient level
        // Level 0 is signaled by first bool
        let is_nonzero = self.decode_bool(192)?;
        
        if !is_nonzero {
            return Ok(0);
        }
        
        // Level 1 is most common
        let is_one = self.decode_bool(128)?;
        
        if is_one {
            return Ok(1);
        }
        
        // Higher levels use exponential golomb-like coding
        let mut level = 2i32;
        let mut more = self.decode_bool(128)?;
        
        while more && level < 256 {
            level += 1;
            more = self.decode_bool(128)?;
        }
        
        Ok(level)
    }
}

// Default intra mode CDF
static INTRA_MODE_CDF: [u16; 14] = [
    2048,   // DC_PRED
    4096,   // V_PRED
    6144,   // H_PRED
    8192,   // D45_PRED
    10240,  // D135_PRED
    12288,  // D113_PRED
    14336,  // D157_PRED
    16384,  // D203_PRED
    18432,  // D67_PRED
    20480,  // SMOOTH_PRED
    22528,  // SMOOTH_V_PRED
    24576,  // SMOOTH_H_PRED
    26624,  // PAETH_PRED
    0,      // Count
];

// Zig-zag scan order for 4x4 block
static SCAN_ORDER_4X4: [usize; 16] = [
    0,  1,  4,  8,
    5,  2,  3,  6,
    9,  12, 13, 10,
    7,  11, 14, 15,
];

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_decoder_creation() {
        let data = vec![0x80, 0x00, 0x80, 0x00];
        let decoder = ArithmeticDecoder::new(&data);
        assert!(decoder.is_ok());
    }
    
    #[test]
    fn test_decode_bool() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut decoder = ArithmeticDecoder::new(&data).unwrap();
        
        // High probability of 1
        let result = decoder.decode_bool(250);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_decode_literal() {
        let data = vec![0xAB, 0xCD, 0xEF, 0x12];
        let mut decoder = ArithmeticDecoder::new(&data).unwrap();
        
        let result = decoder.decode_literal(4);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_partition_decode() {
        let data = vec![0x00, 0x00, 0x00, 0x00];
        let mut decoder = ArithmeticDecoder::new(&data).unwrap();
        
        // Small block should return None partition
        let partition = decoder.decode_partition(4, 4);
        assert!(partition.is_ok());
        assert_eq!(partition.unwrap(), Partition::None);
    }
}
