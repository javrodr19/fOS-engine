//! Flexbox Layout
//!
//! Implements CSS Flexbox layout algorithm.
//! https://www.w3.org/TR/css-flexbox-1/

use crate::{LayoutTree, LayoutBoxId, BoxType};
use crate::box_model::EdgeSizes;

/// Flex container properties
#[derive(Debug, Clone, Copy, Default)]
pub struct FlexContainerStyle {
    /// Main axis direction
    pub direction: FlexDirection,
    /// Whether to wrap
    pub wrap: FlexWrap,
    /// Main axis alignment
    pub justify_content: JustifyContent,
    /// Cross axis alignment
    pub align_items: AlignItems,
    /// Multi-line alignment
    pub align_content: AlignContent,
    /// Gap between items
    pub gap: f32,
}

/// Flex item properties
#[derive(Debug, Clone, Copy)]
pub struct FlexItemStyle {
    /// Grow factor
    pub grow: f32,
    /// Shrink factor
    pub shrink: f32,
    /// Base size
    pub basis: FlexBasis,
    /// Override cross-axis alignment
    pub align_self: Option<AlignItems>,
    /// Order for reordering
    pub order: i32,
}

impl Default for FlexItemStyle {
    fn default() -> Self {
        Self {
            grow: 0.0,
            shrink: 1.0,
            basis: FlexBasis::Auto,
            align_self: None,
            order: 0,
        }
    }
}

/// Flex direction
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    pub fn is_row(&self) -> bool {
        matches!(self, Self::Row | Self::RowReverse)
    }
    
    pub fn is_reversed(&self) -> bool {
        matches!(self, Self::RowReverse | Self::ColumnReverse)
    }
}

/// Flex wrap
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlexWrap {
    #[default]
    Nowrap,
    Wrap,
    WrapReverse,
}

/// Justify content (main axis)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Align items (cross axis)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AlignItems {
    #[default]
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

/// Align content (multi-line)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AlignContent {
    #[default]
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
}

/// Flex basis
#[derive(Debug, Clone, Copy, Default)]
pub enum FlexBasis {
    #[default]
    Auto,
    Length(f32),
    Content,
}

/// A flex line (for wrap)
#[derive(Debug)]
struct FlexLine {
    items: Vec<FlexLineItem>,
    main_size: f32,
    cross_size: f32,
}

#[derive(Debug)]
struct FlexLineItem {
    box_id: LayoutBoxId,
    style: FlexItemStyle,
    base_main_size: f32,
    main_size: f32,
    cross_size: f32,
    main_margin: f32,
    cross_margin: f32,
}

/// Layout a flex container
pub fn layout_flex_container(
    tree: &mut LayoutTree,
    container_id: LayoutBoxId,
    style: FlexContainerStyle,
    item_styles: &[(LayoutBoxId, FlexItemStyle)],
) {
    let container = match tree.get(container_id) {
        Some(c) => c,
        None => return,
    };
    
    let container_main = if style.direction.is_row() {
        container.dimensions.content.width
    } else {
        container.dimensions.content.height
    };
    
    let container_cross = if style.direction.is_row() {
        container.dimensions.content.height
    } else {
        container.dimensions.content.width
    };
    
    let container_x = container.dimensions.content.x;
    let container_y = container.dimensions.content.y;
    
    // Build flex lines
    let mut lines = build_flex_lines(tree, &style, item_styles, container_main);
    
    // Resolve flexible lengths
    for line in &mut lines {
        resolve_flexible_lengths(line, container_main, style.gap);
    }
    
    // Calculate cross sizes
    for line in &mut lines {
        for item in &mut line.items {
            // For now, use base cross size
            line.cross_size = line.cross_size.max(item.cross_size);
        }
    }
    
    // Position items
    let mut cross_pos = 0.0;
    
    for line in &lines {
        let mut main_pos = calculate_main_start(&style, line, container_main);
        let spacing = calculate_spacing(&style, line, container_main);
        
        for (i, item) in line.items.iter().enumerate() {
            let (x, y) = if style.direction.is_row() {
                let x = container_x + if style.direction.is_reversed() {
                    container_main - main_pos - item.main_size
                } else {
                    main_pos
                };
                let y = container_y + cross_pos + align_item_cross(
                    &style, item, line.cross_size
                );
                (x, y)
            } else {
                let y = container_y + if style.direction.is_reversed() {
                    container_main - main_pos - item.main_size
                } else {
                    main_pos
                };
                let x = container_x + cross_pos + align_item_cross(
                    &style, item, line.cross_size
                );
                (x, y)
            };
            
            // Update box dimensions
            if let Some(layout_box) = tree.get_mut(item.box_id) {
                if style.direction.is_row() {
                    layout_box.dimensions.content.x = x;
                    layout_box.dimensions.content.y = y;
                    layout_box.dimensions.content.width = item.main_size;
                    layout_box.dimensions.content.height = item.cross_size;
                } else {
                    layout_box.dimensions.content.x = x;
                    layout_box.dimensions.content.y = y;
                    layout_box.dimensions.content.width = item.cross_size;
                    layout_box.dimensions.content.height = item.main_size;
                }
            }
            
            main_pos += item.main_size + item.main_margin + spacing;
            if i < line.items.len() - 1 {
                main_pos += style.gap;
            }
        }
        
        cross_pos += line.cross_size + style.gap;
    }
}

