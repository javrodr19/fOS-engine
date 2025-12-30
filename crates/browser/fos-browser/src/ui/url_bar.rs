//! URL Bar Component
//!
//! Bottom URL bar with navigation buttons.

/// URL bar height in pixels
pub const URL_BAR_HEIGHT: u32 = 32;

/// Button size  
pub const BUTTON_SIZE: u32 = 28;

/// Colors (ARGB format)
pub mod colors {
    pub const BG: u32 = 0xFF1A1A1A;
    pub const INPUT_BG: u32 = 0xFF0D0D0D;
    pub const TEXT: u32 = 0xFFE0E0E0;
    pub const TEXT_DIM: u32 = 0xFF808080;
    pub const BUTTON_HOVER: u32 = 0xFF2D2D2D;
    pub const BUTTON_DISABLED: u32 = 0xFF404040;
    pub const LOADING: u32 = 0xFF4A9EFF;
}

/// URL bar state
#[derive(Debug)]
pub struct UrlBar {
    /// Current input text
    pub input: String,
    /// Is focused
    pub focused: bool,
    /// Cursor position
    pub cursor: usize,
    /// Hovered button
    hovered_button: Option<UrlBarButton>,
    /// Can go back
    pub can_back: bool,
    /// Can go forward  
    pub can_forward: bool,
    /// Is loading
    pub loading: bool,
    /// Loading progress (0.0 - 1.0)
    pub progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlBarButton {
    Back,
    Forward,
    Reload,
    Menu,
}

impl UrlBar {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            focused: false,
            cursor: 0,
            hovered_button: None,
            can_back: false,
            can_forward: false,
            loading: false,
            progress: 0.0,
        }
    }
    
    /// Set URL
    pub fn set_url(&mut self, url: &str) {
        if !self.focused {
            self.input = url.to_string();
            self.cursor = self.input.len();
        }
    }
    
    /// Handle character input
    pub fn handle_char(&mut self, c: char) {
        if self.focused {
            self.input.insert(self.cursor, c);
            self.cursor += 1;
        }
    }
    
    /// Handle backspace
    pub fn handle_backspace(&mut self) {
        if self.focused && self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
        }
    }
    
    /// Handle delete
    pub fn handle_delete(&mut self) {
        if self.focused && self.cursor < self.input.len() {
            self.input.remove(self.cursor);
        }
    }
    
    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }
    
    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += 1;
        }
    }
    
    /// Move cursor to start
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }
    
    /// Move cursor to end
    pub fn cursor_end(&mut self) {
        self.cursor = self.input.len();
    }
    
    /// Submit the URL (returns URL to navigate to)
    pub fn submit(&mut self) -> Option<String> {
        if self.focused && !self.input.is_empty() {
            self.focused = false;
            Some(self.input.clone())
        } else {
            None
        }
    }
    
    /// Start editing with current URL
    pub fn focus(&mut self) {
        self.focused = true;
        self.cursor = self.input.len();
    }
    
    /// Cancel editing
    pub fn unfocus(&mut self) {
        self.focused = false;
    }
    
    /// Render the URL bar - simplified, keyboard-only (no buttons)
    pub fn render(
        &self,
        buffer: &mut [u32],
        buffer_width: usize,
        buffer_height: usize,
        y_start: usize,
        x_start: usize,
    ) {
        let height = URL_BAR_HEIGHT as usize;
        let width = buffer_width - x_start;
        
        // Fill background with dark teal to match tab bar
        let bg_color = 0xFF1A3A3A; // Dark teal
        for dy in 0..height {
            for dx in 0..width {
                let px = x_start + dx;
                let py = y_start + dy;
                if px < buffer_width && py < buffer_height {
                    buffer[py * buffer_width + px] = bg_color;
                }
            }
        }
        
        // Draw URL text - full width, no buttons
        let text_color = if self.focused { 0xFFFFFFFF } else { 0xFFC0C0C0 };
        
        // Calculate how many chars can fit
        let available_width = width.saturating_sub(16);
        let max_chars = available_width / 7;
        
        let display_url: String = if self.input.len() > max_chars {
            format!("{}…", &self.input[..max_chars.saturating_sub(1)])
        } else {
            self.input.clone()
        };
        
        self.draw_text(
            buffer, buffer_width, buffer_height,
            x_start + 8, y_start + height / 2 - 4,
            &display_url,
            text_color,
        );
        
        // Draw cursor if focused
        if self.focused {
            let visible_cursor = self.cursor.min(max_chars);
            let cursor_x = x_start + 8 + visible_cursor * 7;
            for dy in 4..height - 4 {
                let py = y_start + dy;
                if cursor_x < buffer_width && py < buffer_height {
                    buffer[py * buffer_width + cursor_x] = 0xFFFFFFFF;
                }
            }
        }
        
        // Loading indicator - subtle line at top
        if self.loading {
            let progress_width = ((width as f32) * self.progress.max(0.3)) as usize;
            for dx in 0..progress_width {
                let px = x_start + dx;
                let py = y_start;
                if px < buffer_width && py < buffer_height {
                    buffer[py * buffer_width + px] = colors::LOADING;
                }
            }
        }
    }
    
    /// Draw a button
    #[allow(dead_code)]
    fn draw_button(
        &self,
        buffer: &mut [u32],
        buffer_width: usize,
        buffer_height: usize,
        x: usize,
        y: usize,
        icon: char,
        enabled: bool,
        hovered: bool,
    ) {
        let size = BUTTON_SIZE as usize;
        
        // Background
        let bg_color = if hovered && enabled {
            colors::BUTTON_HOVER
        } else {
            colors::BG
        };
        
        for dy in 0..size {
            for dx in 0..size {
                let px = x + dx;
                let py = y + dy;
                if px < buffer_width && py < buffer_height {
                    buffer[py * buffer_width + px] = bg_color;
                }
            }
        }
        
        // Icon
        let icon_color = if enabled { colors::TEXT } else { colors::BUTTON_DISABLED };
        self.draw_char(
            buffer, buffer_width, buffer_height,
            x as i32 + 10, y as i32 + 8,
            icon, icon_color,
        );
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
    
    /// Draw text
    fn draw_text(
        &self,
        buffer: &mut [u32],
        buffer_width: usize,
        buffer_height: usize,
        x: usize,
        y: usize,
        text: &str,
        color: u32,
    ) {
        let mut char_x = x as i32;
        
        for c in text.chars() {
            if char_x as usize + 8 > buffer_width {
                break;
            }
            self.draw_char(buffer, buffer_width, buffer_height, char_x, y as i32, c, color);
            char_x += 7; // Character width + spacing
        }
    }
    
    /// Handle mouse move
    pub fn handle_mouse_move(&mut self, x: i32, y: i32, url_bar_y: i32, tab_bar_width: i32) {
        self.hovered_button = None;
        
        if y < url_bar_y || y >= url_bar_y + URL_BAR_HEIGHT as i32 {
            return;
        }
        
        let local_x = x - tab_bar_width;
        
        // Check buttons
        if local_x >= 4 && local_x < 4 + BUTTON_SIZE as i32 {
            self.hovered_button = Some(UrlBarButton::Back);
        } else if local_x >= 4 + BUTTON_SIZE as i32 + 2 && local_x < 4 + 2 * BUTTON_SIZE as i32 + 2 {
            self.hovered_button = Some(UrlBarButton::Forward);
        } else if local_x >= 4 + 2 * (BUTTON_SIZE as i32 + 2) && local_x < 4 + 3 * BUTTON_SIZE as i32 + 4 {
            self.hovered_button = Some(UrlBarButton::Reload);
        }
    }
    
    /// Handle click, return action
    pub fn handle_click(&mut self, x: i32, y: i32, url_bar_y: i32, tab_bar_width: i32, total_width: i32) -> Option<UrlBarAction> {
        if y < url_bar_y || y >= url_bar_y + URL_BAR_HEIGHT as i32 {
            self.focused = false;
            return None;
        }
        
        let local_x = x - tab_bar_width;
        
        // Calculate input field bounds
        let input_start = 4 + 3 * (BUTTON_SIZE as i32 + 2) + 4;
        let input_end = total_width - tab_bar_width - BUTTON_SIZE as i32 - 12;
        
        if local_x >= 4 && local_x < 4 + BUTTON_SIZE as i32 {
            self.focused = false;
            if self.can_back {
                return Some(UrlBarAction::Back);
            }
        } else if local_x >= 4 + BUTTON_SIZE as i32 + 2 && local_x < 4 + 2 * BUTTON_SIZE as i32 + 2 {
            self.focused = false;
            if self.can_forward {
                return Some(UrlBarAction::Forward);
            }
        } else if local_x >= 4 + 2 * (BUTTON_SIZE as i32 + 2) && local_x < 4 + 3 * BUTTON_SIZE as i32 + 4 {
            self.focused = false;
            if self.loading {
                return Some(UrlBarAction::Stop);
            } else {
                return Some(UrlBarAction::Reload);
            }
        } else if local_x >= input_start && local_x < input_end {
            // Clicked in input field
            self.focus();
            // Approximate cursor position from click
            let char_offset = ((local_x - input_start - 8) / 7).max(0) as usize;
            self.cursor = char_offset.min(self.input.len());
            return Some(UrlBarAction::Focus);
        } else {
            self.focused = false;
        }
        
        None
    }
}

impl Default for UrlBar {
    fn default() -> Self {
        Self::new()
    }
}

/// URL bar actions
#[derive(Debug, Clone, Copy)]
pub enum UrlBarAction {
    Back,
    Forward,
    Reload,
    Stop,
    Navigate,
    Focus,
    Menu,
}

