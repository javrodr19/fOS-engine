//! Parallel Layout
//!
//! Phase-parallel layout algorithm for efficient multi-core utilization.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

/// Layout node identifier
pub type LayoutNodeId = usize;

/// Intrinsic size result
#[derive(Debug, Clone, Copy, Default)]
pub struct IntrinsicSizes {
    /// Minimum content width
    pub min_content_width: f32,
    /// Maximum content width  
    pub max_content_width: f32,
    /// Minimum content height
    pub min_content_height: f32,
    /// Maximum content height
    pub max_content_height: f32,
}

/// Layout constraints from parent
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutConstraints {
    /// Available width (None = intrinsic)
    pub available_width: Option<f32>,
    /// Available height (None = intrinsic)
    pub available_height: Option<f32>,
    /// Minimum width
    pub min_width: f32,
    /// Maximum width
    pub max_width: f32,
    /// Minimum height
    pub min_height: f32,
    /// Maximum height
    pub max_height: f32,
}

impl LayoutConstraints {
    /// Unconstrained
    pub fn unconstrained() -> Self {
        Self {
            available_width: None,
            available_height: None,
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
        }
    }
    
    /// With available width
    pub fn with_width(width: f32) -> Self {
        Self {
            available_width: Some(width),
            available_height: None,
            min_width: 0.0,
            max_width: width,
            min_height: 0.0,
            max_height: f32::INFINITY,
        }
    }
}

/// Final computed size
#[derive(Debug, Clone, Copy, Default)]
pub struct ComputedSize {
    pub width: f32,
    pub height: f32,
}

/// Final position
#[derive(Debug, Clone, Copy, Default)]
pub struct ComputedPosition {
    pub x: f32,
    pub y: f32,
}

/// Layout result for a node
#[derive(Debug, Clone, Default)]
pub struct LayoutResult {
    /// Node ID
    pub node_id: LayoutNodeId,
    /// Final size
    pub size: ComputedSize,
    /// Position relative to parent
    pub position: ComputedPosition,
    /// Content box (for children positioning)
    pub content_box: ContentBox,
}

/// Content box for child positioning
#[derive(Debug, Clone, Copy, Default)]
pub struct ContentBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Layout node information
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// Node ID
    pub id: LayoutNodeId,
    /// Parent ID
    pub parent_id: Option<LayoutNodeId>,
    /// Children IDs
    pub children: Vec<LayoutNodeId>,
    /// Display type
    pub display: DisplayType,
    /// Box sizing
    pub box_sizing: BoxSizing,
    /// Margin
    pub margin: EdgeSizes,
    /// Padding
    pub padding: EdgeSizes,
    /// Border width
    pub border: EdgeSizes,
    /// Explicit width
    pub width: SizeValue,
    /// Explicit height
    pub height: SizeValue,
    /// Is replaced element (img, video, etc.)
    pub is_replaced: bool,
    /// Intrinsic size (for replaced elements)
    pub intrinsic_size: Option<(f32, f32)>,
}

/// Edge sizes (top, right, bottom, left)
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeSizes {
    /// Horizontal sum
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }
    
    /// Vertical sum
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Size value
#[derive(Debug, Clone, Copy, Default)]
pub enum SizeValue {
    #[default]
    Auto,
    Px(f32),
    Percent(f32),
    MinContent,
    MaxContent,
    FitContent(f32),
}

/// Display type
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayType {
    #[default]
    Block,
    Inline,
    InlineBlock,
    Flex,
    Grid,
    None,
}

/// Box sizing
#[derive(Debug, Clone, Copy, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

/// Layout tree for parallel processing
pub struct LayoutTree {
    nodes: Vec<LayoutNode>,
    roots: Vec<LayoutNodeId>,
}

