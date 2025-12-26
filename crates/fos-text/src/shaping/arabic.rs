//! Arabic Script Shaping
//!
//! Implements Arabic joining analysis and form selection for proper
//! rendering of Arabic, Syriac, and other Arabic-like scripts.

use crate::font::parser::GlyphId;

/// Arabic joining type (from Unicode data)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JoiningType {
    /// Right joining (connects to next)
    Right,
    /// Left joining (connects to previous) - rare
    Left,
    /// Dual joining (connects both sides)
    Dual,
    /// Causing (causes joining but doesn't join itself)
    Causing,
    /// Non-joining
    #[default]
    NonJoining,
    /// Transparent (marks, doesn't affect joining)
    Transparent,
}

/// Arabic positional form
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionalForm {
    /// Isolated form (no connections)
    Isolated,
    /// Initial form (connects to next only)
    Initial,
    /// Medial form (connects both sides)
    Medial,
    /// Final form (connects to previous only)
    Final,
}

impl PositionalForm {
    /// Get OpenType feature tag for this form
    pub fn feature_tag(self) -> [u8; 4] {
        match self {
            PositionalForm::Isolated => *b"isol",
            PositionalForm::Initial => *b"init",
            PositionalForm::Medial => *b"medi",
            PositionalForm::Final => *b"fina",
        }
    }
}

/// Get joining type for a character
pub fn joining_type(c: char) -> JoiningType {
    let code = c as u32;
    
    match code {
        // Arabic right-joining letters (only connect to next)
        0x0622 | // Alef with madda
        0x0623 | // Alef with hamza above
        0x0624 | // Waw with hamza
        0x0625 | // Alef with hamza below
        0x0627 | // Alef
        0x0629 | // Teh marbuta
        0x062F | // Dal
        0x0630 | // Thal
        0x0631 | // Reh
        0x0632 | // Zain
        0x0648 | // Waw
        0x0671..=0x0673 | // Alef variants
        0x0675..=0x0677 | // More alef variants
        0x0688..=0x0699 | // Dal-like letters
        0x06C0 | // Heh with yeh above
        0x06C3 | // Teh marbuta goal
        0x06C4..=0x06CB | // Waw variants
        0x06CD | // Yeh with tail
        0x06CF | // Waw with dot above
        0x06D2 | // Yeh barree
        0x06D3 | // Yeh barree with hamza
        0x06D5 | // Ae
        0x06EE..=0x06EF => JoiningType::Right, // More variants
        
        // Arabic dual-joining letters (connect both sides)
        0x0626 | // Yeh with hamza
        0x0628 | // Beh
        0x062A..=0x062E | // Teh, Theh, Jeem, Hah, Khah
        0x0633..=0x063F | // Seen through Ghain
        0x0641..=0x0647 | // Feh through Heh
        0x0649..=0x064A | // Alef maksura, Yeh
        0x066E..=0x066F | // Dotless beh, dotless qaf
        0x0678..=0x0687 | // Various dual-joining
        0x069A..=0x06BF | // Various dual-joining
        0x06C1..=0x06C2 | // Heh goal variants
        0x06CC | // Farsi yeh
        0x06CE | // Yeh with small v
        0x06D0..=0x06D1 | // E variants
        0x06FA..=0x06FC | // Various
        0x06FF | // Knotted heh
        0x0750..=0x077F | // Arabic Supplement
        0x08A0..=0x08B4 | // Arabic Extended-A
        0x08B6..=0x08C7 => JoiningType::Dual,
        
        // Arabic transparent (combining marks)
        0x064B..=0x065F | // Arabic combining marks
        0x0670 | // Superscript alef
        0x06D6..=0x06DC | // Small high ligatures
        0x06DF..=0x06E4 | // Various marks
        0x06E7..=0x06E8 | // Yeh/noon marks
        0x06EA..=0x06ED | // More marks
        0x08D3..=0x08E1 | // Extended marks
        0x08E3..=0x08FF | // More extended marks
        0xFE00..=0xFE0F => JoiningType::Transparent, // Variation selectors
        
        // Zero-width joiner causes joining
        0x200D => JoiningType::Causing,
        
        // Zero-width non-joiner doesn't join
        0x200C => JoiningType::NonJoining,
        
        // Syriac dual-joining
        0x0710 | // Alaph
        0x0712..=0x072F | // Syriac letters
        0x074D..=0x074F => JoiningType::Dual,
        
        // Syriac right-joining
        0x0711 => JoiningType::Right, // Superscript alaph
        
        // Syriac transparent
        0x0730..=0x074A => JoiningType::Transparent,
        
        // N'Ko dual-joining
        0x07CA..=0x07EA => JoiningType::Dual,
        
        // N'Ko transparent
        0x07EB..=0x07F3 | 0x07FD => JoiningType::Transparent,
        
        // Mandaic dual-joining
        0x0840..=0x0858 => JoiningType::Dual,
        
        // Mandaic transparent
        0x0859..=0x085B => JoiningType::Transparent,
        
        // Joining type non-joining for everything else
        _ => JoiningType::NonJoining,
    }
}

