//! Tab Bar Component
//!
//! Vertical tab bar on the left side - keyboard-only, no buttons.

use crate::tab::{TabId, TabManager};

/// Tab bar width in pixels (wider to show tab titles)
pub const TAB_BAR_WIDTH: u32 = 120;

/// Tab height
pub const TAB_HEIGHT: u32 = 24;

/// Colors (ARGB format) - Teal/green theme
pub mod colors {
    pub const BG: u32 = 0xFF1A3A3A;           // Dark teal background
    pub const TAB_ACTIVE: u32 = 0xFF2A5A5A;   // Lighter teal for active
    pub const TAB_HOVER: u32 = 0xFF254A4A;    // Medium teal for hover
    pub const TAB_INACTIVE: u32 = 0xFF1A3A3A; // Same as bg
    pub const TEXT: u32 = 0xFFE0E0E0;         // White text
    pub const TEXT_DIM: u32 = 0xFF80A0A0;     // Dimmed teal text
    pub const ACCENT: u32 = 0xFF40C0C0;       // Bright teal accent
}

/// Tab bar state
#[derive(Debug)]
pub struct TabBar {
    /// Hovered tab (if any)
    hovered_tab: Option<TabId>,
    /// Hover over new tab button
    hovered_new_tab: bool,
    /// Scroll offset
    scroll_offset: i32,
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            hovered_tab: None,
            hovered_new_tab: false,
            scroll_offset: 0,
        }
    }
    
    /// Render the tab bar
    pub fn render(
        &self,
        buffer: &mut [u32],
        buffer_width: usize,
        buffer_height: usize,
        tabs: &TabManager,
    ) {
        let width = TAB_BAR_WIDTH as usize;
        
        // Fill background
        for y in 0..buffer_height {
            for x in 0..width {
                if x < buffer_width && y < buffer_height {
                    buffer[y * buffer_width + x] = colors::BG;
                }
            }
        }
        
        // Draw tabs
        let mut y_offset = 4i32 - self.scroll_offset;
        
        for tab in tabs.tabs_in_order() {
            let is_active = tabs.is_active(tab.id);
            let is_hovered = self.hovered_tab == Some(tab.id);
            
            let bg_color = if is_active {
                colors::TAB_ACTIVE
            } else if is_hovered {
                colors::TAB_HOVER
            } else {
                colors::TAB_INACTIVE
            };
            
            // Draw tab background
            self.draw_rect(
                buffer,
                buffer_width,
                buffer_height,
                2,
                y_offset as usize,
                width - 4,
                TAB_HEIGHT as usize,
                bg_color,
            );
            
            // Draw active indicator
            if is_active {
                self.draw_rect(
                    buffer,
                    buffer_width,
                    buffer_height,
                    0,
                    y_offset as usize,
                    2,
                    TAB_HEIGHT as usize,
                    colors::ACCENT,
                );
            }
            
            // Draw tab title (truncated to fit)
            let title = &tab.title;
            let max_chars = ((TAB_BAR_WIDTH - 10) / 7) as usize; // ~7px per char
            let display_title: String = if title.len() > max_chars {
                format!("{}…", &title[..max_chars.saturating_sub(1)])
            } else {
                title.clone()
            };
            
            self.draw_text(
                buffer,
                buffer_width,
                buffer_height,
                5,
                y_offset + 6,
                &display_title,
                colors::TEXT,
            );
            
            y_offset += TAB_HEIGHT as i32 + 1;
        }
        // No new tab button - keyboard only (Ctrl+T)
    }
    
    /// Draw a filled rectangle
    fn draw_rect(
        &self,
        buffer: &mut [u32],
        buffer_width: usize,
        buffer_height: usize,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        color: u32,
    ) {
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx;
                let py = y + dy;
                if px < buffer_width && py < buffer_height {
                    buffer[py * buffer_width + px] = color;
                }
            }
        }
    }
    
    /// Draw a character (simple bitmap font)
    fn draw_char(
        &self,
        buffer: &mut [u32],
        buffer_width: usize,
        buffer_height: usize,
        x: i32,
        y: i32,
        c: char,
        color: u32,
    ) {
        super::font::draw_char(buffer, buffer_width, buffer_height, x, y, c, color);
    }
    
    /// Draw text string
    fn draw_text(
        &self,
        buffer: &mut [u32],
        buffer_width: usize,
        buffer_height: usize,
        x: i32,
        y: i32,
        text: &str,
        color: u32,
    ) {
        let mut x_pos = x;
        for c in text.chars() {
            self.draw_char(buffer, buffer_width, buffer_height, x_pos, y, c, color);
            x_pos += 7; // Character width + spacing
        }
    }
    
    /// Handle mouse move
    pub fn handle_mouse_move(&mut self, x: i32, y: i32, tabs: &TabManager) {
        self.hovered_tab = None;
        self.hovered_new_tab = false;
        
        if x < 0 || x >= TAB_BAR_WIDTH as i32 {
            return;
        }
        
        let mut y_offset = 4i32 - self.scroll_offset;
        
        for tab in tabs.tabs_in_order() {
            if y >= y_offset && y < y_offset + TAB_HEIGHT as i32 {
                self.hovered_tab = Some(tab.id);
                return;
            }
            y_offset += TAB_HEIGHT as i32 + 2;
        }
        
        // Check new tab button
        if y >= y_offset + 8 && y < y_offset + 8 + 28 {
            self.hovered_new_tab = true;
        }
    }
    
    /// Handle click, return action
    pub fn handle_click(&self, x: i32, y: i32, tabs: &TabManager) -> Option<TabBarAction> {
        if x < 0 || x >= TAB_BAR_WIDTH as i32 {
            return None;
        }
        
        let mut y_offset = 4i32 - self.scroll_offset;
        
        for tab in tabs.tabs_in_order() {
            if y >= y_offset && y < y_offset + TAB_HEIGHT as i32 {
                return Some(TabBarAction::SelectTab(tab.id));
            }
            y_offset += TAB_HEIGHT as i32 + 2;
        }
        
        // Check new tab button
        if y >= y_offset + 8 && y < y_offset + 8 + 28 {
            return Some(TabBarAction::NewTab);
        }
        
        None
    }
}

impl Default for TabBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Tab bar actions
#[derive(Debug, Clone, Copy)]
pub enum TabBarAction {
    SelectTab(TabId),
    CloseTab(TabId),
    NewTab,
}

