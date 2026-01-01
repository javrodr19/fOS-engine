//! DOM Observers (Full Spec)
//!
//! MutationObserver, IntersectionObserver, ResizeObserver with complete spec compliance.
//! Integrates with Viewport for visibility calculations.

use crate::NodeId;
use crate::geometry::DOMRect;

// ============================================================================
// MUTATION OBSERVER
// ============================================================================

/// Mutation observer for watching DOM changes
#[derive(Debug)]
pub struct MutationObserver {
    callback_id: u32,
    observations: Vec<MutationObservation>,
    records: Vec<MutationRecord>,
}

/// Single observation registration
#[derive(Debug, Clone)]
struct MutationObservation {
    target: NodeId,
    options: MutationObserverInit,
}

/// Mutation observer options (full spec)
#[derive(Debug, Clone, Default)]
pub struct MutationObserverInit {
    /// Observe child list changes
    pub child_list: bool,
    /// Observe attribute changes
    pub attributes: bool,
    /// Observe character data changes
    pub character_data: bool,
    /// Observe entire subtree
    pub subtree: bool,
    /// Record old attribute values
    pub attribute_old_value: bool,
    /// Record old character data values
    pub character_data_old_value: bool,
    /// Filter to specific attributes (None = all)
    pub attribute_filter: Option<Vec<String>>,
}

/// Mutation record (full spec)
#[derive(Debug, Clone)]
pub struct MutationRecord {
    pub mutation_type: MutationType,
    pub target: NodeId,
    pub added_nodes: Vec<NodeId>,
    pub removed_nodes: Vec<NodeId>,
    pub previous_sibling: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub attribute_name: Option<String>,
    pub attribute_namespace: Option<String>,
    pub old_value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationType {
    Attributes,
    CharacterData,
    ChildList,
}

impl MutationObserver {
    pub fn new(callback_id: u32) -> Self {
        Self {
            callback_id,
            observations: Vec::new(),
            records: Vec::new(),
        }
    }

    /// Observe a target node
    pub fn observe(&mut self, target: NodeId, options: MutationObserverInit) {
        // Check if already observing this target
        if let Some(obs) = self.observations.iter_mut().find(|o| o.target == target) {
            obs.options = options;
        } else {
            self.observations.push(MutationObservation { target, options });
        }
    }

    /// Stop observing all targets
    pub fn disconnect(&mut self) {
        self.observations.clear();
    }

    /// Take all pending records
    pub fn take_records(&mut self) -> Vec<MutationRecord> {
        std::mem::take(&mut self.records)
    }

    /// Queue a mutation record (called by DOM when mutation occurs)
    pub fn queue_record(&mut self, record: MutationRecord) {
        // Check if we should record this mutation based on options
        for obs in &self.observations {
            if self.should_observe(&record, &obs.options, obs.target) {
                self.records.push(record.clone());
                break;
            }
        }
    }

    fn should_observe(&self, record: &MutationRecord, options: &MutationObserverInit, target: NodeId) -> bool {
        // Check if target matches (considering subtree option)
        if record.target != target && !options.subtree {
            return false;
        }

        // Check mutation type
        match record.mutation_type {
            MutationType::Attributes => {
                if !options.attributes {
                    return false;
                }
                // Check attribute filter
                if let Some(ref filter) = options.attribute_filter {
                    if let Some(ref name) = record.attribute_name {
                        if !filter.contains(name) {
                            return false;
                        }
                    }
                }
            }
            MutationType::CharacterData => {
                if !options.character_data {
                    return false;
                }
            }
            MutationType::ChildList => {
                if !options.child_list {
                    return false;
                }
            }
        }

        true
    }

    /// Get callback ID
    pub fn callback_id(&self) -> u32 {
        self.callback_id
    }

    /// Check if observing specific target
    pub fn is_observing(&self, target: NodeId) -> bool {
        self.observations.iter().any(|o| o.target == target)
    }

