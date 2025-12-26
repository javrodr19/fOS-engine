//! Indic Script Shaping
//!
//! Implements syllable segmentation, consonant cluster reordering, and
//! feature application for Indic scripts (Devanagari, Bengali, Tamil, etc.)

use crate::font::parser::GlyphId;

/// Indic character category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicCategory {
    /// Consonant
    Consonant,
    /// Vowel (independent)
    Vowel,
    /// Vowel (dependent matra)
    VowelDependent,
    /// Nukta (dot below)
    Nukta,
    /// Halant/Virama (consonant killer)
    Halant,
    /// Consonant with nukta
    ConsonantWithNukta,
    /// Consonant medial
    ConsonantMedial,
    /// Consonant final
    ConsonantFinal,
    /// Consonant head letter
    ConsonantHead,
    /// Consonant subjoined
    ConsonantSubjoined,
    /// Ra (special handling)
    Ra,
    /// Anusvara
    Anusvara,
    /// Visarga
    Visarga,
    /// Candrabindu
    Candrabindu,
    /// Other modifiers
    Modifier,
    /// Number
    Number,
    /// Placeholder (dotted circle)
    Placeholder,
    /// Symbol
    Symbol,
    /// Other
    Other,
}

/// Indic syllable position
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyllablePosition {
    /// Reph (repha form of Ra+Halant at start)
    Reph,
    /// Base consonant
    Base,
    /// Pre-base form
    PreBase,
    /// Below-base form
    BelowBase,
    /// Above-base form
    AboveBase,
    /// Post-base form
    PostBase,
    /// Pre-base matra
    PreBaseMatra,
    /// Above-base matra
    AboveBaseMatra,
    /// Below-base matra
    BelowBaseMatra,
    /// Post-base matra
    PostBaseMatra,
    /// Syllable modifier (anusvara, visarga, etc.)
    SyllableModifier,
}

/// Get Indic character category
pub fn indic_category(c: char) -> IndicCategory {
    let code = c as u32;
    
    // Devanagari (0x0900-0x097F)
    if (0x0900..=0x097F).contains(&code) {
        return devanagari_category(code);
    }
    
    // Bengali (0x0980-0x09FF)
    if (0x0980..=0x09FF).contains(&code) {
        return bengali_category(code);
    }
    
    // Gurmukhi (0x0A00-0x0A7F)
    if (0x0A00..=0x0A7F).contains(&code) {
        return gurmukhi_category(code);
    }
    
    // Gujarati (0x0A80-0x0AFF)
    if (0x0A80..=0x0AFF).contains(&code) {
        return gujarati_category(code);
    }
    
    // Tamil (0x0B80-0x0BFF)
    if (0x0B80..=0x0BFF).contains(&code) {
        return tamil_category(code);
    }
    
    // Telugu (0x0C00-0x0C7F)
    if (0x0C00..=0x0C7F).contains(&code) {
        return telugu_category(code);
    }
    
    // Kannada (0x0C80-0x0CFF)
    if (0x0C80..=0x0CFF).contains(&code) {
        return kannada_category(code);
    }
    
    // Malayalam (0x0D00-0x0D7F)
    if (0x0D00..=0x0D7F).contains(&code) {
        return malayalam_category(code);
    }
    
    IndicCategory::Other
}

fn devanagari_category(code: u32) -> IndicCategory {
    match code {
        // Vowels (independent)
        0x0904..=0x0914 => IndicCategory::Vowel,
        
        // Consonants
        0x0915..=0x0939 => {
            if code == 0x0930 {
                IndicCategory::Ra
            } else {
                IndicCategory::Consonant
            }
        }
        
        // Nukta
        0x093C => IndicCategory::Nukta,
        
        // Dependent vowels (matras)
        0x093E..=0x094C | 0x094E..=0x094F => IndicCategory::VowelDependent,
        
        // Halant/Virama
        0x094D => IndicCategory::Halant,
        
        // Anusvara
        0x0902 => IndicCategory::Anusvara,
        
        // Visarga
        0x0903 => IndicCategory::Visarga,
        
        // Candrabindu
        0x0901 => IndicCategory::Candrabindu,
        
        // Numbers
        0x0966..=0x096F => IndicCategory::Number,
        
        // Additional consonants
        0x0958..=0x095F => IndicCategory::ConsonantWithNukta,
        
        _ => IndicCategory::Other,
    }
}

