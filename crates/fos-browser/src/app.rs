//! Browser Application
//!
//! Main browser window and event loop.

use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::loader::Loader;
use crate::renderer::{PageRenderer, RenderedPage};
use crate::tab::TabManager;
use crate::ui::Chrome;
use crate::ui::tab_bar::TAB_BAR_WIDTH;
use crate::ui::url_bar::URL_BAR_HEIGHT;

/// Browser application
pub struct Browser {
    /// Initial URL to load
    initial_url: String,
}

impl Browser {
    /// Create a new browser instance
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            initial_url: String::new(),
        })
    }
    
    /// Run the browser with an initial URL
    pub fn run(mut self, initial_url: String) -> Result<(), Box<dyn Error>> {
        self.initial_url = initial_url;
        
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Wait);
        
        let mut app = BrowserApp::new(self.initial_url.clone());
        event_loop.run_app(&mut app)?;
        
        Ok(())
    }
}

/// Browser app state for event loop
struct BrowserApp {
    /// Window handle
    window: Option<Arc<Window>>,
    /// Surface for rendering
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    /// Tab manager
    tabs: TabManager,
    /// UI chrome
    chrome: Chrome,
    /// Page loader
    loader: Loader,
    /// Page renderer
    renderer: PageRenderer,
    /// Current rendered page (cached)
    rendered_page: Option<RenderedPage>,
    /// Initial URL
    initial_url: String,
    /// Window dimensions
    width: u32,
    height: u32,
    /// Current modifier state
    modifiers: winit::keyboard::ModifiersState,
    /// Needs page reload
    needs_reload: bool,
}

impl BrowserApp {
    fn new(initial_url: String) -> Self {
        Self {
            window: None,
            surface: None,
            tabs: TabManager::new(),
            chrome: Chrome::new(),
            loader: Loader::new(),
            renderer: PageRenderer::new(800, 600),
            rendered_page: None,
            initial_url,
            width: 1024,
            height: 768,
            modifiers: winit::keyboard::ModifiersState::default(),
            needs_reload: true,
        }
    }
    
    /// Load the current tab's page
    fn load_current_page(&mut self) {
        let url = match self.tabs.active_tab() {
            Some(tab) => tab.url.clone(),
            None => return,
        };
        
        log::info!("Loading: {}", url);
        
        // Load the page
        match self.loader.load_sync(&url) {
            Ok(page) => {
                // Update tab title
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.title = page.title.clone().unwrap_or_else(|| url.clone());
                    tab.loading = false;
                }
                
                // Calculate content area size
                let content_width = self.width.saturating_sub(TAB_BAR_WIDTH);
                let content_height = self.height.saturating_sub(URL_BAR_HEIGHT);
                
                // Update renderer viewport
                self.renderer.set_viewport(content_width, content_height);
                
                // Render the page
                log::info!("Rendering {} bytes of HTML...", page.html.len());
                self.rendered_page = self.renderer.render_html(&page.html, &url);
                
                if let Some(ref rendered) = self.rendered_page {
                    log::info!("Rendered: {}x{} pixels", rendered.width, rendered.height);
                }
            }
            Err(e) => {
                log::error!("Failed to load {}: {}", url, e);
                // Show error page
                let error_html = format!(r#"
                    <!DOCTYPE html>
                    <html>
                    <head><title>Error</title></head>
                    <body style="background: #1a1a1a; color: #ff6b6b; padding: 20px; font-family: sans-serif;">
                        <h1>Failed to load page</h1>
                        <p>URL: {}</p>
                        <p>Error: {}</p>
                    </body>
                    </html>
                "#, url, e);
                
                let content_width = self.width.saturating_sub(TAB_BAR_WIDTH);
                let content_height = self.height.saturating_sub(URL_BAR_HEIGHT);
                self.renderer.set_viewport(content_width, content_height);
                self.rendered_page = self.renderer.render_html(&error_html, &url);
            }
        }
        
        self.needs_reload = false;
    }
    
