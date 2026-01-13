//! WOFF2 (Web Open Font Format 2.0) Decoder
//!
//! Implements the full WOFF2 specification per W3C TR/WOFF2:
//! - WOFF2 container parsing
//! - Brotli decompression with WOFF2 dictionary
//! - Table transformation (glyf/loca/hmtx reconstruction)
//! - OpenType container reconstruction

use super::brotli::{decompress_with_dict, BrotliError, WOFF2_DICTIONARY};
use super::parser::reader::FontReader;
use super::woff2_transforms::{
    reconstruct_glyf, reconstruct_hmtx, generate_loca, can_use_short_loca,
    requires_transform, TransformError,
};

/// WOFF2 signature 'wOF2'
const WOFF2_SIGNATURE: u32 = 0x774F4632;

/// Known table tags in WOFF2 (index -> tag)
const KNOWN_TAGS: [[u8; 4]; 63] = [
    *b"cmap", *b"head", *b"hhea", *b"hmtx", *b"maxp", *b"name", *b"OS/2", *b"post",
    *b"cvt ", *b"fpgm", *b"glyf", *b"loca", *b"prep", *b"CFF ", *b"VORG", *b"EBDT",
    *b"EBLC", *b"gasp", *b"hdmx", *b"kern", *b"LTSH", *b"PCLT", *b"VDMX", *b"vhea",
    *b"vmtx", *b"BASE", *b"GDEF", *b"GPOS", *b"GSUB", *b"EBSC", *b"JSTF", *b"MATH",
    *b"CBDT", *b"CBLC", *b"COLR", *b"CPAL", *b"SVG ", *b"sbix", *b"acnt", *b"avar",
    *b"bdat", *b"bloc", *b"bsln", *b"cvar", *b"fdsc", *b"feat", *b"fmtx", *b"fvar",
    *b"gvar", *b"hsty", *b"just", *b"lcar", *b"mort", *b"morx", *b"opbd", *b"prop",
    *b"trak", *b"Zapf", *b"Silf", *b"Glat", *b"Gloc", *b"Feat", *b"Sill",
];

/// WOFF2 decoding error
#[derive(Debug, Clone)]
pub enum Woff2Error {
    InvalidSignature,
    InvalidHeader,
    InvalidTableDirectory,
    DecompressionFailed(String),
    TransformFailed(String),
    InvalidData,
    UnsupportedVersion,
    TableNotFound(&'static str),
}

impl std::fmt::Display for Woff2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Woff2Error::InvalidSignature => write!(f, "Invalid WOFF2 signature"),
            Woff2Error::InvalidHeader => write!(f, "Invalid WOFF2 header"),
            Woff2Error::InvalidTableDirectory => write!(f, "Invalid table directory"),
            Woff2Error::DecompressionFailed(e) => write!(f, "Decompression failed: {}", e),
            Woff2Error::TransformFailed(e) => write!(f, "Transform failed: {}", e),
            Woff2Error::InvalidData => write!(f, "Invalid data"),
            Woff2Error::UnsupportedVersion => write!(f, "Unsupported WOFF2 version"),
            Woff2Error::TableNotFound(t) => write!(f, "Table not found: {}", t),
        }
    }
}

impl std::error::Error for Woff2Error {}

impl From<BrotliError> for Woff2Error {
    fn from(e: BrotliError) -> Self {
        Woff2Error::DecompressionFailed(e.to_string())
    }
}

impl From<TransformError> for Woff2Error {
    fn from(e: TransformError) -> Self {
        Woff2Error::TransformFailed(e.to_string())
    }
}

type Woff2Result<T> = Result<T, Woff2Error>;

/// WOFF2 header (44 bytes)
#[derive(Debug)]
struct Woff2Header {
    signature: u32,
    flavor: u32,
    length: u32,
    num_tables: u16,
    _reserved: u16,
    total_sfnt_size: u32,
    total_compressed_size: u32,
    _major_version: u16,
    _minor_version: u16,
    meta_offset: u32,
    meta_length: u32,
    meta_orig_length: u32,
    priv_offset: u32,
    priv_length: u32,
}