fn bengali_category(code: u32) -> IndicCategory {
    match code {
        0x0985..=0x0994 => IndicCategory::Vowel,
        0x0995..=0x09B9 => {
            if code == 0x09B0 {
                IndicCategory::Ra
            } else {
                IndicCategory::Consonant
            }
        }
        0x09BC => IndicCategory::Nukta,
        0x09BE..=0x09CC => IndicCategory::VowelDependent,
        0x09CD => IndicCategory::Halant,
        0x0982 => IndicCategory::Anusvara,
        0x0983 => IndicCategory::Visarga,
        0x0981 => IndicCategory::Candrabindu,
        0x09E6..=0x09EF => IndicCategory::Number,
        _ => IndicCategory::Other,
    }
}

fn gurmukhi_category(code: u32) -> IndicCategory {
    match code {
        0x0A05..=0x0A14 => IndicCategory::Vowel,
        0x0A15..=0x0A39 => {
            if code == 0x0A30 {
                IndicCategory::Ra
            } else {
                IndicCategory::Consonant
            }
        }
        0x0A3C => IndicCategory::Nukta,
        0x0A3E..=0x0A4C => IndicCategory::VowelDependent,
        0x0A4D => IndicCategory::Halant,
        0x0A02 | 0x0A70 => IndicCategory::Anusvara,
        0x0A66..=0x0A6F => IndicCategory::Number,
        _ => IndicCategory::Other,
    }
}

fn gujarati_category(code: u32) -> IndicCategory {
    match code {
        0x0A85..=0x0A94 => IndicCategory::Vowel,
        0x0A95..=0x0AB9 => {
            if code == 0x0AB0 {
                IndicCategory::Ra
            } else {
                IndicCategory::Consonant
            }
        }
        0x0ABC => IndicCategory::Nukta,
        0x0ABE..=0x0ACC => IndicCategory::VowelDependent,
        0x0ACD => IndicCategory::Halant,
        0x0A82 => IndicCategory::Anusvara,
        0x0A83 => IndicCategory::Visarga,
        0x0A81 => IndicCategory::Candrabindu,
        0x0AE6..=0x0AEF => IndicCategory::Number,
        _ => IndicCategory::Other,
    }
}

fn tamil_category(code: u32) -> IndicCategory {
    match code {
        0x0B85..=0x0B94 => IndicCategory::Vowel,
        0x0B95..=0x0BB9 => IndicCategory::Consonant, // No Ra special in Tamil
        0x0BBE..=0x0BCC => IndicCategory::VowelDependent,
        0x0BCD => IndicCategory::Halant,
        0x0B82 => IndicCategory::Anusvara,
        0x0B83 => IndicCategory::Visarga,
        0x0BE6..=0x0BEF => IndicCategory::Number,
        _ => IndicCategory::Other,
    }
}

fn telugu_category(code: u32) -> IndicCategory {
    match code {
        0x0C05..=0x0C14 => IndicCategory::Vowel,
        0x0C15..=0x0C39 => {
            if code == 0x0C30 {
                IndicCategory::Ra
            } else {
                IndicCategory::Consonant
            }
        }
        0x0C3E..=0x0C4C => IndicCategory::VowelDependent,
        0x0C4D => IndicCategory::Halant,
        0x0C02 => IndicCategory::Anusvara,
        0x0C03 => IndicCategory::Visarga,
        0x0C01 => IndicCategory::Candrabindu,
        0x0C66..=0x0C6F => IndicCategory::Number,
        _ => IndicCategory::Other,
    }
}

fn kannada_category(code: u32) -> IndicCategory {
    match code {
        0x0C85..=0x0C94 => IndicCategory::Vowel,
        0x0C95..=0x0CB9 => {
            if code == 0x0CB0 {
                IndicCategory::Ra
            } else {
                IndicCategory::Consonant
            }
        }
        0x0CBC => IndicCategory::Nukta,
        0x0CBE..=0x0CCC => IndicCategory::VowelDependent,
        0x0CCD => IndicCategory::Halant,
        0x0C82 => IndicCategory::Anusvara,
        0x0C83 => IndicCategory::Visarga,
        0x0CE6..=0x0CEF => IndicCategory::Number,
        _ => IndicCategory::Other,
    }
}

