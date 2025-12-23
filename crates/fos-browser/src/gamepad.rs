//! Gamepad API
//!
//! Game controller input.

use std::collections::HashMap;

/// Gamepad button
#[derive(Debug, Clone, Copy)]
pub struct GamepadButton {
    pub pressed: bool,
    pub touched: bool,
    pub value: f32,
}

impl Default for GamepadButton {
    fn default() -> Self {
        Self {
            pressed: false,
            touched: false,
            value: 0.0,
        }
    }
}

/// Standard gamepad buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardButton {
    A,
    B,
    X,
    Y,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    Back,
    Start,
    LeftStick,
    RightStick,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    Home,
}

/// Gamepad state
#[derive(Debug, Clone)]
pub struct Gamepad {
    pub id: String,
    pub index: u32,
    pub connected: bool,
    pub mapping: String,
    pub buttons: Vec<GamepadButton>,
    pub axes: Vec<f32>,
    pub timestamp: f64,
}

impl Gamepad {
    pub fn new(id: &str, index: u32) -> Self {
        Self {
            id: id.to_string(),
            index,
            connected: true,
            mapping: "standard".to_string(),
            buttons: vec![GamepadButton::default(); 17],
            axes: vec![0.0; 4],
            timestamp: 0.0,
        }
    }
    
    /// Get button state by index
    pub fn button(&self, index: usize) -> Option<&GamepadButton> {
        self.buttons.get(index)
    }
    
    /// Get axis value
    pub fn axis(&self, index: usize) -> f32 {
        self.axes.get(index).copied().unwrap_or(0.0)
    }
    
    /// Get left stick
    pub fn left_stick(&self) -> (f32, f32) {
        (self.axis(0), self.axis(1))
    }
    
    /// Get right stick
    pub fn right_stick(&self) -> (f32, f32) {
        (self.axis(2), self.axis(3))
    }
}

/// Gamepad event
#[derive(Debug, Clone)]
pub enum GamepadEvent {
    Connected(u32),
    Disconnected(u32),
}

/// Gamepad manager
#[derive(Debug, Default)]
pub struct GamepadManager {
    gamepads: HashMap<u32, Gamepad>,
    pending_events: Vec<GamepadEvent>,
    next_index: u32,
}

impl GamepadManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get all connected gamepads
    pub fn get_gamepads(&self) -> Vec<Option<&Gamepad>> {
        let mut result = vec![None; 4];
        for (idx, gamepad) in &self.gamepads {
            if (*idx as usize) < 4 {
                result[*idx as usize] = Some(gamepad);
            }
        }
        result
    }
    
    /// Get gamepad by index
    pub fn get(&self, index: u32) -> Option<&Gamepad> {
        self.gamepads.get(&index)
    }
    
    /// Connect a gamepad
    pub fn connect(&mut self, id: &str) -> u32 {
        let index = self.next_index;
        self.next_index += 1;
        
        let gamepad = Gamepad::new(id, index);
        self.gamepads.insert(index, gamepad);
        self.pending_events.push(GamepadEvent::Connected(index));
        
        index
    }
    
    /// Disconnect a gamepad
    pub fn disconnect(&mut self, index: u32) -> bool {
        if let Some(mut gamepad) = self.gamepads.remove(&index) {
            gamepad.connected = false;
            self.pending_events.push(GamepadEvent::Disconnected(index));
            true
        } else {
            false
        }
    }
    
    /// Update gamepad state
    pub fn update(&mut self, index: u32, buttons: Vec<GamepadButton>, axes: Vec<f32>, timestamp: f64) {
        if let Some(gamepad) = self.gamepads.get_mut(&index) {
            gamepad.buttons = buttons;
            gamepad.axes = axes;
            gamepad.timestamp = timestamp;
        }
    }
    
    /// Take pending events
    pub fn take_events(&mut self) -> Vec<GamepadEvent> {
        std::mem::take(&mut self.pending_events)
    }
    
    /// Poll gamepads (would read from system in real impl)
    pub fn poll(&mut self) {
        // In real implementation, read from /dev/input or similar
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_gamepad() {
        let mut mgr = GamepadManager::new();
        
        let idx = mgr.connect("Xbox Controller");
        assert!(mgr.get(idx).is_some());
        
        let events = mgr.take_events();
        assert_eq!(events.len(), 1);
        
        mgr.disconnect(idx);
        assert!(mgr.get(idx).is_none());
    }
}
