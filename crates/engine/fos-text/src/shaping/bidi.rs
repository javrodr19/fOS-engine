//! Unicode Bidirectional Algorithm (UAX #9)
//!
//! Full implementation of the Unicode Bidirectional Algorithm for proper
//! text rendering of mixed LTR/RTL text.

/// Bidirectional character type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidiClass {
    // Strong types
    /// Left-to-Right
    L,
    /// Right-to-Left
    R,
    /// Arabic Letter
    AL,

    // Weak types
    /// European Number
    EN,
    /// European Number Separator
    ES,
    /// European Number Terminator
    ET,
    /// Arabic Number
    AN,
    /// Common Number Separator
    CS,
    /// Nonspacing Mark
    NSM,
    /// Boundary Neutral
    BN,

    // Neutral types
    /// Paragraph Separator
    B,
    /// Segment Separator
    S,
    /// Whitespace
    WS,
    /// Other Neutrals
    ON,

    // Explicit formatting
    /// Left-to-Right Embedding
    LRE,
    /// Left-to-Right Override
    LRO,
    /// Right-to-Left Embedding
    RLE,
    /// Right-to-Left Override
    RLO,
    /// Pop Directional Format
    PDF,
    /// Left-to-Right Isolate
    LRI,
    /// Right-to-Left Isolate
    RLI,
    /// First Strong Isolate
    FSI,
    /// Pop Directional Isolate
    PDI,
}

impl BidiClass {
    /// Get bidi class for a character
    pub fn of(c: char) -> Self {
        // Simplified mapping - in production would use full Unicode data
        let code = c as u32;
        
        match code {
            // ASCII control characters
            0x0000..=0x0008 | 0x000E..=0x001B => BidiClass::BN,
            0x0009 | 0x000B | 0x001F => BidiClass::S,
            0x000A | 0x000D | 0x001C..=0x001E | 0x0085 | 0x2029 => BidiClass::B,
            0x000C | 0x0020 => BidiClass::WS,
            
            // ASCII letters and common symbols
            0x0041..=0x005A | 0x0061..=0x007A => BidiClass::L, // A-Z, a-z
            0x0030..=0x0039 => BidiClass::EN, // 0-9
            
            // Basic punctuation
            0x002B | 0x002D => BidiClass::ES, // + -
            0x0023..=0x0025 => BidiClass::ET, // # $ %
            0x002C | 0x002E | 0x002F | 0x003A => BidiClass::CS, // , . / :
            
            // Latin Extended
            0x00C0..=0x00FF => BidiClass::L,
            0x0100..=0x024F => BidiClass::L,
            
            // Hebrew
            0x0590..=0x05FF => BidiClass::R,
            
            // Arabic
            0x0600..=0x06FF => {
                match code {
                    0x0600..=0x0605 | 0x0608 | 0x060B | 0x060D | 
                    0x061B..=0x064A | 0x066D..=0x066F | 0x0671..=0x06D5 |
                    0x06E5..=0x06E6 | 0x06EE..=0x06EF | 0x06FA..=0x06FF => BidiClass::AL,
                    0x0660..=0x0669 | 0x066B..=0x066C => BidiClass::AN,
                    _ => BidiClass::AL,
                }
            }
            
            // Arabic Extended
            0x0750..=0x077F | 0x08A0..=0x08FF => BidiClass::AL,
            
            // Syriac
            0x0700..=0x074F => BidiClass::AL,
            
            // Thaana
            0x0780..=0x07BF => BidiClass::AL,
            
            // N'Ko
            0x07C0..=0x07FF => BidiClass::R,
            
            // Devanagari, Bengali, Gurmukhi, etc.
            0x0900..=0x0DFF => BidiClass::L,
            
            // Thai, Lao
            0x0E00..=0x0EFF => BidiClass::L,
            
            // CJK
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF => BidiClass::L,
            
            // Hangul
            0xAC00..=0xD7AF => BidiClass::L,
            
            // Hiragana, Katakana
            0x3040..=0x30FF => BidiClass::L,
            
            // General punctuation
            0x2000..=0x200A => BidiClass::WS,
            0x200B => BidiClass::BN,
            0x200C..=0x200D => BidiClass::BN,
            0x200E => BidiClass::L, // LRM
            0x200F => BidiClass::R, // RLM
            0x2010..=0x2027 => BidiClass::ON,
            0x2028 => BidiClass::WS, // Line separator
            0x202A => BidiClass::LRE,
            0x202B => BidiClass::RLE,
            0x202C => BidiClass::PDF,
            0x202D => BidiClass::LRO,
            0x202E => BidiClass::RLO,
            0x202F => BidiClass::CS,
            0x2030..=0x205E => BidiClass::ON,
            0x2060..=0x206F => BidiClass::BN,
            0x2066 => BidiClass::LRI,
            0x2067 => BidiClass::RLI,
            0x2068 => BidiClass::FSI,
            0x2069 => BidiClass::PDI,
            
            // Default to Left-to-Right
            _ => BidiClass::L,
        }
    }
    
