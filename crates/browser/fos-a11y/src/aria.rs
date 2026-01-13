//! ARIA Support
//!
//! ARIA roles, states, and properties.

use std::collections::HashMap;

/// ARIA role - Complete WAI-ARIA 1.2 specification (82 roles)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AriaRole {
    // === Landmark Roles (8) ===
    Banner,
    Complementary,
    ContentInfo,
    Form,
    Main,
    Navigation,
    Region,
    Search,
    
    // === Widget Roles (29) ===
    Button,
    Checkbox,
    Combobox,
    Grid,
    GridCell,
    Link,
    Listbox,
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
    SearchBox,
    Slider,
    SpinButton,
    Switch,
    Tab,
    TabList,
    TabPanel,
    TextBox,
    Tree,
    TreeGrid,
    TreeItem,
    Meter,
    
    // === Document Structure Roles (25) ===
    Application,
    Article,
    Blockquote,
    Caption,
    Cell,
    Code,
    ColumnHeader,
    Definition,
    Deletion,
    Directory,
    Document,
    Emphasis,
    Feed,
    Figure,
    Generic,
    Group,
    Heading,
    Img,
    Insertion,
    List,
    ListItem,
    Math,
    None,
    Note,
    Paragraph,
    Presentation,
    Row,
    RowGroup,
    RowHeader,
    Separator,
    Strong,
    Subscript,
    Superscript,
    Table,
    Term,
    Time,
    Toolbar,
    
    // === Live Region Roles (5) ===
    Alert,
    Log,
    Marquee,
    Status,
    Timer,
    
    // === Window Roles (2) ===
    AlertDialog,
    Dialog,
    ToolTip,
    
    // === Abstract Roles (12) - for inheritance only ===
    Command,
    Composite,
    Input,
    Landmark,
    Range,
    Roletype,
    Section,
    Sectionhead,
    Select,
    Structure,
    Widget,
    Window,
}