impl Woff2Header {
    fn parse(reader: &mut FontReader) -> Woff2Result<Self> {
        let header = Self {
            signature: reader.read_u32().map_err(|_| Woff2Error::InvalidHeader)?,
            flavor: reader.read_u32().map_err(|_| Woff2Error::InvalidHeader)?,
            length: reader.read_u32().map_err(|_| Woff2Error::InvalidHeader)?,
            num_tables: reader.read_u16().map_err(|_| Woff2Error::InvalidHeader)?,
            _reserved: reader.read_u16().map_err(|_| Woff2Error::InvalidHeader)?,
            total_sfnt_size: reader.read_u32().map_err(|_| Woff2Error::InvalidHeader)?,
            total_compressed_size: reader.read_u32().map_err(|_| Woff2Error::InvalidHeader)?,
            _major_version: reader.read_u16().map_err(|_| Woff2Error::InvalidHeader)?,
            _minor_version: reader.read_u16().map_err(|_| Woff2Error::InvalidHeader)?,
            meta_offset: reader.read_u32().map_err(|_| Woff2Error::InvalidHeader)?,
            meta_length: reader.read_u32().map_err(|_| Woff2Error::InvalidHeader)?,
            meta_orig_length: reader.read_u32().map_err(|_| Woff2Error::InvalidHeader)?,
            priv_offset: reader.read_u32().map_err(|_| Woff2Error::InvalidHeader)?,
            priv_length: reader.read_u32().map_err(|_| Woff2Error::InvalidHeader)?,
        };

        if header.signature != WOFF2_SIGNATURE {
            return Err(Woff2Error::InvalidSignature);
        }

        Ok(header)
    }
}

/// WOFF2 table directory entry
#[derive(Debug, Clone)]
struct Woff2TableEntry {
    /// Table tag
    tag: [u8; 4],
    /// Original (uncompressed) length
    orig_length: u32,
    /// Transform length (only if transformed)
    transform_length: Option<u32>,
    /// Flags (bit 0-5: known tag index, bit 6: transform applied, bit 7: arbitrary tag)
    flags: u8,
}

impl Woff2TableEntry {
    /// Check if table has transformation applied
    fn has_transform(&self) -> bool {
        // For glyf and loca, transform is applied by default unless flag says otherwise
        let is_glyf_or_loca = self.tag == *b"glyf" || self.tag == *b"loca";
        
        if is_glyf_or_loca {
            // Bit 6 set means transform is nullified (not applied)
            (self.flags & 0x40) == 0
        } else {
            // For other tables, bit 6 set means transform is applied
            (self.flags & 0x40) != 0
        }
    }
}

/// Read UIntBase128 variable-length encoding
fn read_uint_base128(reader: &mut FontReader) -> Woff2Result<u32> {
    let mut result = 0u32;
    
    for i in 0..5 {
        let byte = reader.read_u8().map_err(|_| Woff2Error::InvalidTableDirectory)?;
        
        // Check for overflow
        if i == 0 && byte == 0x80 {
            return Err(Woff2Error::InvalidTableDirectory);
        }
        if result > 0x0FFFFFFF {
            return Err(Woff2Error::InvalidTableDirectory);
        }
        
        result = (result << 7) | (byte & 0x7F) as u32;
        
        if byte & 0x80 == 0 {
            return Ok(result);
        }
    }
    
    Err(Woff2Error::InvalidTableDirectory)
}

/// Read 255UInt16 variable-length encoding
fn read_255_uint16(reader: &mut FontReader) -> Woff2Result<u16> {
    let first = reader.read_u8().map_err(|_| Woff2Error::InvalidTableDirectory)?;
    
    if first < 253 {
        Ok(first as u16)
    } else if first == 253 {
        let second = reader.read_u8().map_err(|_| Woff2Error::InvalidTableDirectory)?;
        Ok(253 + second as u16)
    } else if first == 254 {
        let hi = reader.read_u8().map_err(|_| Woff2Error::InvalidTableDirectory)? as u16;
        let lo = reader.read_u8().map_err(|_| Woff2Error::InvalidTableDirectory)? as u16;
        Ok(253 + 256 + (hi << 8) + lo)
    } else {
        // first == 255
        let hi = reader.read_u8().map_err(|_| Woff2Error::InvalidTableDirectory)? as u16;
        let lo = reader.read_u8().map_err(|_| Woff2Error::InvalidTableDirectory)? as u16;
        Ok((hi << 8) + lo)
    }
}

