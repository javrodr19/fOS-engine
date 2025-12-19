//! Inline Layout
//!
//! Implements inline formatting context (IFC) layout algorithm.
//! Inline boxes flow horizontally and wrap to new lines.

use crate::{LayoutTree, LayoutBoxId, BoxType, BoxDimensions};
use crate::box_model::Rect;

/// Line box - a horizontal line containing inline boxes
#[derive(Debug, Clone)]
pub struct LineBox {
    /// X position of line start
    pub x: f32,
    /// Y position of line
    pub y: f32,
    /// Width of the line
    pub width: f32,
    /// Height of the line (max height of all inline boxes)
    pub height: f32,
    /// Baseline offset from top of line
    pub baseline: f32,
    /// Inline boxes on this line
    pub fragments: Vec<InlineFragment>,
}

/// A fragment of inline content on a line
#[derive(Debug, Clone)]
pub struct InlineFragment {
    /// The layout box this fragment belongs to
    pub box_id: LayoutBoxId,
    /// Position on the line
    pub x: f32,
    /// Width of this fragment
    pub width: f32,
    /// Height of this fragment  
    pub height: f32,
    /// Is this a text fragment?
    pub is_text: bool,
}

/// Inline formatting context
pub struct InlineFormattingContext {
    /// Available width for the line
    container_width: f32,
    /// Starting X position
    start_x: f32,
    /// Current Y position
    cursor_y: f32,
    /// Current X position within line
    cursor_x: f32,
    /// Current line being built
    current_line: LineBox,
    /// Completed lines
    lines: Vec<LineBox>,
    /// Default line height
    line_height: f32,
}

impl InlineFormattingContext {
    /// Create a new inline formatting context
    pub fn new(container_width: f32, start_x: f32, start_y: f32, line_height: f32) -> Self {
        Self {
            container_width,
            start_x,
            cursor_y: start_y,
            cursor_x: start_x,
            current_line: LineBox {
                x: start_x,
                y: start_y,
                width: 0.0,
                height: line_height,
                baseline: line_height * 0.8, // Approximate
                fragments: Vec::new(),
            },
            lines: Vec::new(),
            line_height,
        }
    }
    
    /// Add an inline box to the current line
    pub fn add_inline_box(
        &mut self,
        tree: &mut LayoutTree,
        box_id: LayoutBoxId,
        content_width: f32,
        content_height: f32,
    ) {
        let box_dims = tree.get(box_id)
            .map(|b| (b.dimensions.margin.horizontal() + 
                     b.dimensions.padding.horizontal() +
                     b.dimensions.border.horizontal(),
                     b.dimensions.margin.vertical() +
                     b.dimensions.padding.vertical() +
                     b.dimensions.border.vertical()))
            .unwrap_or((0.0, 0.0));
        
        let total_width = content_width + box_dims.0;
        let total_height = content_height + box_dims.1;
        
        // Check if we need to wrap to new line
        if self.cursor_x + total_width > self.start_x + self.container_width && 
           !self.current_line.fragments.is_empty() {
            self.finish_line();
        }
        
        // Add fragment to current line
        let fragment = InlineFragment {
            box_id,
            x: self.cursor_x,
            width: total_width,
            height: total_height,
            is_text: matches!(tree.get(box_id).map(|b| b.box_type), Some(BoxType::Text)),
        };
        
        self.current_line.fragments.push(fragment);
        self.current_line.height = self.current_line.height.max(total_height);
        self.cursor_x += total_width;
        self.current_line.width = self.cursor_x - self.start_x;
        
        // Update box dimensions
        if let Some(layout_box) = tree.get_mut(box_id) {
            layout_box.dimensions.content.width = content_width;
            layout_box.dimensions.content.height = content_height;
        }
    }
    
