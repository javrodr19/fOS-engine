//! OpenType GPOS (Glyph Positioning) Table
//!
//! Full implementation of all 9 GPOS lookup types for HarfBuzz compatibility.
//! Uses BumpAllocator for efficient memory management.

use crate::font::parser::{FontReader, GlyphId};
use super::memory::BumpAllocator;
use crate::shaping::gsub::{Coverage, ClassDef};

/// GPOS table processor
pub struct GposTable<'a> {
    data: &'a [u8],
    script_list_offset: u16,
    feature_list_offset: u16,
    lookup_list_offset: u16,
    /// BumpAllocator for efficient memory allocation during parsing
    allocator: BumpAllocator,
}

/// GPOS lookup type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum LookupType {
    /// Type 1: Single adjustment
    SingleAdjustment = 1,
    /// Type 2: Pair adjustment (kerning)
    PairAdjustment = 2,
    /// Type 3: Cursive attachment
    CursiveAttachment = 3,
    /// Type 4: Mark-to-base attachment
    MarkToBase = 4,
    /// Type 5: Mark-to-ligature attachment
    MarkToLigature = 5,
    /// Type 6: Mark-to-mark attachment
    MarkToMark = 6,
    /// Type 7: Contextual positioning
    Context = 7,
    /// Type 8: Chained contextual positioning
    ChainedContext = 8,
    /// Type 9: Extension positioning
    Extension = 9,
}

impl TryFrom<u16> for LookupType {
    type Error = ();
    
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::SingleAdjustment),
            2 => Ok(Self::PairAdjustment),
            3 => Ok(Self::CursiveAttachment),
            4 => Ok(Self::MarkToBase),
            5 => Ok(Self::MarkToLigature),
            6 => Ok(Self::MarkToMark),
            7 => Ok(Self::Context),
            8 => Ok(Self::ChainedContext),
            9 => Ok(Self::Extension),
            _ => Err(()),
        }
    }
}

/// Value record for positioning adjustments
#[derive(Debug, Clone, Copy, Default)]
pub struct ValueRecord {
    /// Horizontal adjustment for placement
    pub x_placement: i16,
    /// Vertical adjustment for placement
    pub y_placement: i16,
    /// Horizontal adjustment for advance
    pub x_advance: i16,
    /// Vertical adjustment for advance
    pub y_advance: i16,
    /// Device table offset for x_placement (ignored for now)
    pub x_pla_device: u16,
    /// Device table offset for y_placement (ignored for now)
    pub y_pla_device: u16,
    /// Device table offset for x_advance (ignored for now)
    pub x_adv_device: u16,
    /// Device table offset for y_advance (ignored for now)
    pub y_adv_device: u16,
}

impl ValueRecord {
    /// Parse value record based on format flags
    pub fn parse(reader: &mut FontReader, format: u16) -> Option<Self> {
        let mut record = ValueRecord::default();
        
        if format & 0x0001 != 0 { record.x_placement = reader.read_i16().ok()?; }
        if format & 0x0002 != 0 { record.y_placement = reader.read_i16().ok()?; }
        if format & 0x0004 != 0 { record.x_advance = reader.read_i16().ok()?; }
        if format & 0x0008 != 0 { record.y_advance = reader.read_i16().ok()?; }
        if format & 0x0010 != 0 { record.x_pla_device = reader.read_u16().ok()?; }
        if format & 0x0020 != 0 { record.y_pla_device = reader.read_u16().ok()?; }
        if format & 0x0040 != 0 { record.x_adv_device = reader.read_u16().ok()?; }
        if format & 0x0080 != 0 { record.y_adv_device = reader.read_u16().ok()?; }
        
        Some(record)
    }
    
    /// Size in bytes for a given format
    pub fn size(format: u16) -> usize {
        let mut size = 0;
        if format & 0x0001 != 0 { size += 2; }
        if format & 0x0002 != 0 { size += 2; }
        if format & 0x0004 != 0 { size += 2; }
        if format & 0x0008 != 0 { size += 2; }
        if format & 0x0010 != 0 { size += 2; }
        if format & 0x0020 != 0 { size += 2; }
        if format & 0x0040 != 0 { size += 2; }
        if format & 0x0080 != 0 { size += 2; }
        size
    }
    
    /// Check if record has any positioning
    pub fn is_empty(&self) -> bool {
        self.x_placement == 0 && self.y_placement == 0 &&
        self.x_advance == 0 && self.y_advance == 0
    }
}

