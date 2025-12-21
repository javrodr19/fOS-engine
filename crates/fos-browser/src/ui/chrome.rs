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
    
    /// Render content area placeholder (only when loading)
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
        // Only show loading placeholder if actively loading
        // The actual page content is rendered by the app, not the chrome
        if let Some(tab) = tabs.active_tab() {
            if tab.loading {
                // Fill with background only when loading
                for dy in 0..height {
                    for dx in 0..width {
                        let px = x + dx;
                        let py = y + dy;
                        if px < buffer_width && py < buffer_height {
                            buffer[py * buffer_width + px] = CONTENT_BG;
                        }
                    }
                }
                
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
        super::font::draw_char(buffer, buffer_width, buffer_height, x, y, c, color);
    }
    
    /// Handle mouse move
    pub fn handle_mouse_move(&mut self, x: i32, y: i32) {
        self.mouse_x = x;
        self.mouse_y = y;
        
        // Update tab bar hover
        // Would need tabs reference - for now just track position
    }
    
    /// Handle click - returns URL to navigate to if any
    pub fn handle_click(&mut self, button: MouseButton, tabs: &mut TabManager) -> Option<String> {
        if button != MouseButton::Left {
            return None;
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
            return None;
        }
        
        // Check URL bar
        if let Some(action) = self.url_bar.handle_click(x, y, url_bar_y, TAB_BAR_WIDTH as i32, self.width as i32) {
            match action {
                UrlBarAction::Back => {
                    // TODO: Implement navigation
                }
                UrlBarAction::Forward => {
                    // TODO: Implement navigation
                }
                UrlBarAction::Reload => {
                    if let Some(tab) = tabs.active_tab() {
                        return Some(tab.url.clone());
                    }
                }
                UrlBarAction::Stop => {
                    // TODO: Stop loading
                }
                UrlBarAction::Focus => {
                    // URL bar is now focused, no navigation
                }
                _ => {}
            }
        }
        
        None
    }
    
    /// Check if URL bar is focused
    pub fn is_url_bar_focused(&self) -> bool {
        self.url_bar.focused
    }
    
    /// Handle character input to URL bar
    pub fn handle_char(&mut self, c: char) {
        self.url_bar.handle_char(c);
    }
    
    /// Handle backspace in URL bar
    pub fn handle_backspace(&mut self) {
        self.url_bar.handle_backspace();
    }
    
    /// Handle delete in URL bar
    pub fn handle_delete(&mut self) {
        self.url_bar.handle_delete();
    }
    
    /// Handle arrow keys in URL bar
    pub fn handle_left(&mut self) {
        self.url_bar.cursor_left();
    }
    
    pub fn handle_right(&mut self) {
        self.url_bar.cursor_right();
    }
    
    pub fn handle_home(&mut self) {
        self.url_bar.cursor_home();
    }
    
    pub fn handle_end(&mut self) {
        self.url_bar.cursor_end();
    }
    
    /// Handle Enter key - returns URL to navigate to
    pub fn handle_enter(&mut self) -> Option<String> {
        self.url_bar.submit()
    }
    
    /// Handle Escape key
    pub fn handle_escape(&mut self) {
        self.url_bar.unfocus();
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

