//! Chrome Layout
//!
//! Main browser chrome combining all UI components.

use winit::event::MouseButton;
use crate::tab::TabManager;
use super::tab_bar::{TabBar, TabBarAction, TAB_BAR_WIDTH};
use super::url_bar::{UrlBar, UrlBarAction, URL_BAR_HEIGHT};

/// Content area background color
const CONTENT_BG: u32 = 0xFF0D0D0D;

/// Browser chrome
#[derive(Debug)]
pub struct Chrome {
    /// Tab bar (left)
    pub tab_bar: TabBar,
    /// URL bar (bottom)
    pub url_bar: UrlBar,
    /// Current mouse position
    mouse_x: i32,
    mouse_y: i32,
    /// Window dimensions
    width: u32,
    height: u32,
}

impl Chrome {
    pub fn new() -> Self {
        Self {
            tab_bar: TabBar::new(),
            url_bar: UrlBar::new(),
            mouse_x: 0,
            mouse_y: 0,
            width: 1024,
            height: 768,
        }
    }
    
    /// Render the entire chrome
    pub fn render(
        &mut self,
        buffer: &mut [u32],
        buffer_width: usize,
        buffer_height: usize,
        tabs: &TabManager,
    ) {
        self.width = buffer_width as u32;
        self.height = buffer_height as u32;
        
        // Calculate layout
        let tab_bar_width = TAB_BAR_WIDTH as usize;
        let url_bar_height = URL_BAR_HEIGHT as usize;
        let content_y_end = buffer_height.saturating_sub(url_bar_height);
        
        // Render content area (placeholder - would show page content)
        self.render_content(
            buffer,
            buffer_width,
            buffer_height,
            tab_bar_width,
            0,
            buffer_width - tab_bar_width,
            content_y_end,
            tabs,
        );
        
        // Render tab bar
        self.tab_bar.render(buffer, buffer_width, buffer_height, tabs);
        
        // Update URL bar state from active tab
        if let Some(tab) = tabs.active_tab() {
            self.url_bar.set_url(&tab.url);
            self.url_bar.loading = tab.loading;
        }
        
        // Render URL bar
        self.url_bar.render(
            buffer,
            buffer_width,
            buffer_height,
            content_y_end,
            tab_bar_width,
        );
    }
    
    /// Render content area
    fn render_content(
        &self,
        buffer: &mut [u32],
        buffer_width: usize,
        buffer_height: usize,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        tabs: &TabManager,
    ) {
        // Fill with background
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx;
                let py = y + dy;
                if px < buffer_width && py < buffer_height {
                    buffer[py * buffer_width + px] = CONTENT_BG;
                }
            }
        }
        
        // Show placeholder text if no content
        if let Some(tab) = tabs.active_tab() {
            if tab.page.is_none() {
                self.draw_centered_text(
                    buffer,
                    buffer_width,
                    buffer_height,
                    x + width / 2,
                    y + height / 2,
                    "Loading...",
                    0xFF808080,
                );
            }
            // TODO: Render actual page content here
        }
    }
    
    /// Draw centered text
    fn draw_centered_text(
        &self,
        buffer: &mut [u32],
        buffer_width: usize,
        buffer_height: usize,
        center_x: usize,
        center_y: usize,
        text: &str,
        color: u32,
    ) {
        let text_width = text.len() * 7;
        let start_x = center_x.saturating_sub(text_width / 2);
        let start_y = center_y.saturating_sub(4);
        
        let mut char_x = start_x as i32;
        for c in text.chars() {
            self.draw_char(buffer, buffer_width, buffer_height, char_x, start_y as i32, c, color);
            char_x += 7;
        }
    }
    
    /// Draw a character
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
        let pattern = get_basic_char(c);
        
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
    pub fn handle_mouse_move(&mut self, x: i32, y: i32) {
        self.mouse_x = x;
        self.mouse_y = y;
        
        // Update tab bar hover
        // Would need tabs reference - for now just track position
    }
    
    /// Handle click
    pub fn handle_click(&mut self, button: MouseButton, tabs: &mut TabManager) {
        if button != MouseButton::Left {
            return;
        }
        
        let x = self.mouse_x;
        let y = self.mouse_y;
        
        let url_bar_y = (self.height - URL_BAR_HEIGHT) as i32;
        
        // Check tab bar
        if x < TAB_BAR_WIDTH as i32 {
            if let Some(action) = self.tab_bar.handle_click(x, y, tabs) {
                match action {
                    TabBarAction::SelectTab(id) => {
                        tabs.set_active(id);
                    }
                    TabBarAction::CloseTab(id) => {
                        tabs.close_tab(id);
                    }
                    TabBarAction::NewTab => {
                        tabs.new_tab("about:blank");
                    }
                }
            }
            return;
        }
        
        // Check URL bar
        if let Some(action) = self.url_bar.handle_click(x, y, url_bar_y, TAB_BAR_WIDTH as i32) {
            match action {
                UrlBarAction::Back => {
                    // TODO: Implement navigation
                }
                UrlBarAction::Forward => {
                    // TODO: Implement navigation
                }
                UrlBarAction::Reload => {
                    tabs.reload_active();
                }
                UrlBarAction::Stop => {
                    // TODO: Stop loading
                }
                _ => {}
            }
        }
    }
    
    /// Focus URL bar for text input
    pub fn focus_url_bar(&mut self) {
        self.url_bar.focused = true;
    }
}

impl Default for Chrome {
    fn default() -> Self {
        Self::new()
    }
}

/// Get basic character pattern
fn get_basic_char(c: char) -> [u8; 8] {
    match c {
        'L' => [0b01000000, 0b01000000, 0b01000000, 0b01000000, 0b01000000, 0b01000000, 0b01111110, 0b00000000],
        'o' => [0b00000000, 0b00111100, 0b01000010, 0b01000010, 0b01000010, 0b00111100, 0b00000000, 0b00000000],
        'a' => [0b00000000, 0b00111100, 0b00000010, 0b00111110, 0b01000010, 0b00111110, 0b00000000, 0b00000000],
        'd' => [0b00000010, 0b00000010, 0b00111110, 0b01000010, 0b01000010, 0b00111110, 0b00000000, 0b00000000],
        'i' => [0b00010000, 0b00000000, 0b00110000, 0b00010000, 0b00010000, 0b00111000, 0b00000000, 0b00000000],
        'n' => [0b00000000, 0b01011100, 0b01100010, 0b01000010, 0b01000010, 0b01000010, 0b00000000, 0b00000000],
        'g' => [0b00000000, 0b00111110, 0b01000010, 0b01000010, 0b00111110, 0b00000010, 0b00111100, 0b00000000],
        '.' => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00011000, 0b00011000, 0b00000000],
        _ => [0b00111100, 0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b00111100, 0b00000000],
    }
}