impl LayoutTree {
    /// Create new layout tree
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            roots: Vec::new(),
        }
    }
    
    /// Add a node
    pub fn add_node(&mut self, node: LayoutNode) {
        if node.parent_id.is_none() {
            self.roots.push(node.id);
        }
        self.nodes.push(node);
    }
    
    /// Get node by ID
    pub fn get(&self, id: LayoutNodeId) -> Option<&LayoutNode> {
        self.nodes.get(id)
    }
    
    /// Get all nodes
    pub fn nodes(&self) -> &[LayoutNode] {
        &self.nodes
    }
    
    /// Get roots
    pub fn roots(&self) -> &[LayoutNodeId] {
        &self.roots
    }
    
    /// Number of nodes
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Perform layout in parallel phases
pub fn layout_parallel(tree: &LayoutTree, root_constraints: LayoutConstraints) -> Vec<LayoutResult> {
    let num_nodes = tree.len();
    if num_nodes == 0 {
        return Vec::new();
    }
    
    let num_threads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);
    
    // Phase 1: Compute intrinsic sizes (parallel, bottom-up)
    let intrinsic = compute_intrinsic_sizes_parallel(tree, num_threads);
    
    // Phase 2: Propagate constraints (mostly sequential - top-down)
    let constraints = propagate_constraints(tree, &intrinsic, root_constraints);
    
    // Phase 3: Compute final sizes (parallel per independent subtree)
    let sizes = compute_final_sizes_parallel(tree, &intrinsic, &constraints, num_threads);
    
    // Phase 4: Compute positions (parallel)
    let results = compute_positions_parallel(tree, &sizes, num_threads);
    
    results
}

/// Phase 1: Compute intrinsic sizes bottom-up in parallel
fn compute_intrinsic_sizes_parallel(
    tree: &LayoutTree,
    num_threads: usize,
) -> Vec<IntrinsicSizes> {
    let num_nodes = tree.len();
    let results = Arc::new(Mutex::new(vec![IntrinsicSizes::default(); num_nodes]));
    
    // Process by levels (bottom-up)
    let levels = build_levels_bottom_up(tree);
    
    for level in levels {
        if level.len() <= num_threads {
            // Sequential for small levels
            for &id in &level {
                let size = compute_node_intrinsic_size(tree, id, &results.lock().unwrap());
                results.lock().unwrap()[id] = size;
            }
        } else {
            // Parallel
            let chunk_size = (level.len() + num_threads - 1) / num_threads;
            
            std::thread::scope(|s| {
                for chunk in level.chunks(chunk_size) {
                    let results = Arc::clone(&results);
                    let chunk: Vec<_> = chunk.to_vec();
                    
                    s.spawn(move || {
                        for id in chunk {
                            let size = compute_node_intrinsic_size(tree, id, &results.lock().unwrap());
                            results.lock().unwrap()[id] = size;
                        }
                    });
                }
            });
        }
    }
    
    Arc::try_unwrap(results).unwrap().into_inner().unwrap()
}

/// Build levels from bottom to top (leaves first)
fn build_levels_bottom_up(tree: &LayoutTree) -> Vec<Vec<LayoutNodeId>> {
    let mut levels = Vec::new();
    let mut node_levels: HashMap<LayoutNodeId, usize> = HashMap::new();
    
    // First pass: compute level for each node (max child level + 1)
    fn compute_level(
        tree: &LayoutTree,
        id: LayoutNodeId,
        cache: &mut HashMap<LayoutNodeId, usize>,
    ) -> usize {
        if let Some(&level) = cache.get(&id) {
            return level;
        }
        
        let node = tree.get(id).unwrap();
        let level = if node.children.is_empty() {
            0
        } else {
            node.children.iter()
                .map(|&c| compute_level(tree, c, cache) + 1)
                .max()
                .unwrap_or(0)
        };
        
        cache.insert(id, level);
        level
    }
    
    for node in tree.nodes() {
        compute_level(tree, node.id, &mut node_levels);
    }
    
    // Group by level
    let max_level = node_levels.values().copied().max().unwrap_or(0);
    levels.resize(max_level + 1, Vec::new());
    
    for (&id, &level) in &node_levels {
        // Invert level order (leaves = level 0, roots = max level)
        let inverted = max_level - level;
        levels[inverted].push(id);
    }
    
    levels
}

