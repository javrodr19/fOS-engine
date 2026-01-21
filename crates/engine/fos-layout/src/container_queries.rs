//! Container Queries Layout (Phase 3.2)
//!
//! CSS Container Queries support for size-based conditional styling.
//! Tracks container dimensions for query resolution.

use std::collections::HashMap;

// ============================================================================
// Container Size
// ============================================================================

/// Container size for query resolution
#[derive(Debug, Clone, Copy, Default)]
pub struct ContainerSize {
    /// Inline size (width in horizontal writing mode)
    pub inline_size: f32,
    /// Block size (height in horizontal writing mode)  
    pub block_size: f32,
    /// Aspect ratio (inline / block)
    pub aspect_ratio: f32,
}

impl ContainerSize {
    /// Create new container size
    pub fn new(inline_size: f32, block_size: f32) -> Self {
        let aspect_ratio = if block_size > 0.0 {
            inline_size / block_size
        } else {
            0.0
        };
        Self {
            inline_size,
            block_size,
            aspect_ratio,
        }
    }
    
    /// Create from width and height
    pub fn from_dimensions(width: f32, height: f32) -> Self {
        Self::new(width, height)
    }
}

// ============================================================================
// Container Type
// ============================================================================

/// Container type for containment
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContainerType {
    /// No container
    #[default]
    None,
    /// Inline-size containment only
    InlineSize,
    /// Block-size containment only
    BlockSize,
    /// Both inline and block size containment
    Size,
}

impl ContainerType {
    /// Check if container provides inline-size containment
    pub fn has_inline_containment(&self) -> bool {
        matches!(self, Self::InlineSize | Self::Size)
    }
    
    /// Check if container provides block-size containment
    pub fn has_block_containment(&self) -> bool {
        matches!(self, Self::BlockSize | Self::Size)
    }
}

// ============================================================================
// Container Query Conditions
// ============================================================================

/// Container query feature comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryComparison {
    /// Exact match
    Equal,
    /// Less than
    LessThan,
    /// Less than or equal
    LessThanEqual,
    /// Greater than
    GreaterThan,
    /// Greater than or equal
    GreaterThanEqual,
}

/// Container query condition
#[derive(Debug, Clone)]
pub enum ContainerCondition {
    /// Width query: (min-width: Xpx) or (width < Xpx)
    Width(QueryComparison, f32),
    /// Height query
    Height(QueryComparison, f32),
    /// Aspect ratio query
    AspectRatio(QueryComparison, f32),
    /// Orientation query
    Orientation(Orientation),
    /// Inline size query
    InlineSize(QueryComparison, f32),
    /// Block size query
    BlockSize(QueryComparison, f32),
    /// Logical AND of conditions
    And(Box<ContainerCondition>, Box<ContainerCondition>),
    /// Logical OR of conditions
    Or(Box<ContainerCondition>, Box<ContainerCondition>),
    /// Logical NOT
    Not(Box<ContainerCondition>),
}

/// Orientation for container queries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// Portrait: height >= width
    Portrait,
    /// Landscape: width > height
    Landscape,
    /// Square: width == height
    Square,
}

impl ContainerCondition {
    /// Evaluate condition against container size
    pub fn evaluate(&self, size: &ContainerSize) -> bool {
        match self {
            Self::Width(cmp, value) => compare(*cmp, size.inline_size, *value),
            Self::Height(cmp, value) => compare(*cmp, size.block_size, *value),
            Self::AspectRatio(cmp, value) => compare(*cmp, size.aspect_ratio, *value),
            Self::InlineSize(cmp, value) => compare(*cmp, size.inline_size, *value),
            Self::BlockSize(cmp, value) => compare(*cmp, size.block_size, *value),
            Self::Orientation(o) => {
                let actual = if size.inline_size == size.block_size {
                    Orientation::Square
                } else if size.block_size > size.inline_size {
                    Orientation::Portrait
                } else {
                    Orientation::Landscape
                };
                *o == actual
            }
            Self::And(a, b) => a.evaluate(size) && b.evaluate(size),
            Self::Or(a, b) => a.evaluate(size) || b.evaluate(size),
            Self::Not(c) => !c.evaluate(size),
        }
    }
    
    /// Create min-width condition
    pub fn min_width(value: f32) -> Self {
        Self::Width(QueryComparison::GreaterThanEqual, value)
    }
    
    /// Create max-width condition
    pub fn max_width(value: f32) -> Self {
        Self::Width(QueryComparison::LessThanEqual, value)
    }
    
    /// Create min-height condition
    pub fn min_height(value: f32) -> Self {
        Self::Height(QueryComparison::GreaterThanEqual, value)
    }
    
    /// Create max-height condition
    pub fn max_height(value: f32) -> Self {
        Self::Height(QueryComparison::LessThanEqual, value)
    }
}