fn build_flex_lines(
    tree: &LayoutTree,
    style: &FlexContainerStyle,
    item_styles: &[(LayoutBoxId, FlexItemStyle)],
    container_main: f32,
) -> Vec<FlexLine> {
    let mut lines = Vec::new();
    let mut current_line = FlexLine {
        items: Vec::new(),
        main_size: 0.0,
        cross_size: 0.0,
    };
    
    for (box_id, item_style) in item_styles {
        let layout_box = match tree.get(*box_id) {
            Some(b) => b,
            None => continue,
        };
        
        let (base_main, base_cross) = if style.direction.is_row() {
            (layout_box.dimensions.content.width, layout_box.dimensions.content.height)
        } else {
            (layout_box.dimensions.content.height, layout_box.dimensions.content.width)
        };
        
        let base_main_size = match item_style.basis {
            FlexBasis::Length(l) => l,
            FlexBasis::Auto | FlexBasis::Content => base_main,
        };
        
        // Check if we need to wrap
        if style.wrap != FlexWrap::Nowrap && 
           !current_line.items.is_empty() &&
           current_line.main_size + base_main_size > container_main {
            lines.push(std::mem::replace(&mut current_line, FlexLine {
                items: Vec::new(),
                main_size: 0.0,
                cross_size: 0.0,
            }));
        }
        
        current_line.main_size += base_main_size;
        current_line.items.push(FlexLineItem {
            box_id: *box_id,
            style: *item_style,
            base_main_size,
            main_size: base_main_size,
            cross_size: base_cross,
            main_margin: 0.0,
            cross_margin: 0.0,
        });
    }
    
    if !current_line.items.is_empty() {
        lines.push(current_line);
    }
    
    lines
}

fn resolve_flexible_lengths(line: &mut FlexLine, container_main: f32, gap: f32) {
    let total_gap = gap * (line.items.len().saturating_sub(1)) as f32;
    let available = container_main - total_gap;
    let used: f32 = line.items.iter().map(|i| i.base_main_size).sum();
    let free_space = available - used;
    
    if free_space > 0.0 {
        // Distribute to items with flex-grow
        let total_grow: f32 = line.items.iter().map(|i| i.style.grow).sum();
        if total_grow > 0.0 {
            for item in &mut line.items {
                let ratio = item.style.grow / total_grow;
                item.main_size = item.base_main_size + free_space * ratio;
            }
        }
    } else if free_space < 0.0 {
        // Shrink items with flex-shrink
        let total_shrink: f32 = line.items.iter()
            .map(|i| i.style.shrink * i.base_main_size)
            .sum();
        if total_shrink > 0.0 {
            for item in &mut line.items {
                let ratio = (item.style.shrink * item.base_main_size) / total_shrink;
                item.main_size = (item.base_main_size + free_space * ratio).max(0.0);
            }
        }
    }
    
    line.main_size = line.items.iter().map(|i| i.main_size).sum();
}

fn calculate_main_start(style: &FlexContainerStyle, line: &FlexLine, container_main: f32) -> f32 {
    let free_space = container_main - line.main_size;
    
    match style.justify_content {
        JustifyContent::FlexStart => 0.0,
        JustifyContent::FlexEnd => free_space,
        JustifyContent::Center => free_space / 2.0,
        JustifyContent::SpaceBetween => 0.0,
        JustifyContent::SpaceAround => {
            if line.items.len() > 0 {
                free_space / (line.items.len() * 2) as f32
            } else {
                0.0
            }
        }
        JustifyContent::SpaceEvenly => {
            free_space / (line.items.len() + 1) as f32
        }
    }
}

