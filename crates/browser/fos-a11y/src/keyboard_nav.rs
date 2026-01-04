//! Enhanced Keyboard Navigation
//!
//! Rotor-style navigation, custom shortcuts, and spatial navigation.

use std::collections::HashMap;

/// Navigation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavigationMode {
    #[default]
    Document,
    Headings,
    Landmarks,
    Links,
    FormControls,
    Tables,
    Lists,
    Graphics,
}

impl NavigationMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Document => "Document", Self::Headings => "Headings", Self::Landmarks => "Landmarks",
            Self::Links => "Links", Self::FormControls => "Form Controls", Self::Tables => "Tables",
            Self::Lists => "Lists", Self::Graphics => "Graphics",
        }
    }
    
    pub fn next(self) -> Self {
        match self {
            Self::Document => Self::Headings, Self::Headings => Self::Landmarks, Self::Landmarks => Self::Links,
            Self::Links => Self::FormControls, Self::FormControls => Self::Tables, Self::Tables => Self::Lists,
            Self::Lists => Self::Graphics, Self::Graphics => Self::Document,
        }
    }
    
    pub fn prev(self) -> Self {
        match self {
            Self::Document => Self::Graphics, Self::Headings => Self::Document, Self::Landmarks => Self::Headings,
            Self::Links => Self::Landmarks, Self::FormControls => Self::Links, Self::Tables => Self::FormControls,
            Self::Lists => Self::Tables, Self::Graphics => Self::Lists,
        }
    }
}

/// Keyboard shortcut
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyboardShortcut {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl KeyboardShortcut {
    pub fn new(key: &str) -> Self { Self { key: key.into(), ctrl: false, alt: false, shift: false, meta: false } }
    pub fn ctrl(mut self) -> Self { self.ctrl = true; self }
    pub fn alt(mut self) -> Self { self.alt = true; self }
    pub fn shift(mut self) -> Self { self.shift = true; self }
    
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl { parts.push("Ctrl"); }
        if self.alt { parts.push("Alt"); }
        if self.shift { parts.push("Shift"); }
        if self.meta { parts.push("Cmd"); }
        parts.push(&self.key);
        parts.join("+")
    }
}

/// Navigation action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    NextElement, PrevElement, NextHeading, PrevHeading, NextLandmark, PrevLandmark,
    NextLink, PrevLink, NextFormControl, PrevFormControl, NextTable, PrevTable,
    EnterElement, ExitElement, Activate, ToggleRotor, ChangeRotorMode,
    MoveUp, MoveDown, MoveLeft, MoveRight, // Spatial navigation
}

/// Shortcut registry
#[derive(Debug, Default)]
pub struct ShortcutRegistry {
    shortcuts: HashMap<KeyboardShortcut, NavAction>,
    custom: HashMap<KeyboardShortcut, String>,
}

impl ShortcutRegistry {
    pub fn new() -> Self {
        let mut registry = Self::default();
        // Default shortcuts
        registry.register(KeyboardShortcut::new("Tab"), NavAction::NextElement);
        registry.register(KeyboardShortcut::new("Tab").shift(), NavAction::PrevElement);
        registry.register(KeyboardShortcut::new("h"), NavAction::NextHeading);
        registry.register(KeyboardShortcut::new("h").shift(), NavAction::PrevHeading);
        registry.register(KeyboardShortcut::new("Enter"), NavAction::Activate);
        registry.register(KeyboardShortcut::new("Escape"), NavAction::ExitElement);
        // Arrow keys for spatial navigation
        registry.register(KeyboardShortcut::new("ArrowUp").alt(), NavAction::MoveUp);
        registry.register(KeyboardShortcut::new("ArrowDown").alt(), NavAction::MoveDown);
        registry.register(KeyboardShortcut::new("ArrowLeft").alt(), NavAction::MoveLeft);
        registry.register(KeyboardShortcut::new("ArrowRight").alt(), NavAction::MoveRight);
        registry
    }
    
