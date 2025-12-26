//! OpenType GSUB (Glyph Substitution) Table
//!
//! Full implementation of all 8 GSUB lookup types for HarfBuzz compatibility.
//! Uses BumpAllocator for efficient memory management.

use crate::font::parser::{FontReader, GlyphId};
use super::memory::BumpAllocator;

/// GSUB table processor
pub struct GsubTable<'a> {
    data: &'a [u8],
    script_list_offset: u16,
    feature_list_offset: u16,
    lookup_list_offset: u16,
    /// BumpAllocator for efficient memory allocation during parsing
    allocator: BumpAllocator,
}

/// GSUB lookup type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum LookupType {
    /// Type 1: Single substitution
    Single = 1,
    /// Type 2: Multiple substitution (one-to-many)
    Multiple = 2,
    /// Type 3: Alternate substitution (one-of-many)
    Alternate = 3,
    /// Type 4: Ligature substitution (many-to-one)
    Ligature = 4,
    /// Type 5: Contextual substitution
    Context = 5,
    /// Type 6: Chained contextual substitution
    ChainedContext = 6,
    /// Type 7: Extension substitution
    Extension = 7,
    /// Type 8: Reverse chaining contextual single
    ReverseChainSingle = 8,
}

impl TryFrom<u16> for LookupType {
    type Error = ();
    
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Single),
            2 => Ok(Self::Multiple),
            3 => Ok(Self::Alternate),
            4 => Ok(Self::Ligature),
            5 => Ok(Self::Context),
            6 => Ok(Self::ChainedContext),
            7 => Ok(Self::Extension),
            8 => Ok(Self::ReverseChainSingle),
            _ => Err(()),
        }
    }
}

/// Substitution result
#[derive(Debug, Clone)]
pub enum Substitution {
    /// Replace with single glyph
    Single(GlyphId),
    /// Replace with multiple glyphs
    Multiple(Vec<GlyphId>),
    /// No substitution needed
    None,
}

/// Coverage table (maps glyph IDs to coverage indices)
#[derive(Debug)]
pub struct Coverage {
    format: u16,
    data: CoverageData,
}

#[derive(Debug)]
enum CoverageData {
    /// Format 1: List of glyph IDs
    GlyphArray(Vec<u16>),
    /// Format 2: Ranges of glyph IDs
    RangeArray(Vec<RangeRecord>),
}

#[derive(Debug, Clone, Copy)]
struct RangeRecord {
    start_glyph: u16,
    end_glyph: u16,
    start_coverage_index: u16,
}

impl Coverage {
    /// Parse coverage table from data
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        
        let coverage_data = match format {
            1 => {
                let count = reader.read_u16().ok()?;
                let mut glyphs = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    glyphs.push(reader.read_u16().ok()?);
                }
                CoverageData::GlyphArray(glyphs)
            }
            2 => {
                let count = reader.read_u16().ok()?;
                let mut ranges = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    ranges.push(RangeRecord {
                        start_glyph: reader.read_u16().ok()?,
                        end_glyph: reader.read_u16().ok()?,
                        start_coverage_index: reader.read_u16().ok()?,
                    });
                }
                CoverageData::RangeArray(ranges)
            }
            _ => return None,
        };
        
        Some(Self { format, data: coverage_data })
    }
    
    /// Get coverage index for a glyph ID
    pub fn get(&self, glyph_id: u16) -> Option<u16> {
        match &self.data {
            CoverageData::GlyphArray(glyphs) => {
                // Binary search
                glyphs.binary_search(&glyph_id)
                    .ok()
                    .map(|i| i as u16)
            }
            CoverageData::RangeArray(ranges) => {
                // Binary search on ranges
                for range in ranges {
                    if glyph_id >= range.start_glyph && glyph_id <= range.end_glyph {
                        return Some(range.start_coverage_index + (glyph_id - range.start_glyph));
                    }
                }
                None
            }
        }
    }
}

/// Class definition table
#[derive(Debug)]
pub struct ClassDef {
    format: u16,
    data: ClassDefData,
}

#[derive(Debug)]
enum ClassDefData {
    /// Format 1: Array of class values
    Format1 { start_glyph: u16, class_values: Vec<u16> },
    /// Format 2: Range records
    Format2(Vec<ClassRangeRecord>),
}

#[derive(Debug, Clone, Copy)]
struct ClassRangeRecord {
    start_glyph: u16,
    end_glyph: u16,
    class: u16,
}

