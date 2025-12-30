//! Accessibility
//!
//! ARIA support, accessibility tree, screen reader integration.

use std::collections::HashMap;

/// ARIA role
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AriaRole {
    // Landmark roles
    Banner,
    Complementary,
    ContentInfo,
    Form,
    Main,
    Navigation,
    Region,
    Search,
    
    // Widget roles
    Alert,
    AlertDialog,
    Button,
    Checkbox,
    Dialog,
    GridCell,
    Link,
    Log,
    Marquee,
    MenuItem,
    MenuItemCheckbox,
    MenuItemRadio,
    Option,
    ProgressBar,
    Radio,
    ScrollBar,
    SearchBox,
    Slider,
    SpinButton,
    Status,
    Switch,
    Tab,
    TabPanel,
    TextBox,
    Timer,
    Tooltip,
    TreeItem,
    
    // Document structure roles
    Application,
    Article,
    Cell,
    ColumnHeader,
    Definition,
    Directory,
    Document,
    Feed,
    Figure,
    Group,
    Heading,
    Img,
    List,
    ListItem,
    Math,
    None,
    Note,
    Presentation,
    Row,
    RowGroup,
    RowHeader,
    Separator,
    Table,
    Term,
    Toolbar,
    Tree,
    TreeGrid,
    
    // Live region roles
    Timer2,
    
    // Abstract roles (not used directly)
    Custom(String),
}

impl AriaRole {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s.to_lowercase().as_str() {
            "banner" => Self::Banner,
            "complementary" => Self::Complementary,
            "contentinfo" => Self::ContentInfo,
            "form" => Self::Form,
            "main" => Self::Main,
            "navigation" => Self::Navigation,
            "region" => Self::Region,
            "search" => Self::Search,
            "alert" => Self::Alert,
            "alertdialog" => Self::AlertDialog,
            "button" => Self::Button,
            "checkbox" => Self::Checkbox,
            "dialog" => Self::Dialog,
            "link" => Self::Link,
            "menuitem" => Self::MenuItem,
            "option" => Self::Option,
            "progressbar" => Self::ProgressBar,
            "radio" => Self::Radio,
            "slider" => Self::Slider,
            "spinbutton" => Self::SpinButton,
            "status" => Self::Status,
            "switch" => Self::Switch,
            "tab" => Self::Tab,
            "tabpanel" => Self::TabPanel,
            "textbox" => Self::TextBox,
            "tooltip" => Self::Tooltip,
            "heading" => Self::Heading,
            "img" => Self::Img,
            "list" => Self::List,
            "listitem" => Self::ListItem,
            "table" => Self::Table,
            "row" => Self::Row,
            "cell" => Self::Cell,
            "tree" => Self::Tree,
            "treeitem" => Self::TreeItem,
            "group" => Self::Group,
            "none" => Self::None,
            "presentation" => Self::Presentation,
            _ => Self::Custom(s.to_string()),
        })
    }
    
    pub fn is_landmark(&self) -> bool {
        matches!(self, 
            Self::Banner | Self::Complementary | Self::ContentInfo | 
            Self::Form | Self::Main | Self::Navigation | Self::Region | Self::Search
        )
    }
    
    pub fn is_widget(&self) -> bool {
        matches!(self,
            Self::Button | Self::Checkbox | Self::Link | Self::Radio |
            Self::Slider | Self::SpinButton | Self::Switch | Self::TextBox
        )
    }
}

/// ARIA state/property
#[derive(Debug, Clone)]
pub enum AriaState {
    // Widget states
    Checked(TriState),
    Disabled(bool),
    Expanded(bool),
    Hidden(bool),
    Invalid(bool),
    Pressed(TriState),
    Selected(bool),
    
    // Live region properties
    Atomic(bool),
    Busy(bool),
    Live(LivePoliteness),
    Relevant(RelevantChanges),
    
    // Relationship properties
    ActiveDescendant(String),
    Controls(Vec<String>),
    DescribedBy(Vec<String>),
    LabelledBy(Vec<String>),
    Owns(Vec<String>),
    
    // Other properties
    Label(String),
    Level(u32),
    ValueMin(f64),
    ValueMax(f64),
    ValueNow(f64),
    ValueText(String),
    Orientation(Orientation),
    HasPopup(PopupType),
    Modal(bool),
    Multiline(bool),
    Multiselectable(bool),
    ReadOnly(bool),
    Required(bool),
    Sort(SortDirection),
    Current(CurrentType),
}

/// Tri-state value (true/false/mixed)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriState {
    True,
    False,
    Mixed,
}

/// Live region politeness
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LivePoliteness {
    #[default]
    Off,
    Polite,
    Assertive,
}

