//! ISOBMFF Container Parser
//!
//! Parses the ISO Base Media File Format container used by AVIF.

use super::AvifError;

/// AVIF container metadata extracted from ISOBMFF boxes
#[derive(Debug, Clone)]
pub struct AvifContainer {
    pub brand: [u8; 4],
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub data_offset: usize,
    pub data_size: usize,
    pub alpha_data: Option<AlphaData>,
}

/// Alpha channel data location
#[derive(Debug, Clone)]
pub struct AlphaData {
    pub offset: usize,
    pub size: usize,
    pub width: u32,
    pub height: u32,
}

/// Generic box header
#[derive(Debug, Clone)]
struct BoxHeader {
    size: u64,
    box_type: [u8; 4],
    header_size: usize,
}

/// Item info entry
#[derive(Debug, Clone)]
struct ItemInfo {
    item_id: u32,
    item_type: [u8; 4],
    is_hidden: bool,
}

/// Item location entry
#[derive(Debug, Clone)]
struct ItemLocation {
    item_id: u32,
    extent_offset: u64,
    extent_length: u64,
}

/// Property association
#[derive(Debug, Clone)]
struct PropertyAssociation {
    item_id: u32,
    property_index: u16,
    essential: bool,
}

/// Parse ISOBMFF container to extract AVIF metadata
pub fn parse_container(data: &[u8]) -> Result<AvifContainer, AvifError> {
    let mut parser = ContainerParser::new(data);
    parser.parse()
}

struct ContainerParser<'a> {
    data: &'a [u8],
    pos: usize,
    
    // Parsed info
    brand: [u8; 4],
    primary_item_id: u32,
    alpha_item_id: Option<u32>,
    items: Vec<ItemInfo>,
    locations: Vec<ItemLocation>,
    associations: Vec<PropertyAssociation>,
    properties: Vec<Property>,
    
    // Image properties
    width: u32,
    height: u32,
    bit_depth: u8,
    color_primaries: u8,
    transfer_characteristics: u8,
    matrix_coefficients: u8,
}

#[derive(Debug, Clone)]
enum Property {
    Ispe { width: u32, height: u32 },
    Pixi { bit_depths: Vec<u8> },
    Colr { 
        color_type: [u8; 4], 
        primaries: u8, 
        transfer: u8, 
        matrix: u8,
    },
    Av1C { config: Vec<u8> },
    Auxc { aux_type: String },
    Unknown,
}