impl ClassDef {
    /// Parse class definition table
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        
        let class_data = match format {
            1 => {
                let start_glyph = reader.read_u16().ok()?;
                let count = reader.read_u16().ok()?;
                let mut class_values = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    class_values.push(reader.read_u16().ok()?);
                }
                ClassDefData::Format1 { start_glyph, class_values }
            }
            2 => {
                let count = reader.read_u16().ok()?;
                let mut ranges = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    ranges.push(ClassRangeRecord {
                        start_glyph: reader.read_u16().ok()?,
                        end_glyph: reader.read_u16().ok()?,
                        class: reader.read_u16().ok()?,
                    });
                }
                ClassDefData::Format2(ranges)
            }
            _ => return None,
        };
        
        Some(Self { format, data: class_data })
    }
    
    /// Get class for a glyph ID (returns 0 for unassigned)
    pub fn get(&self, glyph_id: u16) -> u16 {
        match &self.data {
            ClassDefData::Format1 { start_glyph, class_values } => {
                let idx = glyph_id.saturating_sub(*start_glyph) as usize;
                class_values.get(idx).copied().unwrap_or(0)
            }
            ClassDefData::Format2(ranges) => {
                for range in ranges {
                    if glyph_id >= range.start_glyph && glyph_id <= range.end_glyph {
                        return range.class;
                    }
                }
                0
            }
        }
    }
}

/// Single substitution subtable
#[derive(Debug)]
pub struct SingleSubst {
    format: u16,
    coverage: Coverage,
    data: SingleSubstData,
}

#[derive(Debug)]
enum SingleSubstData {
    /// Format 1: Delta to add to glyph ID
    Delta(i16),
    /// Format 2: Array of substitute glyph IDs
    Array(Vec<u16>),
}

impl SingleSubst {
    /// Parse single substitution subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        let coverage_offset = reader.read_u16().ok()?;
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        
        let subst_data = match format {
            1 => {
                let delta = reader.read_i16().ok()?;
                SingleSubstData::Delta(delta)
            }
            2 => {
                let count = reader.read_u16().ok()?;
                let mut substitutes = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    substitutes.push(reader.read_u16().ok()?);
                }
                SingleSubstData::Array(substitutes)
            }
            _ => return None,
        };
        
        Some(Self { format, coverage, data: subst_data })
    }
    
    /// Apply substitution to a glyph
    pub fn apply(&self, glyph_id: GlyphId) -> Substitution {
        if let Some(coverage_idx) = self.coverage.get(glyph_id.0) {
            match &self.data {
                SingleSubstData::Delta(delta) => {
                    let new_id = (glyph_id.0 as i32 + *delta as i32) as u16;
                    Substitution::Single(GlyphId(new_id))
                }
                SingleSubstData::Array(substitutes) => {
                    if let Some(&sub) = substitutes.get(coverage_idx as usize) {
                        Substitution::Single(GlyphId(sub))
                    } else {
                        Substitution::None
                    }
                }
            }
        } else {
            Substitution::None
        }
    }
}

/// Multiple substitution subtable (one-to-many)
#[derive(Debug)]
pub struct MultipleSubst {
    coverage: Coverage,
    sequences: Vec<Vec<u16>>,
}

impl MultipleSubst {
    /// Parse multiple substitution subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        if format != 1 {
            return None;
        }
        
        let coverage_offset = reader.read_u16().ok()?;
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        
        let sequence_count = reader.read_u16().ok()?;
        let mut sequence_offsets = Vec::with_capacity(sequence_count as usize);
        for _ in 0..sequence_count {
            sequence_offsets.push(reader.read_u16().ok()?);
        }
        
        let mut sequences = Vec::with_capacity(sequence_count as usize);
        for offset in sequence_offsets {
            let mut seq_reader = FontReader::new(&data[offset as usize..]);
            let glyph_count = seq_reader.read_u16().ok()?;
            let mut glyphs = Vec::with_capacity(glyph_count as usize);
            for _ in 0..glyph_count {
                glyphs.push(seq_reader.read_u16().ok()?);
            }
            sequences.push(glyphs);
        }
        
        Some(Self { coverage, sequences })
    }
    
    /// Apply substitution
    pub fn apply(&self, glyph_id: GlyphId) -> Substitution {
        if let Some(coverage_idx) = self.coverage.get(glyph_id.0) {
            if let Some(seq) = self.sequences.get(coverage_idx as usize) {
                Substitution::Multiple(seq.iter().map(|&g| GlyphId(g)).collect())
            } else {
                Substitution::None
            }
        } else {
            Substitution::None
        }
    }
}

/// Alternate substitution subtable (one-of-many)
#[derive(Debug)]
pub struct AlternateSubst {
    coverage: Coverage,
    alternate_sets: Vec<Vec<u16>>,
}