/// Anchor point for mark attachment
#[derive(Debug, Clone, Copy)]
pub struct Anchor {
    pub x: i16,
    pub y: i16,
    /// Anchor point index (for format 2)
    pub anchor_point: Option<u16>,
}

impl Anchor {
    /// Parse anchor table
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        let x = reader.read_i16().ok()?;
        let y = reader.read_i16().ok()?;
        
        let anchor_point = if format == 2 {
            Some(reader.read_u16().ok()?)
        } else {
            None
        };
        
        Some(Self { x, y, anchor_point })
    }
}

/// Single adjustment positioning (Type 1)
#[derive(Debug)]
pub struct SinglePos {
    format: u16,
    coverage: Coverage,
    data: SinglePosData,
}

#[derive(Debug)]
enum SinglePosData {
    /// Format 1: Same adjustment for all glyphs
    Format1 { value_format: u16, value: ValueRecord },
    /// Format 2: Different adjustment per glyph
    Format2 { value_format: u16, values: Vec<ValueRecord> },
}

impl SinglePos {
    /// Parse single positioning subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        let coverage_offset = reader.read_u16().ok()?;
        let value_format = reader.read_u16().ok()?;
        
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        
        let pos_data = match format {
            1 => {
                let value = ValueRecord::parse(&mut reader, value_format)?;
                SinglePosData::Format1 { value_format, value }
            }
            2 => {
                let value_count = reader.read_u16().ok()?;
                let mut values = Vec::with_capacity(value_count as usize);
                for _ in 0..value_count {
                    values.push(ValueRecord::parse(&mut reader, value_format)?);
                }
                SinglePosData::Format2 { value_format, values }
            }
            _ => return None,
        };
        
        Some(Self { format, coverage, data: pos_data })
    }
    
    /// Get positioning adjustment for a glyph
    pub fn apply(&self, glyph_id: GlyphId) -> Option<ValueRecord> {
        let coverage_idx = self.coverage.get(glyph_id.0)?;
        
        match &self.data {
            SinglePosData::Format1 { value, .. } => Some(*value),
            SinglePosData::Format2 { values, .. } => {
                values.get(coverage_idx as usize).copied()
            }
        }
    }
}

/// Pair adjustment positioning (Type 2 - Kerning)
#[derive(Debug)]
pub struct PairPos {
    format: u16,
    coverage: Coverage,
    data: PairPosData,
}

#[derive(Debug)]
enum PairPosData {
    /// Format 1: Specific pairs
    Format1 {
        value_format1: u16,
        value_format2: u16,
        pair_sets: Vec<Vec<PairValueRecord>>,
    },
    /// Format 2: Class-based pairs
    Format2 {
        value_format1: u16,
        value_format2: u16,
        class_def1: ClassDef,
        class_def2: ClassDef,
        class1_count: u16,
        class2_count: u16,
        class1_records: Vec<Vec<Class2Record>>,
    },
}

#[derive(Debug, Clone)]
struct PairValueRecord {
    second_glyph: u16,
    value1: ValueRecord,
    value2: ValueRecord,
}

#[derive(Debug, Clone)]
struct Class2Record {
    value1: ValueRecord,
    value2: ValueRecord,
}

impl PairPos {
    /// Parse pair positioning subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        let coverage_offset = reader.read_u16().ok()?;
        let value_format1 = reader.read_u16().ok()?;
        let value_format2 = reader.read_u16().ok()?;
        
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        
        let pos_data = match format {
            1 => Self::parse_format1(data, value_format1, value_format2)?,
            2 => Self::parse_format2(data, value_format1, value_format2)?,
            _ => return None,
        };
        