impl AriaRole {
    /// Parse from string (all 82 roles)
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_lowercase().as_str() {
            // Landmark roles
            "banner" => Self::Banner,
            "complementary" => Self::Complementary,
            "contentinfo" => Self::ContentInfo,
            "form" => Self::Form,
            "main" => Self::Main,
            "navigation" => Self::Navigation,
            "region" => Self::Region,
            "search" => Self::Search,
            
            // Widget roles
            "button" => Self::Button,
            "checkbox" => Self::Checkbox,
            "combobox" => Self::Combobox,
            "grid" => Self::Grid,
            "gridcell" => Self::GridCell,
            "link" => Self::Link,
            "listbox" => Self::Listbox,
            "menu" => Self::Menu,
            "menubar" => Self::MenuBar,
            "menuitem" => Self::MenuItem,
            "menuitemcheckbox" => Self::MenuItemCheckbox,
            "menuitemradio" => Self::MenuItemRadio,
            "option" => Self::Option,
            "progressbar" => Self::ProgressBar,
            "radio" => Self::Radio,
            "radiogroup" => Self::RadioGroup,
            "scrollbar" => Self::ScrollBar,
            "searchbox" => Self::SearchBox,
            "slider" => Self::Slider,
            "spinbutton" => Self::SpinButton,
            "switch" => Self::Switch,
            "tab" => Self::Tab,
            "tablist" => Self::TabList,
            "tabpanel" => Self::TabPanel,
            "textbox" => Self::TextBox,
            "tree" => Self::Tree,
            "treegrid" => Self::TreeGrid,
            "treeitem" => Self::TreeItem,
            "meter" => Self::Meter,
            
            // Document structure roles
            "application" => Self::Application,
            "article" => Self::Article,
            "blockquote" => Self::Blockquote,
            "caption" => Self::Caption,
            "cell" => Self::Cell,
            "code" => Self::Code,
            "columnheader" => Self::ColumnHeader,
            "definition" => Self::Definition,
            "deletion" => Self::Deletion,
            "directory" => Self::Directory,
            "document" => Self::Document,
            "emphasis" => Self::Emphasis,
            "feed" => Self::Feed,
            "figure" => Self::Figure,
            "generic" => Self::Generic,
            "group" => Self::Group,
            "heading" => Self::Heading,
            "img" => Self::Img,
            "insertion" => Self::Insertion,
            "list" => Self::List,
            "listitem" => Self::ListItem,
            "math" => Self::Math,
            "none" | "presentation" => Self::Presentation,
            "note" => Self::Note,
            "paragraph" => Self::Paragraph,
            "row" => Self::Row,
            "rowgroup" => Self::RowGroup,
            "rowheader" => Self::RowHeader,
            "separator" => Self::Separator,
            "strong" => Self::Strong,
            "subscript" => Self::Subscript,
            "superscript" => Self::Superscript,
            "table" => Self::Table,
            "term" => Self::Term,
            "time" => Self::Time,
            "toolbar" => Self::Toolbar,
            
            // Live region roles
            "alert" => Self::Alert,
            "log" => Self::Log,
            "marquee" => Self::Marquee,
            "status" => Self::Status,
            "timer" => Self::Timer,
            
            // Window roles
            "alertdialog" => Self::AlertDialog,
            "dialog" => Self::Dialog,
            "tooltip" => Self::ToolTip,
            
            _ => return None,
        })
    }
    
    /// Check if role is widget (interactive)
    pub fn is_widget(&self) -> bool {
        matches!(self, 
            Self::Button | Self::Checkbox | Self::Combobox | Self::Grid |
            Self::GridCell | Self::Link | Self::Listbox | Self::Menu |
            Self::MenuBar | Self::MenuItem | Self::MenuItemCheckbox |
            Self::MenuItemRadio | Self::Option | Self::ProgressBar |
            Self::Radio | Self::RadioGroup | Self::ScrollBar | Self::SearchBox |
            Self::Slider | Self::SpinButton | Self::Switch | Self::Tab |
            Self::TabList | Self::TabPanel | Self::TextBox | Self::Tree |
            Self::TreeGrid | Self::TreeItem | Self::Meter
        )
    }
    
    /// Check if role is landmark
    pub fn is_landmark(&self) -> bool {
        matches!(self,
            Self::Banner | Self::Complementary | Self::ContentInfo |
            Self::Form | Self::Main | Self::Navigation | Self::Region | Self::Search
        )
    }
    
    /// Check if role is live region
    pub fn is_live_region(&self) -> bool {
        matches!(self, Self::Alert | Self::Log | Self::Marquee | Self::Status | Self::Timer)
    }
    
    /// Check if role is window
    pub fn is_window(&self) -> bool {
        matches!(self, Self::AlertDialog | Self::Dialog | Self::ToolTip)
    }
    
    /// Check if role is abstract (not for direct use)
    pub fn is_abstract(&self) -> bool {
        matches!(self,
            Self::Command | Self::Composite | Self::Input | Self::Landmark |
            Self::Range | Self::Roletype | Self::Section | Self::Sectionhead |
            Self::Select | Self::Structure | Self::Widget | Self::Window
        )
    }
    
    /// Get implicit live region politeness
    pub fn implicit_live_region(&self) -> Option<LiveRegionMode> {
        match self {
            Self::Alert => Some(LiveRegionMode::Assertive),
            Self::Log | Self::Status => Some(LiveRegionMode::Polite),
            Self::Marquee | Self::Timer => Some(LiveRegionMode::Off),
            _ => None,
        }
    }
    
    /// Check if role supports name from content
    pub fn supports_name_from_content(&self) -> bool {
        matches!(self,
            Self::Button | Self::Checkbox | Self::Link | Self::MenuItem |
            Self::MenuItemCheckbox | Self::MenuItemRadio | Self::Option |
            Self::Radio | Self::Switch | Self::Tab | Self::TreeItem |
            Self::Heading | Self::ToolTip
        )
    }
}

/// ARIA state/property
#[derive(Debug, Clone, PartialEq)]
pub enum AriaState {
    // === Boolean States ===
    Checked(Option<bool>), // true, false, mixed (None)
    Disabled(bool),
    Expanded(bool),
    Hidden(bool),
    Invalid(bool),
    Pressed(Option<bool>), // true, false, mixed
    Selected(bool),
    ReadOnly(bool),
    Required(bool),
    
    // === String Properties ===
    Label(String),
    LabelledBy(Vec<String>),
    DescribedBy(Vec<String>),
    
    // === Numeric Properties ===
    ValueNow(f64),
    ValueMin(f64),
    ValueMax(f64),
    ValueText(String),
    Level(u32),
    PosInSet(u32),
    SetSize(u32),
    ColCount(u32),
    ColIndex(u32),
    ColSpan(u32),
    RowCount(u32),
    RowIndex(u32),
    RowSpan(u32),
    
    // === Relationship Properties ===
    ActiveDescendant(String),
    Controls(Vec<String>),
    Details(Vec<String>),
    ErrorMessage(String),
    FlowTo(Vec<String>),
    Owns(Vec<String>),
    
    // === Live Region Properties ===
    Live(LiveRegionMode),
    Atomic(bool),
    Relevant(Vec<LiveRelevant>),
    Busy(bool),
    
    // === Drag and Drop ===
    Grabbed(Option<bool>),
    DropEffect(Vec<DropEffect>),
}