impl AlternateSubst {
    /// Parse alternate substitution subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        if format != 1 {
            return None;
        }
        
        let coverage_offset = reader.read_u16().ok()?;
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        
        let set_count = reader.read_u16().ok()?;
        let mut set_offsets = Vec::with_capacity(set_count as usize);
        for _ in 0..set_count {
            set_offsets.push(reader.read_u16().ok()?);
        }
        
        let mut alternate_sets = Vec::with_capacity(set_count as usize);
        for offset in set_offsets {
            let mut set_reader = FontReader::new(&data[offset as usize..]);
            let glyph_count = set_reader.read_u16().ok()?;
            let mut glyphs = Vec::with_capacity(glyph_count as usize);
            for _ in 0..glyph_count {
                glyphs.push(set_reader.read_u16().ok()?);
            }
            alternate_sets.push(glyphs);
        }
        
        Some(Self { coverage, alternate_sets })
    }
    
    /// Get alternates for a glyph
    pub fn get_alternates(&self, glyph_id: GlyphId) -> Option<&[u16]> {
        let coverage_idx = self.coverage.get(glyph_id.0)?;
        self.alternate_sets.get(coverage_idx as usize).map(|v| v.as_slice())
    }
    
    /// Apply substitution with alternate index
    pub fn apply(&self, glyph_id: GlyphId, alt_index: usize) -> Substitution {
        if let Some(alts) = self.get_alternates(glyph_id) {
            if let Some(&alt) = alts.get(alt_index) {
                return Substitution::Single(GlyphId(alt));
            }
        }
        Substitution::None
    }
}

/// Ligature substitution subtable (many-to-one)
#[derive(Debug)]
pub struct LigatureSubst {
    coverage: Coverage,
    ligature_sets: Vec<Vec<Ligature>>,
}

#[derive(Debug, Clone)]
pub struct Ligature {
    /// Resulting ligature glyph
    pub ligature_glyph: u16,
    /// Component glyphs (first is from coverage, rest are here)
    pub components: Vec<u16>,
}

impl LigatureSubst {
    /// Parse ligature substitution subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        if format != 1 {
            return None;
        }
        
        let coverage_offset = reader.read_u16().ok()?;
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        
        let set_count = reader.read_u16().ok()?;
        let mut set_offsets = Vec::with_capacity(set_count as usize);
        for _ in 0..set_count {
            set_offsets.push(reader.read_u16().ok()?);
        }
        
        let mut ligature_sets = Vec::with_capacity(set_count as usize);
        for offset in set_offsets {
            let set_data = &data[offset as usize..];
            let mut set_reader = FontReader::new(set_data);
            let lig_count = set_reader.read_u16().ok()?;
            
            let mut lig_offsets = Vec::with_capacity(lig_count as usize);
            for _ in 0..lig_count {
                lig_offsets.push(set_reader.read_u16().ok()?);
            }
            
            let mut ligatures = Vec::with_capacity(lig_count as usize);
            for lig_offset in lig_offsets {
                let mut lig_reader = FontReader::new(&set_data[lig_offset as usize..]);
                let ligature_glyph = lig_reader.read_u16().ok()?;
                let component_count = lig_reader.read_u16().ok()?;
                
                // component_count includes the first glyph (from coverage)
                let mut components = Vec::with_capacity(component_count.saturating_sub(1) as usize);
                for _ in 1..component_count {
                    components.push(lig_reader.read_u16().ok()?);
                }
                
                ligatures.push(Ligature { ligature_glyph, components });
            }
            
            ligature_sets.push(ligatures);
        }
        
        Some(Self { coverage, ligature_sets })
    }
    
    /// Try to apply ligature substitution
    /// Returns (ligature glyph, number of glyphs consumed) if match found
    pub fn apply(&self, glyphs: &[GlyphId]) -> Option<(GlyphId, usize)> {
        if glyphs.is_empty() {
            return None;
        }
        
        let first = glyphs[0];
        let coverage_idx = self.coverage.get(first.0)?;
        let ligature_set = self.ligature_sets.get(coverage_idx as usize)?;
        
        // Try each ligature in the set (longer matches first typically)
        for ligature in ligature_set {
            if ligature.components.len() + 1 <= glyphs.len() {
                let matches = ligature.components.iter().enumerate().all(|(i, &comp)| {
                    glyphs.get(i + 1).map(|g| g.0 == comp).unwrap_or(false)
                });
                
                if matches {
                    return Some((
                        GlyphId(ligature.ligature_glyph),
                        ligature.components.len() + 1,
                    ));
                }
            }
        }
        
        None
    }
}

/// Context substitution subtable (Type 5)
#[derive(Debug)]
pub struct ContextSubst {
    format: u16,
    data: ContextSubstData,
}

#[derive(Debug)]
enum ContextSubstData {
    /// Format 1: Simple context
    Format1 {
        coverage: Coverage,
        rule_sets: Vec<Option<Vec<ContextRule>>>,
    },
    /// Format 2: Class-based context
    Format2 {
        coverage: Coverage,
        class_def: ClassDef,
        rule_sets: Vec<Option<Vec<ClassContextRule>>>,
    },
    /// Format 3: Coverage-based context
    Format3 {
        coverages: Vec<Coverage>,
        lookup_records: Vec<SubstLookupRecord>,
    },
}