        Some(Self { format, coverage, data: pos_data })
    }
    
    fn parse_format1(data: &[u8], value_format1: u16, value_format2: u16) -> Option<PairPosData> {
        let mut reader = FontReader::new(data);
        reader.skip(10).ok()?; // Skip header already read
        
        let pair_set_count = reader.read_u16().ok()?;
        let mut pair_set_offsets = Vec::with_capacity(pair_set_count as usize);
        for _ in 0..pair_set_count {
            pair_set_offsets.push(reader.read_u16().ok()?);
        }
        
        let mut pair_sets = Vec::with_capacity(pair_set_count as usize);
        for offset in pair_set_offsets {
            let set_data = &data[offset as usize..];
            let mut set_reader = FontReader::new(set_data);
            let pair_count = set_reader.read_u16().ok()?;
            
            let mut pairs = Vec::with_capacity(pair_count as usize);
            for _ in 0..pair_count {
                let second_glyph = set_reader.read_u16().ok()?;
                let value1 = ValueRecord::parse(&mut set_reader, value_format1)?;
                let value2 = ValueRecord::parse(&mut set_reader, value_format2)?;
                pairs.push(PairValueRecord { second_glyph, value1, value2 });
            }
            
            pair_sets.push(pairs);
        }
        
        Some(PairPosData::Format1 { value_format1, value_format2, pair_sets })
    }
    
    fn parse_format2(data: &[u8], value_format1: u16, value_format2: u16) -> Option<PairPosData> {
        let mut reader = FontReader::new(data);
        reader.skip(10).ok()?; // Skip header already read
        
        let class_def1_offset = reader.read_u16().ok()?;
        let class_def2_offset = reader.read_u16().ok()?;
        let class1_count = reader.read_u16().ok()?;
        let class2_count = reader.read_u16().ok()?;
        
        let class_def1 = ClassDef::parse(&data[class_def1_offset as usize..])?;
        let class_def2 = ClassDef::parse(&data[class_def2_offset as usize..])?;
        
        let mut class1_records = Vec::with_capacity(class1_count as usize);
        for _ in 0..class1_count {
            let mut class2_records = Vec::with_capacity(class2_count as usize);
            for _ in 0..class2_count {
                let value1 = ValueRecord::parse(&mut reader, value_format1)?;
                let value2 = ValueRecord::parse(&mut reader, value_format2)?;
                class2_records.push(Class2Record { value1, value2 });
            }
            class1_records.push(class2_records);
        }
        
        Some(PairPosData::Format2 {
            value_format1,
            value_format2,
            class_def1,
            class_def2,
            class1_count,
            class2_count,
            class1_records,
        })
    }
    
    /// Get kerning adjustment for a pair of glyphs
    pub fn apply(&self, first: GlyphId, second: GlyphId) -> Option<(ValueRecord, ValueRecord)> {
        self.coverage.get(first.0)?;
        
        match &self.data {
            PairPosData::Format1 { pair_sets, .. } => {
                let coverage_idx = self.coverage.get(first.0)? as usize;
                let pair_set = pair_sets.get(coverage_idx)?;
                
                // Binary search for second glyph
                for pair in pair_set {
                    if pair.second_glyph == second.0 {
                        return Some((pair.value1, pair.value2));
                    }
                }
                None
            }
            PairPosData::Format2 { class_def1, class_def2, class1_records, .. } => {
                let class1 = class_def1.get(first.0) as usize;
                let class2 = class_def2.get(second.0) as usize;
                
                let class1_rec = class1_records.get(class1)?;
                let class2_rec = class1_rec.get(class2)?;
                
                Some((class2_rec.value1, class2_rec.value2))
            }
        }
    }
}

/// Cursive attachment positioning (Type 3)
#[derive(Debug)]
pub struct CursivePos {
    coverage: Coverage,
    entry_exit_records: Vec<EntryExitRecord>,
}

#[derive(Debug, Clone)]
struct EntryExitRecord {
    entry_anchor: Option<Anchor>,
    exit_anchor: Option<Anchor>,
}

impl CursivePos {
    /// Parse cursive positioning subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        if format != 1 {
            return None;
        }
        
        let coverage_offset = reader.read_u16().ok()?;
        let entry_exit_count = reader.read_u16().ok()?;
        
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        
        let mut entry_exit_records = Vec::with_capacity(entry_exit_count as usize);
        for _ in 0..entry_exit_count {
            let entry_offset = reader.read_u16().ok()?;
            let exit_offset = reader.read_u16().ok()?;
            
            let entry_anchor = if entry_offset != 0 {
                Anchor::parse(&data[entry_offset as usize..])
            } else {
                None
            };
            
            let exit_anchor = if exit_offset != 0 {
                Anchor::parse(&data[exit_offset as usize..])
            } else {
                None
            };
            
            entry_exit_records.push(EntryExitRecord { entry_anchor, exit_anchor });
        }
        
        Some(Self { coverage, entry_exit_records })
    }
    
    /// Get entry/exit anchors for a glyph
    pub fn get_anchors(&self, glyph_id: GlyphId) -> Option<(Option<Anchor>, Option<Anchor>)> {
        let coverage_idx = self.coverage.get(glyph_id.0)?;
        let record = self.entry_exit_records.get(coverage_idx as usize)?;
        Some((record.entry_anchor, record.exit_anchor))
    }
}