fn compare(cmp: QueryComparison, actual: f32, expected: f32) -> bool {
    match cmp {
        QueryComparison::Equal => (actual - expected).abs() < 0.001,
        QueryComparison::LessThan => actual < expected,
        QueryComparison::LessThanEqual => actual <= expected,
        QueryComparison::GreaterThan => actual > expected,
        QueryComparison::GreaterThanEqual => actual >= expected,
    }
}

// ============================================================================
// Container Context
// ============================================================================

/// Named container registration
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    /// Container name (or empty for unnamed)
    pub name: String,
    /// Container type
    pub container_type: ContainerType,
    /// Current size
    pub size: ContainerSize,
}

/// Container context for tracking sizes during layout
#[derive(Debug, Default)]
pub struct ContainerContext {
    /// Stack of container ancestors (for nested queries)
    container_stack: Vec<ContainerInfo>,
    /// Named containers by name
    named_containers: HashMap<String, ContainerSize>,
}

impl ContainerContext {
    /// Create new context
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Push a container onto the stack
    pub fn push_container(&mut self, info: ContainerInfo) {
        if !info.name.is_empty() {
            self.named_containers.insert(info.name.clone(), info.size);
        }
        self.container_stack.push(info);
    }
    
    /// Pop the current container
    pub fn pop_container(&mut self) -> Option<ContainerInfo> {
        let info = self.container_stack.pop();
        if let Some(ref i) = info {
            if !i.name.is_empty() {
                self.named_containers.remove(&i.name);
            }
        }
        info
    }
    
    /// Get nearest container with required type
    pub fn find_container(&self, required_type: ContainerType) -> Option<&ContainerInfo> {
        self.container_stack.iter().rev().find(|c| {
            match required_type {
                ContainerType::None => true,
                ContainerType::InlineSize => c.container_type.has_inline_containment(),
                ContainerType::BlockSize => c.container_type.has_block_containment(),
                ContainerType::Size => c.container_type == ContainerType::Size,
            }
        })
    }
    
    /// Get container by name
    pub fn get_named_container(&self, name: &str) -> Option<&ContainerSize> {
        self.named_containers.get(name)
    }
    
    /// Evaluate a query against the nearest matching container
    pub fn evaluate_query(
        &self, 
        condition: &ContainerCondition,
        required_type: ContainerType,
    ) -> bool {
        if let Some(container) = self.find_container(required_type) {
            condition.evaluate(&container.size)
        } else {
            false
        }
    }
    
    /// Current container depth
    pub fn depth(&self) -> usize {
        self.container_stack.len()
    }
    
    /// Clear all containers
    pub fn clear(&mut self) {
        self.container_stack.clear();
        self.named_containers.clear();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_container_size() {
        let size = ContainerSize::new(800.0, 600.0);
        assert_eq!(size.inline_size, 800.0);
        assert_eq!(size.block_size, 600.0);
        assert!((size.aspect_ratio - 1.333).abs() < 0.01);
    }
    
    #[test]
    fn test_condition_min_width() {
        let size = ContainerSize::new(500.0, 400.0);
        
        let cond = ContainerCondition::min_width(400.0);
        assert!(cond.evaluate(&size));
        
        let cond = ContainerCondition::min_width(600.0);
        assert!(!cond.evaluate(&size));
    }
    
    #[test]
    fn test_condition_max_width() {
        let size = ContainerSize::new(500.0, 400.0);
        
        let cond = ContainerCondition::max_width(600.0);
        assert!(cond.evaluate(&size));
        
        let cond = ContainerCondition::max_width(400.0);
        assert!(!cond.evaluate(&size));
    }
    
    #[test]
    fn test_condition_and() {
        let size = ContainerSize::new(500.0, 400.0);
        
        let cond = ContainerCondition::And(
            Box::new(ContainerCondition::min_width(400.0)),
            Box::new(ContainerCondition::max_width(600.0)),
        );
        assert!(cond.evaluate(&size));
    }
    
    #[test]
    fn test_orientation() {
        let landscape = ContainerSize::new(800.0, 600.0);
        let portrait = ContainerSize::new(600.0, 800.0);
        let square = ContainerSize::new(500.0, 500.0);
        
        let cond = ContainerCondition::Orientation(Orientation::Landscape);
        assert!(cond.evaluate(&landscape));
        assert!(!cond.evaluate(&portrait));
        
        let cond = ContainerCondition::Orientation(Orientation::Square);
        assert!(cond.evaluate(&square));
    }
    
    #[test]
    fn test_container_context() {
        let mut ctx = ContainerContext::new();
        
        ctx.push_container(ContainerInfo {
            name: "card".to_string(),
            container_type: ContainerType::InlineSize,
            size: ContainerSize::new(300.0, 200.0),
        });
        
        let cond = ContainerCondition::min_width(200.0);
        assert!(ctx.evaluate_query(&cond, ContainerType::InlineSize));
        
        let cond = ContainerCondition::min_width(400.0);
        assert!(!ctx.evaluate_query(&cond, ContainerType::InlineSize));
        
        ctx.pop_container();
        assert_eq!(ctx.depth(), 0);
    }
}
