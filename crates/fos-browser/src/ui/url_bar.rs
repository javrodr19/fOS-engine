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
            hovered_button: None,
            can_back: false,
            can_forward: false,
            loading: false,
            progress: 0.0,
        }
    }
    
    /// Set URL
    pub fn set_url(&mut self, url: &str) {
        self.input = url.to_string();
    }
    
    /// Render the URL bar
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
        
        // Fill background
        for dy in 0..height {
            for dx in 0..width {
                let px = x_start + dx;
                let py = y_start + dy;
                if px < buffer_width && py < buffer_height {
                    buffer[py * buffer_width + px] = colors::BG;
                }
            }
        }
        
        // Draw buttons
        let mut x_offset = x_start + 4;
        
        // Back button
        self.draw_button(
            buffer, buffer_width, buffer_height,
            x_offset, y_start + 2,
            '←',
            self.can_back,
            self.hovered_button == Some(UrlBarButton::Back),
        );
        x_offset += BUTTON_SIZE as usize + 2;
        
        // Forward button
        self.draw_button(
            buffer, buffer_width, buffer_height,
            x_offset, y_start + 2,
            '→',
            self.can_forward,
            self.hovered_button == Some(UrlBarButton::Forward),
        );
        x_offset += BUTTON_SIZE as usize + 2;
        
        // Reload button
        self.draw_button(
            buffer, buffer_width, buffer_height,
            x_offset, y_start + 2,
            if self.loading { '×' } else { '↻' },
            true,
            self.hovered_button == Some(UrlBarButton::Reload),
        );
        x_offset += BUTTON_SIZE as usize + 8;
        
        // URL input field
        let input_x = x_offset;
        let input_width = width - (x_offset - x_start) - BUTTON_SIZE as usize - 12;
        
        // Input background
        for dy in 2..height - 2 {
            for dx in 0..input_width {
                let px = input_x + dx;
                let py = y_start + dy;
                if px < buffer_width && py < buffer_height {
                    buffer[py * buffer_width + px] = colors::INPUT_BG;
                }
            }
        }
        
        // Loading progress bar
        if self.loading && self.progress > 0.0 {
            let progress_width = ((input_width as f32) * self.progress) as usize;
            for dx in 0..progress_width {
                let px = input_x + dx;
                let py = y_start + height - 3;
                if px < buffer_width && py < buffer_height {
                    buffer[py * buffer_width + px] = colors::LOADING;
                }
            }
        }
        
        // Draw URL text
        let display_url = if self.input.len() > 80 {
            format!("{}...", &self.input[..77])
        } else {
            self.input.clone()
        };
        
        self.draw_text(
            buffer, buffer_width, buffer_height,
            input_x + 8, y_start + 10,
            &display_url,
            colors::TEXT,
        );
        
        // Menu button
        let menu_x = buffer_width - BUTTON_SIZE as usize - 4;
        self.draw_button(
            buffer, buffer_width, buffer_height,
            menu_x, y_start + 2,
            '≡',
            true,
            self.hovered_button == Some(UrlBarButton::Menu),
        );
    }
    
    /// Draw a button
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
        // Simple representation - in production use proper font
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
    pub fn handle_click(&self, x: i32, y: i32, url_bar_y: i32, tab_bar_width: i32) -> Option<UrlBarAction> {
        if y < url_bar_y || y >= url_bar_y + URL_BAR_HEIGHT as i32 {
            return None;
        }
        
        let local_x = x - tab_bar_width;
        
        if local_x >= 4 && local_x < 4 + BUTTON_SIZE as i32 {
            if self.can_back {
                return Some(UrlBarAction::Back);
            }
        } else if local_x >= 4 + BUTTON_SIZE as i32 + 2 && local_x < 4 + 2 * BUTTON_SIZE as i32 + 2 {
            if self.can_forward {
                return Some(UrlBarAction::Forward);
            }
        } else if local_x >= 4 + 2 * (BUTTON_SIZE as i32 + 2) && local_x < 4 + 3 * BUTTON_SIZE as i32 + 4 {
            if self.loading {
                return Some(UrlBarAction::Stop);
            } else {
                return Some(UrlBarAction::Reload);
            }
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
    Navigate(/* url would be passed separately */),
    Menu,
}

/// Get 8x8 bitmap pattern for a character
fn get_char_pattern(c: char) -> [u8; 8] {
    match c {
        '←' => [
            0b00000000,
            0b00010000,
            0b00110000,
            0b01111111,
            0b00110000,
            0b00010000,
            0b00000000,
            0b00000000,
        ],
        '→' => [
            0b00000000,
            0b00001000,
            0b00001100,
            0b11111110,
            0b00001100,
            0b00001000,
            0b00000000,
            0b00000000,
        ],
        '↻' => [
            0b00111100,
            0b01000010,
            0b00000010,
            0b00011110,
            0b00000010,
            0b01000010,
            0b00111100,
            0b00000000,
        ],
        '×' => [
            0b00000000,
            0b01000010,
            0b00100100,
            0b00011000,
            0b00100100,
            0b01000010,
            0b00000000,
            0b00000000,
        ],
        '≡' => [
            0b00000000,
            0b01111110,
            0b00000000,
            0b01111110,
            0b00000000,
            0b01111110,
            0b00000000,
            0b00000000,
        ],
        '/' => [
            0b00000010,
            0b00000100,
            0b00001000,
            0b00010000,
            0b00100000,
            0b01000000,
            0b10000000,
            0b00000000,
        ],
        ':' => [
            0b00000000,
            0b00011000,
            0b00011000,
            0b00000000,
            0b00011000,
            0b00011000,
            0b00000000,
            0b00000000,
        ],
        '.' => [
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00011000,
            0b00011000,
            0b00000000,
        ],
        '-' => [
            0b00000000,
            0b00000000,
            0b00000000,
            0b01111110,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
        ],
        '_' => [
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b01111110,
            0b00000000,
        ],
        'a'..='z' | 'A'..='Z' => [
            0b00111100,
            0b01000010,
            0b01000010,
            0b01111110,
            0b01000010,
            0b01000010,
            0b01000010,
            0b00000000,
        ],
        '0'..='9' => [
            0b00111100,
            0b01000010,
            0b01000010,
            0b01000010,
            0b01000010,
            0b01000010,
            0b00111100,
            0b00000000,
        ],
        _ => [0; 8],
    }
}