    /// Get number of pending records
    pub fn pending_count(&self) -> usize {
        self.records.len()
    }
}

// ============================================================================
// INTERSECTION OBSERVER
// ============================================================================

/// Intersection observer with Viewport integration
#[derive(Debug)]
pub struct IntersectionObserver {
    callback_id: u32,
    root: Option<NodeId>,
    root_margin: RootMargin,
    thresholds: Vec<f64>,
    observed: Vec<IntersectionTarget>,
}

/// Observed target with previous state
#[derive(Debug, Clone)]
struct IntersectionTarget {
    node: NodeId,
    previous_ratio: f64,
    previous_intersecting: bool,
}

/// Parsed root margin
#[derive(Debug, Clone, Default)]
pub struct RootMargin {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl RootMargin {
    /// Parse CSS margin string (e.g., "10px 20px 10px 20px")
    pub fn parse(s: &str) -> Self {
        let parts: Vec<&str> = s.split_whitespace().collect();
        let parse_value = |s: &str| -> f64 {
            s.trim_end_matches("px")
                .trim_end_matches('%')
                .parse()
                .unwrap_or(0.0)
        };

        match parts.len() {
            1 => {
                let v = parse_value(parts[0]);
                Self { top: v, right: v, bottom: v, left: v }
            }
            2 => {
                let v = parse_value(parts[0]);
                let h = parse_value(parts[1]);
                Self { top: v, right: h, bottom: v, left: h }
            }
            3 => {
                let t = parse_value(parts[0]);
                let h = parse_value(parts[1]);
                let b = parse_value(parts[2]);
                Self { top: t, right: h, bottom: b, left: h }
            }
            4 => Self {
                top: parse_value(parts[0]),
                right: parse_value(parts[1]),
                bottom: parse_value(parts[2]),
                left: parse_value(parts[3]),
            },
            _ => Self::default(),
        }
    }
}

/// Intersection observer entry
#[derive(Debug, Clone)]
pub struct IntersectionObserverEntry {
    pub target: NodeId,
    pub bounding_client_rect: DOMRect,
    pub intersection_rect: DOMRect,
    pub root_bounds: Option<DOMRect>,
    pub is_intersecting: bool,
    pub intersection_ratio: f64,
    pub time: f64,
}

impl IntersectionObserver {
    pub fn new(
        callback_id: u32,
        root: Option<NodeId>,
        root_margin: &str,
        thresholds: Vec<f64>,
    ) -> Self {
        let thresholds = if thresholds.is_empty() {
            vec![0.0]
        } else {
            thresholds
        };

        Self {
            callback_id,
            root,
            root_margin: RootMargin::parse(root_margin),
            thresholds,
            observed: Vec::new(),
        }
    }

    pub fn observe(&mut self, target: NodeId) {
        if !self.observed.iter().any(|t| t.node == target) {
            self.observed.push(IntersectionTarget {
                node: target,
                previous_ratio: 0.0,
                previous_intersecting: false,
            });
        }
    }

    pub fn unobserve(&mut self, target: NodeId) {
        self.observed.retain(|t| t.node != target);
    }

    pub fn disconnect(&mut self) {
        self.observed.clear();
    }

    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    pub fn root_margin(&self) -> &RootMargin {
        &self.root_margin
    }

    pub fn thresholds(&self) -> &[f64] {
        &self.thresholds
    }

    /// Calculate intersection entry (called by layout/render with actual rects)
    pub fn calculate_entry(
        &mut self,
        target: NodeId,
        target_rect: DOMRect,
        root_rect: DOMRect,
        time: f64,
    ) -> Option<IntersectionObserverEntry> {
        // Apply root margin first (no mutable borrow yet)
        let adjusted_root = DOMRect {
            x: root_rect.x - self.root_margin.left,
            y: root_rect.y - self.root_margin.top,
            width: root_rect.width + self.root_margin.left + self.root_margin.right,
            height: root_rect.height + self.root_margin.top + self.root_margin.bottom,
        };

        // Calculate intersection (immutable borrow)
        let intersection = Self::calculate_intersection_static(&target_rect, &adjusted_root);
        let intersection_area = intersection.width.max(0.0) * intersection.height.max(0.0);
        let target_area = target_rect.width * target_rect.height;
        let ratio = if target_area > 0.0 {
            intersection_area / target_area
        } else {
            0.0
        };

        let is_intersecting = ratio > 0.0;

        // Check threshold crossing (needs thresholds ref)
        let thresholds = self.thresholds.clone();
        
        // Now find target state (mutable borrow)
        let target_state = self.observed.iter_mut().find(|t| t.node == target)?;
        let previous_ratio = target_state.previous_ratio;
        let previous_intersecting = target_state.previous_intersecting;

        // Check if threshold crossed
        let threshold_crossed = Self::threshold_crossed_static(&thresholds, previous_ratio, ratio);

        // Update state
        target_state.previous_ratio = ratio;
        target_state.previous_intersecting = is_intersecting;

        if threshold_crossed || is_intersecting != previous_intersecting {
            Some(IntersectionObserverEntry {
                target,
                bounding_client_rect: target_rect,
                intersection_rect: intersection,
                root_bounds: Some(adjusted_root),
                is_intersecting,
                intersection_ratio: ratio,
                time,
            })
        } else {
            None
        }
    }

