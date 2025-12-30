//! Script Itemization
//!
//! Detects Unicode scripts and segments text into runs of the same script.
//! Uses StringInterner for efficient script/language tag storage.

use super::memory::{StringInterner, InternedString};

/// ISO 15924 script codes as OpenType tags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Script {
    /// Unknown or Common
    Common = tag(b"DFLT"),
    /// Inherited
    Inherited = tag(b"zinh"),
    /// Latin
    Latin = tag(b"latn"),
    /// Greek
    Greek = tag(b"grek"),
    /// Cyrillic
    Cyrillic = tag(b"cyrl"),
    /// Armenian
    Armenian = tag(b"armn"),
    /// Hebrew
    Hebrew = tag(b"hebr"),
    /// Arabic
    Arabic = tag(b"arab"),
    /// Syriac
    Syriac = tag(b"syrc"),
    /// Thaana
    Thaana = tag(b"thaa"),
    /// Devanagari
    Devanagari = tag(b"deva"),
    /// Bengali
    Bengali = tag(b"beng"),
    /// Gurmukhi
    Gurmukhi = tag(b"guru"),
    /// Gujarati
    Gujarati = tag(b"gujr"),
    /// Oriya
    Oriya = tag(b"orya"),
    /// Tamil
    Tamil = tag(b"taml"),
    /// Telugu
    Telugu = tag(b"telu"),
    /// Kannada
    Kannada = tag(b"knda"),
    /// Malayalam
    Malayalam = tag(b"mlym"),
    /// Sinhala
    Sinhala = tag(b"sinh"),
    /// Thai
    Thai = tag(b"thai"),
    /// Lao
    Lao = tag(b"lao "),
    /// Tibetan
    Tibetan = tag(b"tibt"),
    /// Myanmar
    Myanmar = tag(b"mymr"),
    /// Georgian
    Georgian = tag(b"geor"),
    /// Hangul
    Hangul = tag(b"hang"),
    /// Ethiopic
    Ethiopic = tag(b"ethi"),
    /// Cherokee
    Cherokee = tag(b"cher"),
    /// Canadian Aboriginal
    CanadianAboriginal = tag(b"cans"),
    /// Ogham
    Ogham = tag(b"ogam"),
    /// Runic
    Runic = tag(b"runr"),
    /// Khmer
    Khmer = tag(b"khmr"),
    /// Mongolian
    Mongolian = tag(b"mong"),
    /// Hiragana
    Hiragana = tag(b"hira"),
    /// Katakana
    Katakana = tag(b"kana"),
    /// Bopomofo
    Bopomofo = tag(b"bopo"),
    /// Han (CJK)
    Han = tag(b"hani"),
    /// Yi
    Yi = tag(b"yiii"),
    /// OldItalic
    OldItalic = tag(b"ital"),
    /// Gothic
    Gothic = tag(b"goth"),
    /// Deseret
    Deseret = tag(b"dsrt"),
    /// Tagalog
    Tagalog = tag(b"tglg"),
    /// Hanunoo
    Hanunoo = tag(b"hano"),
    /// Buhid
    Buhid = tag(b"buhd"),
    /// Tagbanwa
    Tagbanwa = tag(b"tagb"),
    /// Limbu
    Limbu = tag(b"limb"),
    /// TaiLe
    TaiLe = tag(b"tale"),
    /// LinearB
    LinearB = tag(b"linb"),
    /// Ugaritic
    Ugaritic = tag(b"ugar"),
    /// Shavian
    Shavian = tag(b"shaw"),
    /// Osmanya
    Osmanya = tag(b"osma"),
    /// Cypriot
    Cypriot = tag(b"cprt"),
    /// Braille
    Braille = tag(b"brai"),
    /// Buginese
    Buginese = tag(b"bugi"),
    /// Coptic
    Coptic = tag(b"copt"),
    /// NewTaiLue
    NewTaiLue = tag(b"talu"),
    /// Glagolitic
    Glagolitic = tag(b"glag"),
    /// Tifinagh
    Tifinagh = tag(b"tfng"),
    /// SylotiNagri
    SylotiNagri = tag(b"sylo"),
    /// OldPersian
    OldPersian = tag(b"xpeo"),
    /// Kharoshthi
    Kharoshthi = tag(b"khar"),
    /// Balinese
    Balinese = tag(b"bali"),
    /// Cuneiform
    Cuneiform = tag(b"xsux"),
    /// Phoenician
    Phoenician = tag(b"phnx"),
    /// PhagsPa
    PhagsPa = tag(b"phag"),
    /// Nko
    Nko = tag(b"nko "),
    /// Sundanese
    Sundanese = tag(b"sund"),
    /// Lepcha
    Lepcha = tag(b"lepc"),
    /// OlChiki
    OlChiki = tag(b"olck"),
    /// Vai
    Vai = tag(b"vai "),
    /// Saurashtra
    Saurashtra = tag(b"saur"),
    /// KayahLi
    KayahLi = tag(b"kali"),
    /// Rejang
    Rejang = tag(b"rjng"),
    /// Lycian
    Lycian = tag(b"lyci"),
    /// Carian
    Carian = tag(b"cari"),
    /// Lydian
    Lydian = tag(b"lydi"),
    /// Cham
    Cham = tag(b"cham"),
    /// TaiTham
    TaiTham = tag(b"lana"),
    /// TaiViet
    TaiViet = tag(b"tavt"),
    /// Avestan
    Avestan = tag(b"avst"),
    /// EgyptianHieroglyphs
    EgyptianHieroglyphs = tag(b"egyp"),
    /// Samaritan
    Samaritan = tag(b"samr"),
    /// Lisu
    Lisu = tag(b"lisu"),
    /// Bamum
    Bamum = tag(b"bamu"),
    /// Javanese
    Javanese = tag(b"java"),
    /// MeeteiMayek
    MeeteiMayek = tag(b"mtei"),
    /// ImperialAramaic
    ImperialAramaic = tag(b"armi"),
    /// OldSouthArabian
    OldSouthArabian = tag(b"sarb"),
    /// InscriptionalParthian
    InscriptionalParthian = tag(b"prti"),
    /// InscriptionalPahlavi
    InscriptionalPahlavi = tag(b"phli"),
    /// OldTurkic
    OldTurkic = tag(b"orkh"),
    /// Kaithi
    Kaithi = tag(b"kthi"),
    /// Batak
    Batak = tag(b"batk"),
    /// Brahmi
    Brahmi = tag(b"brah"),
    /// Mandaic
    Mandaic = tag(b"mand"),
}