    /// Add a text run (may span multiple lines)
    pub fn add_text(
        &mut self,
        tree: &mut LayoutTree,
        box_id: LayoutBoxId,
        text: &str,
        char_width: f32, // Simplified: assume monospace
        char_height: f32,
    ) {
        let mut remaining = text;
        
        while !remaining.is_empty() {
            let available_width = self.container_width - (self.cursor_x - self.start_x);
            let max_chars = (available_width / char_width).floor() as usize;
            
            if max_chars == 0 && !self.current_line.fragments.is_empty() {
                self.finish_line();
                continue;
            }
            
            let chars_to_take = max_chars.min(remaining.chars().count()).max(1);
            let break_point = find_word_break(remaining, chars_to_take);
            
            let (chunk, rest) = remaining.split_at(
                remaining.char_indices()
                    .nth(break_point)
                    .map(|(i, _)| i)
                    .unwrap_or(remaining.len())
            );
            
            if !chunk.is_empty() {
                let width = chunk.chars().count() as f32 * char_width;
                let fragment = InlineFragment {
                    box_id,
                    x: self.cursor_x,
                    width,
                    height: char_height,
                    is_text: true,
                };
                
                self.current_line.fragments.push(fragment);
                self.current_line.height = self.current_line.height.max(char_height);
                self.cursor_x += width;
                self.current_line.width = self.cursor_x - self.start_x;
            }
            
            remaining = rest.trim_start();
            
            if !remaining.is_empty() && self.cursor_x >= self.start_x + self.container_width - char_width {
                self.finish_line();
            }
        }
    }
    
    /// Finish the current line and start a new one
    pub fn finish_line(&mut self) {
        if !self.current_line.fragments.is_empty() {
            self.lines.push(self.current_line.clone());
        }
        
        self.cursor_y += self.current_line.height;
        self.cursor_x = self.start_x;
        self.current_line = LineBox {
            x: self.start_x,
            y: self.cursor_y,
            width: 0.0,
            height: self.line_height,
            baseline: self.line_height * 0.8,
            fragments: Vec::new(),
        };
    }
    
    /// Finalize and return all lines
    pub fn finish(mut self) -> Vec<LineBox> {
        self.finish_line();
        self.lines
    }
    
    /// Get current Y position
    pub fn cursor_y(&self) -> f32 {
        self.cursor_y
    }
    
    /// Get total height of all lines
    pub fn total_height(&self) -> f32 {
        self.cursor_y - self.lines.first().map(|l| l.y).unwrap_or(self.cursor_y)
    }
}

/// Find a good word break point
fn find_word_break(text: &str, max_chars: usize) -> usize {
    if max_chars >= text.chars().count() {
        return text.chars().count();
    }
    
    // Look for last space within limit
    let chars: Vec<char> = text.chars().take(max_chars).collect();
    for i in (0..chars.len()).rev() {
        if chars[i].is_whitespace() {
            return i + 1;
        }
    }
    
    // No space found, break at max
    max_chars
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_inline_boxes() {
        let mut tree = LayoutTree::new();
        
        let root = tree.create_box(BoxType::Block, None);
        tree.set_root(root);
        
        let inline1 = tree.create_box(BoxType::Inline, None);
        let inline2 = tree.create_box(BoxType::Inline, None);
        
        let mut ifc = InlineFormattingContext::new(100.0, 0.0, 0.0, 16.0);
        
        ifc.add_inline_box(&mut tree, inline1, 30.0, 16.0);
        ifc.add_inline_box(&mut tree, inline2, 30.0, 16.0);
        
        let lines = ifc.finish();
        
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].fragments.len(), 2);
    }
    
    #[test]
    fn test_line_wrapping() {
        let mut tree = LayoutTree::new();
        
        let inline1 = tree.create_box(BoxType::Inline, None);
        let inline2 = tree.create_box(BoxType::Inline, None);
        let inline3 = tree.create_box(BoxType::Inline, None);
        
        let mut ifc = InlineFormattingContext::new(100.0, 0.0, 0.0, 16.0);
        
        ifc.add_inline_box(&mut tree, inline1, 60.0, 16.0);
        ifc.add_inline_box(&mut tree, inline2, 60.0, 16.0); // Should wrap
        ifc.add_inline_box(&mut tree, inline3, 30.0, 16.0);
        
        let lines = ifc.finish();
        
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].fragments.len(), 1);
        assert_eq!(lines[1].fragments.len(), 2);
    }
    
    #[test]
    fn test_word_break() {
        assert_eq!(find_word_break("hello world", 8), 6); // After "hello "
        assert_eq!(find_word_break("hello", 10), 5); // Whole word
        assert_eq!(find_word_break("abcdefghij", 5), 5); // No space, break at max
    }
}