/// Relevant changes for live regions
#[derive(Debug, Clone, Default)]
pub struct RelevantChanges {
    pub additions: bool,
    pub removals: bool,
    pub text: bool,
    pub all: bool,
}

/// Orientation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// Popup type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupType {
    Menu,
    Listbox,
    Tree,
    Grid,
    Dialog,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
    Other,
    None,
}

/// Current type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentType {
    Page,
    Step,
    Location,
    Date,
    Time,
    True,
}

/// Accessibility tree node
#[derive(Debug)]
pub struct AccessibilityNode {
    /// Node ID (matches DOM node ID)
    pub id: u64,
    /// ARIA role
    pub role: Option<AriaRole>,
    /// Name (accessible name)
    pub name: Option<String>,
    /// Description
    pub description: Option<String>,
    /// ARIA states
    pub states: Vec<AriaState>,
    /// Children
    pub children: Vec<u64>,
    /// Parent
    pub parent: Option<u64>,
    /// Is focusable
    pub focusable: bool,
    /// Tab index
    pub tab_index: Option<i32>,
    /// Bounding box
    pub bounds: Option<AccessibilityBounds>,
}

/// Bounding box
#[derive(Debug, Clone, Copy)]
pub struct AccessibilityBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl AccessibilityNode {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            role: None,
            name: None,
            description: None,
            states: Vec::new(),
            children: Vec::new(),
            parent: None,
            focusable: false,
            tab_index: None,
            bounds: None,
        }
    }
    
    pub fn with_role(mut self, role: AriaRole) -> Self {
        self.role = Some(role);
        self
    }
    
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }
    
    pub fn is_hidden(&self) -> bool {
        self.states.iter().any(|s| matches!(s, AriaState::Hidden(true)))
    }
    
    pub fn get_label(&self) -> Option<&str> {
        for state in &self.states {
            if let AriaState::Label(label) = state {
                return Some(label);
            }
        }
        self.name.as_deref()
    }
}

/// Accessibility tree
#[derive(Debug, Default)]
pub struct AccessibilityTree {
    /// Nodes by ID
    nodes: HashMap<u64, AccessibilityNode>,
    /// Root nodes
    pub roots: Vec<u64>,
    /// Currently focused node
    pub focused: Option<u64>,
}

impl AccessibilityTree {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add node
    pub fn add_node(&mut self, node: AccessibilityNode) {
        if node.parent.is_none() {
            self.roots.push(node.id);
        }
        self.nodes.insert(node.id, node);
    }
    
    /// Get node
    pub fn get(&self, id: u64) -> Option<&AccessibilityNode> {
        self.nodes.get(&id)
    }
    
    /// Get mutable node
    pub fn get_mut(&mut self, id: u64) -> Option<&mut AccessibilityNode> {
        self.nodes.get_mut(&id)
    }
    
    /// Remove node
    pub fn remove(&mut self, id: u64) {
        self.nodes.remove(&id);
        self.roots.retain(|&r| r != id);
    }
    
    /// Get all focusable nodes in tab order
    pub fn get_tab_order(&self) -> Vec<u64> {
        let mut focusable: Vec<(u64, i32)> = self.nodes.values()
            .filter(|n| n.focusable && !n.is_hidden())
            .map(|n| (n.id, n.tab_index.unwrap_or(0)))
            .collect();
        
        // Sort: negative tabindex, then 0 in DOM order, then positive
        focusable.sort_by(|a, b| {
            match (a.1, b.1) {
                (x, y) if x < 0 && y >= 0 => std::cmp::Ordering::Greater,
                (x, y) if x >= 0 && y < 0 => std::cmp::Ordering::Less,
                (0, 0) => a.0.cmp(&b.0), // DOM order
                (x, y) => x.cmp(&y),
            }
        });
        
        focusable.into_iter().map(|(id, _)| id).collect()
    }
    
    /// Get landmarks
    pub fn get_landmarks(&self) -> Vec<u64> {
        self.nodes.values()
            .filter(|n| n.role.as_ref().map(|r| r.is_landmark()).unwrap_or(false))
            .map(|n| n.id)
            .collect()
    }
    
    /// Get live regions
    pub fn get_live_regions(&self) -> Vec<u64> {
        self.nodes.values()
            .filter(|n| n.states.iter().any(|s| matches!(s, AriaState::Live(_))))
            .map(|n| n.id)
            .collect()
    }
    