/// Create OpenType tag from 4 bytes
const fn tag(bytes: &[u8; 4]) -> u32 {
    ((bytes[0] as u32) << 24) |
    ((bytes[1] as u32) << 16) |
    ((bytes[2] as u32) << 8) |
    (bytes[3] as u32)
}

impl Script {
    /// Get OpenType tag bytes
    pub fn tag_bytes(self) -> [u8; 4] {
        let t = self as u32;
        [
            ((t >> 24) & 0xFF) as u8,
            ((t >> 16) & 0xFF) as u8,
            ((t >> 8) & 0xFF) as u8,
            (t & 0xFF) as u8,
        ]
    }
    
    /// Get script from Unicode codepoint
    pub fn of(c: char) -> Self {
        let code = c as u32;
        
        match code {
            // Basic Latin and Latin Extended
            0x0041..=0x005A | 0x0061..=0x007A | // A-Z, a-z
            0x00C0..=0x00FF | // Latin-1 Supplement
            0x0100..=0x024F | // Latin Extended-A/B
            0x1E00..=0x1EFF | // Latin Extended Additional
            0x2C60..=0x2C7F | // Latin Extended-C
            0xA720..=0xA7FF | // Latin Extended-D
            0xAB30..=0xAB6F => Script::Latin, // Latin Extended-E
            
            // Greek
            0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
            
            // Cyrillic
            0x0400..=0x04FF | 0x0500..=0x052F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => Script::Cyrillic,
            
            // Armenian
            0x0530..=0x058F | 0xFB00..=0xFB17 => Script::Armenian,
            
            // Hebrew
            0x0590..=0x05FF | 0xFB1D..=0xFB4F => Script::Hebrew,
            
            // Arabic
            0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 
            0xFB50..=0xFDFF | 0xFE70..=0xFEFF => Script::Arabic,
            
            // Syriac
            0x0700..=0x074F | 0x0860..=0x086F => Script::Syriac,
            
            // Thaana
            0x0780..=0x07BF => Script::Thaana,
            
            // N'Ko
            0x07C0..=0x07FF => Script::Nko,
            
            // Devanagari
            0x0900..=0x097F | 0xA8E0..=0xA8FF => Script::Devanagari,
            
            // Bengali
            0x0980..=0x09FF => Script::Bengali,
            
            // Gurmukhi
            0x0A00..=0x0A7F => Script::Gurmukhi,
            
            // Gujarati
            0x0A80..=0x0AFF => Script::Gujarati,
            
            // Oriya
            0x0B00..=0x0B7F => Script::Oriya,
            
            // Tamil
            0x0B80..=0x0BFF => Script::Tamil,
            
            // Telugu
            0x0C00..=0x0C7F => Script::Telugu,
            
            // Kannada
            0x0C80..=0x0CFF => Script::Kannada,
            
            // Malayalam
            0x0D00..=0x0D7F => Script::Malayalam,
            
            // Sinhala
            0x0D80..=0x0DFF => Script::Sinhala,
            
            // Thai
            0x0E00..=0x0E7F => Script::Thai,
            
            // Lao
            0x0E80..=0x0EFF => Script::Lao,
            
            // Tibetan
            0x0F00..=0x0FFF => Script::Tibetan,
            
            // Myanmar
            0x1000..=0x109F | 0xAA60..=0xAA7F => Script::Myanmar,
            
            // Georgian
            0x10A0..=0x10FF | 0x2D00..=0x2D2F => Script::Georgian,
            
            // Hangul
            0x1100..=0x11FF | 0xAC00..=0xD7AF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xD7B0..=0xD7FF => Script::Hangul,
            
            // Ethiopic
            0x1200..=0x137F | 0x1380..=0x139F | 0x2D80..=0x2DDF | 0xAB00..=0xAB2F => Script::Ethiopic,
            
            // Cherokee
            0x13A0..=0x13FF | 0xAB70..=0xABBF => Script::Cherokee,
            
            // Canadian Aboriginal
            0x1400..=0x167F | 0x18B0..=0x18FF => Script::CanadianAboriginal,
            
            // Ogham
            0x1680..=0x169F => Script::Ogham,
            
            // Runic
            0x16A0..=0x16FF => Script::Runic,
            
            // Khmer
            0x1780..=0x17FF | 0x19E0..=0x19FF => Script::Khmer,
            
            // Mongolian
            0x1800..=0x18AF => Script::Mongolian,
            
            // Hiragana
            0x3040..=0x309F | 0x1B000..=0x1B0FF => Script::Hiragana,
            
            // Katakana
            0x30A0..=0x30FF | 0x31F0..=0x31FF | 0xFF65..=0xFF9F => Script::Katakana,
            
            // Bopomofo
            0x3100..=0x312F | 0x31A0..=0x31BF => Script::Bopomofo,
            
            // Han (CJK)
            0x2E80..=0x2EFF | // CJK Radicals Supplement
            0x2F00..=0x2FDF | // Kangxi Radicals
            0x3400..=0x4DBF | // CJK Extension A
            0x4E00..=0x9FFF | // CJK Unified Ideographs
            0xF900..=0xFAFF | // CJK Compatibility Ideographs
            0x20000..=0x2A6DF | // CJK Extension B
            0x2A700..=0x2B73F | // CJK Extension C
            0x2B740..=0x2B81F | // CJK Extension D
            0x2B820..=0x2CEAF | // CJK Extension E
            0x2CEB0..=0x2EBEF | // CJK Extension F
            0x2F800..=0x2FA1F => Script::Han, // CJK Compatibility Supplement
            
            // Yi
            0xA000..=0xA48F | 0xA490..=0xA4CF => Script::Yi,
            
            // Common/Unknown - punctuation, symbols, etc.
            0x0000..=0x0040 | 0x005B..=0x0060 | 0x007B..=0x00BF |
            0x2000..=0x206F | // General Punctuation
            0x2070..=0x209F | // Superscripts/Subscripts
            0x20A0..=0x20CF | // Currency Symbols
            0x2100..=0x214F | // Letterlike Symbols
            0x2150..=0x218F | // Number Forms
            0x2190..=0x21FF | // Arrows
            0x2200..=0x22FF | // Mathematical Operators
            0x2300..=0x23FF | // Misc Technical
            0x2400..=0x243F | // Control Pictures
            0x2500..=0x257F | // Box Drawing
            0x2580..=0x259F | // Block Elements
            0x25A0..=0x25FF | // Geometric Shapes
            0x2600..=0x26FF | // Misc Symbols
            0x2700..=0x27BF | // Dingbats
            0x3000..=0x303F | // CJK Symbols and Punctuation
            0xFF00..=0xFF64 | // Halfwidth/Fullwidth Forms
            0xFFA0..=0xFFEF => Script::Common,
            
            // Inherited (combining marks that inherit script)
            0x0300..=0x036F | // Combining Diacritical Marks
            0x1AB0..=0x1AFF | // Combining Diacritical Marks Extended
            0x1DC0..=0x1DFF | // Combining Diacritical Marks Supplement
            0x20D0..=0x20FF | // Combining Diacritical Marks for Symbols
            0xFE00..=0xFE0F | // Variation Selectors
            0xFE20..=0xFE2F => Script::Inherited, // Combining Half Marks
            
            _ => Script::Common,
        }
    }
    
