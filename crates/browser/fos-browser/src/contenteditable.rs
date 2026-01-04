//! ContentEditable Support
//!
//! Rich text editing with execCommand implementation.

use std::collections::HashMap;

/// Editing command
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditCommand {
    Bold, Italic, Underline, StrikeThrough,
    Subscript, Superscript,
    JustifyLeft, JustifyCenter, JustifyRight, JustifyFull,
    InsertOrderedList, InsertUnorderedList,
    Indent, Outdent,
    CreateLink, Unlink,
    InsertImage, InsertHorizontalRule,
    FormatBlock, RemoveFormat,
    Copy, Cut, Paste, Delete,
    Undo, Redo,
    SelectAll,
    FontName, FontSize, ForeColor, BackColor,
    InsertText, InsertHTML, InsertParagraph, InsertLineBreak,
}

impl EditCommand {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.to_lowercase().as_str() {
            "bold" => Self::Bold, "italic" => Self::Italic, "underline" => Self::Underline,
            "strikethrough" => Self::StrikeThrough, "subscript" => Self::Subscript,
            "superscript" => Self::Superscript, "justifyleft" => Self::JustifyLeft,
            "justifycenter" => Self::JustifyCenter, "justifyright" => Self::JustifyRight,
            "justifyfull" => Self::JustifyFull, "insertorderedlist" => Self::InsertOrderedList,
            "insertunorderedlist" => Self::InsertUnorderedList, "indent" => Self::Indent,
            "outdent" => Self::Outdent, "createlink" => Self::CreateLink, "unlink" => Self::Unlink,
            "insertimage" => Self::InsertImage, "inserthorizontalrule" => Self::InsertHorizontalRule,
            "formatblock" => Self::FormatBlock, "removeformat" => Self::RemoveFormat,
            "copy" => Self::Copy, "cut" => Self::Cut, "paste" => Self::Paste, "delete" => Self::Delete,
            "undo" => Self::Undo, "redo" => Self::Redo, "selectall" => Self::SelectAll,
            "fontname" => Self::FontName, "fontsize" => Self::FontSize,
            "forecolor" => Self::ForeColor, "backcolor" => Self::BackColor,
            "inserttext" => Self::InsertText, "inserthtml" => Self::InsertHTML,
            "insertparagraph" => Self::InsertParagraph, "insertlinebreak" => Self::InsertLineBreak,
            _ => return None,
        })
    }
    
    pub fn requires_value(&self) -> bool {
        matches!(self, Self::CreateLink | Self::InsertImage | Self::FormatBlock |
                 Self::FontName | Self::FontSize | Self::ForeColor | Self::BackColor |
                 Self::InsertText | Self::InsertHTML)
    }
}

/// Selection in editable content
#[derive(Debug, Clone, Default)]
pub struct EditSelection {
    pub start_node: u64,
    pub start_offset: usize,
    pub end_node: u64,
    pub end_offset: usize,
    pub collapsed: bool,
}

impl EditSelection {
    pub fn collapsed_at(node: u64, offset: usize) -> Self {
        Self { start_node: node, start_offset: offset, end_node: node, end_offset: offset, collapsed: true }
    }
    
    pub fn collapse_to_start(&mut self) {
        self.end_node = self.start_node;
        self.end_offset = self.start_offset;
        self.collapsed = true;
    }
}

/// Undo/Redo entry
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub html_before: String,
    pub html_after: String,
    pub selection_before: EditSelection,
    pub selection_after: EditSelection,
}

/// ContentEditable editor
#[derive(Debug)]
pub struct ContentEditor {
    pub element_id: u64,
    pub editable: bool,
    pub selection: EditSelection,
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
    max_undo: usize,
    command_states: HashMap<EditCommand, bool>,
}

impl ContentEditor {
    pub fn new(element_id: u64) -> Self {
        Self { element_id, editable: true, selection: EditSelection::default(),
               undo_stack: Vec::new(), redo_stack: Vec::new(), max_undo: 100,
               command_states: HashMap::new() }
    }
    
    pub fn exec_command(&mut self, command: EditCommand, value: Option<&str>) -> bool {
        if !self.editable { return false; }
        
        match command {
            EditCommand::Undo => self.undo(),
            EditCommand::Redo => self.redo(),
            EditCommand::Bold | EditCommand::Italic | EditCommand::Underline => {
                self.toggle_format(command);
                true
            }
            _ => {
                // Other commands would modify DOM
                true
            }
        }
    }
    
    pub fn query_command_state(&self, command: EditCommand) -> bool {
        self.command_states.get(&command).copied().unwrap_or(false)
    }
    
    pub fn query_command_enabled(&self, command: EditCommand) -> bool {
        if !self.editable { return false; }
        match command {
            EditCommand::Undo => !self.undo_stack.is_empty(),
            EditCommand::Redo => !self.redo_stack.is_empty(),
            _ => true,
        }
    }
    
    fn toggle_format(&mut self, command: EditCommand) {
        let current = self.command_states.get(&command).copied().unwrap_or(false);
        self.command_states.insert(command, !current);
    }
    
    pub fn push_undo(&mut self, entry: UndoEntry) {
        self.undo_stack.push(entry);
        if self.undo_stack.len() > self.max_undo {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }
    
    fn undo(&mut self) -> bool {
        if let Some(mut entry) = self.undo_stack.pop() {
            std::mem::swap(&mut entry.html_before, &mut entry.html_after);
            self.selection = entry.selection_before.clone();
            self.redo_stack.push(entry);
            true
        } else { false }
    }
    
    fn redo(&mut self) -> bool {
        if let Some(mut entry) = self.redo_stack.pop() {
            std::mem::swap(&mut entry.html_before, &mut entry.html_after);
            self.selection = entry.selection_after.clone();
            self.undo_stack.push(entry);
            true
        } else { false }
    }
    
    pub fn can_undo(&self) -> bool { !self.undo_stack.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo_stack.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_command_parse() {
        assert_eq!(EditCommand::parse("bold"), Some(EditCommand::Bold));
        assert_eq!(EditCommand::parse("ITALIC"), Some(EditCommand::Italic));
    }
    
    #[test]
    fn test_editor() {
        let mut editor = ContentEditor::new(1);
        assert!(editor.exec_command(EditCommand::Bold, None));
        assert!(editor.query_command_state(EditCommand::Bold));
    }
}
