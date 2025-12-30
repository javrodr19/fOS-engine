//! Line breaking (simplified UAX #14)

/// Line break opportunity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakOpportunity {
    /// No break allowed here
    NoBreak,
    /// Break allowed (soft break)
    AllowBreak,
    /// Must break (hard break - newline)
    MustBreak,
}

/// Line breaker (simplified UAX #14 implementation)
pub struct LineBreaker;

impl LineBreaker {
    /// Find break opportunities in text
    pub fn break_opportunities(text: &str) -> Vec<(usize, BreakOpportunity)> {
        let mut breaks = Vec::new();
        let mut chars = text.char_indices().peekable();
        
        while let Some((i, c)) = chars.next() {
            let opp = match c {
                // Hard breaks
                '\n' => BreakOpportunity::MustBreak,
                '\r' => {
                    // CRLF
                    if chars.peek().map(|(_, c)| *c) == Some('\n') {
                        chars.next();
                    }
                    BreakOpportunity::MustBreak
                }
                // Soft breaks after space
                ' ' | '\t' => BreakOpportunity::AllowBreak,
                // Soft breaks after CJK chars (simplified)
                c if is_cjk(c) => BreakOpportunity::AllowBreak,
                // Soft breaks after hyphens
                '-' => BreakOpportunity::AllowBreak,
                // No break by default
                _ => BreakOpportunity::NoBreak,
            };
            
            if opp != BreakOpportunity::NoBreak {
                breaks.push((i + c.len_utf8(), opp));
            }
        }
        
        breaks
    }
    
    /// Split text into lines that fit within max_width
    pub fn break_lines(
        text: &str,
        max_width: f32,
        mut measure_fn: impl FnMut(&str) -> f32,
    ) -> Vec<(usize, usize)> {
        if text.is_empty() {
            return Vec::new();
        }
        
        let breaks = Self::break_opportunities(text);
        let mut lines = Vec::new();
        let mut line_start = 0;
        let mut last_break = 0;
        
        for (break_pos, opp) in breaks {
            let segment = &text[line_start..break_pos];
            let width = measure_fn(segment);
            
            if width > max_width && last_break > line_start {
                // Line would be too long, break at last opportunity
                lines.push((line_start, last_break));
                line_start = last_break;
            }
            
            if opp == BreakOpportunity::MustBreak {
                lines.push((line_start, break_pos));
                line_start = break_pos;
            }
            
            last_break = break_pos;
        }
        
        // Final line
        if line_start < text.len() {
            lines.push((line_start, text.len()));
        }
        
        lines
    }
}

/// Check if character is CJK (simplified check)
fn is_cjk(c: char) -> bool {
    let code = c as u32;
    // CJK Unified Ideographs
    (0x4E00..=0x9FFF).contains(&code) ||
    // Hiragana
    (0x3040..=0x309F).contains(&code) ||
    // Katakana
    (0x30A0..=0x30FF).contains(&code) ||
    // Hangul Syllables
    (0xAC00..=0xD7AF).contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_break_opportunities() {
        let breaks = LineBreaker::break_opportunities("hello world");
        assert!(!breaks.is_empty());
        assert_eq!(breaks[0].0, 6); // After "hello "
    }
    
    #[test]
    fn test_hard_break() {
        let breaks = LineBreaker::break_opportunities("line1\nline2");
        assert!(breaks.iter().any(|(_, o)| *o == BreakOpportunity::MustBreak));
    }
}