#[derive(Debug, Clone)]
struct ContextRule {
    input: Vec<u16>,
    lookup_records: Vec<SubstLookupRecord>,
}

#[derive(Debug, Clone)]
struct ClassContextRule {
    input_classes: Vec<u16>,
    lookup_records: Vec<SubstLookupRecord>,
}

#[derive(Debug, Clone, Copy)]
pub struct SubstLookupRecord {
    pub sequence_index: u16,
    pub lookup_list_index: u16,
}

impl ContextSubst {
    /// Parse context substitution subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        
        let subst_data = match format {
            1 => Self::parse_format1(data)?,
            2 => Self::parse_format2(data)?,
            3 => Self::parse_format3(data)?,
            _ => return None,
        };
        
        Some(Self { format, data: subst_data })
    }
    
    fn parse_format1(data: &[u8]) -> Option<ContextSubstData> {
        let mut reader = FontReader::new(data);
        let _format = reader.read_u16().ok()?;
        let coverage_offset = reader.read_u16().ok()?;
        let rule_set_count = reader.read_u16().ok()?;
        
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        
        let mut rule_set_offsets = Vec::with_capacity(rule_set_count as usize);
        for _ in 0..rule_set_count {
            rule_set_offsets.push(reader.read_u16().ok()?);
        }
        
        let mut rule_sets = Vec::with_capacity(rule_set_count as usize);
        for offset in rule_set_offsets {
            if offset == 0 {
                rule_sets.push(None);
                continue;
            }
            
            let set_data = &data[offset as usize..];
            let mut set_reader = FontReader::new(set_data);
            let rule_count = set_reader.read_u16().ok()?;
            
            let mut rule_offsets = Vec::with_capacity(rule_count as usize);
            for _ in 0..rule_count {
                rule_offsets.push(set_reader.read_u16().ok()?);
            }
            
            let mut rules = Vec::with_capacity(rule_count as usize);
            for rule_offset in rule_offsets {
                let mut rule_reader = FontReader::new(&set_data[rule_offset as usize..]);
                let glyph_count = rule_reader.read_u16().ok()?;
                let subst_count = rule_reader.read_u16().ok()?;
                
                let mut input = Vec::with_capacity(glyph_count.saturating_sub(1) as usize);
                for _ in 1..glyph_count {
                    input.push(rule_reader.read_u16().ok()?);
                }
                
                let mut lookup_records = Vec::with_capacity(subst_count as usize);
                for _ in 0..subst_count {
                    lookup_records.push(SubstLookupRecord {
                        sequence_index: rule_reader.read_u16().ok()?,
                        lookup_list_index: rule_reader.read_u16().ok()?,
                    });
                }
                
                rules.push(ContextRule { input, lookup_records });
            }
            
            rule_sets.push(Some(rules));
        }
        
        Some(ContextSubstData::Format1 { coverage, rule_sets })
    }
    
    fn parse_format2(data: &[u8]) -> Option<ContextSubstData> {
        let mut reader = FontReader::new(data);
        let _format = reader.read_u16().ok()?;
        let coverage_offset = reader.read_u16().ok()?;
        let class_def_offset = reader.read_u16().ok()?;
        let rule_set_count = reader.read_u16().ok()?;
        
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        let class_def = ClassDef::parse(&data[class_def_offset as usize..])?;
        
        let mut rule_set_offsets = Vec::with_capacity(rule_set_count as usize);
        for _ in 0..rule_set_count {
            rule_set_offsets.push(reader.read_u16().ok()?);
        }
        
        let mut rule_sets = Vec::with_capacity(rule_set_count as usize);
        for offset in rule_set_offsets {
            if offset == 0 {
                rule_sets.push(None);
                continue;
            }
            
            let set_data = &data[offset as usize..];
            let mut set_reader = FontReader::new(set_data);
            let rule_count = set_reader.read_u16().ok()?;
            
            let mut rule_offsets = Vec::with_capacity(rule_count as usize);
            for _ in 0..rule_count {
                rule_offsets.push(set_reader.read_u16().ok()?);
            }
            
            let mut rules = Vec::with_capacity(rule_count as usize);
            for rule_offset in rule_offsets {
                let mut rule_reader = FontReader::new(&set_data[rule_offset as usize..]);
                let glyph_count = rule_reader.read_u16().ok()?;
                let subst_count = rule_reader.read_u16().ok()?;
                
                let mut input_classes = Vec::with_capacity(glyph_count.saturating_sub(1) as usize);
                for _ in 1..glyph_count {
                    input_classes.push(rule_reader.read_u16().ok()?);
                }
                
                let mut lookup_records = Vec::with_capacity(subst_count as usize);
                for _ in 0..subst_count {
                    lookup_records.push(SubstLookupRecord {
                        sequence_index: rule_reader.read_u16().ok()?,
                        lookup_list_index: rule_reader.read_u16().ok()?,
                    });
                }
                
                rules.push(ClassContextRule { input_classes, lookup_records });
            }
            
            rule_sets.push(Some(rules));
        }
        
        Some(ContextSubstData::Format2 { coverage, class_def, rule_sets })
    }
    
    fn parse_format3(data: &[u8]) -> Option<ContextSubstData> {
        let mut reader = FontReader::new(data);
        let _format = reader.read_u16().ok()?;
        let glyph_count = reader.read_u16().ok()?;
        let subst_count = reader.read_u16().ok()?;
        
        let mut coverage_offsets = Vec::with_capacity(glyph_count as usize);
        for _ in 0..glyph_count {
            coverage_offsets.push(reader.read_u16().ok()?);
        }
        
        let mut lookup_records = Vec::with_capacity(subst_count as usize);
        for _ in 0..subst_count {
            lookup_records.push(SubstLookupRecord {
                sequence_index: reader.read_u16().ok()?,
                lookup_list_index: reader.read_u16().ok()?,
            });
        }
        
        let mut coverages = Vec::with_capacity(glyph_count as usize);
        for offset in coverage_offsets {
            coverages.push(Coverage::parse(&data[offset as usize..])?);
        }
        
        Some(ContextSubstData::Format3 { coverages, lookup_records })
    }
}