    /// Check if script is RTL
    pub fn is_rtl(self) -> bool {
        matches!(self, 
            Script::Arabic | Script::Hebrew | Script::Syriac | 
            Script::Thaana | Script::Nko | Script::Mandaic |
            Script::Samaritan | Script::ImperialAramaic |
            Script::InscriptionalParthian | Script::InscriptionalPahlavi |
            Script::OldSouthArabian | Script::Avestan | Script::Phoenician
        )
    }
    
    /// Check if this is a complex script requiring special shaping
    pub fn is_complex(self) -> bool {
        matches!(self,
            Script::Arabic | Script::Hebrew | Script::Syriac | Script::Thaana | Script::Nko |
            Script::Devanagari | Script::Bengali | Script::Gurmukhi | Script::Gujarati |
            Script::Oriya | Script::Tamil | Script::Telugu | Script::Kannada |
            Script::Malayalam | Script::Sinhala | Script::Thai | Script::Lao |
            Script::Tibetan | Script::Myanmar | Script::Khmer | Script::Hangul
        )
    }
}

impl Default for Script {
    fn default() -> Self {
        Script::Common
    }
}

/// A run of text with the same script
#[derive(Debug, Clone)]
pub struct ScriptRun {
    /// Start byte offset
    pub start: usize,
    /// End byte offset (exclusive)
    pub end: usize,
    /// Script of this run
    pub script: Script,
}