/// Compute intrinsic size for a single node
fn compute_node_intrinsic_size(
    tree: &LayoutTree,
    id: LayoutNodeId,
    child_sizes: &[IntrinsicSizes],
) -> IntrinsicSizes {
    let node = tree.get(id).unwrap();
    
    // For replaced elements, use intrinsic size
    if node.is_replaced {
        if let Some((w, h)) = node.intrinsic_size {
            return IntrinsicSizes {
                min_content_width: w,
                max_content_width: w,
                min_content_height: h,
                max_content_height: h,
            };
        }
    }
    
    // Aggregate children sizes based on display type
    match node.display {
        DisplayType::Block => {
            let mut min_w = 0.0f32;
            let mut max_w = 0.0f32;
            let mut min_h = 0.0f32;
            let mut max_h = 0.0f32;
            
            for &child_id in &node.children {
                let child = &child_sizes[child_id];
                min_w = min_w.max(child.min_content_width);
                max_w = max_w.max(child.max_content_width);
                min_h += child.min_content_height;
                max_h += child.max_content_height;
            }
            
            // Add padding and border
            let extra_w = node.padding.horizontal() + node.border.horizontal();
            let extra_h = node.padding.vertical() + node.border.vertical();
            
            IntrinsicSizes {
                min_content_width: min_w + extra_w,
                max_content_width: max_w + extra_w,
                min_content_height: min_h + extra_h,
                max_content_height: max_h + extra_h,
            }
        }
        DisplayType::Flex => {
            // Simplified flex intrinsic size
            let mut main_min = 0.0f32;
            let mut main_max = 0.0f32;
            let mut cross_min = 0.0f32;
            let mut cross_max = 0.0f32;
            
            for &child_id in &node.children {
                let child = &child_sizes[child_id];
                main_min += child.min_content_width;
                main_max += child.max_content_width;
                cross_min = cross_min.max(child.min_content_height);
                cross_max = cross_max.max(child.max_content_height);
            }
            
            let extra_w = node.padding.horizontal() + node.border.horizontal();
            let extra_h = node.padding.vertical() + node.border.vertical();
            
            IntrinsicSizes {
                min_content_width: main_min + extra_w,
                max_content_width: main_max + extra_w,
                min_content_height: cross_min + extra_h,
                max_content_height: cross_max + extra_h,
            }
        }
        DisplayType::Grid => {
            // Simplified grid intrinsic size
            let mut max_child_w = 0.0f32;
            let mut max_child_h = 0.0f32;
            
            for &child_id in &node.children {
                let child = &child_sizes[child_id];
                max_child_w = max_child_w.max(child.max_content_width);
                max_child_h = max_child_h.max(child.max_content_height);
            }
            
            let extra_w = node.padding.horizontal() + node.border.horizontal();
            let extra_h = node.padding.vertical() + node.border.vertical();
            
            IntrinsicSizes {
                min_content_width: max_child_w + extra_w,
                max_content_width: max_child_w + extra_w,
                min_content_height: max_child_h + extra_h,
                max_content_height: max_child_h + extra_h,
            }
        }
        _ => IntrinsicSizes::default(),
    }
}

/// Phase 2: Propagate constraints top-down
fn propagate_constraints(
    tree: &LayoutTree,
    intrinsic: &[IntrinsicSizes],
    root_constraints: LayoutConstraints,
) -> Vec<LayoutConstraints> {
    let mut constraints = vec![LayoutConstraints::unconstrained(); tree.len()];
    
    // Process roots with initial constraints
    for &root in tree.roots() {
        propagate_node_constraints(tree, root, root_constraints, &mut constraints, intrinsic);
    }
    
    constraints
}

fn propagate_node_constraints(
    tree: &LayoutTree,
    id: LayoutNodeId,
    parent_constraints: LayoutConstraints,
    constraints: &mut [LayoutConstraints],
    _intrinsic: &[IntrinsicSizes],
) {
    let node = tree.get(id).unwrap();
    
    // Compute this node's constraints based on explicit size and parent
    let width = match node.width {
        SizeValue::Px(w) => Some(w),
        SizeValue::Percent(p) => parent_constraints.available_width.map(|pw| pw * p / 100.0),
        _ => parent_constraints.available_width,
    };
    
    let height = match node.height {
        SizeValue::Px(h) => Some(h),
        SizeValue::Percent(p) => parent_constraints.available_height.map(|ph| ph * p / 100.0),
        _ => None,
    };
    
    let node_constraints = LayoutConstraints {
        available_width: width,
        available_height: height,
        min_width: 0.0,
        max_width: width.unwrap_or(parent_constraints.max_width),
        min_height: 0.0,
        max_height: height.unwrap_or(parent_constraints.max_height),
    };
    
    constraints[id] = node_constraints;
    
    // Propagate to children
    let content_width = width.map(|w| {
        w - node.padding.horizontal() - node.border.horizontal()
    });
    
    let child_constraints = LayoutConstraints {
        available_width: content_width,
        available_height: None,
        min_width: 0.0,
        max_width: content_width.unwrap_or(node_constraints.max_width),
        min_height: 0.0,
        max_height: f32::INFINITY,
    };
    
    for &child_id in &node.children {
        propagate_node_constraints(tree, child_id, child_constraints, constraints, _intrinsic);
    }
}