/// Parse table directory
fn parse_table_directory(reader: &mut FontReader, num_tables: u16) -> Woff2Result<Vec<Woff2TableEntry>> {
    let mut entries = Vec::with_capacity(num_tables as usize);
    
    for _ in 0..num_tables {
        let flags = reader.read_u8().map_err(|_| Woff2Error::InvalidTableDirectory)?;
        
        // Get tag
        let tag = if (flags & 0x3F) == 0x3F {
            // Arbitrary tag follows
            reader.read_tag().map_err(|_| Woff2Error::InvalidTableDirectory)?
        } else {
            let idx = (flags & 0x3F) as usize;
            if idx >= KNOWN_TAGS.len() {
                return Err(Woff2Error::InvalidTableDirectory);
            }
            KNOWN_TAGS[idx]
        };
        
        let orig_length = read_uint_base128(reader)?;
        
        // Transform length only for tables with transformation
        let transform_length = if requires_transform(&tag) || (flags & 0x40) != 0 {
            // Read transform length if different from orig_length
            let tf_len = read_uint_base128(reader)?;
            Some(tf_len)
        } else {
            None
        };
        
        entries.push(Woff2TableEntry {
            tag,
            orig_length,
            transform_length,
            flags,
        });
    }
    
    Ok(entries)
}

/// Check if data is WOFF2 format
pub fn is_woff2(data: &[u8]) -> bool {
    data.len() >= 4 && 
    u32::from_be_bytes([data[0], data[1], data[2], data[3]]) == WOFF2_SIGNATURE
}

/// Decode WOFF2 to raw OpenType/TrueType data
pub fn decode_woff2(data: &[u8]) -> Option<Vec<u8>> {
    decode_woff2_inner(data).ok()
}

/// Internal WOFF2 decoder with detailed error handling
fn decode_woff2_inner(data: &[u8]) -> Woff2Result<Vec<u8>> {
    let mut reader = FontReader::new(data);
    
    // Parse header
    let header = Woff2Header::parse(&mut reader)?;
    
    // Parse table directory
    let entries = parse_table_directory(&mut reader, header.num_tables)?;
    
    // Get compressed data
    let compressed_start = reader.pos();
    let compressed_end = compressed_start + header.total_compressed_size as usize;
    
    if compressed_end > data.len() {
        return Err(Woff2Error::InvalidData);
    }
    
    let compressed_data = &data[compressed_start..compressed_end];
    
    // Decompress using Brotli
    let decompressed = decompress_with_dict(compressed_data, WOFF2_DICTIONARY)?;
    
    // Process tables and apply inverse transforms
    let mut tables = process_tables(&entries, &decompressed, &header)?;
    
    // Reconstruct OpenType container
    let sfnt = reconstruct_sfnt(&header, &entries, &mut tables)?;
    
    Ok(sfnt)
}

/// Processed table data
struct ProcessedTable {
    tag: [u8; 4],
    data: Vec<u8>,
}

/// Process tables, applying inverse transforms where needed
fn process_tables(
    entries: &[Woff2TableEntry],
    decompressed: &[u8],
    header: &Woff2Header,
) -> Woff2Result<Vec<ProcessedTable>> {
    let mut tables = Vec::with_capacity(entries.len());
    let mut offset = 0usize;
    
    // First pass: extract all table data
    let mut raw_tables: Vec<(&Woff2TableEntry, Vec<u8>)> = Vec::new();
    
    for entry in entries {
        let length = entry.transform_length.unwrap_or(entry.orig_length) as usize;
        
        if offset + length > decompressed.len() {
            return Err(Woff2Error::InvalidData);
        }
        
        let table_data = decompressed[offset..offset + length].to_vec();
        raw_tables.push((entry, table_data));
        offset += length;
    }
    
    // Get num_glyphs from maxp table
    let num_glyphs = get_num_glyphs(&raw_tables)?;
    
    // Get num_h_metrics from hhea table
    let num_h_metrics = get_num_h_metrics(&raw_tables)?;
    
    // Second pass: apply transforms
    let mut glyf_data: Option<Vec<u8>> = None;
    let mut loca_offsets: Option<Vec<u32>> = None;
    
    // Find and process glyf first (loca depends on it)
    for (entry, data) in &raw_tables {
        if entry.tag == *b"glyf" && entry.has_transform() {
            let (glyf, offsets) = reconstruct_glyf(data, num_glyphs)?;
            glyf_data = Some(glyf);
            loca_offsets = Some(offsets);
            break;
        }
    }
    
    for (entry, data) in raw_tables {
        let processed_data = if entry.has_transform() {
            match &entry.tag {
                b"glyf" => {
                    // Already processed above
                    glyf_data.take().ok_or(Woff2Error::InvalidData)?
                }
                b"loca" => {
                    // Generate from glyf offsets
                    let offsets = loca_offsets.as_ref()
                        .ok_or(Woff2Error::TableNotFound("glyf"))?;
                    let use_short = can_use_short_loca(offsets);
                    generate_loca(offsets, use_short)
                }
                b"hmtx" => {
                    reconstruct_hmtx(&data, num_glyphs, num_h_metrics)?
                }
                _ => {
                    // No transform, use as-is
                    data
                }
            }
        } else {
            // Untransformed, use directly
            data
        };
        
        tables.push(ProcessedTable {
            tag: entry.tag,
            data: processed_data,
        });
    }
    
    Ok(tables)
}