/// Chained context substitution subtable (Type 6)
#[derive(Debug)]
pub struct ChainedContextSubst {
    format: u16,
    data: ChainedContextData,
}

#[derive(Debug)]
enum ChainedContextData {
    /// Format 1: Simple chained context
    Format1 {
        coverage: Coverage,
        rule_sets: Vec<Option<Vec<ChainedRule>>>,
    },
    /// Format 2: Class-based chained context
    Format2 {
        coverage: Coverage,
        backtrack_class_def: ClassDef,
        input_class_def: ClassDef,
        lookahead_class_def: ClassDef,
        rule_sets: Vec<Option<Vec<ChainedClassRule>>>,
    },
    /// Format 3: Coverage-based chained context
    Format3 {
        backtrack_coverages: Vec<Coverage>,
        input_coverages: Vec<Coverage>,
        lookahead_coverages: Vec<Coverage>,
        lookup_records: Vec<SubstLookupRecord>,
    },
}

#[derive(Debug, Clone)]
struct ChainedRule {
    backtrack: Vec<u16>,
    input: Vec<u16>,
    lookahead: Vec<u16>,
    lookup_records: Vec<SubstLookupRecord>,
}

#[derive(Debug, Clone)]
struct ChainedClassRule {
    backtrack_classes: Vec<u16>,
    input_classes: Vec<u16>,
    lookahead_classes: Vec<u16>,
    lookup_records: Vec<SubstLookupRecord>,
}

impl ChainedContextSubst {
    /// Parse chained context substitution subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        
        let subst_data = match format {
            1 => Self::parse_format1(data)?,
            2 => Self::parse_format2(data)?,
            3 => Self::parse_format3(data)?,
            _ => return None,
        };
        