/// Phase 3: Compute final sizes in parallel
fn compute_final_sizes_parallel(
    tree: &LayoutTree,
    intrinsic: &[IntrinsicSizes],
    constraints: &[LayoutConstraints],
    num_threads: usize,
) -> Vec<ComputedSize> {
    let num_nodes = tree.len();
    let results = Arc::new(Mutex::new(vec![ComputedSize::default(); num_nodes]));
    
    // Can parallelize as constraints are already computed
    if num_nodes <= num_threads * 2 {
        for id in 0..num_nodes {
            let size = compute_node_final_size(tree, id, intrinsic, constraints);
            results.lock().unwrap()[id] = size;
        }
    } else {
        let chunk_size = (num_nodes + num_threads - 1) / num_threads;
        let ids: Vec<_> = (0..num_nodes).collect();
        
        std::thread::scope(|s| {
            for chunk in ids.chunks(chunk_size) {
                let results = Arc::clone(&results);
                let chunk: Vec<_> = chunk.to_vec();
                
                s.spawn(move || {
                    for id in chunk {
                        let size = compute_node_final_size(tree, id, intrinsic, constraints);
                        results.lock().unwrap()[id] = size;
                    }
                });
            }
        });
    }
    
    Arc::try_unwrap(results).unwrap().into_inner().unwrap()
}

fn compute_node_final_size(
    tree: &LayoutTree,
    id: LayoutNodeId,
    intrinsic: &[IntrinsicSizes],
    constraints: &[LayoutConstraints],
) -> ComputedSize {
    let node = tree.get(id).unwrap();
    let node_intrinsic = &intrinsic[id];
    let node_constraints = &constraints[id];
    
    let width = match node.width {
        SizeValue::Px(w) => w,
        SizeValue::MinContent => node_intrinsic.min_content_width,
        SizeValue::MaxContent => node_intrinsic.max_content_width,
        _ => node_constraints.available_width.unwrap_or(node_intrinsic.max_content_width),
    };
    
    let height = match node.height {
        SizeValue::Px(h) => h,
        SizeValue::MinContent => node_intrinsic.min_content_height,
        SizeValue::MaxContent => node_intrinsic.max_content_height,
        _ => node_intrinsic.max_content_height,
    };
    
    ComputedSize {
        width: width.clamp(node_constraints.min_width, node_constraints.max_width),
        height: height.clamp(node_constraints.min_height, node_constraints.max_height),
    }
}

/// Phase 4: Compute positions in parallel
fn compute_positions_parallel(
    tree: &LayoutTree,
    sizes: &[ComputedSize],
    num_threads: usize,
) -> Vec<LayoutResult> {
    let num_nodes = tree.len();
    let results = Arc::new(Mutex::new(vec![LayoutResult::default(); num_nodes]));
    
    // Process by levels (top-down for position)
    let levels = build_levels_top_down(tree);
    
    for level in levels {
        if level.len() <= num_threads {
            for &id in &level {
                let result = compute_node_position(tree, id, sizes, &results.lock().unwrap());
                results.lock().unwrap()[id] = result;
            }
        } else {
            let chunk_size = (level.len() + num_threads - 1) / num_threads;
            
            std::thread::scope(|s| {
                for chunk in level.chunks(chunk_size) {
                    let results = Arc::clone(&results);
                    let chunk: Vec<_> = chunk.to_vec();
                    
                    s.spawn(move || {
                        for id in chunk {
                            let result = compute_node_position(tree, id, sizes, &results.lock().unwrap());
                            results.lock().unwrap()[id] = result;
                        }
                    });
                }
            });
        }
    }
    
    Arc::try_unwrap(results).unwrap().into_inner().unwrap()
}

