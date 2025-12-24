//! WOFF (Web Open Font Format) decoder
//!
//! WOFF1 uses ZLIB/DEFLATE compression. We use a simplified decompression
//! approach leveraging LZ4 for the compressed streams.

use super::parser::reader::FontReader;

/// WOFF signature
const WOFF_SIGNATURE: u32 = 0x774F4646; // 'wOFF'

/// WOFF1 header
#[derive(Debug)]
struct WoffHeader {
    signature: u32,
    flavor: u32,
    length: u32,
    num_tables: u16,
    total_sfnt_size: u32,
    major_version: u16,
    minor_version: u16,
    meta_offset: u32,
    meta_length: u32,
    meta_orig_length: u32,
    priv_offset: u32,
    priv_length: u32,
}

/// WOFF table directory entry
#[derive(Debug)]
struct WoffTableEntry {
    tag: [u8; 4],
    offset: u32,
    comp_length: u32,
    orig_length: u32,
    orig_checksum: u32,
}

/// Decode WOFF1 to raw OpenType/TrueType data
pub fn decode_woff(data: &[u8]) -> Option<Vec<u8>> {
    let mut reader = FontReader::new(data);
    
    // Parse header
    let signature = reader.read_u32().ok()?;
    if signature != WOFF_SIGNATURE {
        return None;
    }
    
    let flavor = reader.read_u32().ok()?;
    let _length = reader.read_u32().ok()?;
    let num_tables = reader.read_u16().ok()?;
    let _reserved = reader.read_u16().ok()?;
    let total_sfnt_size = reader.read_u32().ok()?;
    let _major_version = reader.read_u16().ok()?;
    let _minor_version = reader.read_u16().ok()?;
    let _meta_offset = reader.read_u32().ok()?;
    let _meta_length = reader.read_u32().ok()?;
    let _meta_orig_length = reader.read_u32().ok()?;
    let _priv_offset = reader.read_u32().ok()?;
    let _priv_length = reader.read_u32().ok()?;
    
    // Parse table entries
    let mut entries = Vec::with_capacity(num_tables as usize);
    for _ in 0..num_tables {
        let tag = reader.read_tag().ok()?;
        let offset = reader.read_u32().ok()?;
        let comp_length = reader.read_u32().ok()?;
        let orig_length = reader.read_u32().ok()?;
        let orig_checksum = reader.read_u32().ok()?;
        
        entries.push(WoffTableEntry {
            tag,
            offset,
            comp_length,
            orig_length,
            orig_checksum,
        });
    }
    
    // Calculate output size
    let sfnt_header_size = 12 + num_tables as usize * 16;
    let mut output = vec![0u8; total_sfnt_size as usize];
    
    // Write sfnt header
    {
        let mut pos = 0;
        // sfnt version (flavor)
        output[pos..pos + 4].copy_from_slice(&flavor.to_be_bytes());
        pos += 4;
        // numTables
        output[pos..pos + 2].copy_from_slice(&num_tables.to_be_bytes());
        pos += 2;
        
        // Calculate search params
        let entry_selector = (num_tables as f32).log2().floor() as u16;
        let search_range = (1u16 << entry_selector) * 16;
        let range_shift = (num_tables * 16).saturating_sub(search_range);
        
        output[pos..pos + 2].copy_from_slice(&search_range.to_be_bytes());
        pos += 2;
        output[pos..pos + 2].copy_from_slice(&entry_selector.to_be_bytes());
        pos += 2;
        output[pos..pos + 2].copy_from_slice(&range_shift.to_be_bytes());
    }
    
    // Track where to write table data
    let mut table_offset = sfnt_header_size;
    
    // Process each table
    for (i, entry) in entries.iter().enumerate() {
        // Write table directory entry
        let dir_offset = 12 + i * 16;
        output[dir_offset..dir_offset + 4].copy_from_slice(&entry.tag);
        output[dir_offset + 4..dir_offset + 8].copy_from_slice(&entry.orig_checksum.to_be_bytes());
        output[dir_offset + 8..dir_offset + 12].copy_from_slice(&(table_offset as u32).to_be_bytes());
        output[dir_offset + 12..dir_offset + 16].copy_from_slice(&entry.orig_length.to_be_bytes());
        
        // Extract table data
        let table_data = &data[entry.offset as usize..(entry.offset + entry.comp_length) as usize];
        
        if entry.comp_length == entry.orig_length {
            // Uncompressed
            output[table_offset..table_offset + entry.orig_length as usize]
                .copy_from_slice(table_data);
        } else {
            // Compressed with DEFLATE - use miniz_oxide or simplified decompression
            if let Some(decompressed) = decompress_deflate(table_data, entry.orig_length as usize) {
                output[table_offset..table_offset + entry.orig_length as usize]
                    .copy_from_slice(&decompressed);
            } else {
                return None;
            }
        }
        
        // Align to 4 bytes
        table_offset += entry.orig_length as usize;
        while table_offset % 4 != 0 {
            table_offset += 1;
        }
    }
    
    Some(output)
}