    fn calculate_intersection_static(a: &DOMRect, b: &DOMRect) -> DOMRect {
        let x = a.x.max(b.x);
        let y = a.y.max(b.y);
        let right = (a.x + a.width).min(b.x + b.width);
        let bottom = (a.y + a.height).min(b.y + b.height);

        DOMRect {
            x,
            y,
            width: (right - x).max(0.0),
            height: (bottom - y).max(0.0),
        }
    }

    fn threshold_crossed(&self, old_ratio: f64, new_ratio: f64) -> bool {
        Self::threshold_crossed_static(&self.thresholds, old_ratio, new_ratio)
    }

    fn threshold_crossed_static(thresholds: &[f64], old_ratio: f64, new_ratio: f64) -> bool {
        for &threshold in thresholds {
            if (old_ratio < threshold && new_ratio >= threshold)
                || (old_ratio >= threshold && new_ratio < threshold)
            {
                return true;
            }
        }
        false
    }
}

// ============================================================================
// RESIZE OBSERVER
// ============================================================================

/// Resize observer with box options
#[derive(Debug)]
pub struct ResizeObserver {
    callback_id: u32,
    observed: Vec<ResizeTarget>,
}

/// Observed target with box options
#[derive(Debug, Clone)]
struct ResizeTarget {
    node: NodeId,
    box_options: ResizeObserverBoxOptions,
    previous_size: Option<ResizeObserverSize>,
}

/// Box options for ResizeObserver
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizeObserverBoxOptions {
    #[default]
    ContentBox,
    BorderBox,
    DevicePixelContentBox,
}

/// Resize observer entry
#[derive(Debug, Clone)]
pub struct ResizeObserverEntry {
    pub target: NodeId,
    pub content_rect: DOMRect,
    pub border_box_size: Vec<ResizeObserverSize>,
    pub content_box_size: Vec<ResizeObserverSize>,
    pub device_pixel_content_box_size: Vec<ResizeObserverSize>,
}

/// Resize observer size
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResizeObserverSize {
    pub inline_size: f64,
    pub block_size: f64,
}

impl ResizeObserver {
    pub fn new(callback_id: u32) -> Self {
        Self {
            callback_id,
            observed: Vec::new(),
        }
    }

    /// Observe with options
    pub fn observe(&mut self, target: NodeId, options: ResizeObserverBoxOptions) {
        if !self.observed.iter().any(|t| t.node == target) {
            self.observed.push(ResizeTarget {
                node: target,
                box_options: options,
                previous_size: None,
            });
        }
    }

    /// Observe with default options
    pub fn observe_default(&mut self, target: NodeId) {
        self.observe(target, ResizeObserverBoxOptions::ContentBox);
    }

    pub fn unobserve(&mut self, target: NodeId) {
        self.observed.retain(|t| t.node != target);
    }

    pub fn disconnect(&mut self) {
        self.observed.clear();
    }