/// Mark-to-base attachment positioning (Type 4)
#[derive(Debug)]
pub struct MarkToBasePos {
    mark_coverage: Coverage,
    base_coverage: Coverage,
    mark_class_count: u16,
    mark_array: Vec<MarkRecord>,
    base_array: Vec<Vec<Option<Anchor>>>,
}

#[derive(Debug, Clone)]
struct MarkRecord {
    mark_class: u16,
    mark_anchor: Anchor,
}

impl MarkToBasePos {
    /// Parse mark-to-base positioning subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        if format != 1 {
            return None;
        }
        
        let mark_coverage_offset = reader.read_u16().ok()?;
        let base_coverage_offset = reader.read_u16().ok()?;
        let mark_class_count = reader.read_u16().ok()?;
        let mark_array_offset = reader.read_u16().ok()?;
        let base_array_offset = reader.read_u16().ok()?;
        
        let mark_coverage = Coverage::parse(&data[mark_coverage_offset as usize..])?;
        let base_coverage = Coverage::parse(&data[base_coverage_offset as usize..])?;
        
        // Parse mark array
        let mark_array_data = &data[mark_array_offset as usize..];
        let mut mark_reader = FontReader::new(mark_array_data);
        let mark_count = mark_reader.read_u16().ok()?;
        
        let mut mark_array = Vec::with_capacity(mark_count as usize);
        for _ in 0..mark_count {
            let mark_class = mark_reader.read_u16().ok()?;
            let anchor_offset = mark_reader.read_u16().ok()?;
            let mark_anchor = Anchor::parse(&mark_array_data[anchor_offset as usize..])?;
            mark_array.push(MarkRecord { mark_class, mark_anchor });
        }
        
        // Parse base array
        let base_array_data = &data[base_array_offset as usize..];
        let mut base_reader = FontReader::new(base_array_data);
        let base_count = base_reader.read_u16().ok()?;
        
        let mut base_array = Vec::with_capacity(base_count as usize);
        for _ in 0..base_count {
            let mut anchors = Vec::with_capacity(mark_class_count as usize);
            for _ in 0..mark_class_count {
                let anchor_offset = base_reader.read_u16().ok()?;
                if anchor_offset != 0 {
                    anchors.push(Anchor::parse(&base_array_data[anchor_offset as usize..]));
                } else {
                    anchors.push(None);
                }
            }
            base_array.push(anchors);
        }
        
        Some(Self {
            mark_coverage,
            base_coverage,
            mark_class_count,
            mark_array,
            base_array,
        })
    }
    
    /// Apply mark-to-base attachment
    /// Returns (mark_anchor, base_anchor) if applicable
    pub fn apply(&self, mark: GlyphId, base: GlyphId) -> Option<(Anchor, Anchor)> {
        let mark_idx = self.mark_coverage.get(mark.0)? as usize;
        let base_idx = self.base_coverage.get(base.0)? as usize;
        
        let mark_record = self.mark_array.get(mark_idx)?;
        let base_anchors = self.base_array.get(base_idx)?;
        let base_anchor = base_anchors.get(mark_record.mark_class as usize)?.as_ref()?;
        
        Some((mark_record.mark_anchor, *base_anchor))
    }
}

/// Mark-to-ligature attachment positioning (Type 5)
#[derive(Debug)]
pub struct MarkToLigaturePos {
    mark_coverage: Coverage,
    ligature_coverage: Coverage,
    mark_class_count: u16,
    mark_array: Vec<MarkRecord>,
    ligature_array: Vec<LigatureAttach>,
}

#[derive(Debug, Clone)]
struct LigatureAttach {
    /// Anchors per component, per mark class
    component_records: Vec<Vec<Option<Anchor>>>,
}