impl<'a> ContainerParser<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            brand: [0; 4],
            primary_item_id: 0,
            alpha_item_id: None,
            items: Vec::new(),
            locations: Vec::new(),
            associations: Vec::new(),
            properties: Vec::new(),
            width: 0,
            height: 0,
            bit_depth: 8,
            color_primaries: 1,
            transfer_characteristics: 13,
            matrix_coefficients: 6,
        }
    }
    
    fn parse(&mut self) -> Result<AvifContainer, AvifError> {
        // Parse all top-level boxes
        while self.pos < self.data.len() {
            let box_header = self.read_box_header()?;
            let box_end = self.pos + (box_header.size as usize) - box_header.header_size;
            
            match &box_header.box_type {
                b"ftyp" => self.parse_ftyp(box_end)?,
                b"meta" => self.parse_meta(box_end)?,
                b"mdat" => { /* Will use iloc for data location */ }
                _ => { /* Skip unknown boxes */ }
            }
            
            self.pos = box_end;
        }
        
        // Validate we found required data
        if self.brand == [0; 4] {
            return Err(AvifError::InvalidBox("Missing ftyp box".into()));
        }
        
        // Find primary item location
        let primary_loc = self.locations.iter()
            .find(|l| l.item_id == self.primary_item_id)
            .ok_or_else(|| AvifError::InvalidBox("Primary item location not found".into()))?;
        
        // Find alpha location if present
        let alpha_data = self.alpha_item_id.and_then(|id| {
            self.locations.iter()
                .find(|l| l.item_id == id)
                .map(|loc| {
                    // Try to get alpha dimensions from properties
                    let (w, h) = self.get_item_dimensions(id).unwrap_or((self.width, self.height));
                    AlphaData {
                        offset: loc.extent_offset as usize,
                        size: loc.extent_length as usize,
                        width: w,
                        height: h,
                    }
                })
        });
        
        Ok(AvifContainer {
            brand: self.brand,
            width: self.width,
            height: self.height,
            bit_depth: self.bit_depth,
            color_primaries: self.color_primaries,
            transfer_characteristics: self.transfer_characteristics,
            matrix_coefficients: self.matrix_coefficients,
            data_offset: primary_loc.extent_offset as usize,
            data_size: primary_loc.extent_length as usize,
            alpha_data,
        })
    }
    
    fn get_item_dimensions(&self, item_id: u32) -> Option<(u32, u32)> {
        for assoc in &self.associations {
            if assoc.item_id == item_id {
                let idx = assoc.property_index as usize;
                if idx > 0 && idx <= self.properties.len() {
                    if let Property::Ispe { width, height } = &self.properties[idx - 1] {
                        return Some((*width, *height));
                    }
                }
            }
        }
        None
    }
    
    fn read_box_header(&mut self) -> Result<BoxHeader, AvifError> {
        if self.pos + 8 > self.data.len() {
            return Err(AvifError::InvalidData);
        }
        
        let size = u32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]) as u64;
        
        let box_type: [u8; 4] = self.data[self.pos + 4..self.pos + 8].try_into()
            .map_err(|_| AvifError::InvalidData)?;
        
        self.pos += 8;
        
        let (size, header_size) = if size == 1 {
            // Extended size
            if self.pos + 8 > self.data.len() {
                return Err(AvifError::InvalidData);
            }
            let extended = u64::from_be_bytes([
                self.data[self.pos], self.data[self.pos + 1],
                self.data[self.pos + 2], self.data[self.pos + 3],
                self.data[self.pos + 4], self.data[self.pos + 5],
                self.data[self.pos + 6], self.data[self.pos + 7],
            ]);
            self.pos += 8;
            (extended, 16)
        } else if size == 0 {
            // Size extends to end of file
            ((self.data.len() - self.pos + 8) as u64, 8)
        } else {
            (size, 8)
        };
        
        Ok(BoxHeader { size, box_type, header_size })
    }
    
    fn parse_ftyp(&mut self, box_end: usize) -> Result<(), AvifError> {
        if self.pos + 4 > box_end {
            return Err(AvifError::InvalidBox("ftyp too short".into()));
        }
        
        self.brand = self.data[self.pos..self.pos + 4].try_into()
            .map_err(|_| AvifError::InvalidData)?;
        
        // Validate AVIF brand
        if self.brand != *b"avif" && self.brand != *b"avis" && self.brand != *b"mif1" {
            return Err(AvifError::UnsupportedFormat);
        }
        
        Ok(())
    }
    
    fn parse_meta(&mut self, box_end: usize) -> Result<(), AvifError> {
        // Skip version and flags
        if self.pos + 4 > box_end {
            return Err(AvifError::InvalidBox("meta too short".into()));
        }
        self.pos += 4;
        
        // Parse child boxes
        while self.pos < box_end {
            let box_header = self.read_box_header()?;
            let child_end = self.pos + (box_header.size as usize) - box_header.header_size;
            
            if child_end > box_end {
                break;
            }
            
            match &box_header.box_type {
                b"hdlr" => { /* Handler box - skip */ }
                b"pitm" => self.parse_pitm(child_end)?,
                b"iinf" => self.parse_iinf(child_end)?,
                b"iloc" => self.parse_iloc(child_end)?,
                b"iprp" => self.parse_iprp(child_end)?,
                b"iref" => self.parse_iref(child_end)?,
                _ => {}
            }
            
            self.pos = child_end;
        }
        
        Ok(())
    }
    
    fn parse_pitm(&mut self, box_end: usize) -> Result<(), AvifError> {
        if self.pos >= box_end {
            return Err(AvifError::InvalidBox("pitm too short".into()));
        }
        
        let version = self.data[self.pos];
        self.pos += 4; // Skip version and flags
        
        self.primary_item_id = if version == 0 {
            if self.pos + 2 > box_end {
                return Err(AvifError::InvalidData);
            }
            let id = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]) as u32;
            self.pos += 2;
            id
        } else {
            if self.pos + 4 > box_end {
                return Err(AvifError::InvalidData);
            }
            let id = u32::from_be_bytes([
                self.data[self.pos], self.data[self.pos + 1],
                self.data[self.pos + 2], self.data[self.pos + 3],
            ]);
            self.pos += 4;
            id
        };
        
        Ok(())
    }
    
    fn parse_iinf(&mut self, box_end: usize) -> Result<(), AvifError> {
        if self.pos + 4 > box_end {
            return Err(AvifError::InvalidBox("iinf too short".into()));
        }
        
        let version = self.data[self.pos];
        self.pos += 4;
        
        let entry_count = if version == 0 {
            if self.pos + 2 > box_end { return Err(AvifError::InvalidData); }
            let count = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]) as u32;
            self.pos += 2;
            count
        } else {
            if self.pos + 4 > box_end { return Err(AvifError::InvalidData); }
            let count = u32::from_be_bytes([
                self.data[self.pos], self.data[self.pos + 1],
                self.data[self.pos + 2], self.data[self.pos + 3],
            ]);
            self.pos += 4;
            count
        };
        
        for _ in 0..entry_count {
            if self.pos >= box_end {
                break;
            }
            let box_header = self.read_box_header()?;
            if &box_header.box_type == b"infe" {
                let infe_end = self.pos + (box_header.size as usize) - box_header.header_size;
                self.parse_infe(infe_end.min(box_end))?;
                self.pos = infe_end;
            }
        }
        
        Ok(())
    }
    
    fn parse_infe(&mut self, box_end: usize) -> Result<(), AvifError> {
        if self.pos + 4 > box_end {
            return Ok(());
        }
        
        let version = self.data[self.pos];
        self.pos += 4;
        
        let item_id = if version < 3 {
            if self.pos + 2 > box_end { return Ok(()); }
            let id = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]) as u32;
            self.pos += 2;
            id
        } else {
            if self.pos + 4 > box_end { return Ok(()); }
            let id = u32::from_be_bytes([
                self.data[self.pos], self.data[self.pos + 1],
                self.data[self.pos + 2], self.data[self.pos + 3],
            ]);
            self.pos += 4;
            id
        };
        
        // Skip item_protection_index
        self.pos += 2;
        
        if version >= 2 {
            if self.pos + 4 > box_end { return Ok(()); }
            let item_type: [u8; 4] = self.data[self.pos..self.pos + 4].try_into()
                .map_err(|_| AvifError::InvalidData)?;
            self.pos += 4;
            
            self.items.push(ItemInfo {
                item_id,
                item_type,
                is_hidden: false,
            });
        }
        
        Ok(())
    }
    
    fn parse_iloc(&mut self, box_end: usize) -> Result<(), AvifError> {
        if self.pos + 4 > box_end {
            return Err(AvifError::InvalidBox("iloc too short".into()));
        }
        
        let version = self.data[self.pos];
        self.pos += 4;
        
        if self.pos + 2 > box_end {
            return Err(AvifError::InvalidData);
        }
        
        let offset_size = (self.data[self.pos] >> 4) & 0x0F;
        let length_size = self.data[self.pos] & 0x0F;
        let base_offset_size = (self.data[self.pos + 1] >> 4) & 0x0F;
        let index_size = if version == 1 || version == 2 {
            self.data[self.pos + 1] & 0x0F
        } else {
            0
        };
        self.pos += 2;
        
        let item_count = if version < 2 {
            if self.pos + 2 > box_end { return Err(AvifError::InvalidData); }
            let count = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]) as u32;
            self.pos += 2;
            count
        } else {
            if self.pos + 4 > box_end { return Err(AvifError::InvalidData); }
            let count = u32::from_be_bytes([
                self.data[self.pos], self.data[self.pos + 1],
                self.data[self.pos + 2], self.data[self.pos + 3],
            ]);
            self.pos += 4;
            count
        };
        
        for _ in 0..item_count {
            if self.pos >= box_end {
                break;
            }
            
            let item_id = if version < 2 {
                if self.pos + 2 > box_end { break; }
                let id = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]) as u32;
                self.pos += 2;
                id
            } else {
                if self.pos + 4 > box_end { break; }
                let id = u32::from_be_bytes([
                    self.data[self.pos], self.data[self.pos + 1],
                    self.data[self.pos + 2], self.data[self.pos + 3],
                ]);
                self.pos += 4;
                id
            };
            
            if version == 1 || version == 2 {
                self.pos += 2; // construction_method
            }
            
            self.pos += 2; // data_reference_index
            
            // Skip base_offset
            self.pos += base_offset_size as usize;
            
            if self.pos + 2 > box_end { break; }
            let extent_count = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
            self.pos += 2;
            
            for _ in 0..extent_count {
                if index_size > 0 {
                    self.pos += index_size as usize;
                }
                
                let extent_offset = self.read_variable_int(offset_size)?;
                let extent_length = self.read_variable_int(length_size)?;
                
                self.locations.push(ItemLocation {
                    item_id,
                    extent_offset,
                    extent_length,
                });
            }
        }
        
        Ok(())
    }
    
    fn read_variable_int(&mut self, size: u8) -> Result<u64, AvifError> {
        if size == 0 {
            return Ok(0);
        }
        
        if self.pos + (size as usize) > self.data.len() {
            return Err(AvifError::InvalidData);
        }
        
        let mut value: u64 = 0;
        for i in 0..size as usize {
            value = (value << 8) | (self.data[self.pos + i] as u64);
        }
        self.pos += size as usize;
        
        Ok(value)
    }
    
    fn parse_iprp(&mut self, box_end: usize) -> Result<(), AvifError> {
        while self.pos < box_end {
            let box_header = self.read_box_header()?;
            let child_end = self.pos + (box_header.size as usize) - box_header.header_size;
            
            match &box_header.box_type {
                b"ipco" => self.parse_ipco(child_end.min(box_end))?,
                b"ipma" => self.parse_ipma(child_end.min(box_end))?,
                _ => {}
            }
            
            self.pos = child_end;
        }
        
        Ok(())
    }
    
    fn parse_ipco(&mut self, box_end: usize) -> Result<(), AvifError> {
        while self.pos < box_end {
            let box_header = self.read_box_header()?;
            let prop_end = self.pos + (box_header.size as usize) - box_header.header_size;
            
            let property = match &box_header.box_type {
                b"ispe" => self.parse_ispe(prop_end)?,
                b"pixi" => self.parse_pixi(prop_end)?,
                b"colr" => self.parse_colr(prop_end)?,
                b"av1C" => self.parse_av1c(prop_end)?,
                b"auxC" => self.parse_auxc(prop_end)?,
                _ => Property::Unknown,
            };
            
            self.properties.push(property);
            self.pos = prop_end;
        }
        
        Ok(())
    }
    
    fn parse_ispe(&mut self, box_end: usize) -> Result<Property, AvifError> {
        self.pos += 4; // Skip version and flags
        
        if self.pos + 8 > box_end {
            return Err(AvifError::InvalidBox("ispe too short".into()));
        }
        
        let width = u32::from_be_bytes([
            self.data[self.pos], self.data[self.pos + 1],
            self.data[self.pos + 2], self.data[self.pos + 3],
        ]);
        let height = u32::from_be_bytes([
            self.data[self.pos + 4], self.data[self.pos + 5],
            self.data[self.pos + 6], self.data[self.pos + 7],
        ]);
        
        Ok(Property::Ispe { width, height })
    }
    
    fn parse_pixi(&mut self, box_end: usize) -> Result<Property, AvifError> {
        self.pos += 4; // Skip version and flags
        
        if self.pos >= box_end {
            return Ok(Property::Unknown);
        }
        
        let num_channels = self.data[self.pos] as usize;
        self.pos += 1;
        
        let mut bit_depths = Vec::with_capacity(num_channels);
        for _ in 0..num_channels {
            if self.pos >= box_end {
                break;
            }
            bit_depths.push(self.data[self.pos]);
            self.pos += 1;
        }
        
        Ok(Property::Pixi { bit_depths })
    }
    
    fn parse_colr(&mut self, box_end: usize) -> Result<Property, AvifError> {
        if self.pos + 4 > box_end {
            return Ok(Property::Unknown);
        }
        
        let color_type: [u8; 4] = self.data[self.pos..self.pos + 4].try_into()
            .map_err(|_| AvifError::InvalidData)?;
        self.pos += 4;
        
        if &color_type == b"nclx" {
            if self.pos + 4 > box_end {
                return Ok(Property::Unknown);
            }
            
            let primaries = (u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]) & 0xFF) as u8;
            let transfer = (u16::from_be_bytes([self.data[self.pos + 2], self.data[self.pos + 3]]) & 0xFF) as u8;
            self.pos += 4;
            
            let matrix = if self.pos + 2 <= box_end {
                (u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]) & 0xFF) as u8
            } else {
                6
            };
            
            Ok(Property::Colr { color_type, primaries, transfer, matrix })
        } else {
            Ok(Property::Colr { color_type, primaries: 1, transfer: 13, matrix: 6 })
        }
    }
    
    fn parse_av1c(&mut self, box_end: usize) -> Result<Property, AvifError> {
        let config = self.data[self.pos..box_end].to_vec();
        Ok(Property::Av1C { config })
    }
    
    fn parse_auxc(&mut self, box_end: usize) -> Result<Property, AvifError> {
        self.pos += 4; // Skip version and flags
        
        // Read null-terminated aux_type string
        let mut aux_type = String::new();
        while self.pos < box_end && self.data[self.pos] != 0 {
            aux_type.push(self.data[self.pos] as char);
            self.pos += 1;
        }
        
        Ok(Property::Auxc { aux_type })
    }
    
    fn parse_ipma(&mut self, box_end: usize) -> Result<(), AvifError> {
        if self.pos + 4 > box_end {
            return Ok(());
        }
        
        let version = self.data[self.pos];
        let flags = self.data[self.pos + 3];
        self.pos += 4;
        
        if self.pos + 4 > box_end {
            return Ok(());
        }
        
        let entry_count = u32::from_be_bytes([
            self.data[self.pos], self.data[self.pos + 1],
            self.data[self.pos + 2], self.data[self.pos + 3],
        ]);
        self.pos += 4;
        
        for _ in 0..entry_count {
            if self.pos >= box_end {
                break;
            }
            
            let item_id = if version < 1 {
                if self.pos + 2 > box_end { break; }
                let id = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]) as u32;
                self.pos += 2;
                id
            } else {
                if self.pos + 4 > box_end { break; }
                let id = u32::from_be_bytes([
                    self.data[self.pos], self.data[self.pos + 1],
                    self.data[self.pos + 2], self.data[self.pos + 3],
                ]);
                self.pos += 4;
                id
            };
            
            if self.pos >= box_end { break; }
            let assoc_count = self.data[self.pos];
            self.pos += 1;
            
            for _ in 0..assoc_count {
                if self.pos >= box_end { break; }
                
                let (essential, property_index) = if flags & 1 != 0 {
                    if self.pos + 2 > box_end { break; }
                    let byte1 = self.data[self.pos];
                    let byte2 = self.data[self.pos + 1];
                    self.pos += 2;
                    let essential = byte1 & 0x80 != 0;
                    let idx = (((byte1 & 0x7F) as u16) << 8) | (byte2 as u16);
                    (essential, idx)
                } else {
                    let byte = self.data[self.pos];
                    self.pos += 1;
                    let essential = byte & 0x80 != 0;
                    let idx = (byte & 0x7F) as u16;
                    (essential, idx)
                };
                
                self.associations.push(PropertyAssociation {
                    item_id,
                    property_index,
                    essential,
                });
                
                // Apply properties to primary item
                if item_id == self.primary_item_id && property_index > 0 {
                    let idx = (property_index - 1) as usize;
                    if idx < self.properties.len() {
                        match &self.properties[idx] {
                            Property::Ispe { width, height } => {
                                self.width = *width;
                                self.height = *height;
                            }
                            Property::Pixi { bit_depths } => {
                                if !bit_depths.is_empty() {
                                    self.bit_depth = bit_depths[0];
                                }
                            }
                            Property::Colr { primaries, transfer, matrix, .. } => {
                                self.color_primaries = *primaries;
                                self.transfer_characteristics = *transfer;
                                self.matrix_coefficients = *matrix;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn parse_iref(&mut self, box_end: usize) -> Result<(), AvifError> {
        if self.pos + 4 > box_end {
            return Ok(());
        }
        
        let version = self.data[self.pos];
        self.pos += 4;
        
        while self.pos < box_end {
            let box_header = self.read_box_header()?;
            let ref_end = self.pos + (box_header.size as usize) - box_header.header_size;
            
            if &box_header.box_type == b"auxl" {
                // Auxiliary image reference
                let from_id = if version == 0 {
                    if self.pos + 2 > ref_end { break; }
                    let id = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]) as u32;
                    self.pos += 2;
                    id
                } else {
                    if self.pos + 4 > ref_end { break; }
                    let id = u32::from_be_bytes([
                        self.data[self.pos], self.data[self.pos + 1],
                        self.data[self.pos + 2], self.data[self.pos + 3],
                    ]);
                    self.pos += 4;
                    id
                };
                
                if self.pos + 2 > ref_end { break; }
                let ref_count = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
                self.pos += 2;
                
                for _ in 0..ref_count {
                    let to_id = if version == 0 {
                        if self.pos + 2 > ref_end { break; }
                        let id = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]) as u32;
                        self.pos += 2;
                        id
                    } else {
                        if self.pos + 4 > ref_end { break; }
                        let id = u32::from_be_bytes([
                            self.data[self.pos], self.data[self.pos + 1],
                            self.data[self.pos + 2], self.data[self.pos + 3],
                        ]);
                        self.pos += 4;
                        id
                    };
                    
                    // Check if this is an alpha reference to our primary item
                    if to_id == self.primary_item_id {
                        // Check if from_id is an alpha auxiliary
                        if self.is_alpha_item(from_id) {
                            self.alpha_item_id = Some(from_id);
                        }
                    }
                }
            }
            
            self.pos = ref_end;
        }
        
        Ok(())
    }
    
    fn is_alpha_item(&self, item_id: u32) -> bool {
        for assoc in &self.associations {
            if assoc.item_id == item_id {
                let idx = (assoc.property_index.saturating_sub(1)) as usize;
                if idx < self.properties.len() {
                    if let Property::Auxc { aux_type } = &self.properties[idx] {
                        if aux_type.contains("alpha") || aux_type == "urn:mpeg:mpegB:cicp:systems:auxiliary:alpha" {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_box(box_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let size = (8 + data.len()) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&size.to_be_bytes());
        out.extend_from_slice(box_type);
        out.extend_from_slice(data);
        out
    }
    
    #[test]
    fn test_parse_ftyp() {
        let ftyp = make_box(b"ftyp", b"avif\x00\x00\x00\x00mif1miafMA1B");
        
        let mut parser = ContainerParser::new(&ftyp);
        let header = parser.read_box_header().unwrap();
        assert_eq!(&header.box_type, b"ftyp");
        parser.parse_ftyp(ftyp.len()).unwrap();
        assert_eq!(&parser.brand, b"avif");
    }
    
    #[test]
    fn test_invalid_brand() {
        let ftyp = make_box(b"ftyp", b"mp41\x00\x00\x00\x00");
        
        let mut parser = ContainerParser::new(&ftyp);
        let _ = parser.read_box_header().unwrap();
        assert!(parser.parse_ftyp(ftyp.len()).is_err());
    }
}
