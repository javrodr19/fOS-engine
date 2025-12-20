//! ARIA Support
//!
//! ARIA roles, states, and properties.

use std::collections::HashMap;

/// ARIA role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    Menu,
    MenuBar,
    MenuItem,
    MenuItemCheckbox,
    MenuItemRadio,
    Option,
    ProgressBar,
    Radio,
    RadioGroup,
    ScrollBar,
    Slider,
    SpinButton,
    Status,
    Switch,
    Tab,
    TabList,
    TabPanel,
    TextBox,
    Timer,
    ToolTip,
    Tree,
    TreeGrid,
    TreeItem,
    
    // Document structure
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
    
    // Abstract (not to be used directly)
    Application,
    Generic,
}

impl AriaRole {
    /// Parse from string
    pub fn parse(s: &str) -> Option<Self> {
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
            "menu" => Self::Menu,
            "menubar" => Self::MenuBar,
            "menuitem" => Self::MenuItem,
            "option" => Self::Option,
            "progressbar" => Self::ProgressBar,
            "radio" => Self::Radio,
            "radiogroup" => Self::RadioGroup,
            "slider" => Self::Slider,
            "spinbutton" => Self::SpinButton,
            "status" => Self::Status,
            "switch" => Self::Switch,
            "tab" => Self::Tab,
            "tablist" => Self::TabList,
            "tabpanel" => Self::TabPanel,
            "textbox" => Self::TextBox,
            "tooltip" => Self::ToolTip,
            "tree" => Self::Tree,
            "treeitem" => Self::TreeItem,
            "article" => Self::Article,
            "heading" => Self::Heading,
            "img" => Self::Img,
            "list" => Self::List,
            "listitem" => Self::ListItem,
            "table" => Self::Table,
            "row" => Self::Row,
            "cell" => Self::Cell,
            "none" | "presentation" => Self::Presentation,
            _ => return None,
        })
    }
    
    /// Check if role is widget
    pub fn is_widget(&self) -> bool {
        matches!(self, 
            Self::Button | Self::Checkbox | Self::Link | Self::Menu |
            Self::MenuItem | Self::Radio | Self::Slider | Self::SpinButton |
            Self::Switch | Self::Tab | Self::TextBox | Self::TreeItem
        )
    }
    
    /// Check if role is landmark
    pub fn is_landmark(&self) -> bool {
        matches!(self,
            Self::Banner | Self::Complementary | Self::ContentInfo |
            Self::Form | Self::Main | Self::Navigation | Self::Region | Self::Search
        )
    }
}

/// ARIA state/property
#[derive(Debug, Clone, PartialEq)]
pub enum AriaState {
    // Boolean states
    Checked(Option<bool>), // true, false, mixed (None)
    Disabled(bool),
    Expanded(bool),
    Hidden(bool),
    Invalid(bool),
    Pressed(Option<bool>), // true, false, mixed
    Selected(bool),
    
    // String states
    Label(String),
    LabelledBy(Vec<String>),
    DescribedBy(Vec<String>),
    
    // Numeric
    ValueNow(f64),
    ValueMin(f64),
    ValueMax(f64),
    ValueText(String),
    Level(u32),
    PosInSet(u32),
    SetSize(u32),
    
    // Live region
    Live(LiveRegionMode),
    Atomic(bool),
    Relevant(Vec<LiveRelevant>),
    Busy(bool),
}

/// Live region mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveRegionMode {
    Off,
    Polite,
    Assertive,
}

/// Live region relevant values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveRelevant {
    Additions,
    Removals,
    Text,
    All,
}

/// ARIA attributes on an element
#[derive(Debug, Clone, Default)]
pub struct AriaAttributes {
    pub role: Option<AriaRole>,
    pub states: HashMap<String, AriaState>,
}

impl AriaAttributes {
    pub fn new() -> Self { Self::default() }
    
    /// Set role
    pub fn set_role(&mut self, role: AriaRole) {
        self.role = Some(role);
    }
    
    /// Parse from HTML attributes
    pub fn from_attributes(attrs: &HashMap<String, String>) -> Self {
        let mut aria = Self::new();
        
        if let Some(role) = attrs.get("role") {
            aria.role = AriaRole::parse(role);
        }
        
        // Parse aria-* attributes
        for (key, value) in attrs {
            if let Some(name) = key.strip_prefix("aria-") {
                if let Some(state) = Self::parse_state(name, value) {
                    aria.states.insert(name.to_string(), state);
                }
            }
        }
        
        aria
    }
    
    fn parse_state(name: &str, value: &str) -> Option<AriaState> {
        Some(match name {
            "checked" => AriaState::Checked(match value {
                "true" => Some(true),
                "false" => Some(false),
                "mixed" => None,
                _ => return None,
            }),
            "disabled" => AriaState::Disabled(value == "true"),
            "expanded" => AriaState::Expanded(value == "true"),
            "hidden" => AriaState::Hidden(value == "true"),
            "invalid" => AriaState::Invalid(value == "true"),
            "selected" => AriaState::Selected(value == "true"),
            "label" => AriaState::Label(value.to_string()),
            "labelledby" => AriaState::LabelledBy(value.split_whitespace().map(String::from).collect()),
            "describedby" => AriaState::DescribedBy(value.split_whitespace().map(String::from).collect()),
            "valuenow" => AriaState::ValueNow(value.parse().ok()?),
            "valuemin" => AriaState::ValueMin(value.parse().ok()?),
            "valuemax" => AriaState::ValueMax(value.parse().ok()?),
            "level" => AriaState::Level(value.parse().ok()?),
            "live" => AriaState::Live(match value {
                "polite" => LiveRegionMode::Polite,
                "assertive" => LiveRegionMode::Assertive,
                _ => LiveRegionMode::Off,
            }),
            "atomic" => AriaState::Atomic(value == "true"),
            "busy" => AriaState::Busy(value == "true"),
            _ => return None,
        })
    }
    
    /// Get label
    pub fn get_label(&self) -> Option<&str> {
        self.states.get("label").and_then(|s| {
            if let AriaState::Label(l) = s { Some(l.as_str()) } else { None }
        })
    }
    
    /// Check if expanded
    pub fn is_expanded(&self) -> Option<bool> {
        self.states.get("expanded").and_then(|s| {
            if let AriaState::Expanded(e) = s { Some(*e) } else { None }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_role() {
        assert_eq!(AriaRole::parse("button"), Some(AriaRole::Button));
        assert_eq!(AriaRole::parse("navigation"), Some(AriaRole::Navigation));
        assert!(AriaRole::Button.is_widget());
        assert!(AriaRole::Navigation.is_landmark());
    }
    
    #[test]
    fn test_aria_attributes() {
        let mut attrs = HashMap::new();
        attrs.insert("role".into(), "button".into());
        attrs.insert("aria-expanded".into(), "true".into());
        attrs.insert("aria-label".into(), "Menu".into());
        
        let aria = AriaAttributes::from_attributes(&attrs);
        assert_eq!(aria.role, Some(AriaRole::Button));
        assert_eq!(aria.is_expanded(), Some(true));
        assert_eq!(aria.get_label(), Some("Menu"));
    }
}