fn build_levels_top_down(tree: &LayoutTree) -> Vec<Vec<LayoutNodeId>> {
    let mut levels = Vec::new();
    let mut current = tree.roots().to_vec();
    
    while !current.is_empty() {
        levels.push(current.clone());
        
        let mut next = Vec::new();
        for id in current {
            if let Some(node) = tree.get(id) {
                next.extend(node.children.iter());
            }
        }
        current = next;
    }
    
    levels
}

fn compute_node_position(
    tree: &LayoutTree,
    id: LayoutNodeId,
    sizes: &[ComputedSize],
    parent_results: &[LayoutResult],
) -> LayoutResult {
    let node = tree.get(id).unwrap();
    let size = sizes[id];
    
    let (x, y) = if let Some(parent_id) = node.parent_id {
        let parent_result = &parent_results[parent_id];
        let parent_node = tree.get(parent_id).unwrap();
        
        // Simple block positioning
        let mut offset_y = parent_result.content_box.y;
        
        // Calculate offset based on previous siblings
        for &sibling_id in &parent_node.children {
            if sibling_id == id {
                break;
            }
            offset_y += sizes[sibling_id].height;
            if let Some(sibling) = tree.get(sibling_id) {
                offset_y += sibling.margin.vertical();
            }
        }
        
        (
            parent_result.content_box.x + node.margin.left,
            offset_y + node.margin.top,
        )
    } else {
        (node.margin.left, node.margin.top)
    };
    
    let content_x = x + node.padding.left + node.border.left;
    let content_y = y + node.padding.top + node.border.top;
    let content_w = size.width - node.padding.horizontal() - node.border.horizontal();
    let content_h = size.height - node.padding.vertical() - node.border.vertical();
    
    LayoutResult {
        node_id: id,
        size,
        position: ComputedPosition { x, y },
        content_box: ContentBox {
            x: content_x,
            y: content_y,
            width: content_w.max(0.0),
            height: content_h.max(0.0),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_layout() {
        let mut tree = LayoutTree::new();
        
        tree.add_node(LayoutNode {
            id: 0,
            parent_id: None,
            children: vec![],
            display: DisplayType::Block,
            box_sizing: BoxSizing::ContentBox,
            margin: EdgeSizes::default(),
            padding: EdgeSizes::default(),
            border: EdgeSizes::default(),
            width: SizeValue::Px(100.0),
            height: SizeValue::Px(50.0),
            is_replaced: false,
            intrinsic_size: None,
        });
        
        let results = layout_parallel(&tree, LayoutConstraints::with_width(800.0));
        
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].size.width, 100.0);
        assert_eq!(results[0].size.height, 50.0);
    }
    
    #[test]
    fn test_nested_layout() {
        let mut tree = LayoutTree::new();
        
        // Parent
        tree.add_node(LayoutNode {
            id: 0,
            parent_id: None,
            children: vec![1],
            display: DisplayType::Block,
            box_sizing: BoxSizing::ContentBox,
            margin: EdgeSizes::default(),
            padding: EdgeSizes { top: 10.0, right: 10.0, bottom: 10.0, left: 10.0 },
            border: EdgeSizes::default(),
            width: SizeValue::Px(200.0),
            height: SizeValue::Auto,
            is_replaced: false,
            intrinsic_size: None,
        });
        
        // Child
        tree.add_node(LayoutNode {
            id: 1,
            parent_id: Some(0),
            children: vec![],
            display: DisplayType::Block,
            box_sizing: BoxSizing::ContentBox,
            margin: EdgeSizes::default(),
            padding: EdgeSizes::default(),
            border: EdgeSizes::default(),
            width: SizeValue::Auto,
            height: SizeValue::Px(30.0),
            is_replaced: false,
            intrinsic_size: None,
        });
        
        let results = layout_parallel(&tree, LayoutConstraints::with_width(800.0));
        
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].size.width, 200.0);
        // Child should be inside parent's content box
        assert!(results[1].position.x >= 10.0);
        assert!(results[1].position.y >= 10.0);
    }
}