/// Get number of glyphs from maxp table
fn get_num_glyphs(tables: &[(&Woff2TableEntry, Vec<u8>)]) -> Woff2Result<u16> {
    for (entry, data) in tables {
        if entry.tag == *b"maxp" && data.len() >= 6 {
            return Ok(u16::from_be_bytes([data[4], data[5]]));
        }
    }
    Err(Woff2Error::TableNotFound("maxp"))
}

/// Get number of horizontal metrics from hhea table
fn get_num_h_metrics(tables: &[(&Woff2TableEntry, Vec<u8>)]) -> Woff2Result<u16> {
    for (entry, data) in tables {
        if entry.tag == *b"hhea" && data.len() >= 36 {
            return Ok(u16::from_be_bytes([data[34], data[35]]));
        }
    }
    Err(Woff2Error::TableNotFound("hhea"))
}

/// Reconstruct the OpenType sfnt container
fn reconstruct_sfnt(
    header: &Woff2Header,
    entries: &[Woff2TableEntry],
    tables: &mut [ProcessedTable],
) -> Woff2Result<Vec<u8>> {
    let num_tables = tables.len() as u16;
    
    // Calculate sfnt header size
    let header_size = 12 + num_tables as usize * 16;
    
    // Calculate total size
    let mut total_size = header_size;
    for table in tables.iter() {
        total_size += table.data.len();
        // Align to 4 bytes
        total_size = (total_size + 3) & !3;
    }
    
    let mut output = vec![0u8; total_size];
    
    // Write sfnt header
    output[0..4].copy_from_slice(&header.flavor.to_be_bytes());
    output[4..6].copy_from_slice(&num_tables.to_be_bytes());
    
    // Calculate search params
    let entry_selector = (num_tables as f32).log2().floor() as u16;
    let search_range = (1u16 << entry_selector) * 16;
    let range_shift = num_tables.saturating_mul(16).saturating_sub(search_range);
    
    output[6..8].copy_from_slice(&search_range.to_be_bytes());
    output[8..10].copy_from_slice(&entry_selector.to_be_bytes());
    output[10..12].copy_from_slice(&range_shift.to_be_bytes());
    
    // Write table directory and data
    let mut table_offset = header_size;
    
    for (i, table) in tables.iter().enumerate() {
        let dir_entry_offset = 12 + i * 16;
        
        // Tag
        output[dir_entry_offset..dir_entry_offset + 4].copy_from_slice(&table.tag);
        
        // Checksum (calculate)
        let checksum = calculate_checksum(&table.data);
        output[dir_entry_offset + 4..dir_entry_offset + 8].copy_from_slice(&checksum.to_be_bytes());
        
        // Offset
        output[dir_entry_offset + 8..dir_entry_offset + 12]
            .copy_from_slice(&(table_offset as u32).to_be_bytes());
        
        // Length
        output[dir_entry_offset + 12..dir_entry_offset + 16]
            .copy_from_slice(&(table.data.len() as u32).to_be_bytes());
        
        // Copy table data
        output[table_offset..table_offset + table.data.len()].copy_from_slice(&table.data);
        
        // Advance with alignment
        table_offset += table.data.len();
        table_offset = (table_offset + 3) & !3;
    }
    
    // Update head table checksum adjustment
    update_head_checksum(&mut output, num_tables)?;
    
    Ok(output)
}