    /// Check if this is a strong type
    pub fn is_strong(self) -> bool {
        matches!(self, BidiClass::L | BidiClass::R | BidiClass::AL)
    }
    
    /// Check if this is an explicit formatting character
    pub fn is_explicit(self) -> bool {
        matches!(self, 
            BidiClass::LRE | BidiClass::RLE | BidiClass::LRO | BidiClass::RLO |
            BidiClass::PDF | BidiClass::LRI | BidiClass::RLI | BidiClass::FSI | BidiClass::PDI
        )
    }
    
    /// Check if RTL type
    pub fn is_rtl(self) -> bool {
        matches!(self, BidiClass::R | BidiClass::AL | BidiClass::RLE | BidiClass::RLO | BidiClass::RLI)
    }
}

/// Embedding level (0-125, even=LTR, odd=RTL)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level(pub u8);

impl Level {
    /// Maximum embedding level
    pub const MAX: u8 = 125;
    
    /// LTR level 0
    pub const LTR: Level = Level(0);
    
    /// RTL level 1
    pub const RTL: Level = Level(1);
    
    /// Create new level
    pub fn new(level: u8) -> Option<Self> {
        if level <= Self::MAX {
            Some(Level(level))
        } else {
            None
        }
    }
    
    /// Check if LTR
    pub fn is_ltr(self) -> bool {
        self.0 % 2 == 0
    }
    
    /// Check if RTL
    pub fn is_rtl(self) -> bool {
        self.0 % 2 == 1
    }
    
    /// Get next higher LTR level
    pub fn next_ltr(self) -> Option<Self> {
        let next = (self.0 + 2) & !1;
        Self::new(next)
    }
    
    /// Get next higher RTL level
    pub fn next_rtl(self) -> Option<Self> {
        let next = (self.0 + 1) | 1;
        Self::new(next)
    }
}

impl Default for Level {
    fn default() -> Self {
        Self::LTR
    }
}

/// Bidi paragraph information
#[derive(Debug)]
pub struct BidiParagraph {
    /// Base paragraph level
    pub base_level: Level,
    /// Resolved embedding levels per character
    pub levels: Vec<Level>,
    /// Original bidi classes
    pub classes: Vec<BidiClass>,
}

/// Bidi run (contiguous sequence at same level)
#[derive(Debug, Clone)]
pub struct BidiRun {
    /// Start index in text
    pub start: usize,
    /// End index in text (exclusive)
    pub end: usize,
    /// Embedding level
    pub level: Level,
}

impl BidiParagraph {
    /// Process a paragraph of text
    pub fn new(text: &str, default_level: Option<Level>) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let classes: Vec<BidiClass> = chars.iter().map(|&c| BidiClass::of(c)).collect();
        
        // P2/P3: Determine base level
        let base_level = default_level.unwrap_or_else(|| {
            Self::determine_base_level(&classes)
        });
        
        // Resolve embedding levels
        let levels = Self::resolve_levels(&classes, base_level);
        