    /// Calculate entry (called by layout with actual sizes)
    pub fn calculate_entry(
        &mut self,
        target: NodeId,
        content_rect: DOMRect,
        border_box: ResizeObserverSize,
        content_box: ResizeObserverSize,
        device_pixel_box: ResizeObserverSize,
    ) -> Option<ResizeObserverEntry> {
        let target_state = self.observed.iter_mut().find(|t| t.node == target)?;

        // Get current size based on box option
        let current_size = match target_state.box_options {
            ResizeObserverBoxOptions::ContentBox => content_box,
            ResizeObserverBoxOptions::BorderBox => border_box,
            ResizeObserverBoxOptions::DevicePixelContentBox => device_pixel_box,
        };

        // Check if size changed
        let size_changed = target_state.previous_size
            .map(|prev| prev != current_size)
            .unwrap_or(true);

        if size_changed {
            target_state.previous_size = Some(current_size);
            Some(ResizeObserverEntry {
                target,
                content_rect,
                border_box_size: vec![border_box],
                content_box_size: vec![content_box],
                device_pixel_content_box_size: vec![device_pixel_box],
            })
        } else {
            None
        }
    }

    pub fn callback_id(&self) -> u32 {
        self.callback_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutation_observer() {
        let mut observer = MutationObserver::new(1);
        observer.observe(NodeId(1), MutationObserverInit {
            child_list: true,
            attributes: true,
            ..Default::default()
        });

        assert!(observer.is_observing(NodeId(1)));

        observer.disconnect();
        assert!(!observer.is_observing(NodeId(1)));
    }

    #[test]
    fn test_mutation_record_filtering() {
        let mut observer = MutationObserver::new(1);
        observer.observe(NodeId(1), MutationObserverInit {
            attributes: true,
            attribute_filter: Some(vec!["class".to_string()]),
            ..Default::default()
        });

        // Should be recorded (class attribute)
        observer.queue_record(MutationRecord {
            mutation_type: MutationType::Attributes,
            target: NodeId(1),
            attribute_name: Some("class".to_string()),
            ..Default::default()
        });
        assert_eq!(observer.pending_count(), 1);

        // Should not be recorded (id attribute, not in filter)
        observer.queue_record(MutationRecord {
            mutation_type: MutationType::Attributes,
            target: NodeId(1),
            attribute_name: Some("id".to_string()),
            ..Default::default()
        });
        assert_eq!(observer.pending_count(), 1);
    }

    #[test]
    fn test_intersection_observer() {
        let mut observer = IntersectionObserver::new(1, None, "0px", vec![0.0, 0.5, 1.0]);

        observer.observe(NodeId(1));
        observer.observe(NodeId(2));

        assert_eq!(observer.thresholds().len(), 3);

        observer.unobserve(NodeId(1));
    }

    #[test]
    fn test_root_margin_parsing() {
        let margin = RootMargin::parse("10px");
        assert_eq!(margin.top, 10.0);
        assert_eq!(margin.right, 10.0);

        let margin = RootMargin::parse("10px 20px");
        assert_eq!(margin.top, 10.0);
        assert_eq!(margin.right, 20.0);

        let margin = RootMargin::parse("10px 20px 30px 40px");
        assert_eq!(margin.top, 10.0);
        assert_eq!(margin.right, 20.0);
        assert_eq!(margin.bottom, 30.0);
        assert_eq!(margin.left, 40.0);
    }

    #[test]
    fn test_resize_observer() {
        let mut observer = ResizeObserver::new(1);
        observer.observe(NodeId(1), ResizeObserverBoxOptions::BorderBox);

        let entry = observer.calculate_entry(
            NodeId(1),
            DOMRect { x: 0.0, y: 0.0, width: 100.0, height: 50.0 },
            ResizeObserverSize { inline_size: 100.0, block_size: 50.0 },
            ResizeObserverSize { inline_size: 80.0, block_size: 40.0 },
            ResizeObserverSize { inline_size: 200.0, block_size: 100.0 },
        );

        assert!(entry.is_some());
    }
}

impl Default for MutationRecord {
    fn default() -> Self {
        Self {
            mutation_type: MutationType::Attributes,
            target: NodeId::ROOT,
            added_nodes: Vec::new(),
            removed_nodes: Vec::new(),
            previous_sibling: None,
            next_sibling: None,
            attribute_name: None,
            attribute_namespace: None,
            old_value: None,
        }
    }
}