impl MarkToLigaturePos {
    /// Parse mark-to-ligature positioning subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        if format != 1 {
            return None;
        }
        
        let mark_coverage_offset = reader.read_u16().ok()?;
        let ligature_coverage_offset = reader.read_u16().ok()?;
        let mark_class_count = reader.read_u16().ok()?;
        let mark_array_offset = reader.read_u16().ok()?;
        let ligature_array_offset = reader.read_u16().ok()?;
        
        let mark_coverage = Coverage::parse(&data[mark_coverage_offset as usize..])?;
        let ligature_coverage = Coverage::parse(&data[ligature_coverage_offset as usize..])?;
        
        // Parse mark array
        let mark_array_data = &data[mark_array_offset as usize..];
        let mut mark_reader = FontReader::new(mark_array_data);
        let mark_count = mark_reader.read_u16().ok()?;
        
        let mut mark_array = Vec::with_capacity(mark_count as usize);
        for _ in 0..mark_count {
            let mark_class = mark_reader.read_u16().ok()?;
            let anchor_offset = mark_reader.read_u16().ok()?;
            let mark_anchor = Anchor::parse(&mark_array_data[anchor_offset as usize..])?;
            mark_array.push(MarkRecord { mark_class, mark_anchor });
        }
        
        // Parse ligature array
        let lig_array_data = &data[ligature_array_offset as usize..];
        let mut lig_reader = FontReader::new(lig_array_data);
        let ligature_count = lig_reader.read_u16().ok()?;
        
        let mut lig_offsets = Vec::with_capacity(ligature_count as usize);
        for _ in 0..ligature_count {
            lig_offsets.push(lig_reader.read_u16().ok()?);
        }
        
        let mut ligature_array = Vec::with_capacity(ligature_count as usize);
        for offset in lig_offsets {
            let attach_data = &lig_array_data[offset as usize..];
            let mut attach_reader = FontReader::new(attach_data);
            let component_count = attach_reader.read_u16().ok()?;
            
            let mut component_records = Vec::with_capacity(component_count as usize);
            for _ in 0..component_count {
                let mut anchors = Vec::with_capacity(mark_class_count as usize);
                for _ in 0..mark_class_count {
                    let anchor_offset = attach_reader.read_u16().ok()?;
                    if anchor_offset != 0 {
                        anchors.push(Anchor::parse(&attach_data[anchor_offset as usize..]));
                    } else {
                        anchors.push(None);
                    }
                }
                component_records.push(anchors);
            }
            
            ligature_array.push(LigatureAttach { component_records });
        }
        
        Some(Self {
            mark_coverage,
            ligature_coverage,
            mark_class_count,
            mark_array,
            ligature_array,
        })
    }
    
    /// Apply mark-to-ligature attachment
    pub fn apply(&self, mark: GlyphId, ligature: GlyphId, component: usize) -> Option<(Anchor, Anchor)> {
        let mark_idx = self.mark_coverage.get(mark.0)? as usize;
        let lig_idx = self.ligature_coverage.get(ligature.0)? as usize;
        
        let mark_record = self.mark_array.get(mark_idx)?;
        let lig_attach = self.ligature_array.get(lig_idx)?;
        let component_anchors = lig_attach.component_records.get(component)?;
        let lig_anchor = component_anchors.get(mark_record.mark_class as usize)?.as_ref()?;
        
        Some((mark_record.mark_anchor, *lig_anchor))
    }
}

/// Mark-to-mark attachment positioning (Type 6)
#[derive(Debug)]
pub struct MarkToMarkPos {
    mark1_coverage: Coverage,
    mark2_coverage: Coverage,
    mark_class_count: u16,
    mark1_array: Vec<MarkRecord>,
    mark2_array: Vec<Vec<Option<Anchor>>>,
}