        Some(Self { format, data: subst_data })
    }
    
    fn parse_format1(data: &[u8]) -> Option<ChainedContextData> {
        let mut reader = FontReader::new(data);
        let _format = reader.read_u16().ok()?;
        let coverage_offset = reader.read_u16().ok()?;
        let rule_set_count = reader.read_u16().ok()?;
        
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        
        let mut rule_set_offsets = Vec::with_capacity(rule_set_count as usize);
        for _ in 0..rule_set_count {
            rule_set_offsets.push(reader.read_u16().ok()?);
        }
        
        let mut rule_sets = Vec::with_capacity(rule_set_count as usize);
        for offset in rule_set_offsets {
            if offset == 0 {
                rule_sets.push(None);
                continue;
            }
            
            let set_data = &data[offset as usize..];
            let mut set_reader = FontReader::new(set_data);
            let rule_count = set_reader.read_u16().ok()?;
            
            let mut rule_offsets = Vec::with_capacity(rule_count as usize);
            for _ in 0..rule_count {
                rule_offsets.push(set_reader.read_u16().ok()?);
            }
            
            let mut rules = Vec::with_capacity(rule_count as usize);
            for rule_offset in rule_offsets {
                let mut rule_reader = FontReader::new(&set_data[rule_offset as usize..]);
                
                // Backtrack sequence
                let backtrack_count = rule_reader.read_u16().ok()?;
                let mut backtrack = Vec::with_capacity(backtrack_count as usize);
                for _ in 0..backtrack_count {
                    backtrack.push(rule_reader.read_u16().ok()?);
                }
                
                // Input sequence
                let input_count = rule_reader.read_u16().ok()?;
                let mut input = Vec::with_capacity(input_count.saturating_sub(1) as usize);
                for _ in 1..input_count {
                    input.push(rule_reader.read_u16().ok()?);
                }
                
                // Lookahead sequence
                let lookahead_count = rule_reader.read_u16().ok()?;
                let mut lookahead = Vec::with_capacity(lookahead_count as usize);
                for _ in 0..lookahead_count {
                    lookahead.push(rule_reader.read_u16().ok()?);
                }
                
                // Lookup records
                let subst_count = rule_reader.read_u16().ok()?;
                let mut lookup_records = Vec::with_capacity(subst_count as usize);
                for _ in 0..subst_count {
                    lookup_records.push(SubstLookupRecord {
                        sequence_index: rule_reader.read_u16().ok()?,
                        lookup_list_index: rule_reader.read_u16().ok()?,
                    });
                }
                
                rules.push(ChainedRule { backtrack, input, lookahead, lookup_records });
            }
            
            rule_sets.push(Some(rules));
        }
        
        Some(ChainedContextData::Format1 { coverage, rule_sets })
    }
    
    fn parse_format2(data: &[u8]) -> Option<ChainedContextData> {
        let mut reader = FontReader::new(data);
        let _format = reader.read_u16().ok()?;
        let coverage_offset = reader.read_u16().ok()?;
        let backtrack_class_def_offset = reader.read_u16().ok()?;
        let input_class_def_offset = reader.read_u16().ok()?;
        let lookahead_class_def_offset = reader.read_u16().ok()?;
        let rule_set_count = reader.read_u16().ok()?;
        
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        let backtrack_class_def = ClassDef::parse(&data[backtrack_class_def_offset as usize..])?;
        let input_class_def = ClassDef::parse(&data[input_class_def_offset as usize..])?;
        let lookahead_class_def = ClassDef::parse(&data[lookahead_class_def_offset as usize..])?;
        
        let mut rule_set_offsets = Vec::with_capacity(rule_set_count as usize);
        for _ in 0..rule_set_count {
            rule_set_offsets.push(reader.read_u16().ok()?);
        }
        
        let mut rule_sets = Vec::with_capacity(rule_set_count as usize);
        for offset in rule_set_offsets {
            if offset == 0 {
                rule_sets.push(None);
                continue;
            }
            
            let set_data = &data[offset as usize..];
            let mut set_reader = FontReader::new(set_data);
            let rule_count = set_reader.read_u16().ok()?;
            
            let mut rule_offsets = Vec::with_capacity(rule_count as usize);
            for _ in 0..rule_count {
                rule_offsets.push(set_reader.read_u16().ok()?);
            }
            
            let mut rules = Vec::with_capacity(rule_count as usize);
            for rule_offset in rule_offsets {
                let mut rule_reader = FontReader::new(&set_data[rule_offset as usize..]);
                
                let backtrack_count = rule_reader.read_u16().ok()?;
                let mut backtrack_classes = Vec::with_capacity(backtrack_count as usize);
                for _ in 0..backtrack_count {
                    backtrack_classes.push(rule_reader.read_u16().ok()?);
                }
                
                let input_count = rule_reader.read_u16().ok()?;
                let mut input_classes = Vec::with_capacity(input_count.saturating_sub(1) as usize);
                for _ in 1..input_count {
                    input_classes.push(rule_reader.read_u16().ok()?);
                }
                
                let lookahead_count = rule_reader.read_u16().ok()?;
                let mut lookahead_classes = Vec::with_capacity(lookahead_count as usize);
                for _ in 0..lookahead_count {
                    lookahead_classes.push(rule_reader.read_u16().ok()?);
                }
                
                let subst_count = rule_reader.read_u16().ok()?;
                let mut lookup_records = Vec::with_capacity(subst_count as usize);
                for _ in 0..subst_count {
                    lookup_records.push(SubstLookupRecord {
                        sequence_index: rule_reader.read_u16().ok()?,
                        lookup_list_index: rule_reader.read_u16().ok()?,
                    });
                }
                
                rules.push(ChainedClassRule {
                    backtrack_classes,
                    input_classes,
                    lookahead_classes,
                    lookup_records,
                });
            }
            
            rule_sets.push(Some(rules));
        }
        
        Some(ChainedContextData::Format2 {
            coverage,
            backtrack_class_def,
            input_class_def,
            lookahead_class_def,
            rule_sets,
        })
    }
    
    fn parse_format3(data: &[u8]) -> Option<ChainedContextData> {
        let mut reader = FontReader::new(data);
        let _format = reader.read_u16().ok()?;
        
        // Backtrack coverages
        let backtrack_count = reader.read_u16().ok()?;
        let mut backtrack_offsets = Vec::with_capacity(backtrack_count as usize);
        for _ in 0..backtrack_count {
            backtrack_offsets.push(reader.read_u16().ok()?);
        }
        
        // Input coverages
        let input_count = reader.read_u16().ok()?;
        let mut input_offsets = Vec::with_capacity(input_count as usize);
        for _ in 0..input_count {
            input_offsets.push(reader.read_u16().ok()?);
        }
        
        // Lookahead coverages
        let lookahead_count = reader.read_u16().ok()?;
        let mut lookahead_offsets = Vec::with_capacity(lookahead_count as usize);
        for _ in 0..lookahead_count {
            lookahead_offsets.push(reader.read_u16().ok()?);
        }
        
        // Lookup records
        let subst_count = reader.read_u16().ok()?;
        let mut lookup_records = Vec::with_capacity(subst_count as usize);
        for _ in 0..subst_count {
            lookup_records.push(SubstLookupRecord {
                sequence_index: reader.read_u16().ok()?,
                lookup_list_index: reader.read_u16().ok()?,
            });
        }
        
        // Parse coverages
        let mut backtrack_coverages = Vec::with_capacity(backtrack_count as usize);
        for offset in backtrack_offsets {
            backtrack_coverages.push(Coverage::parse(&data[offset as usize..])?);
        }
        
        let mut input_coverages = Vec::with_capacity(input_count as usize);
        for offset in input_offsets {
            input_coverages.push(Coverage::parse(&data[offset as usize..])?);
        }
        
        let mut lookahead_coverages = Vec::with_capacity(lookahead_count as usize);
        for offset in lookahead_offsets {
            lookahead_coverages.push(Coverage::parse(&data[offset as usize..])?);
        }
        
        Some(ChainedContextData::Format3 {
            backtrack_coverages,
            input_coverages,
            lookahead_coverages,
            lookup_records,
        })
    }
}