/// Script itemizer for text segmentation
pub struct ScriptItemizer {
    /// Interned script/language tags for efficiency
    interner: StringInterner,
}

impl ScriptItemizer {
    /// Create new script itemizer
    pub fn new() -> Self {
        Self {
            interner: StringInterner::new(),
        }
    }
    
    /// Segment text into script runs
    pub fn itemize(&self, text: &str) -> Vec<ScriptRun> {
        if text.is_empty() {
            return Vec::new();
        }
        
        let mut runs = Vec::new();
        let mut chars = text.char_indices().peekable();
        
        let mut current_script = Script::Common;
        let mut run_start = 0;
        let mut last_real_script = Script::Common;
        
        while let Some((byte_offset, c)) = chars.next() {
            let char_script = Script::of(c);
            
            // Resolve Common and Inherited scripts
            let resolved_script = match char_script {
                Script::Common | Script::Inherited => {
                    // Inherit from surrounding context
                    if last_real_script != Script::Common {
                        last_real_script
                    } else {
                        // Look ahead for a real script
                        let mut lookahead = chars.clone();
                        let mut found = Script::Common;
                        while let Some((_, ahead_c)) = lookahead.next() {
                            let ahead_script = Script::of(ahead_c);
                            if ahead_script != Script::Common && ahead_script != Script::Inherited {
                                found = ahead_script;
                                break;
                            }
                        }
                        found
                    }
                }
                _ => {
                    last_real_script = char_script;
                    char_script
                }
            };
            
            // Check for script change
            if resolved_script != current_script && current_script != Script::Common {
                // End current run
                runs.push(ScriptRun {
                    start: run_start,
                    end: byte_offset,
                    script: current_script,
                });
                run_start = byte_offset;
            }
            
            current_script = resolved_script;
        }
        
        // Final run
        runs.push(ScriptRun {
            start: run_start,
            end: text.len(),
            script: current_script,
        });
        
        runs
    }
    
    /// Get interned OpenType script tag
    pub fn intern_script_tag(&mut self, script: Script) -> InternedString {
        let bytes = script.tag_bytes();
        let s = std::str::from_utf8(&bytes).unwrap_or("DFLT").trim();
        self.interner.intern(s)
    }
    
    /// Get string for an interned tag
    pub fn get_tag_string(&self, interned: &InternedString) -> Option<&str> {
        self.interner.get(interned)
    }
    
    /// Get the underlying string interner
    pub fn interner(&self) -> &StringInterner {
        &self.interner
    }
    
    /// Get the underlying string interner mutably
    pub fn interner_mut(&mut self) -> &mut StringInterner {
        &mut self.interner
    }
}

impl Default for ScriptItemizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Language tag for OpenType features
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Language(pub u32);