fn calculate_spacing(style: &FlexContainerStyle, line: &FlexLine, container_main: f32) -> f32 {
    if line.items.len() <= 1 {
        return 0.0;
    }
    
    let free_space = container_main - line.main_size;
    let gaps = (line.items.len() - 1) as f32;
    
    match style.justify_content {
        JustifyContent::SpaceBetween => free_space / gaps,
        JustifyContent::SpaceAround => free_space / (line.items.len() * 2) as f32,
        JustifyContent::SpaceEvenly => free_space / (line.items.len() + 1) as f32,
        _ => 0.0,
    }
}

fn align_item_cross(style: &FlexContainerStyle, item: &FlexLineItem, line_cross: f32) -> f32 {
    let align = item.style.align_self.unwrap_or(style.align_items);
    let free = line_cross - item.cross_size;
    
    match align {
        AlignItems::FlexStart => 0.0,
        AlignItems::FlexEnd => free,
        AlignItems::Center => free / 2.0,
        AlignItems::Stretch => 0.0, // Would need to resize
        AlignItems::Baseline => 0.0, // Simplified
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_flex_row() {
        let mut tree = LayoutTree::new();
        
        let container = tree.create_box(BoxType::Flex, None);
        tree.set_root(container);
        
        if let Some(c) = tree.get_mut(container) {
            c.dimensions.content.width = 300.0;
            c.dimensions.content.height = 100.0;
        }
        
        let item1 = tree.create_box(BoxType::FlexItem, None);
        let item2 = tree.create_box(BoxType::FlexItem, None);
        
        if let Some(b) = tree.get_mut(item1) {
            b.dimensions.content.width = 50.0;
            b.dimensions.content.height = 50.0;
        }
        if let Some(b) = tree.get_mut(item2) {
            b.dimensions.content.width = 50.0;
            b.dimensions.content.height = 50.0;
        }
        
        let style = FlexContainerStyle::default();
        let items = vec![
            (item1, FlexItemStyle::default()),
            (item2, FlexItemStyle::default()),
        ];
        
        layout_flex_container(&mut tree, container, style, &items);
        
        let b1 = tree.get(item1).unwrap();
        let b2 = tree.get(item2).unwrap();
        
        assert_eq!(b1.dimensions.content.x, 0.0);
        assert_eq!(b2.dimensions.content.x, 50.0);
    }
    
    #[test]
    fn test_flex_grow() {
        let mut tree = LayoutTree::new();
        
        let container = tree.create_box(BoxType::Flex, None);
        if let Some(c) = tree.get_mut(container) {
            c.dimensions.content.width = 300.0;
            c.dimensions.content.height = 100.0;
        }
        
        let item1 = tree.create_box(BoxType::FlexItem, None);
        let item2 = tree.create_box(BoxType::FlexItem, None);
        
        if let Some(b) = tree.get_mut(item1) {
            b.dimensions.content.width = 50.0;
        }
        if let Some(b) = tree.get_mut(item2) {
            b.dimensions.content.width = 50.0;
        }
        
        let style = FlexContainerStyle::default();
        let items = vec![
            (item1, FlexItemStyle { grow: 1.0, ..Default::default() }),
            (item2, FlexItemStyle { grow: 2.0, ..Default::default() }),
        ];
        
        layout_flex_container(&mut tree, container, style, &items);
        
        let b1 = tree.get(item1).unwrap();
        let b2 = tree.get(item2).unwrap();
        
        // 200 free space, 1:2 ratio
        // item1: 50 + 200*(1/3) ≈ 116.67
        // item2: 50 + 200*(2/3) ≈ 183.33
        assert!((b1.dimensions.content.width - 116.67).abs() < 1.0);
        assert!((b2.dimensions.content.width - 183.33).abs() < 1.0);
    }
    
    #[test]
    fn test_justify_center() {
        let mut tree = LayoutTree::new();
        
        let container = tree.create_box(BoxType::Flex, None);
        if let Some(c) = tree.get_mut(container) {
            c.dimensions.content.width = 300.0;
        }
        
        let item = tree.create_box(BoxType::FlexItem, None);
        if let Some(b) = tree.get_mut(item) {
            b.dimensions.content.width = 100.0;
        }
        
        let style = FlexContainerStyle {
            justify_content: JustifyContent::Center,
            ..Default::default()
        };
        
        layout_flex_container(&mut tree, container, style, &[(item, Default::default())]);
        
        let b = tree.get(item).unwrap();
        assert_eq!(b.dimensions.content.x, 100.0); // (300-100)/2
    }
}