    pub fn register(&mut self, shortcut: KeyboardShortcut, action: NavAction) {
        self.shortcuts.insert(shortcut, action);
    }
    
    pub fn get_action(&self, shortcut: &KeyboardShortcut) -> Option<&NavAction> {
        self.shortcuts.get(shortcut)
    }
    
    pub fn register_custom(&mut self, shortcut: KeyboardShortcut, command: String) {
        self.custom.insert(shortcut, command);
    }
}

/// Spatial navigation direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction { Up, Down, Left, Right }

/// Element position for spatial navigation
#[derive(Debug, Clone)]
pub struct ElementRect {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ElementRect {
    pub fn center(&self) -> (f64, f64) { (self.x + self.width / 2.0, self.y + self.height / 2.0) }
}

/// Spatial navigator
#[derive(Debug, Default)]
pub struct SpatialNavigator {
    elements: Vec<ElementRect>,
    current: Option<u64>,
}

impl SpatialNavigator {
    pub fn new() -> Self { Self::default() }
    
    pub fn set_elements(&mut self, elements: Vec<ElementRect>) { self.elements = elements; }
    pub fn set_current(&mut self, id: u64) { self.current = Some(id); }
    
    pub fn navigate(&mut self, direction: Direction) -> Option<u64> {
        let current = self.current?;
        let current_rect = self.elements.iter().find(|e| e.id == current)?;
        let (cx, cy) = current_rect.center();
        
        let mut best: Option<(u64, f64)> = None;
        
        for elem in &self.elements {
            if elem.id == current { continue; }
            let (ex, ey) = elem.center();
            
            let valid = match direction {
                Direction::Up => ey < cy, Direction::Down => ey > cy,
                Direction::Left => ex < cx, Direction::Right => ex > cx,
            };
            
            if !valid { continue; }
            
            let dist = ((ex - cx).powi(2) + (ey - cy).powi(2)).sqrt();
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((elem.id, dist));
            }
        }
        
        if let Some((id, _)) = best { self.current = Some(id); return Some(id); }
        None
    }
}

/// Keyboard navigation manager
#[derive(Debug, Default)]
pub struct KeyboardNavManager {
    mode: NavigationMode,
    shortcuts: ShortcutRegistry,
    spatial: SpatialNavigator,
    enabled: bool,
}

impl KeyboardNavManager {
    pub fn new() -> Self { Self { shortcuts: ShortcutRegistry::new(), enabled: true, ..Default::default() } }
    
    pub fn mode(&self) -> NavigationMode { self.mode }
    pub fn set_mode(&mut self, mode: NavigationMode) { self.mode = mode; }
    pub fn next_mode(&mut self) { self.mode = self.mode.next(); }
    pub fn prev_mode(&mut self) { self.mode = self.mode.prev(); }
    
    pub fn handle_key(&mut self, shortcut: &KeyboardShortcut) -> Option<NavAction> {
        if !self.enabled { return None; }
        self.shortcuts.get_action(shortcut).copied()
    }
    
    pub fn spatial_nav(&mut self) -> &mut SpatialNavigator { &mut self.spatial }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_navigation_mode() {
        let mode = NavigationMode::Headings;
        assert_eq!(mode.next(), NavigationMode::Landmarks);
        assert_eq!(mode.prev(), NavigationMode::Document);
    }
    
    #[test]
    fn test_shortcut() {
        let shortcut = KeyboardShortcut::new("Tab").shift();
        assert_eq!(shortcut.display(), "Shift+Tab");
    }
    
    #[test]
    fn test_spatial_navigation() {
        let mut nav = SpatialNavigator::new();
        nav.set_elements(vec![
            ElementRect { id: 1, x: 0.0, y: 0.0, width: 100.0, height: 50.0 },
            ElementRect { id: 2, x: 0.0, y: 100.0, width: 100.0, height: 50.0 },
        ]);
        nav.set_current(1);
        assert_eq!(nav.navigate(Direction::Down), Some(2));
    }
}