/// Arabic shaper state
#[derive(Debug)]
pub struct ArabicShaper {
    /// Joining types for input characters
    joining_types: Vec<JoiningType>,
    /// Resolved positional forms
    forms: Vec<PositionalForm>,
}

impl ArabicShaper {
    /// Create new Arabic shaper
    pub fn new() -> Self {
        Self {
            joining_types: Vec::new(),
            forms: Vec::new(),
        }
    }
    
    /// Analyze text and determine positional forms
    pub fn analyze(&mut self, text: &str) {
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        
        self.joining_types.clear();
        self.forms.clear();
        
        if len == 0 {
            return;
        }
        
        // Get joining types
        self.joining_types = chars.iter().map(|&c| joining_type(c)).collect();
        self.forms = vec![PositionalForm::Isolated; len];
        
        // Resolve forms based on joining context
        for i in 0..len {
            let jt = self.joining_types[i];
            
            // Skip transparent and non-joining
            if jt == JoiningType::Transparent || jt == JoiningType::NonJoining {
                continue;
            }
            
            let can_join_prev = self.can_join_previous(i);
            let can_join_next = self.can_join_next(i);
            
            self.forms[i] = match jt {
                JoiningType::Right => {
                    if can_join_next {
                        PositionalForm::Initial
                    } else {
                        PositionalForm::Isolated
                    }
                }
                JoiningType::Left => {
                    if can_join_prev {
                        PositionalForm::Final
                    } else {
                        PositionalForm::Isolated
                    }
                }
                JoiningType::Dual => {
                    match (can_join_prev, can_join_next) {
                        (true, true) => PositionalForm::Medial,
                        (true, false) => PositionalForm::Final,
                        (false, true) => PositionalForm::Initial,
                        (false, false) => PositionalForm::Isolated,
                    }
                }
                _ => PositionalForm::Isolated,
            };
        }
    }
    
    /// Check if position can join to previous character
    fn can_join_previous(&self, pos: usize) -> bool {
        if pos == 0 {
            return false;
        }
        
        // Look backwards, skipping transparent characters
        for i in (0..pos).rev() {
            let jt = self.joining_types[i];
            match jt {
                JoiningType::Transparent => continue,
                JoiningType::Dual | JoiningType::Left | JoiningType::Causing => return true,
                _ => return false,
            }
        }
        
        false
    }
    
    /// Check if position can join to next character
    fn can_join_next(&self, pos: usize) -> bool {
        if pos >= self.joining_types.len() - 1 {
            return false;
        }
        
        // Look forwards, skipping transparent characters
        for i in (pos + 1)..self.joining_types.len() {
            let jt = self.joining_types[i];
            match jt {
                JoiningType::Transparent => continue,
                JoiningType::Dual | JoiningType::Right | JoiningType::Causing => return true,
                _ => return false,
            }
        }
        
        false
    }
    
    /// Get positional form for a character index
    pub fn form(&self, index: usize) -> Option<PositionalForm> {
        self.forms.get(index).copied()
    }
    
    /// Get all forms
    pub fn forms(&self) -> &[PositionalForm] {
        &self.forms
    }
    
    /// Get OpenType features to apply for Arabic shaping
    pub fn required_features() -> &'static [[u8; 4]] {
        static FEATURES: [[u8; 4]; 12] = [
            *b"ccmp", *b"isol", *b"fina", *b"medi", *b"init", *b"rlig",
            *b"rclt", *b"calt", *b"liga", *b"dlig", *b"cswh", *b"mset",
        ];
        &FEATURES
    }
    
    /// Get GPOS features for Arabic
    pub fn positioning_features() -> &'static [[u8; 4]] {
        static FEATURES: [[u8; 4]; 4] = [*b"curs", *b"kern", *b"mark", *b"mkmk"];
        &FEATURES
    }
}

impl Default for ArabicShaper {
    fn default() -> Self {
        Self::new()
    }
}

