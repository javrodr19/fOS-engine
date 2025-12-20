//! Tab Bar Component
//!
//! Vertical tab bar on the left side.

use crate::tab::{TabId, TabManager};

/// Tab bar width in pixels
pub const TAB_BAR_WIDTH: u32 = 40;

/// Tab height
pub const TAB_HEIGHT: u32 = 36;

/// Colors (ARGB format)
pub mod colors {
    pub const BG: u32 = 0xFF1A1A1A;
    pub const TAB_ACTIVE: u32 = 0xFF2D2D2D;
    pub const TAB_HOVER: u32 = 0xFF252525;
    pub const TAB_INACTIVE: u32 = 0xFF1A1A1A;
    pub const TEXT: u32 = 0xFFE0E0E0;
    pub const TEXT_DIM: u32 = 0xFF808080;
    pub const ACCENT: u32 = 0xFF4A9EFF;
    pub const NEW_TAB_BTN: u32 = 0xFF3D3D3D;
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
            
            // Draw first letter of title as icon
            let first_char = tab.title.chars().next().unwrap_or('?');
            self.draw_char(
                buffer,
                buffer_width,
                buffer_height,
                (width / 2) as i32 - 4,
                y_offset + 10,
                first_char,
                colors::TEXT,
            );
            
            y_offset += TAB_HEIGHT as i32 + 2;
        }
        
        // Draw new tab button
        let new_tab_y = y_offset as usize + 8;
        if new_tab_y + 28 < buffer_height {
            let btn_color = if self.hovered_new_tab {
                colors::TAB_HOVER
            } else {
                colors::NEW_TAB_BTN
            };
            
            self.draw_rect(
                buffer,
                buffer_width,
                buffer_height,
                6,
                new_tab_y,
                width - 12,
                28,
                btn_color,
            );
            
            // Draw + symbol
            self.draw_char(
                buffer,
                buffer_width,
                buffer_height,
                (width / 2) as i32 - 4,
                new_tab_y as i32 + 6,
                '+',
                colors::TEXT_DIM,
            );
        }
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
        // Simple 8x8 bitmap font placeholder
        // In production, would use proper font rendering
        let pattern = get_char_pattern(c);
        
        for (row, &bits) in pattern.iter().enumerate() {
            for col in 0..8 {
                if (bits >> (7 - col)) & 1 == 1 {
                    let px = (x + col) as usize;
                    let py = (y + row as i32) as usize;
                    if px < buffer_width && py < buffer_height {
                        buffer[py * buffer_width + px] = color;
                    }
                }
            }
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

/// Get 8x8 bitmap pattern for a character
fn get_char_pattern(c: char) -> [u8; 8] {
    // Minimal bitmap font for common characters
    match c {
        '+' => [
            0b00000000,
            0b00010000,
            0b00010000,
            0b01111100,
            0b00010000,
            0b00010000,
            0b00000000,
            0b00000000,
        ],
        'A'..='Z' | 'a'..='z' => {
            // Simple letter representation
            let idx = c.to_ascii_uppercase() as u8 - b'A';
            [
                0b01111100,
                0b10000010,
                0b10000010,
                0b11111110,
                0b10000010,
                0b10000010,
                0b10000010,
                0b00000000,
            ]
        }
        '0'..='9' => [
            0b01111100,
            0b10000110,
            0b10001010,
            0b10010010,
            0b10100010,
            0b11000010,
            0b01111100,
            0b00000000,
        ],
        _ => [
            0b01111110,
            0b01000010,
            0b01000010,
            0b01000010,
            0b01000010,
            0b01000010,
            0b01111110,
            0b00000000,
        ],
    }
}
