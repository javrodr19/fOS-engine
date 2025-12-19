//! fOS Layout Engine
//!
//! CSS box model and layout algorithms.

mod box_model;
mod layout_tree;

pub use box_model::{BoxDimensions, EdgeSizes};
pub use layout_tree::{LayoutBox, LayoutTree};

/// Perform layout on a styled DOM tree
pub fn layout(_viewport_width: f32, _viewport_height: f32) -> LayoutTree {
    tracing::info!("Performing layout");
    LayoutTree::new()
}
