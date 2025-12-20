//! Container Queries
//!
//! CSS container queries for responsive components.

use std::collections::HashMap;

/// Container query context
#[derive(Debug, Clone)]
pub struct ContainerContext {
    /// Container name
    pub name: Option<String>,
    /// Container type
    pub container_type: ContainerType,
    /// Current dimensions
    pub width: f32,
    pub height: f32,
    /// Aspect ratio
    pub aspect_ratio: f32,
    /// Inline size
    pub inline_size: f32,
    /// Block size
    pub block_size: f32,
}

/// Container type
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContainerType {
    #[default]
    Normal,
    InlineSize,
    Size,
}

/// Container query condition
#[derive(Debug, Clone)]
pub enum ContainerQuery {
    MinWidth(f32),
    MaxWidth(f32),
    MinHeight(f32),
    MaxHeight(f32),
    Width(f32),
    Height(f32),
    AspectRatio(f32, f32),
    Orientation(Orientation),
    And(Vec<ContainerQuery>),
    Or(Vec<ContainerQuery>),
    Not(Box<ContainerQuery>),
}

/// Orientation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

impl ContainerContext {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            name: None,
            container_type: ContainerType::Normal,
            width,
            height,
            aspect_ratio: if height > 0.0 { width / height } else { 1.0 },
            inline_size: width,
            block_size: height,
        }
    }
    
    /// Check if query matches
    pub fn matches(&self, query: &ContainerQuery) -> bool {
        match query {
            ContainerQuery::MinWidth(w) => self.width >= *w,
            ContainerQuery::MaxWidth(w) => self.width <= *w,
            ContainerQuery::MinHeight(h) => self.height >= *h,
            ContainerQuery::MaxHeight(h) => self.height <= *h,
            ContainerQuery::Width(w) => (self.width - w).abs() < 0.01,
            ContainerQuery::Height(h) => (self.height - h).abs() < 0.01,
            ContainerQuery::AspectRatio(w, h) => {
                let target = w / h;
                (self.aspect_ratio - target).abs() < 0.01
            }
            ContainerQuery::Orientation(o) => {
                let current = if self.width > self.height {
                    Orientation::Landscape
                } else {
                    Orientation::Portrait
                };
                current == *o
            }
            ContainerQuery::And(queries) => queries.iter().all(|q| self.matches(q)),
            ContainerQuery::Or(queries) => queries.iter().any(|q| self.matches(q)),
            ContainerQuery::Not(query) => !self.matches(query),
        }
    }
}

/// Container registry
#[derive(Debug, Default)]
pub struct ContainerRegistry {
    containers: HashMap<String, ContainerContext>,
}

impl ContainerRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a container
    pub fn register(&mut self, name: &str, context: ContainerContext) {
        self.containers.insert(name.to_string(), context);
    }
    
    /// Get container by name
    pub fn get(&self, name: &str) -> Option<&ContainerContext> {
        self.containers.get(name)
    }
    
    /// Find matching container
    pub fn find_ancestor(&self, names: &[&str]) -> Option<&ContainerContext> {
        for name in names {
            if let Some(ctx) = self.containers.get(*name) {
                return Some(ctx);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_container_query() {
        let ctx = ContainerContext::new(400.0, 300.0);
        
        assert!(ctx.matches(&ContainerQuery::MinWidth(300.0)));
        assert!(!ctx.matches(&ContainerQuery::MinWidth(500.0)));
        assert!(ctx.matches(&ContainerQuery::Orientation(Orientation::Landscape)));
    }
}