    /// Focus next element
    pub fn focus_next(&mut self) -> Option<u64> {
        let order = self.get_tab_order();
        if order.is_empty() {
            return None;
        }
        
        let next_idx = if let Some(current) = self.focused {
            order.iter().position(|&id| id == current)
                .map(|i| (i + 1) % order.len())
                .unwrap_or(0)
        } else {
            0
        };
        
        self.focused = Some(order[next_idx]);
        self.focused
    }
    
    /// Focus previous element
    pub fn focus_prev(&mut self) -> Option<u64> {
        let order = self.get_tab_order();
        if order.is_empty() {
            return None;
        }
        
        let prev_idx = if let Some(current) = self.focused {
            order.iter().position(|&id| id == current)
                .map(|i| if i == 0 { order.len() - 1 } else { i - 1 })
                .unwrap_or(order.len() - 1)
        } else {
            order.len() - 1
        };
        
        self.focused = Some(order[prev_idx]);
        self.focused
    }
}

/// Focus trap for modals
#[derive(Debug)]
pub struct FocusTrap {
    /// Container node ID
    pub container: u64,
    /// First focusable element
    pub first: Option<u64>,
    /// Last focusable element
    pub last: Option<u64>,
    /// Is active
    pub active: bool,
}

impl FocusTrap {
    pub fn new(container: u64) -> Self {
        Self {
            container,
            first: None,
            last: None,
            active: false,
        }
    }
    
    /// Activate trap
    pub fn activate(&mut self, first: u64, last: u64) {
        self.first = Some(first);
        self.last = Some(last);
        self.active = true;
    }
    
    /// Deactivate trap
    pub fn deactivate(&mut self) {
        self.active = false;
    }
    
    /// Handle tab key - returns focus target
    pub fn handle_tab(&self, current: u64, shift: bool) -> Option<u64> {
        if !self.active {
            return None;
        }
        
        if shift {
            if Some(current) == self.first {
                return self.last;
            }
        } else {
            if Some(current) == self.last {
                return self.first;
            }
        }
        
        None
    }
}

/// Skip link
#[derive(Debug, Clone)]
pub struct SkipLink {
    pub text: String,
    pub target_id: u64,
}

/// Live region announcement
#[derive(Debug, Clone)]
pub struct LiveAnnouncement {
    pub text: String,
    pub politeness: LivePoliteness,
    pub timestamp: u64,
}

/// Screen reader output
#[derive(Debug, Default)]
pub struct ScreenReaderOutput {
    /// Pending announcements
    pub announcements: Vec<LiveAnnouncement>,
}

impl ScreenReaderOutput {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Announce text
    pub fn announce(&mut self, text: &str, politeness: LivePoliteness) {
        self.announcements.push(LiveAnnouncement {
            text: text.to_string(),
            politeness,
            timestamp: 0, // Would use actual timestamp
        });
    }
    
    /// Get next announcement
    pub fn next(&mut self) -> Option<LiveAnnouncement> {
        if self.announcements.is_empty() {
            None
        } else {
            // Assertive announcements first
            if let Some(idx) = self.announcements.iter()
                .position(|a| a.politeness == LivePoliteness::Assertive) {
                Some(self.announcements.remove(idx))
            } else {
                Some(self.announcements.remove(0))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_aria_role() {
        let role = AriaRole::from_str("button").unwrap();
        assert_eq!(role, AriaRole::Button);
        assert!(role.is_widget());
        
        let landmark = AriaRole::from_str("navigation").unwrap();
        assert!(landmark.is_landmark());
    }
    
    #[test]
    fn test_accessibility_tree() {
        let mut tree = AccessibilityTree::new();
        
        let mut node1 = AccessibilityNode::new(1);
        node1.focusable = true;
        node1.tab_index = Some(0);
        
        let mut node2 = AccessibilityNode::new(2);
        node2.focusable = true;
        node2.tab_index = Some(0);
        
        tree.add_node(node1);
        tree.add_node(node2);
        
        let order = tree.get_tab_order();
        assert_eq!(order.len(), 2);
    }
    
    #[test]
    fn test_focus_trap() {
        let mut trap = FocusTrap::new(1);
        trap.activate(2, 5);
        
        // At last element, tab should go to first
        assert_eq!(trap.handle_tab(5, false), Some(2));
        // At first element, shift+tab should go to last
        assert_eq!(trap.handle_tab(2, true), Some(5));
    }
    
    #[test]
    fn test_screen_reader() {
        let mut sr = ScreenReaderOutput::new();
        
        sr.announce("Hello world", LivePoliteness::Polite);
        sr.announce("Urgent!", LivePoliteness::Assertive);
        
        // Assertive should come first
        let first = sr.next().unwrap();
        assert_eq!(first.politeness, LivePoliteness::Assertive);
    }
}