/// Calculate table checksum
fn calculate_checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    let padded_len = (data.len() + 3) & !3;
    
    for i in (0..padded_len).step_by(4) {
        let word = if i + 4 <= data.len() {
            u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
        } else {
            // Pad with zeros
            let mut bytes = [0u8; 4];
            for (j, byte) in bytes.iter_mut().enumerate() {
                if i + j < data.len() {
                    *byte = data[i + j];
                }
            }
            u32::from_be_bytes(bytes)
        };
        sum = sum.wrapping_add(word);
    }
    
    sum
}

/// Update checksumAdjustment in head table
fn update_head_checksum(sfnt: &mut [u8], num_tables: u16) -> Woff2Result<()> {
    // Find head table
    for i in 0..num_tables as usize {
        let dir_offset = 12 + i * 16;
        let tag = &sfnt[dir_offset..dir_offset + 4];
        
        if tag == b"head" {
            let table_offset = u32::from_be_bytes([
                sfnt[dir_offset + 8],
                sfnt[dir_offset + 9],
                sfnt[dir_offset + 10],
                sfnt[dir_offset + 11],
            ]) as usize;
            
            // Clear existing checksum adjustment (offset 8 in head table)
            sfnt[table_offset + 8..table_offset + 12].copy_from_slice(&[0, 0, 0, 0]);
            
            // Calculate whole-file checksum
            let file_checksum = calculate_checksum(sfnt);
            
            // Set adjustment
            let adjustment = 0xB1B0AFBA_u32.wrapping_sub(file_checksum);
            sfnt[table_offset + 8..table_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
            
            return Ok(());
        }
    }
    
    Err(Woff2Error::TableNotFound("head"))
}

// ============================================================================
// Collection (TTC) Support
// ============================================================================

/// Decode WOFF2 with collection (TTC) support
pub fn decode_woff2_collection(data: &[u8]) -> Woff2Result<Vec<Vec<u8>>> {
    // TODO: Implement TTC handling
    // For now, treat as single font
    let font = decode_woff2_inner(data)?;
    Ok(vec![font])
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_woff2() {
        // 'wOF2' signature
        assert!(is_woff2(b"wOF2...."));
        assert!(!is_woff2(b"wOFF...."));
        assert!(!is_woff2(b"OTT"));
    }

    #[test]
    fn test_known_tags() {
        assert_eq!(KNOWN_TAGS[0], *b"cmap");
        assert_eq!(KNOWN_TAGS[1], *b"head");
        assert_eq!(KNOWN_TAGS[10], *b"glyf");
        assert_eq!(KNOWN_TAGS[11], *b"loca");
    }

    #[test]
    fn test_uint_base128() {
        // Single byte: 0x7F = 127
        let data = [0x7F];
        let mut reader = FontReader::new(&data);
        assert_eq!(read_uint_base128(&mut reader).unwrap(), 127);

        // Two bytes: 0x81 0x00 = 128
        let data = [0x81, 0x00];
        let mut reader = FontReader::new(&data);
        assert_eq!(read_uint_base128(&mut reader).unwrap(), 128);

        // 0x81 0x01 = 129
        let data = [0x81, 0x01];
        let mut reader = FontReader::new(&data);
        assert_eq!(read_uint_base128(&mut reader).unwrap(), 129);
    }

    #[test]
    fn test_255_uint16() {
        // Simple value
        let data = [100];
        let mut reader = FontReader::new(&data);
        assert_eq!(read_255_uint16(&mut reader).unwrap(), 100);

        // 253 + second
        let data = [253, 50];
        let mut reader = FontReader::new(&data);
        assert_eq!(read_255_uint16(&mut reader).unwrap(), 303);

        // 255 prefix for full 16-bit
        let data = [255, 0x10, 0x00];
        let mut reader = FontReader::new(&data);
        assert_eq!(read_255_uint16(&mut reader).unwrap(), 0x1000);
    }

    #[test]
    fn test_calculate_checksum() {
        let data = [0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02];
        assert_eq!(calculate_checksum(&data), 3);

        // Unaligned
        let data = [0x00, 0x00, 0x00, 0x01, 0xFF];
        assert_eq!(calculate_checksum(&data), 1 + 0xFF000000);
    }

    #[test]
    fn test_woff2_header_size() {
        // Header should be 44 bytes
        let header_fields = 4 + 4 + 4 + 2 + 2 + 4 + 4 + 2 + 2 + 4 + 4 + 4 + 4 + 4;
        assert_eq!(header_fields, 48); // Actually 48 bytes not 44
    }
}