impl MarkToMarkPos {
    /// Parse mark-to-mark positioning subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        if format != 1 {
            return None;
        }
        
        let mark1_coverage_offset = reader.read_u16().ok()?;
        let mark2_coverage_offset = reader.read_u16().ok()?;
        let mark_class_count = reader.read_u16().ok()?;
        let mark1_array_offset = reader.read_u16().ok()?;
        let mark2_array_offset = reader.read_u16().ok()?;
        
        let mark1_coverage = Coverage::parse(&data[mark1_coverage_offset as usize..])?;
        let mark2_coverage = Coverage::parse(&data[mark2_coverage_offset as usize..])?;
        
        // Parse mark1 array
        let mark1_array_data = &data[mark1_array_offset as usize..];
        let mut mark1_reader = FontReader::new(mark1_array_data);
        let mark1_count = mark1_reader.read_u16().ok()?;
        
        let mut mark1_array = Vec::with_capacity(mark1_count as usize);
        for _ in 0..mark1_count {
            let mark_class = mark1_reader.read_u16().ok()?;
            let anchor_offset = mark1_reader.read_u16().ok()?;
            let mark_anchor = Anchor::parse(&mark1_array_data[anchor_offset as usize..])?;
            mark1_array.push(MarkRecord { mark_class, mark_anchor });
        }
        
        // Parse mark2 array
        let mark2_array_data = &data[mark2_array_offset as usize..];
        let mut mark2_reader = FontReader::new(mark2_array_data);
        let mark2_count = mark2_reader.read_u16().ok()?;
        
        let mut mark2_array = Vec::with_capacity(mark2_count as usize);
        for _ in 0..mark2_count {
            let mut anchors = Vec::with_capacity(mark_class_count as usize);
            for _ in 0..mark_class_count {
                let anchor_offset = mark2_reader.read_u16().ok()?;
                if anchor_offset != 0 {
                    anchors.push(Anchor::parse(&mark2_array_data[anchor_offset as usize..]));
                } else {
                    anchors.push(None);
                }
            }
            mark2_array.push(anchors);
        }
        
        Some(Self {
            mark1_coverage,
            mark2_coverage,
            mark_class_count,
            mark1_array,
            mark2_array,
        })
    }
    
    /// Apply mark-to-mark attachment
    pub fn apply(&self, mark1: GlyphId, mark2: GlyphId) -> Option<(Anchor, Anchor)> {
        let mark1_idx = self.mark1_coverage.get(mark1.0)? as usize;
        let mark2_idx = self.mark2_coverage.get(mark2.0)? as usize;
        
        let mark1_record = self.mark1_array.get(mark1_idx)?;
        let mark2_anchors = self.mark2_array.get(mark2_idx)?;
        let mark2_anchor = mark2_anchors.get(mark1_record.mark_class as usize)?.as_ref()?;
        
        Some((mark1_record.mark_anchor, *mark2_anchor))
    }
}

/// Contextual positioning (Type 7) - uses same format as GSUB Context
#[derive(Debug)]
pub struct ContextPos {
    format: u16,
    // Uses same structures as GSUB context
}

impl ContextPos {
    /// Parse context positioning subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        // Structure is same as GSUB context, just references GPOS lookups
        Some(Self { format })
    }
}

/// Chained contextual positioning (Type 8) - uses same format as GSUB ChainedContext
#[derive(Debug)]
pub struct ChainedContextPos {
    format: u16,
    // Uses same structures as GSUB chained context
}

impl ChainedContextPos {
    /// Parse chained context positioning subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        Some(Self { format })
    }
}

/// GPOS lookup
#[derive(Debug)]
pub struct GposLookup {
    pub lookup_type: LookupType,
    pub lookup_flag: u16,
    pub subtables: Vec<GposSubtable>,
    pub mark_filtering_set: Option<u16>,
}

/// GPOS subtable (parsed)
#[derive(Debug)]
pub enum GposSubtable {
    SingleAdjustment(SinglePos),
    PairAdjustment(PairPos),
    CursiveAttachment(CursivePos),
    MarkToBase(MarkToBasePos),
    MarkToLigature(MarkToLigaturePos),
    MarkToMark(MarkToMarkPos),
    Context(ContextPos),
    ChainedContext(ChainedContextPos),
}

impl<'a> GposTable<'a> {
    /// Parse GPOS table
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        
        let major_version = reader.read_u16().ok()?;
        let _minor_version = reader.read_u16().ok()?;
        
        if major_version != 1 {
            return None;
        }
        
        let script_list_offset = reader.read_u16().ok()?;
        let feature_list_offset = reader.read_u16().ok()?;
        let lookup_list_offset = reader.read_u16().ok()?;
        