/// Check if data is WOFF format
pub fn is_woff(data: &[u8]) -> bool {
    data.len() >= 4 && 
    u32::from_be_bytes([data[0], data[1], data[2], data[3]]) == WOFF_SIGNATURE
}

/// Simple DEFLATE decompression (raw deflate without zlib header)
/// This is a minimal implementation for WOFF - for full support, 
/// consider using miniz_oxide crate
fn decompress_deflate(compressed: &[u8], expected_size: usize) -> Option<Vec<u8>> {
    // Skip 2-byte zlib header if present
    let data = if compressed.len() >= 2 && (compressed[0] & 0x0F == 8) {
        &compressed[2..]
    } else {
        compressed
    };
    
    // Minimal deflate decoder for uncompressed blocks
    let mut output = Vec::with_capacity(expected_size);
    let mut reader = BitReader::new(data);
    
    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;
        
        match btype {
            0 => {
                // Stored block
                reader.align_byte();
                let len = reader.read_u16_le()?;
                let _nlen = reader.read_u16_le()?;
                for _ in 0..len {
                    output.push(reader.read_byte()?);
                }
            }
            1 | 2 => {
                // Fixed/dynamic Huffman - complex, return None for now
                // Full implementation would require Huffman tree decompression
                return None;
            }
            _ => return None,
        }
        
        if bfinal == 1 {
            break;
        }
    }
    
    if output.len() == expected_size {
        Some(output)
    } else {
        None
    }
}

/// Bit reader for DEFLATE
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_pos: u8,
    current_byte: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_pos: 0,
            current_byte: if data.is_empty() { 0 } else { data[0] },
        }
    }
    
    fn read_bits(&mut self, count: u8) -> Option<u32> {
        let mut result = 0u32;
        for i in 0..count {
            if self.pos >= self.data.len() {
                return None;
            }
            let bit = (self.current_byte >> self.bit_pos) & 1;
            result |= (bit as u32) << i;
            self.bit_pos += 1;
            if self.bit_pos == 8 {
                self.bit_pos = 0;
                self.pos += 1;
                if self.pos < self.data.len() {
                    self.current_byte = self.data[self.pos];
                }
            }
        }
        Some(result)
    }
    
    fn align_byte(&mut self) {
        if self.bit_pos != 0 {
            self.bit_pos = 0;
            self.pos += 1;
            if self.pos < self.data.len() {
                self.current_byte = self.data[self.pos];
            }
        }
    }
    
    fn read_u16_le(&mut self) -> Option<u16> {
        let lo = self.read_byte()? as u16;
        let hi = self.read_byte()? as u16;
        Some(lo | (hi << 8))
    }
    
    fn read_byte(&mut self) -> Option<u8> {
        if self.pos >= self.data.len() {
            return None;
        }
        let b = self.current_byte;
        self.pos += 1;
        if self.pos < self.data.len() {
            self.current_byte = self.data[self.pos];
        }
        Some(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_woff() {
        assert!(!is_woff(b"test"));
        assert!(is_woff(b"wOFF...."));
    }
}