/// Drop effect for drag and drop
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropEffect {
    Copy,
    Execute,
    Link,
    Move,
    None,
    Popup,
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
            // Boolean states
            "checked" => AriaState::Checked(match value {
                "true" => Some(true),
                "false" => Some(false),
                "mixed" => None,
                _ => return None,
            }),
            "pressed" => AriaState::Pressed(match value {
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
            "readonly" => AriaState::ReadOnly(value == "true"),
            "required" => AriaState::Required(value == "true"),
            
            // String properties
            "label" => AriaState::Label(value.to_string()),
            "labelledby" | "labeledby" => AriaState::LabelledBy(value.split_whitespace().map(String::from).collect()),
            "describedby" => AriaState::DescribedBy(value.split_whitespace().map(String::from).collect()),
            "valuetext" => AriaState::ValueText(value.to_string()),
            
            // Numeric properties
            "valuenow" => AriaState::ValueNow(value.parse().ok()?),
            "valuemin" => AriaState::ValueMin(value.parse().ok()?),
            "valuemax" => AriaState::ValueMax(value.parse().ok()?),
            "level" => AriaState::Level(value.parse().ok()?),
            "posinset" => AriaState::PosInSet(value.parse().ok()?),
            "setsize" => AriaState::SetSize(value.parse().ok()?),
            "colcount" => AriaState::ColCount(value.parse().ok()?),
            "colindex" => AriaState::ColIndex(value.parse().ok()?),
            "colspan" => AriaState::ColSpan(value.parse().ok()?),
            "rowcount" => AriaState::RowCount(value.parse().ok()?),
            "rowindex" => AriaState::RowIndex(value.parse().ok()?),
            "rowspan" => AriaState::RowSpan(value.parse().ok()?),
            
            // Relationship properties
            "activedescendant" => AriaState::ActiveDescendant(value.to_string()),
            "controls" => AriaState::Controls(value.split_whitespace().map(String::from).collect()),
            "details" => AriaState::Details(value.split_whitespace().map(String::from).collect()),
            "errormessage" => AriaState::ErrorMessage(value.to_string()),
            "flowto" => AriaState::FlowTo(value.split_whitespace().map(String::from).collect()),
            "owns" => AriaState::Owns(value.split_whitespace().map(String::from).collect()),
            
            // Live region properties
            "live" => AriaState::Live(match value {
                "polite" => LiveRegionMode::Polite,
                "assertive" => LiveRegionMode::Assertive,
                _ => LiveRegionMode::Off,
            }),
            "atomic" => AriaState::Atomic(value == "true"),
            "busy" => AriaState::Busy(value == "true"),
            "relevant" => AriaState::Relevant(
                value.split_whitespace()
                    .filter_map(|s| match s {
                        "additions" => Some(LiveRelevant::Additions),
                        "removals" => Some(LiveRelevant::Removals),
                        "text" => Some(LiveRelevant::Text),
                        "all" => Some(LiveRelevant::All),
                        _ => None,
                    })
                    .collect()
            ),
            
            // Drag and drop
            "grabbed" => AriaState::Grabbed(match value {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            }),
            "dropeffect" => AriaState::DropEffect(
                value.split_whitespace()
                    .filter_map(|s| match s {
                        "copy" => Some(DropEffect::Copy),
                        "execute" => Some(DropEffect::Execute),
                        "link" => Some(DropEffect::Link),
                        "move" => Some(DropEffect::Move),
                        "none" => Some(DropEffect::None),
                        "popup" => Some(DropEffect::Popup),
                        _ => None,
                    })
                    .collect()
            ),
            
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
    
    /// Check if disabled
    pub fn is_disabled(&self) -> bool {
        self.states.get("disabled").map_or(false, |s| {
            matches!(s, AriaState::Disabled(true))
        })
    }
    
    /// Check if hidden
    pub fn is_hidden(&self) -> bool {
        self.states.get("hidden").map_or(false, |s| {
            matches!(s, AriaState::Hidden(true))
        })
    }
    
    /// Get live region mode
    pub fn get_live_mode(&self) -> Option<LiveRegionMode> {
        self.states.get("live").and_then(|s| {
            if let AriaState::Live(mode) = s { Some(*mode) } else { None }
        })
    }
    
    /// Get controls (relationship)
    pub fn get_controls(&self) -> Option<&[String]> {
        self.states.get("controls").and_then(|s| {
            if let AriaState::Controls(ids) = s { Some(ids.as_slice()) } else { None }
        })
    }
    
    /// Get owns (relationship)
    pub fn get_owns(&self) -> Option<&[String]> {
        self.states.get("owns").and_then(|s| {
            if let AriaState::Owns(ids) = s { Some(ids.as_slice()) } else { None }
        })
    }
    
    /// Get active descendant
    pub fn get_active_descendant(&self) -> Option<&str> {
        self.states.get("activedescendant").and_then(|s| {
            if let AriaState::ActiveDescendant(id) = s { Some(id.as_str()) } else { None }
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
