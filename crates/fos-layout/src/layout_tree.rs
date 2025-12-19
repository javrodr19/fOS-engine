//! Layout Tree

use crate::BoxDimensions;

/// Layout tree
#[derive(Debug, Default)]
pub struct LayoutTree {
    pub boxes: Vec<LayoutBox>,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self { boxes: Vec::new() }
    }
}

/// A box in the layout tree
#[derive(Debug)]
pub struct LayoutBox {
    pub dimensions: BoxDimensions,
    pub box_type: BoxType,
    pub children: Vec<usize>,
}

/// Type of layout box
#[derive(Debug)]
pub enum BoxType {
    Block,
    Inline,
    InlineBlock,
    Flex,
    Grid,
    Anonymous,
}