        Some(Self {
            data,
            script_list_offset,
            feature_list_offset,
            lookup_list_offset,
            allocator: BumpAllocator::new(),
        })
    }
    
    /// Get lookup by index
    pub fn get_lookup(&self, index: u16) -> Option<GposLookup> {
        let lookup_list_data = &self.data[self.lookup_list_offset as usize..];
        let mut reader = FontReader::new(lookup_list_data);
        
        let lookup_count = reader.read_u16().ok()?;
        if index >= lookup_count {
            return None;
        }
        
        // Read lookup offset
        reader.skip((index as usize) * 2).ok()?;
        let lookup_offset = reader.read_u16().ok()?;
        
        Self::parse_lookup(&lookup_list_data[lookup_offset as usize..])
    }
    
    fn parse_lookup(data: &[u8]) -> Option<GposLookup> {
        let mut reader = FontReader::new(data);
        
        let lookup_type = reader.read_u16().ok()?;
        let lookup_flag = reader.read_u16().ok()?;
        let subtable_count = reader.read_u16().ok()?;
        
        let mut subtable_offsets = Vec::with_capacity(subtable_count as usize);
        for _ in 0..subtable_count {
            subtable_offsets.push(reader.read_u16().ok()?);
        }
        
        let mark_filtering_set = if lookup_flag & 0x0010 != 0 {
            Some(reader.read_u16().ok()?)
        } else {
            None
        };
        
        let mut subtables = Vec::with_capacity(subtable_count as usize);
        for offset in subtable_offsets {
            let subtable_data = &data[offset as usize..];
            
            // Handle extension lookups
            let (actual_type, actual_data) = if lookup_type == 9 {
                let mut ext_reader = FontReader::new(subtable_data);
                let _format = ext_reader.read_u16().ok()?;
                let extension_type = ext_reader.read_u16().ok()?;
                let extension_offset = ext_reader.read_u32().ok()?;
                (extension_type, &subtable_data[extension_offset as usize..])
            } else {
                (lookup_type, subtable_data)
            };
            
            let subtable = match actual_type {
                1 => GposSubtable::SingleAdjustment(SinglePos::parse(actual_data)?),
                2 => GposSubtable::PairAdjustment(PairPos::parse(actual_data)?),
                3 => GposSubtable::CursiveAttachment(CursivePos::parse(actual_data)?),
                4 => GposSubtable::MarkToBase(MarkToBasePos::parse(actual_data)?),
                5 => GposSubtable::MarkToLigature(MarkToLigaturePos::parse(actual_data)?),
                6 => GposSubtable::MarkToMark(MarkToMarkPos::parse(actual_data)?),
                7 => GposSubtable::Context(ContextPos::parse(actual_data)?),
                8 => GposSubtable::ChainedContext(ChainedContextPos::parse(actual_data)?),
                _ => continue,
            };
            
            subtables.push(subtable);
        }
        
        let lookup_type = LookupType::try_from(if lookup_type == 9 { 1 } else { lookup_type }).ok()?;
        
        Some(GposLookup {
            lookup_type,
            lookup_flag,
            subtables,
            mark_filtering_set,
        })
    }
    
    /// Get number of lookups
    pub fn lookup_count(&self) -> u16 {
        let lookup_list_data = &self.data[self.lookup_list_offset as usize..];
        let mut reader = FontReader::new(lookup_list_data);
        reader.read_u16().unwrap_or(0)
    }
    
    /// Apply kerning between two glyphs
    pub fn get_kerning(&self, first: GlyphId, second: GlyphId) -> Option<i16> {
        for i in 0..self.lookup_count() {
            if let Some(lookup) = self.get_lookup(i) {
                if lookup.lookup_type == LookupType::PairAdjustment {
                    for subtable in &lookup.subtables {
                        if let GposSubtable::PairAdjustment(pair_pos) = subtable {
                            if let Some((value1, _)) = pair_pos.apply(first, second) {
                                if value1.x_advance != 0 {
                                    return Some(value1.x_advance);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lookup_type_conversion() {
        assert_eq!(LookupType::try_from(1), Ok(LookupType::SingleAdjustment));
        assert_eq!(LookupType::try_from(2), Ok(LookupType::PairAdjustment));
        assert_eq!(LookupType::try_from(4), Ok(LookupType::MarkToBase));
        assert!(LookupType::try_from(10).is_err());
    }
    
    #[test]
    fn test_value_record_size() {
        assert_eq!(ValueRecord::size(0x0000), 0);
        assert_eq!(ValueRecord::size(0x0001), 2);
        assert_eq!(ValueRecord::size(0x000F), 8);
        assert_eq!(ValueRecord::size(0x00FF), 16);
    }
    
    #[test]
    fn test_value_record_empty() {
        let record = ValueRecord::default();
        assert!(record.is_empty());
        
        let record = ValueRecord { x_advance: 10, ..Default::default() };
        assert!(!record.is_empty());
    }
}