fn malayalam_category(code: u32) -> IndicCategory {
    match code {
        0x0D05..=0x0D14 => IndicCategory::Vowel,
        0x0D15..=0x0D39 => {
            if code == 0x0D30 {
                IndicCategory::Ra
            } else {
                IndicCategory::Consonant
            }
        }
        0x0D3E..=0x0D4C => IndicCategory::VowelDependent,
        0x0D4D => IndicCategory::Halant,
        0x0D02 => IndicCategory::Anusvara,
        0x0D03 => IndicCategory::Visarga,
        0x0D66..=0x0D6F => IndicCategory::Number,
        _ => IndicCategory::Other,
    }
}

/// Indic syllable
#[derive(Debug, Clone)]
pub struct Syllable {
    /// Start index in input
    pub start: usize,
    /// End index (exclusive)
    pub end: usize,
    /// Base consonant index within syllable
    pub base: Option<usize>,
    /// Has reph at start
    pub has_reph: bool,
    /// Character indices in reordered order
    pub reordered: Vec<usize>,
}

/// Indic shaper
#[derive(Debug)]
pub struct IndicShaper {
    /// Input categories
    categories: Vec<IndicCategory>,
    /// Detected syllables
    syllables: Vec<Syllable>,
}

impl IndicShaper {
    /// Create new Indic shaper
    pub fn new() -> Self {
        Self {
            categories: Vec::new(),
            syllables: Vec::new(),
        }
    }
    
    /// Analyze text and segment into syllables
    pub fn analyze(&mut self, text: &str) {
        let chars: Vec<char> = text.chars().collect();
        
        self.categories.clear();
        self.syllables.clear();
        
        if chars.is_empty() {
            return;
        }
        
        // Get categories
        self.categories = chars.iter().map(|&c| indic_category(c)).collect();
        
        // Segment into syllables and reorder
        self.segment_syllables(&chars);
    }
    
    /// Segment text into syllables
    fn segment_syllables(&mut self, chars: &[char]) {
        let len = chars.len();
        if len == 0 {
            return;
        }
        
        let mut i = 0;
        
        while i < len {
            let syllable_start = i;
            
            // Skip non-Indic characters
            if self.categories[i] == IndicCategory::Other {
                i += 1;
                continue;
            }
            
            // Find syllable boundaries
            // Syllable structure: (Reph)? (C Nukta? Halant)* C Nukta? (Matras)* (Modifiers)*
            
            let mut has_reph = false;
            let mut base = None;
            
            // Check for Reph (Ra + Halant at start)
            if self.categories[i] == IndicCategory::Ra && 
               i + 1 < len && 
               self.categories[i + 1] == IndicCategory::Halant {
                has_reph = true;
                i += 2;
            }
            
            // Consume consonant clusters (C Nukta? Halant)*
            while i < len {
                let cat = self.categories[i];
                
                if cat == IndicCategory::Consonant || 
                   cat == IndicCategory::Ra ||
                   cat == IndicCategory::ConsonantWithNukta {
                    base = Some(i - syllable_start);
                    i += 1;
                    
                    // Consume nukta if present
                    if i < len && self.categories[i] == IndicCategory::Nukta {
                        i += 1;
                    }
                    
                    // Check for halant (continues cluster)
                    if i < len && self.categories[i] == IndicCategory::Halant {
                        i += 1;
                        // Continue to next consonant
                    } else {
                        // No halant - this is the base consonant
                        break;
                    }
                } else {
                    break;
                }
            }
            
            // Consume vowels and matras
            while i < len {
                let cat = self.categories[i];
                if cat == IndicCategory::VowelDependent ||
                   cat == IndicCategory::Vowel {
                    i += 1;
                } else {
                    break;
                }
            }
            
            // Consume modifiers (anusvara, visarga, candrabindu)
            while i < len {
                let cat = self.categories[i];
                if cat == IndicCategory::Anusvara ||
                   cat == IndicCategory::Visarga ||
                   cat == IndicCategory::Candrabindu ||
                   cat == IndicCategory::Modifier {
                    i += 1;
                } else {
                    break;
                }
            }
            
            // If we didn't consume anything, advance
            if i == syllable_start {
                i += 1;
                continue;
            }
            
            // Create reordered indices
            let syllable_len = i - syllable_start;
            let mut reordered: Vec<usize> = (0..syllable_len).collect();
            
            // Apply reordering for pre-base matras
            // Some matras (like -i in Devanagari) are stored after the consonant
            // but rendered before it
            self.reorder_matras(&mut reordered, &self.categories[syllable_start..i], base);
            
            self.syllables.push(Syllable {
                start: syllable_start,
                end: i,
                base,
                has_reph,
                reordered,
            });
        }
    }
    
