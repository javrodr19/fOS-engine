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

use crate::tab::TabManager;
use crate::ui::Chrome;

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
    /// Initial URL
    initial_url: String,
    /// Window dimensions
    width: u32,
    height: u32,
    /// Current modifier state
    modifiers: winit::keyboard::ModifiersState,
}

impl BrowserApp {
    fn new(initial_url: String) -> Self {
        Self {
            window: None,
            surface: None,
            tabs: TabManager::new(),
            chrome: Chrome::new(),
            initial_url,
            width: 1024,
            height: 768,
            modifiers: winit::keyboard::ModifiersState::default(),
        }
    }
    
    /// Render the browser UI and content
    fn render(&mut self) {
        let Some(surface) = &mut self.surface else { return };
        let Some(window) = &self.window else { return };
        
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        
        self.width = size.width;
        self.height = size.height;
        
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
        
        // Clear to background color
        let bg_color = 0xFF0D0D0D; // Dark gray ARGB
        buffer.fill(bg_color);
        
        // Render UI chrome
        self.chrome.render(
            &mut buffer,
            size.width as usize,
            size.height as usize,
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
        
        match event.physical_key {
            PhysicalKey::Code(KeyCode::KeyT) if ctrl => {
                // Ctrl+T: New tab
                self.tabs.new_tab("about:blank");
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::KeyW) if ctrl => {
                // Ctrl+W: Close tab
                self.tabs.close_active_tab();
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::KeyL) if ctrl => {
                // Ctrl+L: Focus URL bar
                self.chrome.focus_url_bar();
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::KeyR) if ctrl => {
                // Ctrl+R: Reload
                self.tabs.reload_active();
                self.request_redraw();
            }
            PhysicalKey::Code(KeyCode::F5) => {
                // F5: Reload
                self.tabs.reload_active();
                self.request_redraw();
            }
            _ => {}
        }
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
                    self.chrome.handle_click(button, &mut self.tabs);
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
