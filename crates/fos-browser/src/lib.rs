//! fOS Browser - Minimalist Web Browser
//!
//! A lightweight web browser built on the fOS Engine.
//! Designed for minimal resource usage.

pub mod app;
pub mod page;
pub mod tab;
pub mod navigation;
pub mod loader;
pub mod ui;
pub mod renderer;

pub use app::Browser;
pub use page::Page;
pub use tab::Tab;
pub use renderer::{PageRenderer, RenderedPage};