    /// Render the browser UI and content
    fn render(&mut self) {
        // Load page if needed
        if self.needs_reload {
            self.load_current_page();
        }
        
        let Some(window) = &self.window else { return };
        
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        
        // Check if viewport changed
        if size.width != self.width || size.height != self.height {
            self.width = size.width;
            self.height = size.height;
            self.needs_reload = true;
            self.load_current_page();
        }
        
        let Some(surface) = &mut self.surface else { return };
        
        // Resize surface if needed
        let _ = surface.resize(
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );
        
        // Get buffer
        let mut buffer = match surface.buffer_mut() {
            Ok(b) => b,
            Err(_) => return,
        };
        
        let buffer_width = size.width as usize;
        let buffer_height = size.height as usize;
        
        // Clear to background color
        let bg_color = 0xFF0D0D0D; // Dark gray ARGB (softbuffer uses 0xAARRGGBB)
        buffer.fill(bg_color);
        
        // Render page content in content area (inline to avoid borrow issues)
        let content_x = TAB_BAR_WIDTH as usize;
        let content_height = buffer_height.saturating_sub(URL_BAR_HEIGHT as usize);
        
        if let Some(ref rendered) = self.rendered_page {
            // Log first time we render
            static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !LOGGED.load(std::sync::atomic::Ordering::Relaxed) {
                LOGGED.store(true, std::sync::atomic::Ordering::Relaxed);
                log::info!("Copying {}x{} pixels to buffer at x={}", rendered.width, rendered.height, content_x);
                // Check first few pixels
                if rendered.pixels.len() >= 4 {
                    log::info!("First pixel RGBA: {},{},{},{}", 
                        rendered.pixels[0], rendered.pixels[1], rendered.pixels[2], rendered.pixels[3]);
                }
            }
            
            // Copy rendered pixels to buffer
            // rendered.pixels is RGBA bytes, buffer is ARGB u32
            for y in 0..rendered.height.min(content_height as u32) {
                for x in 0..rendered.width.min((buffer_width - content_x) as u32) {
                    let src_idx = ((y * rendered.width + x) * 4) as usize;
                    let dst_x = content_x + x as usize;
                    let dst_y = y as usize;
                    
                    if src_idx + 3 < rendered.pixels.len() && dst_y < buffer_height && dst_x < buffer_width {
                        let r = rendered.pixels[src_idx] as u32;
                        let g = rendered.pixels[src_idx + 1] as u32;
                        let b = rendered.pixels[src_idx + 2] as u32;
                        let a = rendered.pixels[src_idx + 3] as u32;
                        
                        // Convert RGBA to ARGB (softbuffer format: 0xAARRGGBB)
                        let pixel = (a << 24) | (r << 16) | (g << 8) | b;
                        buffer[dst_y * buffer_width + dst_x] = pixel;
                    }
                }
            }
        } else {
            // No rendered page - draw a placeholder rectangle
            let placeholder_color = 0xFF1A3A5A; // Dark blue
            for y in 50..150.min(content_height) {
                for x in content_x..content_x + 200 {
                    if x < buffer_width && y < buffer_height {
                        buffer[y * buffer_width + x] = placeholder_color;
                    }
                }
            }
        }
        
        // Render UI chrome on top
        self.chrome.render(
            &mut buffer,
            buffer_width,
            buffer_height,
            &self.tabs,
        );
        
        // Present
        let _ = buffer.present();
    }
    
    /// Handle keyboard input
    fn handle_key(&mut self, event: KeyEvent, modifiers: &winit::keyboard::ModifiersState) {
        if event.state != ElementState::Pressed {
            return;
        }
        
        let ctrl = modifiers.control_key();
        
        // If URL bar is focused, handle text input
        if self.chrome.is_url_bar_focused() {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Enter) => {
                    if let Some(url) = self.chrome.handle_enter() {
                        self.navigate_to(&url);
                    }
                    self.request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.chrome.handle_escape();
                    self.request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::Backspace) => {
                    self.chrome.handle_backspace();
                    self.request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::Delete) => {
                    self.chrome.handle_delete();
                    self.request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::ArrowLeft) => {
                    self.chrome.handle_left();
                    self.request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::ArrowRight) => {
                    self.chrome.handle_right();
                    self.request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::Home) => {
                    self.chrome.handle_home();
                    self.request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::End) => {
                    self.chrome.handle_end();
                    self.request_redraw();
                    return;
                }
                _ => {
                    // Handle text input
                    if let Some(text) = &event.text {
                        for c in text.chars() {
                            if c.is_ascii_graphic() || c == ' ' {
                                self.chrome.handle_char(c);
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                }
            }
        }
        
        // Global shortcuts
        match event.physical_key {
            PhysicalKey::Code(KeyCode::KeyT) if ctrl => {
                // Ctrl+T: New tab
                self.tabs.new_tab("about:blank");
                self.needs_reload = true;
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::KeyW) if ctrl => {
                // Ctrl+W: Close tab
                self.tabs.close_active_tab();
                self.needs_reload = true;
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::KeyL) if ctrl => {
                // Ctrl+L: Focus URL bar
                self.chrome.focus_url_bar();
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::KeyR) if ctrl => {
                // Ctrl+R: Reload
                self.needs_reload = true;
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::F5) => {
                // F5: Reload
                self.needs_reload = true;
                self.request_redraw();
            }
            _ => {}
        }
    }
    
    /// Navigate to a URL
    fn navigate_to(&mut self, url: &str) {
        // Normalize URL
        let normalized = if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("about:") {
            url.to_string()
        } else if url.contains('.') {
            format!("https://{}", url)
        } else {
            // Treat as search query (could be made configurable)
            format!("https://duckduckgo.com/?q={}", url.replace(' ', "+"))
        };
        
        // Update active tab URL
        if let Some(tab) = self.tabs.active_tab_mut() {
            tab.url = normalized;
            tab.loading = true;
        }
        
        self.needs_reload = true;
        self.request_redraw();
    }
    
    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl ApplicationHandler for BrowserApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        
        // Create window
        let attrs = Window::default_attributes()
            .with_title("fOS Browser")
            .with_inner_size(winit::dpi::LogicalSize::new(1024, 768));
        
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        
        // Create software rendering surface
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        
        self.window = Some(window);
        self.surface = Some(surface);
        
        // Create initial tab
        if !self.initial_url.is_empty() {
            self.tabs.new_tab(&self.initial_url);
        } else {
            self.tabs.new_tab("about:blank");
        }
        
        self.needs_reload = true;
        self.request_redraw();
    }
    
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            WindowEvent::Resized(_) => {
                self.request_redraw();
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let modifiers = self.modifiers;
                self.handle_key(event, &modifiers);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if state == ElementState::Pressed {
                    // Handle click
                    if let Some(url) = self.chrome.handle_click(button, &mut self.tabs) {
                        self.navigate_to(&url);
                    }
                    self.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.chrome.handle_mouse_move(position.x as i32, position.y as i32);
            }
            _ => {}
        }
    }
}