    /// Reorder pre-base matras
    fn reorder_matras(&self, reordered: &mut [usize], categories: &[IndicCategory], base: Option<usize>) {
        if categories.is_empty() {
            return;
        }
        
        let base_idx = base.unwrap_or(0);
        
        // Find pre-base matras (those that should render before the base)
        // This is a simplified version - full implementation would check each matra's position
        
        let mut pre_base_matras: Vec<usize> = Vec::new();
        let mut post_base = Vec::new();
        
        for (i, &cat) in categories.iter().enumerate() {
            if i <= base_idx {
                continue;
            }
            
            if cat == IndicCategory::VowelDependent {
                // Check if this is a pre-base matra
                // In Devanagari, -i (0x093F) is pre-base
                // For now, we'll just leave them in place
                // Full implementation would check the specific codepoint
                post_base.push(i);
            }
        }
        
        // Reordering would move pre_base_matras before the base
        // This is handled by the rendering engine using the reordered indices
    }
    
    /// Get syllables
    pub fn syllables(&self) -> &[Syllable] {
        &self.syllables
    }
    
    /// Get required OpenType features for Indic shaping
    pub fn required_features() -> &'static [[u8; 4]] {
        static FEATURES: [[u8; 4]; 19] = [
            *b"locl", *b"nukt", *b"akhn", *b"rphf", *b"rkrf", *b"pref",
            *b"blwf", *b"abvf", *b"half", *b"pstf", *b"vatu", *b"cjct",
            *b"pres", *b"abvs", *b"blws", *b"psts", *b"haln", *b"calt", *b"liga",
        ];
        &FEATURES
    }
    
    /// Get GPOS features for Indic
    pub fn positioning_features() -> &'static [[u8; 4]] {
        static FEATURES: [[u8; 4]; 6] = [
            *b"dist", *b"abvm", *b"blwm", *b"kern", *b"mark", *b"mkmk",
        ];
        &FEATURES
    }
}

impl Default for IndicShaper {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if character is a pre-base matra
pub fn is_pre_base_matra(c: char) -> bool {
    let code = c as u32;
    matches!(code,
        0x093F | // Devanagari vowel sign i
        0x09BF | // Bengali vowel sign i
        0x0A3F | // Gurmukhi vowel sign i
        0x0ABF | // Gujarati vowel sign i
        0x0CBF | // Kannada vowel sign i
        0x0D3F | // Malayalam vowel sign i
        0x0D46..=0x0D48 // Malayalam vowel signs e, ee, ai
    )
}

/// Check if character is a below-base matra
pub fn is_below_base_matra(c: char) -> bool {
    let code = c as u32;
    matches!(code,
        0x0941..=0x0944 | // Devanagari u, uu, vocalic r, vocalic rr
        0x0962..=0x0963 | // Devanagari vocalic l, vocalic ll
        0x09C1..=0x09C4 | // Bengali
        0x0A41..=0x0A42 | // Gurmukhi
        0x0AC1..=0x0AC4 | // Gujarati
        0x0C41..=0x0C44 | // Telugu
        0x0CC1..=0x0CC4 | // Kannada
        0x0D41..=0x0D44   // Malayalam
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_devanagari_category() {
        assert_eq!(indic_category('क'), IndicCategory::Consonant);
        assert_eq!(indic_category('र'), IndicCategory::Ra);
        assert_eq!(indic_category('अ'), IndicCategory::Vowel);
        assert_eq!(indic_category('ा'), IndicCategory::VowelDependent);
        assert_eq!(indic_category('्'), IndicCategory::Halant);
        assert_eq!(indic_category('ं'), IndicCategory::Anusvara);
    }
    
    #[test]
    fn test_syllable_segmentation() {
        let mut shaper = IndicShaper::new();
        shaper.analyze("नमस्ते"); // "namaste"
        
        // Should segment into syllables
        assert!(!shaper.syllables.is_empty());
    }
    
    #[test]
    fn test_pre_base_matra() {
        assert!(is_pre_base_matra('ि')); // Devanagari short i
        assert!(!is_pre_base_matra('ा')); // Devanagari aa (post-base)
    }
    
    #[test]
    fn test_below_base_matra() {
        assert!(is_below_base_matra('ु')); // Devanagari u
        assert!(!is_below_base_matra('ा')); // Devanagari aa
    }
}