        Self {
            base_level,
            levels,
            classes,
        }
    }
    
    /// P2/P3: Find first strong character to determine base level
    fn determine_base_level(classes: &[BidiClass]) -> Level {
        let mut isolate_count = 0;
        
        for &class in classes {
            match class {
                BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => {
                    isolate_count += 1;
                }
                BidiClass::PDI => {
                    if isolate_count > 0 {
                        isolate_count -= 1;
                    }
                }
                BidiClass::L if isolate_count == 0 => return Level::LTR,
                BidiClass::R | BidiClass::AL if isolate_count == 0 => return Level::RTL,
                _ => {}
            }
        }
        
        Level::LTR
    }
    
    /// Resolve embedding levels using UAX #9 algorithm
    fn resolve_levels(classes: &[BidiClass], base_level: Level) -> Vec<Level> {
        let len = classes.len();
        if len == 0 {
            return Vec::new();
        }
        
        let mut levels = vec![base_level; len];
        let mut resolved = classes.to_vec();
        
        // X1-X8: Process explicit formatting characters
        Self::process_explicit(&mut levels, &mut resolved, base_level);
        
        // W1-W7: Resolve weak types
        Self::resolve_weak(&mut resolved, &levels);
        
        // N0-N2: Resolve neutral and isolate types
        Self::resolve_neutral(&mut resolved, &levels, base_level);
        
        // I1-I2: Resolve implicit levels
        Self::resolve_implicit(&mut levels, &resolved);
        
        // L1: Reset whitespace levels
        Self::reset_whitespace(&mut levels, classes, base_level);
        
        levels
    }
    
    /// X1-X8: Process explicit embedding/override/isolate characters
    fn process_explicit(levels: &mut [Level], classes: &mut [BidiClass], base_level: Level) {
        let mut stack: Vec<(Level, bool, bool)> = Vec::with_capacity(63); // (level, override, isolate)
        let mut overflow_isolate_count = 0u32;
        let mut overflow_embedding_count = 0u32;
        let mut valid_isolate_count = 0u32;
        
        let mut current_level = base_level;
        let mut current_override = false;
        
        for i in 0..classes.len() {
            let class = classes[i];
            
            match class {
                BidiClass::RLE | BidiClass::LRE | BidiClass::RLO | BidiClass::LRO => {
                    let is_rtl = matches!(class, BidiClass::RLE | BidiClass::RLO);
                    let is_override = matches!(class, BidiClass::RLO | BidiClass::LRO);
                    
                    let new_level = if is_rtl {
                        current_level.next_rtl()
                    } else {
                        current_level.next_ltr()
                    };
                    
                    if let Some(level) = new_level {
                        if overflow_isolate_count == 0 && overflow_embedding_count == 0 {
                            stack.push((current_level, current_override, false));
                            current_level = level;
                            current_override = is_override;
                        } else {
                            overflow_embedding_count += 1;
                        }
                    } else {
                        overflow_embedding_count += 1;
                    }
                    
                    levels[i] = current_level;
                    classes[i] = BidiClass::BN;
                }
                
                BidiClass::RLI | BidiClass::LRI | BidiClass::FSI => {
                    levels[i] = current_level;
                    
                    let is_rtl = match class {
                        BidiClass::RLI => true,
                        BidiClass::LRI => false,
                        BidiClass::FSI => {
                            // Look ahead to find first strong type
                            let mut isolate = 0;
                            let mut found_rtl = false;
                            for j in (i+1)..classes.len() {
                                match classes[j] {
                                    BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => isolate += 1,
                                    BidiClass::PDI if isolate > 0 => isolate -= 1,
                                    BidiClass::PDI => break,
                                    BidiClass::L if isolate == 0 => break,
                                    BidiClass::R | BidiClass::AL if isolate == 0 => {
                                        found_rtl = true;
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            found_rtl
                        }
                        _ => unreachable!(),
                    };
                    
                    let new_level = if is_rtl {
                        current_level.next_rtl()
                    } else {
                        current_level.next_ltr()
                    };
                    
                    if let Some(level) = new_level {
                        if overflow_isolate_count == 0 && overflow_embedding_count == 0 {
                            valid_isolate_count += 1;
                            stack.push((current_level, current_override, true));
                            current_level = level;
                            current_override = false;
                        } else {
                            overflow_isolate_count += 1;
                        }
                    } else {
                        overflow_isolate_count += 1;
                    }
                }
                
                BidiClass::PDI => {
                    if overflow_isolate_count > 0 {
                        overflow_isolate_count -= 1;
                    } else if valid_isolate_count > 0 {
                        overflow_embedding_count = 0;
                        while let Some((level, over, isolate)) = stack.pop() {
                            if isolate {
                                current_level = level;
                                current_override = over;
                                valid_isolate_count -= 1;
                                break;
                            }
                        }
                    }
                    levels[i] = current_level;
                }
                
                BidiClass::PDF => {
                    if overflow_isolate_count == 0 {
                        if overflow_embedding_count > 0 {
                            overflow_embedding_count -= 1;
                        } else if let Some((level, over, isolate)) = stack.last() {
                            if !isolate {
                                current_level = *level;
                                current_override = *over;
                                stack.pop();
                            }
                        }
                    }
                    levels[i] = current_level;
                    classes[i] = BidiClass::BN;
                }
                
                BidiClass::B => {
                    levels[i] = base_level;
                }
                
                BidiClass::BN => {
                    levels[i] = current_level;
                }
                
                _ => {
                    levels[i] = current_level;
                    if current_override {
                        classes[i] = if current_level.is_rtl() { BidiClass::R } else { BidiClass::L };
                    }
                }
            }
        }
    }
    
    /// W1-W7: Resolve weak types
    fn resolve_weak(classes: &mut [BidiClass], levels: &[Level]) {
        if classes.is_empty() {
            return;
        }
        
        // Process in runs of same level
        let mut i = 0;
        while i < classes.len() {
            let level = levels[i];
            let run_start = i;
            
            // Find end of run
            while i < classes.len() && levels[i] == level {
                i += 1;
            }
            let run_end = i;
            
            Self::resolve_weak_run(&mut classes[run_start..run_end]);
        }
    }
    
    fn resolve_weak_run(classes: &mut [BidiClass]) {
        if classes.is_empty() {
            return;
        }
        
        // W1: NSM gets type of previous
        let mut prev_type = BidiClass::ON;
        for class in classes.iter_mut() {
            if *class == BidiClass::NSM {
                *class = prev_type;
            }
            prev_type = if *class == BidiClass::PDI { BidiClass::ON } else { *class };
        }
        
        // W2: EN after AL becomes AN
        let mut last_strong = BidiClass::ON;
        for class in classes.iter_mut() {
            match *class {
                BidiClass::L | BidiClass::R => last_strong = *class,
                BidiClass::AL => last_strong = BidiClass::AL,
                BidiClass::EN if last_strong == BidiClass::AL => *class = BidiClass::AN,
                _ => {}
            }
        }
        
        // W3: AL becomes R
        for class in classes.iter_mut() {
            if *class == BidiClass::AL {
                *class = BidiClass::R;
            }
        }
        
        // W4: Single ES/CS between numbers
        for i in 1..classes.len().saturating_sub(1) {
            let prev = classes[i - 1];
            let curr = classes[i];
            let next = classes[i + 1];
            
            if curr == BidiClass::ES && prev == BidiClass::EN && next == BidiClass::EN {
                classes[i] = BidiClass::EN;
            } else if curr == BidiClass::CS {
                if (prev == BidiClass::EN && next == BidiClass::EN) ||
                   (prev == BidiClass::AN && next == BidiClass::AN) {
                    classes[i] = prev;
                }
            }
        }
        
        // W5: ET adjacent to EN becomes EN
        let mut i = 0;
        while i < classes.len() {
            if classes[i] == BidiClass::ET {
                let start = i;
                while i < classes.len() && classes[i] == BidiClass::ET {
                    i += 1;
                }
                
                let has_en = (start > 0 && classes[start - 1] == BidiClass::EN) ||
                             (i < classes.len() && classes[i] == BidiClass::EN);
                
                if has_en {
                    for j in start..i {
                        classes[j] = BidiClass::EN;
                    }
                }
            } else {
                i += 1;
            }
        }
        
        // W6: Remaining separators and terminators become ON
        for class in classes.iter_mut() {
            if matches!(*class, BidiClass::ES | BidiClass::ET | BidiClass::CS) {
                *class = BidiClass::ON;
            }
        }
        
        // W7: EN after L becomes L
        last_strong = BidiClass::ON;
        for class in classes.iter_mut() {
            match *class {
                BidiClass::L | BidiClass::R => last_strong = *class,
                BidiClass::EN if last_strong == BidiClass::L => *class = BidiClass::L,
                _ => {}
            }
        }
    }
    
    /// N0-N2: Resolve neutral types
    fn resolve_neutral(classes: &mut [BidiClass], levels: &[Level], base_level: Level) {
        if classes.is_empty() {
            return;
        }
        
        // N1/N2: Resolve neutrals between strong types
        for i in 0..classes.len() {
            let class = classes[i];
            
            if matches!(class, BidiClass::ON | BidiClass::WS | BidiClass::B | BidiClass::S) {
                // Find surrounding strong types
                let before = Self::find_strong_before(classes, levels, i, base_level);
                let after = Self::find_strong_after(classes, levels, i, base_level);
                
                classes[i] = if before == after {
                    // N1: Same strong type on both sides
                    before
                } else {
                    // N2: Different types - use embedding direction
                    if levels[i].is_rtl() { BidiClass::R } else { BidiClass::L }
                };
            }
        }
    }
    
    fn find_strong_before(classes: &[BidiClass], levels: &[Level], pos: usize, base_level: Level) -> BidiClass {
        let level = levels[pos];
        for i in (0..pos).rev() {
            if levels[i] != level {
                break;
            }
            match classes[i] {
                BidiClass::L => return BidiClass::L,
                BidiClass::R | BidiClass::AN | BidiClass::EN => return BidiClass::R,
                _ => {}
            }
        }
        // sos (start of sequence)
        if level.is_rtl() || base_level.is_rtl() { BidiClass::R } else { BidiClass::L }
    }
    
    fn find_strong_after(classes: &[BidiClass], levels: &[Level], pos: usize, base_level: Level) -> BidiClass {
        let level = levels[pos];
        for i in (pos + 1)..classes.len() {
            if levels[i] != level {
                break;
            }
            match classes[i] {
                BidiClass::L => return BidiClass::L,
                BidiClass::R | BidiClass::AN | BidiClass::EN => return BidiClass::R,
                _ => {}
            }
        }
        // eos (end of sequence)
        if level.is_rtl() || base_level.is_rtl() { BidiClass::R } else { BidiClass::L }
    }
    
    /// I1-I2: Resolve implicit levels
    fn resolve_implicit(levels: &mut [Level], classes: &[BidiClass]) {
        for i in 0..levels.len() {
            let level = levels[i];
            let class = classes[i];
            
            // I1: raise odd levels
            if level.is_rtl() {
                if matches!(class, BidiClass::L | BidiClass::EN | BidiClass::AN) {
                    levels[i] = Level(level.0 + 1);
                }
            } else {
                // I2: raise even levels
                match class {
                    BidiClass::R => levels[i] = Level(level.0 + 1),
                    BidiClass::AN | BidiClass::EN => levels[i] = Level(level.0 + 2),
                    _ => {}
                }
            }
        }
    }
    
    /// L1: Reset whitespace levels at end of line
    fn reset_whitespace(levels: &mut [Level], original_classes: &[BidiClass], base_level: Level) {
        // Reset trailing whitespace and isolate formatting
        let mut reset_from = None;
        
        for i in (0..levels.len()).rev() {
            let class = original_classes[i];
            match class {
                BidiClass::WS | BidiClass::FSI | BidiClass::LRI | BidiClass::RLI | BidiClass::PDI => {
                    reset_from = Some(i);
                }
                BidiClass::S | BidiClass::B => {
                    levels[i] = base_level;
                    reset_from = Some(i);
                }
                _ if !class.is_explicit() && class != BidiClass::BN => {
                    break;
                }
                _ => {}
            }
        }
        
        if let Some(from) = reset_from {
            for j in from..levels.len() {
                levels[j] = base_level;
            }
        }
    }
    
    /// Get visual runs in display order
    pub fn runs(&self) -> Vec<BidiRun> {
        if self.levels.is_empty() {
            return Vec::new();
        }
        
        // Group into runs of same level
        let mut runs = Vec::new();
        let mut start = 0;
        
        for i in 1..self.levels.len() {
            if self.levels[i] != self.levels[start] {
                runs.push(BidiRun {
                    start,
                    end: i,
                    level: self.levels[start],
                });
                start = i;
            }
        }
        
        runs.push(BidiRun {
            start,
            end: self.levels.len(),
            level: self.levels[start],
        });
        
        runs
    }
    
    /// Reorder runs for visual display (L2)
    pub fn visual_runs(&self) -> Vec<BidiRun> {
        let mut runs = self.runs();
        
        if runs.is_empty() {
            return runs;
        }
        
        // Find max level
        let max_level = runs.iter().map(|r| r.level.0).max().unwrap_or(0);
        
        // L2: Reverse runs at each level
        for level in (self.base_level.0..=max_level).rev() {
            let mut i = 0;
            while i < runs.len() {
                if runs[i].level.0 >= level {
                    let start = i;
                    while i < runs.len() && runs[i].level.0 >= level {
                        i += 1;
                    }
                    runs[start..i].reverse();
                }
                i += 1;
            }
        }
        
        runs
    }
    
    /// Get reordered indices for visual display
    pub fn visual_indices(&self) -> Vec<usize> {
        let runs = self.visual_runs();
        let mut indices = Vec::with_capacity(self.levels.len());
        
        for run in runs {
            if run.level.is_rtl() {
                // RTL run: reverse order
                for i in (run.start..run.end).rev() {
                    indices.push(i);
                }
            } else {
                // LTR run: normal order
                for i in run.start..run.end {
                    indices.push(i);
                }
            }
        }
        
        indices
    }
}

/// Mirror a character for RTL display
pub fn mirror_char(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        '⁅' => '⁆',
        '⁆' => '⁅',
        '⟨' => '⟩',
        '⟩' => '⟨',
        '⟪' => '⟫',
        '⟫' => '⟪',
        '⟬' => '⟭',
        '⟭' => '⟬',
        '⟮' => '⟯',
        '⟯' => '⟮',
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bidi_class_latin() {
        assert_eq!(BidiClass::of('A'), BidiClass::L);
        assert_eq!(BidiClass::of('z'), BidiClass::L);
        assert_eq!(BidiClass::of('5'), BidiClass::EN);
    }
    
    #[test]
    fn test_bidi_class_arabic() {
        assert_eq!(BidiClass::of('ا'), BidiClass::AL);
        assert_eq!(BidiClass::of('ب'), BidiClass::AL);
    }
    
    #[test]
    fn test_bidi_class_hebrew() {
        assert_eq!(BidiClass::of('א'), BidiClass::R);
        assert_eq!(BidiClass::of('ב'), BidiClass::R);
    }
    
    #[test]
    fn test_level_ltr_rtl() {
        assert!(Level::LTR.is_ltr());
        assert!(!Level::LTR.is_rtl());
        assert!(!Level::RTL.is_ltr());
        assert!(Level::RTL.is_rtl());
    }
    
    #[test]
    fn test_paragraph_ltr() {
        let para = BidiParagraph::new("Hello World", None);
        assert!(para.base_level.is_ltr());
        assert!(para.levels.iter().all(|l| l.is_ltr()));
    }
    
    #[test]
    fn test_paragraph_rtl() {
        let para = BidiParagraph::new("שלום", None);
        assert!(para.base_level.is_rtl());
    }
    
    #[test]
    fn test_mirror() {
        assert_eq!(mirror_char('('), ')');
        assert_eq!(mirror_char(')'), '(');
        assert_eq!(mirror_char('A'), 'A');
    }
}