/// Arabic presentation forms mapping
/// Maps base character + form to presentation form codepoint
pub fn get_presentation_form(c: char, form: PositionalForm) -> Option<char> {
    let code = c as u32;
    
    // Arabic Presentation Forms-B (0xFE70-0xFEFF)
    // These are provided for compatibility; shaping should use GSUB
    let base = match code {
        0x0621 => Some(0xFE80), // Hamza
        0x0622 => Some(0xFE81), // Alef with madda
        0x0623 => Some(0xFE83), // Alef with hamza above
        0x0624 => Some(0xFE85), // Waw with hamza
        0x0625 => Some(0xFE87), // Alef with hamza below
        0x0626 => Some(0xFE89), // Yeh with hamza
        0x0627 => Some(0xFE8D), // Alef
        0x0628 => Some(0xFE8F), // Beh
        0x0629 => Some(0xFE93), // Teh marbuta
        0x062A => Some(0xFE95), // Teh
        0x062B => Some(0xFE99), // Theh
        0x062C => Some(0xFE9D), // Jeem
        0x062D => Some(0xFEA1), // Hah
        0x062E => Some(0xFEA5), // Khah
        0x062F => Some(0xFEA9), // Dal
        0x0630 => Some(0xFEAB), // Thal
        0x0631 => Some(0xFEAD), // Reh
        0x0632 => Some(0xFEAF), // Zain
        0x0633 => Some(0xFEB1), // Seen
        0x0634 => Some(0xFEB5), // Sheen
        0x0635 => Some(0xFEB9), // Sad
        0x0636 => Some(0xFEBD), // Dad
        0x0637 => Some(0xFEC1), // Tah
        0x0638 => Some(0xFEC5), // Zah
        0x0639 => Some(0xFEC9), // Ain
        0x063A => Some(0xFECD), // Ghain
        0x0641 => Some(0xFED1), // Feh
        0x0642 => Some(0xFED5), // Qaf
        0x0643 => Some(0xFED9), // Kaf
        0x0644 => Some(0xFEDD), // Lam
        0x0645 => Some(0xFEE1), // Meem
        0x0646 => Some(0xFEE5), // Noon
        0x0647 => Some(0xFEE9), // Heh
        0x0648 => Some(0xFEED), // Waw
        0x0649 => Some(0xFEEF), // Alef maksura
        0x064A => Some(0xFEF1), // Yeh
        _ => None,
    }?;
    
    // Forms are typically at: isolated, final, initial, medial
    let offset = match form {
        PositionalForm::Isolated => 0,
        PositionalForm::Final => 1,
        PositionalForm::Initial => 2,
        PositionalForm::Medial => 3,
    };
    
    // Check if this character has all 4 forms
    let has_four_forms = matches!(code,
        0x0626 | 0x0628 | 0x062A | 0x062B | 0x062C | 0x062D | 0x062E |
        0x0633 | 0x0634 | 0x0635 | 0x0636 | 0x0637 | 0x0638 | 0x0639 |
        0x063A | 0x0641 | 0x0642 | 0x0643 | 0x0644 | 0x0645 | 0x0646 |
        0x0647 | 0x064A
    );
    
    if has_four_forms {
        char::from_u32(base + offset)
    } else {
        // Right-joining letters only have isolated and final
        match form {
            PositionalForm::Isolated | PositionalForm::Initial => char::from_u32(base),
            PositionalForm::Final | PositionalForm::Medial => char::from_u32(base + 1),
        }
    }
}

/// Lam-Alef ligature detection
pub fn is_lam_alef_sequence(first: char, second: char) -> bool {
    first == '\u{0644}' && // Lam
    matches!(second, '\u{0622}' | '\u{0623}' | '\u{0625}' | '\u{0627}')
}

/// Get Lam-Alef ligature presentation form
pub fn lam_alef_ligature(alef: char, form: PositionalForm) -> Option<char> {
    let base = match alef {
        '\u{0622}' => 0xFEF5, // Lam-Alef with madda
        '\u{0623}' => 0xFEF7, // Lam-Alef with hamza above
        '\u{0625}' => 0xFEF9, // Lam-Alef with hamza below
        '\u{0627}' => 0xFEFB, // Lam-Alef
        _ => return None,
    };
    
    let offset = match form {
        PositionalForm::Isolated | PositionalForm::Initial => 0,
        PositionalForm::Final | PositionalForm::Medial => 1,
    };
    
    char::from_u32(base + offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_joining_type_alef() {
        assert_eq!(joining_type('\u{0627}'), JoiningType::Right);
    }
    
    #[test]
    fn test_joining_type_beh() {
        assert_eq!(joining_type('\u{0628}'), JoiningType::Dual);
    }
    
    #[test]
    fn test_joining_type_space() {
        assert_eq!(joining_type(' '), JoiningType::NonJoining);
    }
    
    #[test]
    fn test_joining_type_fatha() {
        assert_eq!(joining_type('\u{064E}'), JoiningType::Transparent);
    }
    
    #[test]
    fn test_arabic_shaper_isolated() {
        let mut shaper = ArabicShaper::new();
        shaper.analyze("ا"); // Single alef
        assert_eq!(shaper.form(0), Some(PositionalForm::Isolated));
    }
    
    #[test]
    fn test_arabic_shaper_word() {
        let mut shaper = ArabicShaper::new();
        shaper.analyze("بسم"); // "bsm"
        
        // Beh should be initial (connects right)
        assert_eq!(shaper.form(0), Some(PositionalForm::Initial));
        // Seen should be medial (connects both)
        assert_eq!(shaper.form(1), Some(PositionalForm::Medial));
        // Meem should be final (connects left)
        assert_eq!(shaper.form(2), Some(PositionalForm::Final));
    }
    
    #[test]
    fn test_lam_alef_detection() {
        assert!(is_lam_alef_sequence('\u{0644}', '\u{0627}'));
        assert!(!is_lam_alef_sequence('\u{0628}', '\u{0627}'));
    }
    
    #[test]
    fn test_presentation_form() {
        let form = get_presentation_form('\u{0628}', PositionalForm::Initial);
        assert!(form.is_some());
    }
}