impl Language {
    /// Default language
    pub const DEFAULT: Language = Language(tag(b"dflt"));
    
    /// English
    pub const ENGLISH: Language = Language(tag(b"ENG "));
    
    /// Arabic
    pub const ARABIC: Language = Language(tag(b"ARA "));
    
    /// Chinese (Simplified)
    pub const CHINESE_SIMPLIFIED: Language = Language(tag(b"ZHS "));
    
    /// Chinese (Traditional)
    pub const CHINESE_TRADITIONAL: Language = Language(tag(b"ZHT "));
    
    /// Japanese
    pub const JAPANESE: Language = Language(tag(b"JAN "));
    
    /// Korean
    pub const KOREAN: Language = Language(tag(b"KOR "));
    
    /// Hindi
    pub const HINDI: Language = Language(tag(b"HIN "));
    
    /// Create from BCP 47 language tag
    pub fn from_bcp47(tag: &str) -> Self {
        let primary = tag.split('-').next().unwrap_or("en");
        
        match primary.to_lowercase().as_str() {
            "ar" => Self::ARABIC,
            "zh" => {
                if tag.contains("Hans") || tag.contains("CN") {
                    Self::CHINESE_SIMPLIFIED
                } else {
                    Self::CHINESE_TRADITIONAL
                }
            }
            "ja" => Self::JAPANESE,
            "ko" => Self::KOREAN,
            "hi" => Self::HINDI,
            "en" => Self::ENGLISH,
            _ => Self::DEFAULT,
        }
    }
    
    /// Get tag bytes
    pub fn tag_bytes(self) -> [u8; 4] {
        let t = self.0;
        [
            ((t >> 24) & 0xFF) as u8,
            ((t >> 16) & 0xFF) as u8,
            ((t >> 8) & 0xFF) as u8,
            (t & 0xFF) as u8,
        ]
    }
}

impl Default for Language {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Text direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// Left-to-right
    #[default]
    LeftToRight,
    /// Right-to-left
    RightToLeft,
    /// Top-to-bottom
    TopToBottom,
    /// Bottom-to-top
    BottomToTop,
}

impl Direction {
    /// Check if horizontal
    pub fn is_horizontal(self) -> bool {
        matches!(self, Direction::LeftToRight | Direction::RightToLeft)
    }
    
    /// Check if vertical
    pub fn is_vertical(self) -> bool {
        matches!(self, Direction::TopToBottom | Direction::BottomToTop)
    }
    
    /// Get direction from script
    pub fn from_script(script: Script) -> Self {
        if script.is_rtl() {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_script_detection_latin() {
        assert_eq!(Script::of('A'), Script::Latin);
        assert_eq!(Script::of('z'), Script::Latin);
        assert_eq!(Script::of('é'), Script::Latin);
    }
    
    #[test]
    fn test_script_detection_arabic() {
        assert_eq!(Script::of('ا'), Script::Arabic);
        assert_eq!(Script::of('ب'), Script::Arabic);
    }
    
    #[test]
    fn test_script_detection_cjk() {
        assert_eq!(Script::of('中'), Script::Han);
        assert_eq!(Script::of('あ'), Script::Hiragana);
        assert_eq!(Script::of('カ'), Script::Katakana);
        assert_eq!(Script::of('한'), Script::Hangul);
    }
    
    #[test]
    fn test_script_rtl() {
        assert!(Script::Arabic.is_rtl());
        assert!(Script::Hebrew.is_rtl());
        assert!(!Script::Latin.is_rtl());
    }
    
    #[test]
    fn test_script_itemizer() {
        let itemizer = ScriptItemizer::new();
        
        // Pure Latin
        let runs = itemizer.itemize("Hello World");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].script, Script::Latin);
        
        // Mixed Latin and Arabic
        let runs = itemizer.itemize("Hello مرحبا");
        assert_eq!(runs.len(), 2);
    }
    
    #[test]
    fn test_language_from_bcp47() {
        assert_eq!(Language::from_bcp47("en-US"), Language::ENGLISH);
        assert_eq!(Language::from_bcp47("ar"), Language::ARABIC);
        assert_eq!(Language::from_bcp47("zh-Hans"), Language::CHINESE_SIMPLIFIED);
        assert_eq!(Language::from_bcp47("ja"), Language::JAPANESE);
    }
    
    #[test]
    fn test_direction_from_script() {
        assert_eq!(Direction::from_script(Script::Latin), Direction::LeftToRight);
        assert_eq!(Direction::from_script(Script::Arabic), Direction::RightToLeft);
    }
}
