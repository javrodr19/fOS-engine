//! Console API
//!
//! console.log, warn, error, etc.

use std::collections::VecDeque;
use std::fmt;

/// Console log level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
    Trace,
}

/// Console message
#[derive(Debug, Clone)]
pub struct ConsoleMessage {
    pub level: LogLevel,
    pub message: String,
    pub args: Vec<ConsoleValue>,
    pub timestamp: u64,
    pub source: Option<SourceLocation>,
    pub stack_trace: Option<Vec<StackFrame>>,
}

/// Console value (for object inspection)
#[derive(Debug, Clone)]
pub enum ConsoleValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Object(Vec<(String, Box<ConsoleValue>)>),
    Array(Vec<ConsoleValue>),
    Function(String),
    Symbol(String),
    Error { name: String, message: String },
}

impl fmt::Display for ConsoleValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undefined => write!(f, "undefined"),
            Self::Null => write!(f, "null"),
            Self::Boolean(b) => write!(f, "{}", b),
            Self::Number(n) => write!(f, "{}", n),
            Self::String(s) => write!(f, "\"{}\"", s),
            Self::Object(props) => {
                write!(f, "{{")?;
                for (i, (k, v)) in props.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Self::Array(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Self::Function(name) => write!(f, "ƒ {}", name),
            Self::Symbol(s) => write!(f, "Symbol({})", s),
            Self::Error { name, message } => write!(f, "{}: {}", name, message),
        }
    }
}

/// Source location
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub url: String,
    pub line: u32,
    pub column: u32,
}

/// Stack frame
#[derive(Debug, Clone)]
pub struct StackFrame {
    pub function_name: String,
    pub url: String,
    pub line: u32,
    pub column: u32,
}

/// Console
#[derive(Debug, Default)]
pub struct Console {
    messages: VecDeque<ConsoleMessage>,
    max_messages: usize,
    timers: std::collections::HashMap<String, u64>,
    counters: std::collections::HashMap<String, u32>,
}

impl Console {
    pub fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            max_messages: 1000,
            timers: std::collections::HashMap::new(),
            counters: std::collections::HashMap::new(),
        }
    }
    
    fn add_message(&mut self, level: LogLevel, message: String, args: Vec<ConsoleValue>) {
        let msg = ConsoleMessage {
            level,
            message,
            args,
            timestamp: current_time_ms(),
            source: None,
            stack_trace: None,
        };
        
        self.messages.push_back(msg);
        while self.messages.len() > self.max_messages {
            self.messages.pop_front();
        }
    }
    
    /// console.log
    pub fn log(&mut self, message: &str, args: Vec<ConsoleValue>) {
        self.add_message(LogLevel::Log, message.to_string(), args);
    }
    
    /// console.info
    pub fn info(&mut self, message: &str, args: Vec<ConsoleValue>) {
        self.add_message(LogLevel::Info, message.to_string(), args);
    }
    
    /// console.warn
    pub fn warn(&mut self, message: &str, args: Vec<ConsoleValue>) {
        self.add_message(LogLevel::Warn, message.to_string(), args);
    }
    
    /// console.error
    pub fn error(&mut self, message: &str, args: Vec<ConsoleValue>) {
        self.add_message(LogLevel::Error, message.to_string(), args);
    }
    
    /// console.debug
    pub fn debug(&mut self, message: &str, args: Vec<ConsoleValue>) {
        self.add_message(LogLevel::Debug, message.to_string(), args);
    }
    
    /// console.trace
    pub fn trace(&mut self, message: &str) {
        let mut msg = ConsoleMessage {
            level: LogLevel::Trace,
            message: message.to_string(),
            args: Vec::new(),
            timestamp: current_time_ms(),
            source: None,
            stack_trace: Some(Vec::new()), // Would capture actual stack
        };
        self.messages.push_back(msg);
    }
    
    /// console.assert
    pub fn assert(&mut self, condition: bool, message: &str) {
        if !condition {
            self.error(&format!("Assertion failed: {}", message), Vec::new());
        }
    }
    
    /// console.clear
    pub fn clear(&mut self) {
        self.messages.clear();
    }
    
    /// console.count
    pub fn count(&mut self, label: &str) {
        let count = {
            let entry = self.counters.entry(label.to_string()).or_insert(0);
            *entry += 1;
            *entry
        };
        self.log(&format!("{}: {}", label, count), Vec::new());
    }
    
    /// console.countReset
    pub fn count_reset(&mut self, label: &str) {
        self.counters.insert(label.to_string(), 0);
    }
    
    /// console.time
    pub fn time(&mut self, label: &str) {
        self.timers.insert(label.to_string(), current_time_ms());
    }
    
    /// console.timeEnd
    pub fn time_end(&mut self, label: &str) {
        if let Some(start) = self.timers.remove(label) {
            let elapsed = current_time_ms() - start;
            self.log(&format!("{}: {}ms", label, elapsed), Vec::new());
        }
    }
    
    /// console.timeLog
    pub fn time_log(&mut self, label: &str) {
        if let Some(start) = self.timers.get(label) {
            let elapsed = current_time_ms() - start;
            self.log(&format!("{}: {}ms", label, elapsed), Vec::new());
        }
    }
    
    /// console.group
    pub fn group(&mut self, label: &str) {
        self.log(&format!("▶ {}", label), Vec::new());
    }
    
    /// console.groupEnd
    pub fn group_end(&mut self) {
        // Would handle group nesting
    }
    
    /// console.table
    pub fn table(&mut self, data: ConsoleValue) {
        self.log(&format!("[Table] {}", data), Vec::new());
    }
    
    /// Get all messages
    pub fn get_messages(&self) -> &VecDeque<ConsoleMessage> {
        &self.messages
    }
    
    /// Get messages by level
    pub fn get_by_level(&self, level: LogLevel) -> Vec<&ConsoleMessage> {
        self.messages.iter().filter(|m| m.level == level).collect()
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_console_log() {
        let mut console = Console::new();
        console.log("Hello", vec![ConsoleValue::String("World".into())]);
        
        assert_eq!(console.messages.len(), 1);
        assert_eq!(console.messages[0].level, LogLevel::Log);
    }
    
    #[test]
    fn test_console_count() {
        let mut console = Console::new();
        console.count("clicks");
        console.count("clicks");
        
        assert_eq!(*console.counters.get("clicks").unwrap(), 2);
    }
}
