//! Browser Application
//!
//! Main browser window and event loop.

use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
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
use crate::network::NetworkManager;
use crate::page::Page;
use crate::devtools::DevTools;
use crate::accessibility::AccessibilityManager;
use crate::media::MediaManager;
use crate::canvas::CanvasManager;
use crate::advanced_net::AdvancedNetworking;
use crate::security::SecurityManager;
use crate::memory::MemoryIntegration;

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
    /// Scroll offset (vertical)
    scroll_offset: f32,
    /// Y position where rendered buffer starts in document (for sliding window)
    render_start_y: f32,
    /// Current page HTML (for scroll re-rendering)
    current_html: String,
    /// Current page URL
    current_url: String,
    /// Background render result receiver (for async scroll rendering)
    bg_render_rx: Option<Receiver<(RenderedPage, f32)>>,
    /// Pending render start Y (waiting for background render to complete)
    pending_render_start: Option<f32>,
    /// Mouse position
    mouse_x: i32,
    mouse_y: i32,
    /// Resize is pending (for debouncing)
    resize_pending: bool,
    /// Network manager with HTTP cache
    network: NetworkManager,
    /// Current page with JavaScript runtime
    current_page: Option<Page>,
    /// Last timer check time
    last_timer_check: std::time::Instant,
    /// Developer tools
    devtools: DevTools,
    /// Accessibility manager
    a11y: AccessibilityManager,
    /// Media manager
    media: MediaManager,
    /// Canvas manager
    canvas: CanvasManager,
    /// Advanced networking (WebSocket, XHR, SSE)
    _advanced_net: AdvancedNetworking,
    /// Security manager (CSP, sandbox, privacy)
    _security: SecurityManager,
    /// Memory integration (pressure, hibernation)
    _memory: MemoryIntegration,
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
            scroll_offset: 0.0,
            render_start_y: 0.0,
            current_html: String::new(),
            current_url: String::new(),
            bg_render_rx: None,
            pending_render_start: None,
            mouse_x: 0,
            mouse_y: 0,
            resize_pending: false,
            network: NetworkManager::new(),
            current_page: None,
            last_timer_check: std::time::Instant::now(),
            devtools: DevTools::new(),
            a11y: AccessibilityManager::new(),
            media: MediaManager::new(),
            canvas: CanvasManager::new(),
            _advanced_net: AdvancedNetworking::new(),
            _security: SecurityManager::new(),
            _memory: MemoryIntegration::new(),
        }
    }
    
    /// Load the current tab's page
    fn load_current_page(&mut self) {
        // Get tab info
        let (url, needs_network, cached_html) = match self.tabs.active_tab() {
            Some(tab) => (tab.url.clone(), tab.needs_network_load, tab.cached_html.clone()),
            None => return,
        };
        
        log::info!("Loading: {} (network: {})", url, needs_network);
        
        // If we have cached HTML and don't need network, just re-render
        if let Some(ref html) = cached_html {
            if !needs_network {
                log::info!("Using cached HTML ({} bytes)", html.len());
                // Reset scroll only if URL changed (not resize)
                let reset_scroll = self.current_url != url;
                self.render_page(html, &url, reset_scroll);
                self.needs_reload = false;
                return;
            }
        }
        
        // Try network cache first, then fetch
        // Log to DevTools network panel
        let request_id = self.devtools.log_request(&url, "GET");
        let fetch_result = self.network.fetch_html(&url);
        
        match fetch_result {
            Ok(html) => {
                // Log successful response
                self.devtools.log_response(request_id, 200, "OK");
                
                // Create page with JavaScript runtime
                let mut page = Page::from_html(&url, html.clone());
                
                // Update tab with loaded content and cache the HTML
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.title = page.title.clone().unwrap_or_else(|| url.clone());
                    tab.loading = false;
                    tab.cached_html = Some(html.clone());
                    tab.needs_network_load = false;
                }
                
                // Initialize JavaScript (if scripts exist)
                if let Err(e) = page.initialize_javascript() {
                    log::warn!("Failed to initialize JavaScript: {}", e);
                    self.devtools.warn(&format!("JS init failed: {}", e));
                }
                
                // Store the page
                self.current_page = Some(page);
                
                // Reset scroll for new page loads
                self.render_page(&html, &url, true);
                
                // Execute scripts after initial render
                if let Some(ref mut page) = self.current_page {
                    if let Err(e) = page.execute_scripts() {
                        log::warn!("Failed to execute scripts: {}", e);
                        self.devtools.error(&format!("Script error: {}", e));
                    }
                    
                    // Build accessibility tree and extract media/canvas from DOM
                    if let Some(doc) = page.document() {
                        let doc_guard = doc.lock().unwrap();
                        
                        // Accessibility tree
                        self.a11y.build_from_document(&doc_guard);
                        let a11y_stats = self.a11y.stats();
                        log::info!("Built a11y tree: {} focusable elements, {} links", 
                            a11y_stats.focusable_count, a11y_stats.link_count);
                        
                        // Media elements
                        self.media.extract_from_document(&doc_guard);
                        let media_stats = self.media.stats();
                        if media_stats.video_count > 0 || media_stats.audio_count > 0 {
                            log::info!("Found media: {} videos, {} audios", 
                                media_stats.video_count, media_stats.audio_count);
                        }
                        
                        // Canvas elements
                        self.canvas.extract_from_document(&doc_guard);
                        let canvas_stats = self.canvas.stats();
                        if canvas_stats.canvas_count > 0 {
                            log::info!("Found {} canvas elements ({} total pixels)", 
                                canvas_stats.canvas_count, canvas_stats.total_pixels);
                        }
                    }
                }
            }
            Err(e) => {
                // Log failed request
                self.devtools.log_network_error(request_id, &e.to_string());
                // Network cache failed, try loader as fallback
                log::warn!("Network fetch failed, trying loader: {}", e);
                match self.loader.load_sync(&url) {
                    Ok(page) => {
                        // Update tab with loaded content and cache the HTML
                        if let Some(tab) = self.tabs.active_tab_mut() {
                            tab.title = page.title.clone().unwrap_or_else(|| url.clone());
                            tab.loading = false;
                            tab.cached_html = Some(page.html.clone());
                            tab.needs_network_load = false;
                        }
                        
                        // Reset scroll for new page loads
                        self.render_page(&page.html, &url, true);
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
                        self.rendered_page = self.renderer.render_html(&error_html, &url, 0.0);
                    }
                }
            }
        }
        
        self.needs_reload = false;
    }
    
    /// Render a page from HTML (helper for caching)
    /// If reset_scroll is false, keeps current scroll position (for resize)
    fn render_page(&mut self, html: &str, url: &str, reset_scroll: bool) {
        let content_width = self.width.saturating_sub(TAB_BAR_WIDTH);
        let content_height = self.height.saturating_sub(URL_BAR_HEIGHT);
        
        // Render to a buffer for scrolling (5x viewport height)
        // Links will be re-captured during scroll when re-rendering is triggered
        let render_height = content_height * 5;
        self.renderer.set_viewport(content_width, render_height);
        
        log::info!("Rendering {} bytes of HTML...", html.len());
        self.current_html = html.to_string();
        self.current_url = url.to_string();
        
        if reset_scroll {
            self.scroll_offset = 0.0;
            self.render_start_y = 0.0;
        }
        
        self.rendered_page = self.renderer.render_html(html, url, self.render_start_y);
        
        if let Some(ref rendered) = self.rendered_page {
            log::info!("Rendered: {}x{} pixels", rendered.width, rendered.height);
        }
    }
    
    /// Process JavaScript timers (call periodically)
    fn process_js_timers(&mut self) {
        // Check every 16ms (60fps)
        if self.last_timer_check.elapsed() < std::time::Duration::from_millis(16) {
            return;
        }
        self.last_timer_check = std::time::Instant::now();
        
        if let Some(ref mut page) = self.current_page {
            if page.has_pending_timers() {
                if let Err(e) = page.process_timers() {
                    log::warn!("Timer processing error: {}", e);
                }
                // Request redraw if timers ran (DOM might have changed)
                self.request_redraw();
            }
        }
    }
    
    /// Render the browser UI and content
    fn render(&mut self) {
        // Check for completed background render
        if let Some(ref rx) = self.bg_render_rx {
            match rx.try_recv() {
                Ok((rendered, new_start)) => {
                    // Background render completed - swap in new buffer
                    self.rendered_page = Some(rendered);
                    self.render_start_y = new_start;
                    self.pending_render_start = None;
                    self.bg_render_rx = None;
                }
                Err(TryRecvError::Empty) => {
                    // Still rendering - keep current buffer
                }
                Err(TryRecvError::Disconnected) => {
                    // Thread died - clear pending state
                    self.pending_render_start = None;
                    self.bg_render_rx = None;
                }
            }
        }
        
        // Load page if needed
        if self.needs_reload {
            self.load_current_page();
        }
        
        let Some(window) = &self.window else { return };
        
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        
        // Check if viewport changed - debounce by marking pending instead of immediate re-render
        if size.width != self.width || size.height != self.height {
            self.width = size.width;
            self.height = size.height;
            self.resize_pending = true;
            // Don't re-render immediately - wait for resize to stabilize
        }
        
        // If resize is pending and no background render is active, do the re-render now
        // This allows rapid resizes to batch together
        if self.resize_pending && self.bg_render_rx.is_none() {
            self.resize_pending = false;
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
            }
            
            // Apply scroll offset when copying pixels (much more efficient than re-rendering)
            // scroll_offset is in document coordinates, render_start_y is where buffer starts
            let scroll_y = (self.scroll_offset - self.render_start_y).max(0.0) as u32;
            
            // Copy rendered pixels to buffer with scroll offset
            // rendered.pixels is RGBA bytes, buffer is ARGB u32
            for y in 0..content_height.min(buffer_height) as u32 {
                // Source y includes scroll offset relative to rendered buffer
                let src_y = y + scroll_y;
                
                if src_y >= rendered.height {
                    // Past end of content - fill with white
                    for x in 0..rendered.width.min((buffer_width - content_x) as u32) {
                        let dst_x = content_x + x as usize;
                        let dst_y = y as usize;
                        if dst_y < buffer_height && dst_x < buffer_width {
                            buffer[dst_y * buffer_width + dst_x] = 0xFFFFFFFF; // White
                        }
                    }
                    continue;
                }
                
                for x in 0..rendered.width.min((buffer_width - content_x) as u32) {
                    let src_idx = ((src_y * rendered.width + x) * 4) as usize;
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
        
        // Global shortcuts (keyboard-only UI)
        match event.physical_key {
            // Tab management
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
            PhysicalKey::Code(KeyCode::KeyO) if ctrl => {
                // Ctrl+O: Go to tab above (previous)
                self.tabs.select_previous_tab();
                self.needs_reload = true;
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::KeyL) if ctrl => {
                // Ctrl+L: Go to tab below (next)
                self.tabs.select_next_tab();
                self.needs_reload = true;
                self.request_redraw();
            }
            
            // URL bar
            PhysicalKey::Code(KeyCode::KeyI) if ctrl => {
                // Ctrl+I: Focus URL bar
                self.chrome.focus_url_bar();
                self.request_redraw();
            }
            
            // Navigation (history)
            PhysicalKey::Code(KeyCode::KeyK) if ctrl => {
                // Ctrl+K: Go back in history
                if let Some(tab) = self.tabs.active_tab_mut() {
                    if tab.go_back().is_some() {
                        self.needs_reload = true;
                    }
                }
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::Semicolon) if ctrl => {
                // Ctrl+; (Ñ on Spanish keyboard): Go forward in history
                if let Some(tab) = self.tabs.active_tab_mut() {
                    if tab.go_forward().is_some() {
                        self.needs_reload = true;
                    }
                }
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::BracketLeft) if ctrl => {
                // Ctrl+[: Alternative go back
                if let Some(tab) = self.tabs.active_tab_mut() {
                    if tab.go_back().is_some() {
                        self.needs_reload = true;
                    }
                }
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::BracketRight) if ctrl => {
                // Ctrl+]: Alternative go forward
                if let Some(tab) = self.tabs.active_tab_mut() {
                    if tab.go_forward().is_some() {
                        self.needs_reload = true;
                    }
                }
                self.request_redraw();
            }
            
            // Page actions
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
            PhysicalKey::Code(KeyCode::F12) => {
                // F12: Toggle DevTools
                self.devtools.toggle();
                if self.devtools.is_open() {
                    // Log to console when opening
                    self.devtools.log("DevTools opened");
                    // Inspect current page DOM
                    if let Some(ref page) = self.current_page {
                        if let Some(doc) = page.document() {
                            let doc_guard = doc.lock().unwrap();
                            self.devtools.inspect_document(&doc_guard);
                        }
                    }
                }
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::Escape) => {
                // Escape: Unfocus URL bar / stop loading
                self.chrome.url_bar.unfocus();
                self.request_redraw();
            }
            
            // Scrolling (when URL bar not focused)
            PhysicalKey::Code(KeyCode::ArrowDown) => {
                self.scroll_offset += 40.0;
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::ArrowUp) => {
                self.scroll_offset = (self.scroll_offset - 40.0).max(0.0);
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::PageDown) => {
                let viewport_height = self.height.saturating_sub(URL_BAR_HEIGHT) as f32;
                self.scroll_offset += viewport_height * 0.9;
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::PageUp) => {
                let viewport_height = self.height.saturating_sub(URL_BAR_HEIGHT) as f32;
                self.scroll_offset = (self.scroll_offset - viewport_height * 0.9).max(0.0);
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::Home) if ctrl => {
                // Ctrl+Home: Go to top of page
                self.scroll_offset = 0.0;
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::End) if ctrl => {
                // Ctrl+End: Go to bottom of page
                if let Some(ref rendered) = self.rendered_page {
                    let viewport_height = self.height.saturating_sub(URL_BAR_HEIGHT) as f32;
                    self.scroll_offset = (rendered.content_height - viewport_height).max(0.0);
                }
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::Space) => {
                // Space: Scroll down (when not in URL bar)
                self.scroll_offset += 200.0;
                self.request_redraw();
            }
            
            // Accessibility: Tab navigation
            PhysicalKey::Code(KeyCode::Tab) => {
                if modifiers.shift_key() {
                    // Shift+Tab: Focus previous element
                    if let Some(_id) = self.a11y.focus_prev() {
                        log::debug!("Focused previous element");
                        self.request_redraw();
                    }
                } else {
                    // Tab: Focus next element
                    if let Some(_id) = self.a11y.focus_next() {
                        log::debug!("Focused next element");
                        self.request_redraw();
                    }
                }
            }
            PhysicalKey::Code(KeyCode::Enter) if !self.chrome.is_url_bar_focused() => {
                // Enter: Activate focused link
                if let Some(url) = self.a11y.get_focused_link_url().map(String::from) {
                    self.navigate_to(&url);
                }
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
        
        // Update URL bar to show the URL we're navigating to
        self.chrome.url_bar.set_url(&normalized);
        
        // Use tab.navigate() to properly set needs_network_load and record history
        if let Some(tab) = self.tabs.active_tab_mut() {
            tab.navigate(&normalized);
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
                if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
                    // First check chrome (tabs, url bar)
                    if let Some(url) = self.chrome.handle_click(button, &mut self.tabs) {
                        self.navigate_to(&url);
                    } else {
                        // Check for link clicks in content area
                        // Content starts after tab bar
                        let content_x = self.mouse_x - TAB_BAR_WIDTH as i32;
                        let content_y = self.mouse_y;
                        
                        if content_x >= 0 && content_y >= 0 {
                            // Links are stored in render buffer coordinates
                            // Display applies scroll_y shift (scroll_offset - render_start_y)
                            // To match, convert screen y to buffer y by adding the scroll shift
                            let scroll_y = (self.scroll_offset - self.render_start_y).max(0.0);
                            let hit_x = content_x as f32;
                            let hit_y = content_y as f32 + scroll_y;  // Screen to buffer coords
                            
                            // Check link regions
                            if let Some(ref rendered) = self.rendered_page {
                                for link in &rendered.links {
                                    if hit_x >= link.x && hit_x <= link.x + link.width &&
                                       hit_y >= link.y && hit_y <= link.y + link.height {
                                        // Found a link click!
                                        let href = link.href.clone();
                                        
                                        // Handle anchor links (in-page navigation)
                                        if href.starts_with("#") {
                                            let anchor_id = &href[1..]; // Remove # prefix
                                            if let Some(ref rendered) = self.rendered_page {
                                                // Find anchor with matching ID
                                                for anchor in &rendered.anchors {
                                                    if anchor.id == anchor_id {
                                                        log::info!("Scrolling to anchor: #{}", anchor_id);
                                                        // Scroll to anchor position (with small margin at top)
                                                        self.scroll_offset = (anchor.y + self.render_start_y - 10.0).max(0.0);
                                                        self.request_redraw();
                                                        break;
                                                    }
                                                }
                                            }
                                            break;
                                        }
                                        
                                        // Handle relative URLs
                                        let full_url = if href.starts_with("http://") || href.starts_with("https://") {
                                            href
                                        } else if href.starts_with("/") {
                                            // Absolute path - prepend origin
                                            if let Ok(base) = fos_engine::url::Url::parse(&self.current_url) {
                                                format!("{}://{}{}", base.scheme(), base.host_str().unwrap_or(""), href)
                                            } else {
                                                href
                                            }
                                        } else {
                                            // Relative path
                                            if let Ok(base) = fos_engine::url::Url::parse(&self.current_url) {
                                                if let Ok(joined) = base.join(&href) {
                                                    joined.to_string()
                                                } else {
                                                    href
                                                }
                                            } else {
                                                href
                                            }
                                        };
                                        
                                        log::info!("Navigating to: {}", full_url);
                                        self.navigate_to(&full_url);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    self.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_x = position.x as i32;
                self.mouse_y = position.y as i32;
                self.chrome.handle_mouse_move(self.mouse_x, self.mouse_y);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Handle scroll with sliding window buffer
                let scroll_amount = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 40.0,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };
                self.scroll_offset = (self.scroll_offset - scroll_amount).max(0.0);
                
                // Sliding window: check if scroll is outside the rendered buffer
                // Buffer covers [render_start_y, render_start_y + buffer_height]
                // If scroll goes outside, re-center the buffer
                if let Some(ref rendered) = self.rendered_page {
                    let viewport_height = self.height.saturating_sub(URL_BAR_HEIGHT) as f32;
                    let buffer_height = rendered.height as f32;
                    
                    // Calculate position relative to rendered buffer
                    let scroll_in_buffer = self.scroll_offset - self.render_start_y;
                    // Trigger re-render when scrolling past 60% of buffer to capture links ahead of time
                    let buffer_threshold = buffer_height * 0.4; // 40% remaining = 60% scrolled
                    
                    let needs_recenter = 
                        // Scrolling up past buffer start (with some margin)
                        scroll_in_buffer < viewport_height && self.render_start_y > 0.0 ||
                        // Scrolling down - trigger when 60% through buffer
                        scroll_in_buffer + viewport_height > buffer_height - buffer_threshold;
                    
                    // Only trigger background render if not already pending
                    if needs_recenter && !self.current_html.is_empty() && self.pending_render_start.is_none() {
                        // New render_start_y centers scroll in buffer
                        let new_start = (self.scroll_offset - viewport_height * 2.0).max(0.0);
                        self.pending_render_start = Some(new_start);
                        
                        // Spawn background render thread
                        let (tx, rx) = channel();
                        self.bg_render_rx = Some(rx);
                        
                        let html = self.current_html.clone();
                        let url = self.current_url.clone();
                        let content_width = self.width.saturating_sub(TAB_BAR_WIDTH);
                        let render_height = (viewport_height * 5.0) as u32;
                        
                        std::thread::spawn(move || {
                            let mut renderer = PageRenderer::new(content_width, render_height);
                            if let Some(rendered) = renderer.render_html(&html, &url, new_start) {
                                let _ = tx.send((rendered, new_start));
                            }
                        });
                    }
                }
                
                self.request_redraw();
            }
            _ => {}
        }
    }
    
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Process JavaScript timers during idle time
        self.process_js_timers();
    }
}