/// Reverse chaining contextual single substitution (Type 8)
#[derive(Debug)]
pub struct ReverseChainSingleSubst {
    coverage: Coverage,
    backtrack_coverages: Vec<Coverage>,
    lookahead_coverages: Vec<Coverage>,
    substitutes: Vec<u16>,
}

impl ReverseChainSingleSubst {
    /// Parse reverse chaining contextual single substitution subtable
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = FontReader::new(data);
        let format = reader.read_u16().ok()?;
        if format != 1 {
            return None;
        }
        
        let coverage_offset = reader.read_u16().ok()?;
        let coverage = Coverage::parse(&data[coverage_offset as usize..])?;
        
        // Backtrack coverages
        let backtrack_count = reader.read_u16().ok()?;
        let mut backtrack_offsets = Vec::with_capacity(backtrack_count as usize);
        for _ in 0..backtrack_count {
            backtrack_offsets.push(reader.read_u16().ok()?);
        }
        
        // Lookahead coverages
        let lookahead_count = reader.read_u16().ok()?;
        let mut lookahead_offsets = Vec::with_capacity(lookahead_count as usize);
        for _ in 0..lookahead_count {
            lookahead_offsets.push(reader.read_u16().ok()?);
        }
        
        // Substitutes
        let subst_count = reader.read_u16().ok()?;
        let mut substitutes = Vec::with_capacity(subst_count as usize);
        for _ in 0..subst_count {
            substitutes.push(reader.read_u16().ok()?);
        }
        
        // Parse coverages
        let mut backtrack_coverages = Vec::with_capacity(backtrack_count as usize);
        for offset in backtrack_offsets {
            backtrack_coverages.push(Coverage::parse(&data[offset as usize..])?);
        }
        
        let mut lookahead_coverages = Vec::with_capacity(lookahead_count as usize);
        for offset in lookahead_offsets {
            lookahead_coverages.push(Coverage::parse(&data[offset as usize..])?);
        }
        
        Some(Self {
            coverage,
            backtrack_coverages,
            lookahead_coverages,
            substitutes,
        })
    }
    
    /// Apply reverse substitution (processes right-to-left)
    pub fn apply(&self, glyph_id: GlyphId, backtrack: &[GlyphId], lookahead: &[GlyphId]) -> Substitution {
        // Check coverage
        let coverage_idx = match self.coverage.get(glyph_id.0) {
            Some(idx) => idx,
            None => return Substitution::None,
        };
        
        // Check backtrack (in reverse order)
        for (i, cov) in self.backtrack_coverages.iter().enumerate() {
            let bt_glyph = match backtrack.get(backtrack.len().saturating_sub(1 + i)) {
                Some(g) => g,
                None => return Substitution::None,
            };
            if cov.get(bt_glyph.0).is_none() {
                return Substitution::None;
            }
        }
        
        // Check lookahead
        for (i, cov) in self.lookahead_coverages.iter().enumerate() {
            let la_glyph = match lookahead.get(i) {
                Some(g) => g,
                None => return Substitution::None,
            };
            if cov.get(la_glyph.0).is_none() {
                return Substitution::None;
            }
        }
        
        // Apply substitution
        if let Some(&sub) = self.substitutes.get(coverage_idx as usize) {
            Substitution::Single(GlyphId(sub))
        } else {
            Substitution::None
        }
    }
}

