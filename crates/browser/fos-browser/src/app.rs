//! Browser Application
//!
//! Main browser window and event loop.

use std::error::Error;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
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
use fos_dom::Document;
use crate::devtools::DevTools;
use crate::accessibility::AccessibilityManager;
use crate::media::MediaManager;
use crate::canvas::CanvasManager;
use crate::advanced_net::AdvancedNetworking;
use crate::security::SecurityManager;
use crate::memory::MemoryIntegration;

/// Interval for running JavaScript timers while any are pending
const TIMER_TICK: Duration = Duration::from_millis(16);

/// Background color outside the page (0xAARRGGBB)
const WINDOW_BACKGROUND: u32 = 0xFF0D0D0D;

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
        // Page scripts' network requests finish on other threads; each
        // completion wakes the loop, so none waits for the next input event
        let proxy = Mutex::new(event_loop.create_proxy());
        app.network_waker = Some(Arc::new(move || {
            let _ = proxy.lock().unwrap_or_else(|p| p.into_inner()).send_event(());
        }));
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
    /// Scroll offset (vertical, document coordinates)
    scroll_offset: f32,
    /// Y position where rendered buffer starts in document (for sliding window)
    render_start_y: f32,
    /// Current page URL (final URL after redirects)
    current_url: String,
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
    last_timer_check: Instant,
    /// Wakes the event loop when a page script's network request finishes
    network_waker: Option<crate::script_fetch::Waker>,
    /// The page the next load was started from (a link followed); None
    /// when the user started it. Decides its SameSite cookies and Referer.
    navigation_initiator: Option<String>,
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
            current_url: String::new(),
            mouse_x: 0,
            mouse_y: 0,
            resize_pending: false,
            network: NetworkManager::new(),
            current_page: None,
            last_timer_check: Instant::now(),
            network_waker: None,
            navigation_initiator: None,
            devtools: DevTools::new(),
            a11y: AccessibilityManager::new(),
            media: MediaManager::new(),
            canvas: CanvasManager::new(),
            _advanced_net: AdvancedNetworking::new(),
            _security: SecurityManager::new(),
            _memory: MemoryIntegration::new(),
        }
    }

    /// Width of the page area
    fn content_width(&self) -> u32 {
        self.width.saturating_sub(TAB_BAR_WIDTH)
    }

    /// Height of the visible page area
    fn viewport_height(&self) -> f32 {
        self.height.saturating_sub(URL_BAR_HEIGHT) as f32
    }

    /// Load the current tab's page
    fn load_current_page(&mut self) {
        // Only the load right after a link is followed comes from it
        let initiator = self.navigation_initiator.take();
        // Get tab info
        let (url, needs_network, cached_html) = match self.tabs.active_tab() {
            Some(tab) => (tab.url.clone(), tab.needs_network_load, tab.cached_html.clone()),
            None => return,
        };

        // A tab switch rebuilds the page from its cached HTML, without the network
        if let (Some(html), false) = (cached_html, needs_network) {
            log::info!("Using cached HTML ({} bytes)", html.len());
            // Keep the scroll position if this is the page already shown
            let reset_scroll = self.current_url != url;
            self.show_page(Page::from_html(&url, html), reset_scroll);
            self.run_page_scripts();
            self.needs_reload = false;
            return;
        }

        log::info!("Loading: {}", url);
        let request_id = self.devtools.log_request(&url, "GET");

        // about: and file: pages never touch the network. Either way the
        // HTML is parsed once, into the page's DOM, which scripts and
        // rendering share.
        let loaded = if Loader::is_local_url(&url) {
            self.loader.load_sync(&url)
                .map(|page| (page, 200))
                .map_err(|e| e.to_string())
        } else {
            self.network.fetch_page_from(&url, initiator.as_deref())
                .map(|fetched| (Page::from_html(&fetched.url, fetched.html), fetched.status))
                .map_err(|e| e.to_string())
        };

        match loaded {
            Ok((page, status)) => {
                self.devtools.log_response(request_id, status, if status < 400 { "OK" } else { "Error" });

                let final_url = page.url.clone();
                if final_url != url {
                    log::info!("Redirected to {}", final_url);
                    self.chrome.url_bar.set_url(&final_url);
                }

                // Update tab with loaded content and cache the HTML
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.set_final_url(&final_url);
                    tab.title = page.title.clone().unwrap_or_else(|| final_url.clone());
                    tab.loading = false;
                    tab.cached_html = Some(page.html.clone());
                    tab.needs_network_load = false;
                }

                // Paint first, then run scripts
                self.show_page(page, true);

                // Jump to the fragment, if the URL has one
                if let Some((_, fragment)) = final_url.split_once('#') {
                    self.scroll_to_anchor(fragment);
                }

                self.run_page_scripts();
            }
            Err(e) => {
                // Log failed request
                self.devtools.log_network_error(request_id, &e);
                log::error!("Failed to load {}: {}", url, e);

                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.loading = false;
                    tab.title = "Error".to_string();
                }

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
                "#, escape_html(&url), escape_html(&e));

                self.show_page(Page::from_html(&url, error_html), true);
            }
        }

        self.needs_reload = false;
    }

    /// Make `page` the displayed page and render it.
    /// If reset_scroll is false, keeps the current scroll position.
    fn show_page(&mut self, mut page: Page, reset_scroll: bool) {
        // Stylesheets block the first render, as in other browsers:
        // painting without them would show the page unstyled first
        page.stylesheets = self.load_stylesheets(&page);
        self.renderer.set_stylesheets(page.stylesheets.clone());
        log::info!("Rendering {} bytes of HTML...", page.html.len());
        self.current_url = page.url.clone();
        self.current_page = Some(page);

        if reset_scroll {
            self.scroll_offset = 0.0;
            self.render_start_y = 0.0;
        }

        self.rerender_at(self.render_start_y);

        if let Some(ref rendered) = self.rendered_page {
            log::info!("Rendered: {}x{} pixels", rendered.width, rendered.height);
        }
    }

    /// Fetch the page's external stylesheets
    fn load_stylesheets(&mut self, page: &Page) -> crate::css_loader::Stylesheets {
        crate::css_loader::load_for_page(&mut self.network, page)
    }

    /// Run the current page's scripts, then update everything derived from
    /// its DOM, which the scripts may have changed
    fn run_page_scripts(&mut self) {
        let Some(page) = self.current_page.as_mut() else { return };

        page.set_cookie_jar(self.network.cookie_jar().clone());
        if let Err(e) = page.initialize_javascript() {
            log::warn!("Failed to initialize JavaScript: {}", e);
            self.devtools.warn(&format!("JS init failed: {}", e));
        }
        if let Some(waker) = self.network_waker.clone() {
            page.set_network_waker(waker);
        }
        let network = &mut self.network;
        let page_url = page.url.clone();
        if let Err(e) = page.execute_scripts_with(&mut PageFetcher { network, page_url: &page_url }) {
            log::warn!("Failed to execute scripts: {}", e);
            self.devtools.error(&format!("Script error: {}", e));
        }

        self.refresh_if_dom_changed();
        self.follow_script_navigation();

        // Build accessibility tree and extract media/canvas from DOM
        let Some(doc) = self.current_document() else { return };
        let doc_guard = lock_document(&doc);

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

    /// The current page's DOM
    fn current_document(&self) -> Option<Arc<Mutex<Document>>> {
        self.current_page.as_ref().and_then(Page::document)
    }

    /// Re-render if the DOM changed since it was laid out (e.g. by a script)
    fn refresh_if_dom_changed(&mut self) {
        let Some(doc) = self.current_document() else { return };
        let current = self.renderer.is_layout_current(&lock_document(&doc));
        if !current {
            log::debug!("DOM changed, re-rendering");
            self.rerender_at(self.render_start_y);
            self.ensure_render_covers_scroll();
            self.request_redraw();
        }
    }

    /// Render the page buffer starting at document position `start_y`.
    ///
    /// The layout is cached by the renderer until the DOM or the width
    /// changes, so this usually only repaints.
    fn rerender_at(&mut self, start_y: f32) {
        self.renderer.set_viewport(self.content_width(), self.buffer_height());

        let Some(doc) = self.current_document() else { return };
        // Free the old buffer first, so two are never alive at once
        self.rendered_page = None;
        let rendered = self.renderer.render_document(&lock_document(&doc), start_y);
        if let Some(rendered) = rendered {
            self.rendered_page = Some(rendered);
            self.render_start_y = start_y;
        }
        self.sync_page_geometry();
    }

    /// Move the page buffer to document position `start_y`, repainting only
    /// the rows that were not in the previous buffer
    fn rerender_scrolled(&mut self, start_y: f32) {
        let Some(previous) = self.rendered_page.take() else {
            return self.rerender_at(start_y);
        };
        self.renderer.set_viewport(self.content_width(), self.buffer_height());

        let Some(doc) = self.current_document() else { return };
        let rendered = self.renderer.render_document_scrolled(&lock_document(&doc), start_y, previous);
        if let Some(rendered) = rendered {
            self.rendered_page = Some(rendered);
            self.render_start_y = start_y;
        }
        self.sync_page_geometry();
    }

    /// Height of the page pixel buffer: exactly the visible area. Scrolling
    /// moves the rows that stay visible and paints only the exposed ones, so
    /// no off-screen rows need to be kept (at 1920x1080 an off-screen margin
    /// of one viewport each way would cost 16 MB).
    fn buffer_height(&self) -> u32 {
        (self.viewport_height() as u32).max(1)
    }

    /// Clamp the scroll position to the document and move the page buffer
    /// to it
    fn ensure_render_covers_scroll(&mut self) {
        let viewport_height = self.viewport_height();
        let Some(rendered) = self.rendered_page.as_ref() else { return };

        let max_scroll = (rendered.content_height - viewport_height).max(0.0);
        self.scroll_offset = self.scroll_offset.clamp(0.0, max_scroll);

        // A whole-pixel origin lets the renderer reuse the rows the old and
        // new buffers share
        let start = self.scroll_offset.round();
        if start != self.render_start_y {
            self.rerender_scrolled(start);
        }
    }

    /// Scroll by `delta` pixels (positive is down)
    fn scroll_by(&mut self, delta: f32) {
        self.scroll_offset += delta;
        self.ensure_render_covers_scroll();
        self.request_redraw();
    }

    /// Scroll to the element with the given id
    fn scroll_to_anchor(&mut self, id: &str) {
        let Some(rendered) = self.rendered_page.as_ref() else { return };
        if let Some(anchor) = rendered.anchors.iter().find(|a| a.id == id) {
            log::info!("Scrolling to anchor: #{}", id);
            // Small margin at the top
            self.scroll_offset = (anchor.y + self.render_start_y - 10.0).max(0.0);
            self.ensure_render_covers_scroll();
            self.request_redraw();
        }
    }

    /// Run the page's JavaScript timers that are due
    fn process_js_timers(&mut self) {
        let Some(page) = self.current_page.as_mut() else { return };
        if !page.next_timer_due().is_some_and(|due| due <= Instant::now()) {
            return;
        }
        self.last_timer_check = Instant::now();
        let network = &mut self.network;
        let page_url = page.url.clone();
        if let Err(e) = page.process_timers_with(&mut PageFetcher { network, page_url: &page_url }) {
            log::warn!("Timer processing error: {}", e);
        }
        // Timer callbacks may have changed the DOM or navigated
        self.refresh_if_dom_changed();
        self.follow_script_navigation();
    }

    /// Hand the page's finished network requests to its scripts
    fn process_js_network(&mut self) {
        let Some(page) = self.current_page.as_mut() else { return };
        if !page.has_pending_network() {
            return;
        }
        let network = &mut self.network;
        let page_url = page.url.clone();
        match page.process_network_with(&mut PageFetcher { network, page_url: &page_url }) {
            Ok(true) => {
                // Callbacks may have changed the DOM or navigated
                self.refresh_if_dom_changed();
                self.follow_script_navigation();
            }
            Ok(false) => {}
            Err(e) => log::warn!("Network callback error: {}", e),
        }
    }

    /// Give the page's scripts the current layout, viewport and scroll
    /// position (`getBoundingClientRect`, `innerWidth`, `scrollY`, ...)
    fn sync_page_geometry(&mut self) {
        let layout = self.renderer.layout_snapshot();
        let viewport = (self.content_width() as f32, self.viewport_height());
        let scroll = (0.0, self.scroll_offset);
        if let Some(rt) = self.current_page.as_mut().and_then(|p| p.js_runtime.as_mut()) {
            rt.set_layout(layout, viewport, scroll);
        }
    }

    /// Go where the page's scripts asked to (`location.href = ...`), and
    /// scroll where they asked to (`scrollTo`, `scrollIntoView`)
    fn follow_script_navigation(&mut self) {
        let scroll = self.current_page.as_mut().and_then(|p| p.js_runtime.as_mut()).and_then(|r| r.take_scroll_request());
        if let Some(y) = scroll {
            self.scroll_offset = y;
            self.ensure_render_covers_scroll();
            self.request_redraw();
        }
        let Some(url) = self.current_page.as_mut().and_then(Page::take_script_navigation) else { return };
        log::info!("Script navigation to {}", url);
        self.follow_link(&url);
    }

    /// Render the browser UI and content
    fn render(&mut self) {
        let Some(size) = self.window.as_ref().map(|w| w.inner_size()) else { return };
        if size.width == 0 || size.height == 0 {
            return;
        }

        // Track the window size before loading, so a page is rendered once
        // at the right size
        if size.width != self.width || size.height != self.height {
            self.width = size.width;
            self.height = size.height;
            self.resize_pending = true;
        }

        if self.needs_reload {
            // Load page if needed (renders at the current size)
            self.load_current_page();
            self.resize_pending = false;
        } else if self.resize_pending {
            // Re-render at the new size (the parsed page is reused)
            self.resize_pending = false;
            if self.current_page.is_some() {
                let start = self.render_start_y;
                self.rerender_at(start);
                self.ensure_render_covers_scroll();
            }
        }

        let Some(surface) = &mut self.surface else { return };

        // Resize surface if needed
        let (Some(surface_width), Some(surface_height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        if surface.resize(surface_width, surface_height).is_err() {
            return;
        }

        // Get buffer
        let mut buffer = match surface.buffer_mut() {
            Ok(b) => b,
            Err(_) => return,
        };

        let buffer_width = size.width as usize;
        let buffer_height = size.height as usize;

        // Clear to background color
        buffer.fill(WINDOW_BACKGROUND);

        // Render page content in content area
        let content_x = (TAB_BAR_WIDTH as usize).min(buffer_width);
        let content_height = buffer_height.saturating_sub(URL_BAR_HEIGHT as usize);

        if let Some(ref rendered) = self.rendered_page {
            // scroll_offset is in document coordinates, render_start_y is where the buffer starts
            let scroll_y = (self.scroll_offset - self.render_start_y).max(0.0) as usize;
            let src_width = rendered.width as usize;
            let copy_width = src_width.min(buffer_width - content_x);

            // Both buffers use 0xAARRGGBB, so each visible row is one copy
            for y in 0..content_height {
                let dst_start = y * buffer_width + content_x;
                let dst = &mut buffer[dst_start..dst_start + copy_width];
                let src_y = y + scroll_y;
                if src_y < rendered.height as usize {
                    let src_start = src_y * src_width;
                    dst.copy_from_slice(&rendered.pixels[src_start..src_start + copy_width]);
                } else {
                    // Past end of content
                    dst.fill(0xFFFFFFFF);
                }
            }
        } else {
            // No rendered page - draw a placeholder rectangle
            let placeholder_color = 0xFF1A3A5A; // Dark blue
            for y in 50..150.min(content_height) {
                let start = y * buffer_width + content_x;
                let end = (start + 200).min((y + 1) * buffer_width);
                buffer[start..end].fill(placeholder_color);
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
                    // Handle text input (any printable character, not just ASCII)
                    if let Some(text) = &event.text {
                        for c in text.chars() {
                            if !c.is_control() {
                                self.chrome.handle_char(c);
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                }
            }
        }

        let viewport_height = self.viewport_height();

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

            // Navigation (history): Ctrl+K / Ctrl+[ back, Ctrl+; (Ñ on Spanish keyboard) / Ctrl+] forward
            PhysicalKey::Code(KeyCode::KeyK) | PhysicalKey::Code(KeyCode::BracketLeft) if ctrl => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    if tab.go_back().is_some() {
                        self.needs_reload = true;
                    }
                }
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::Semicolon) | PhysicalKey::Code(KeyCode::BracketRight) if ctrl => {
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
                self.reload();
            }
            PhysicalKey::Code(KeyCode::F5) => {
                // F5: Reload
                self.reload();
            }
            PhysicalKey::Code(KeyCode::F12) => {
                // F12: Toggle DevTools
                self.devtools.toggle();
                if self.devtools.is_open() {
                    // Log to console when opening
                    self.devtools.log("DevTools opened");
                    // Inspect current page DOM
                    if let Some(doc) = self.current_document() {
                        self.devtools.inspect_document(&lock_document(&doc));
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
            PhysicalKey::Code(KeyCode::ArrowDown) => self.scroll_by(40.0),
            PhysicalKey::Code(KeyCode::ArrowUp) => self.scroll_by(-40.0),
            PhysicalKey::Code(KeyCode::PageDown) => self.scroll_by(viewport_height * 0.9),
            PhysicalKey::Code(KeyCode::PageUp) => self.scroll_by(-viewport_height * 0.9),
            PhysicalKey::Code(KeyCode::Space) if modifiers.shift_key() => self.scroll_by(-viewport_height * 0.9),
            PhysicalKey::Code(KeyCode::Space) => self.scroll_by(viewport_height * 0.9),
            PhysicalKey::Code(KeyCode::Home) if ctrl => {
                // Ctrl+Home: Go to top of page
                self.scroll_offset = 0.0;
                self.scroll_by(0.0);
            }
            PhysicalKey::Code(KeyCode::End) if ctrl => {
                // Ctrl+End: Go to bottom of page (clamped to the content)
                self.scroll_offset = f32::MAX / 2.0;
                self.scroll_by(0.0);
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
                if let Some(href) = self.a11y.get_focused_link_url().map(String::from) {
                    self.follow_link(&href);
                }
            }

            _ => {}
        }
    }

    /// Reload the current page from the network
    fn reload(&mut self) {
        if let Some(tab) = self.tabs.active_tab_mut() {
            tab.needs_network_load = true;
        }
        self.needs_reload = true;
        self.request_redraw();
    }

    /// Navigate to a URL or search typed by the user
    fn navigate_to(&mut self, input: &str) {
        let normalized = crate::navigation::omnibox_to_url(input);
        self.navigation_initiator = None;

        // Update URL bar to show the URL we're navigating to
        self.chrome.url_bar.set_url(&normalized);

        // Use tab.navigate() to properly set needs_network_load and record history
        if let Some(tab) = self.tabs.active_tab_mut() {
            tab.navigate(&normalized);
        }

        self.needs_reload = true;
        self.request_redraw();
    }

    /// Follow a link from the current page
    fn follow_link(&mut self, href: &str) {
        let href = href.trim();
        let lower = href.to_ascii_lowercase();

        if lower.starts_with("javascript:") {
            let code = percent_decode(&href["javascript:".len()..]);
            if let Some(page) = self.current_page.as_mut().and_then(|p| p.js_runtime.as_mut()) {
                if let Err(e) = page.eval(&code) {
                    log::warn!("javascript: URL failed: {}", e);
                }
            }
            self.refresh_if_dom_changed();
            return;
        }
        if lower.starts_with("mailto:") || lower.starts_with("tel:") {
            log::info!("External link not supported: {}", href);
            return;
        }

        // Resolve against the page's final URL (RFC 3986)
        let target = fos_net::url_util::resolve(&self.current_url, href);

        // Same-document fragment links only scroll
        let (target_doc, fragment) = match target.split_once('#') {
            Some((doc, fragment)) => (doc, Some(fragment)),
            None => (target.as_str(), None),
        };
        let current_doc = self.current_url.split('#').next().unwrap_or("");
        if let Some(fragment) = fragment {
            if target_doc == current_doc {
                let fragment = fragment.to_string();
                self.scroll_to_anchor(&fragment);
                return;
            }
        }

        log::info!("Navigating to: {}", target);
        self.navigate_to(&target);
        self.navigation_initiator = Some(self.current_url.clone());
    }

    /// Handle a click inside the page area
    fn handle_content_click(&mut self) {
        // Content starts after tab bar
        let content_x = self.mouse_x - TAB_BAR_WIDTH as i32;
        let content_y = self.mouse_y;
        if content_x < 0 || content_y < 0 {
            return;
        }

        // Links are stored in render buffer coordinates; convert from the screen
        let scroll_y = (self.scroll_offset - self.render_start_y).max(0.0);
        let hit_x = content_x as f32;
        let hit_y = content_y as f32 + scroll_y;

        // Text under the pointer belongs to a DOM element: the page's click
        // handlers run first and decide whether a link is followed
        let doc_x = hit_x;
        let doc_y = content_y as f32 + self.scroll_offset;
        let has_js = self.current_page.as_ref().is_some_and(|p| p.js_runtime.is_some());
        if has_js {
            if let Some(node) = self.renderer.node_at(doc_x, doc_y) {
                let href = self.current_page.as_mut().and_then(|p| p.dispatch_click(node));
                self.refresh_if_dom_changed();
                self.follow_script_navigation();
                if let Some(href) = href {
                    self.follow_link(&href);
                }
                return;
            }
        }

        let href = self.rendered_page.as_ref().and_then(|rendered| {
            rendered.links.iter()
                .find(|link| {
                    hit_x >= link.x && hit_x <= link.x + link.width &&
                    hit_y >= link.y && hit_y <= link.y + link.height
                })
                .map(|link| link.href.clone())
        });

        if let Some(href) = href {
            self.follow_link(&href);
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Fetches the scripts and modules of the page at `page_url` through the
/// browser's network stack (HTTP cache, cookies, parallel connections)
struct PageFetcher<'a> {
    network: &'a mut NetworkManager,
    page_url: &'a str,
}

impl crate::js_runtime::ScriptSource for PageFetcher<'_> {
    fn fetch(&mut self, url: &str) -> Option<String> {
        if Loader::is_local_url(url) {
            return fetch_local_script(self.page_url, url);
        }
        match self.network.fetch(url, Some(self.page_url)) {
            Ok(result) => Some(crate::charset::decode_html(result.body, Some(&result.content_type))),
            Err(e) => {
                log::warn!("Failed to fetch script {}: {}", url, e);
                None
            }
        }
    }

    fn fetch_many(&mut self, urls: &[String]) -> Vec<Option<String>> {
        let mut out = vec![None; urls.len()];
        let mut remote = Vec::new();
        for (i, url) in urls.iter().enumerate() {
            if Loader::is_local_url(url) {
                out[i] = fetch_local_script(self.page_url, url);
            } else {
                remote.push(i);
            }
        }
        let remote_urls: Vec<String> = remote.iter().map(|&i| urls[i].clone()).collect();
        for (&i, result) in remote.iter().zip(self.network.fetch_many(&remote_urls, Some(self.page_url))) {
            match result {
                Ok(r) => out[i] = Some(crate::charset::decode_html(r.body, Some(&r.content_type))),
                Err(e) => log::warn!("Failed to fetch script {}: {}", urls[i], e),
            }
        }
        out
    }
}

/// A script from a local file: only local pages may load them
fn fetch_local_script(page_url: &str, url: &str) -> Option<String> {
    if !Loader::is_local_url(page_url) {
        log::warn!("Not allowed to load local resource {url} from {page_url}");
        return None;
    }
    let path = crate::loader::file_url_to_path(url)?;
    let bytes = std::fs::read(path).ok()?;
    Some(crate::charset::decode_html(bytes, Some("text/javascript")))
}

/// Decode `%XX` escapes (javascript: URLs)
fn percent_decode(s: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Escape text for inclusion in HTML
/// Lock a page's DOM. A panic while it was locked may leave it half
/// mutated, but it is still a valid tree, so rendering carries on.
fn lock_document(doc: &Mutex<Document>) -> MutexGuard<'_, Document> {
    doc.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
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

        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(e) => {
                log::error!("Failed to create window: {}", e);
                event_loop.exit();
                return;
            }
        };

        // Create software rendering surface
        let surface = softbuffer::Context::new(window.clone())
            .and_then(|context| softbuffer::Surface::new(&context, window.clone()));
        let surface = match surface {
            Ok(surface) => surface,
            Err(e) => {
                log::error!("Failed to create rendering surface: {}", e);
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window);
        self.surface = Some(surface);

        // Create initial tab (a local file path on the command line opens as file:)
        if !self.initial_url.is_empty() {
            let local_path = std::path::Path::new(&self.initial_url);
            let url = match local_path.canonicalize() {
                Ok(path) if local_path.exists() && !self.initial_url.contains("://") => {
                    format!("file://{}", path.display())
                }
                _ => crate::navigation::omnibox_to_url(&self.initial_url),
            };
            self.chrome.url_bar.set_url(&url);
            self.tabs.new_tab(&url);
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
                    // First check chrome (tabs, url bar), then links in the page
                    if let Some(url) = self.chrome.handle_click(button, &mut self.tabs) {
                        self.navigate_to(&url);
                    } else {
                        self.handle_content_click();
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
                let scroll_amount = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 40.0,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };
                self.scroll_by(-scroll_amount);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Deliver finished network requests, then run due timers
        self.process_js_network();
        self.process_js_timers();

        // Wake up for the next timer tick only while timers are pending;
        // otherwise sleep until the next input event (zero idle CPU)
        match self.current_page.as_ref().and_then(Page::next_timer_due) {
            // Timers never fire more often than every TIMER_TICK, which
            // keeps a page spinning on `setTimeout(f, 0)` from pegging a core
            Some(due) => {
                let earliest = self.last_timer_check + TIMER_TICK;
                event_loop.set_control_flow(ControlFlow::WaitUntil(due.max(earliest)));
            }
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<script>&\""), "&lt;script&gt;&amp;&quot;");
    }

    #[test]
    fn only_local_pages_load_local_scripts() {
        let dir = std::env::temp_dir().join(format!("fos-local-script-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("data.json");
        std::fs::write(&file, r#"{"secret": 1}"#).unwrap();
        let url = format!("file://{}", file.display());
        assert_eq!(fetch_local_script(&format!("file://{}/page.html", dir.display()), &url).as_deref(), Some(r#"{"secret": 1}"#));
        assert_eq!(fetch_local_script("https://evil.example/", &url), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