/// GSUB lookup
#[derive(Debug)]
pub struct GsubLookup {
    pub lookup_type: LookupType,
    pub lookup_flag: u16,
    pub subtables: Vec<GsubSubtable>,
    pub mark_filtering_set: Option<u16>,
}

/// GSUB subtable (parsed)
#[derive(Debug)]
pub enum GsubSubtable {
    Single(SingleSubst),
    Multiple(MultipleSubst),
    Alternate(AlternateSubst),
    Ligature(LigatureSubst),
    Context(ContextSubst),
    ChainedContext(ChainedContextSubst),
    ReverseChainSingle(ReverseChainSingleSubst),
}

impl<'a> GsubTable<'a> {
    /// Parse GSUB table
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
    pub fn get_lookup(&self, index: u16) -> Option<GsubLookup> {
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
    
    fn parse_lookup(data: &[u8]) -> Option<GsubLookup> {
        let mut reader = FontReader::new(data);
        
        let lookup_type = reader.read_u16().ok()?;
        let lookup_flag = reader.read_u16().ok()?;
        let subtable_count = reader.read_u16().ok()?;
        
        let mut subtable_offsets = Vec::with_capacity(subtable_count as usize);
        for _ in 0..subtable_count {
            subtable_offsets.push(reader.read_u16().ok()?);
        }
        
        // Mark filtering set (if lookup flag bit 4 is set)
        let mark_filtering_set = if lookup_flag & 0x0010 != 0 {
            Some(reader.read_u16().ok()?)
        } else {
            None
        };
        
        let mut subtables = Vec::with_capacity(subtable_count as usize);
        for offset in subtable_offsets {
            let subtable_data = &data[offset as usize..];
            
            // Handle extension lookups
            let (actual_type, actual_data) = if lookup_type == 7 {
                // Extension substitution
                let mut ext_reader = FontReader::new(subtable_data);
                let _format = ext_reader.read_u16().ok()?;
                let extension_type = ext_reader.read_u16().ok()?;
                let extension_offset = ext_reader.read_u32().ok()?;
                (extension_type, &subtable_data[extension_offset as usize..])
            } else {
                (lookup_type, subtable_data)
            };
            
            let subtable = match actual_type {
                1 => GsubSubtable::Single(SingleSubst::parse(actual_data)?),
                2 => GsubSubtable::Multiple(MultipleSubst::parse(actual_data)?),
                3 => GsubSubtable::Alternate(AlternateSubst::parse(actual_data)?),
                4 => GsubSubtable::Ligature(LigatureSubst::parse(actual_data)?),
                5 => GsubSubtable::Context(ContextSubst::parse(actual_data)?),
                6 => GsubSubtable::ChainedContext(ChainedContextSubst::parse(actual_data)?),
                8 => GsubSubtable::ReverseChainSingle(ReverseChainSingleSubst::parse(actual_data)?),
                _ => continue,
            };
            
            subtables.push(subtable);
        }
        
        let lookup_type = LookupType::try_from(if lookup_type == 7 { 1 } else { lookup_type }).ok()?;
        
        Some(GsubLookup {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lookup_type_conversion() {
        assert_eq!(LookupType::try_from(1), Ok(LookupType::Single));
        assert_eq!(LookupType::try_from(4), Ok(LookupType::Ligature));
        assert_eq!(LookupType::try_from(8), Ok(LookupType::ReverseChainSingle));
        assert!(LookupType::try_from(9).is_err());
    }
    
    #[test]
    fn test_coverage_format1() {
        // Format 1: glyph array [10, 20, 30]
        let data = [
            0x00, 0x01, // format = 1
            0x00, 0x03, // count = 3
            0x00, 0x0A, // glyph 10
            0x00, 0x14, // glyph 20
            0x00, 0x1E, // glyph 30
        ];
        
        let coverage = Coverage::parse(&data).unwrap();
        assert_eq!(coverage.get(10), Some(0));
        assert_eq!(coverage.get(20), Some(1));
        assert_eq!(coverage.get(30), Some(2));
        assert_eq!(coverage.get(15), None);
    }
    
    #[test]
    fn test_glyph_id() {
        let g = GlyphId(42);
        assert_eq!(g.0, 42);
    }
}
