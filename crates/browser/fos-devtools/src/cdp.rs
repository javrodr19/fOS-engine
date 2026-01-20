//! Chrome DevTools Protocol (CDP) Server
//!
//! Implements the CDP protocol for compatibility with Chrome DevTools frontend
//! and other CDP clients.

use std::collections::HashMap;
use crate::{Console, ConsoleValue, Debugger, DebuggerState, PerformancePanel, NetworkPanel};

/// CDP server for handling DevTools protocol communication
#[derive(Debug)]
pub struct CdpServer {
    /// Active sessions
    sessions: HashMap<String, CdpSession>,
    /// Next session ID
    next_session_id: u64,
    /// Event listeners
    enabled_domains: Vec<String>,
    /// Pending events
    pending_events: Vec<CdpEvent>,
}

impl Default for CdpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl CdpServer {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            next_session_id: 0,
            enabled_domains: Vec::new(),
            pending_events: Vec::new(),
        }
    }
    
    /// Create a new session
    pub fn create_session(&mut self) -> String {
        let id = format!("session-{}", self.next_session_id);
        self.next_session_id += 1;
        self.sessions.insert(id.clone(), CdpSession::new(id.clone()));
        id
    }
    
    /// Close a session
    pub fn close_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
    }
    
    /// Handle incoming CDP command
    pub fn handle_command(&mut self, cmd: CdpCommand) -> CdpResponse {
        let method = cmd.method.as_str();
        
        // Route to appropriate domain handler
        if method.starts_with("Runtime.") {
            self.handle_runtime(cmd)
        } else if method.starts_with("Debugger.") {
            self.handle_debugger(cmd)
        } else if method.starts_with("Network.") {
            self.handle_network(cmd)
        } else if method.starts_with("DOM.") {
            self.handle_dom(cmd)
        } else if method.starts_with("CSS.") {
            self.handle_css(cmd)
        } else if method.starts_with("Page.") {
            self.handle_page(cmd)
        } else if method.starts_with("Console.") {
            self.handle_console(cmd)
        } else if method.starts_with("Profiler.") {
            self.handle_profiler(cmd)
        } else if method.starts_with("HeapProfiler.") {
            self.handle_heap_profiler(cmd)
        } else if method.starts_with("Target.") {
            self.handle_target(cmd)
        } else {
            CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method))
        }
    }
    
    /// Enable a domain
    pub fn enable_domain(&mut self, domain: &str) {
        if !self.enabled_domains.contains(&domain.to_string()) {
            self.enabled_domains.push(domain.to_string());
        }
    }
    
    /// Disable a domain
    pub fn disable_domain(&mut self, domain: &str) {
        self.enabled_domains.retain(|d| d != domain);
    }
    
    /// Check if domain is enabled
    pub fn is_domain_enabled(&self, domain: &str) -> bool {
        self.enabled_domains.contains(&domain.to_string())
    }
    
    /// Emit an event
    pub fn emit_event(&mut self, event: CdpEvent) {
        self.pending_events.push(event);
    }
    
    /// Get and clear pending events
    pub fn take_events(&mut self) -> Vec<CdpEvent> {
        std::mem::take(&mut self.pending_events)
    }
    
    // === Domain Handlers ===
    
    fn handle_runtime(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "Runtime.enable" => {
                self.enable_domain("Runtime");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Runtime.disable" => {
                self.disable_domain("Runtime");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Runtime.evaluate" => {
                let expression = cmd.params.get("expression")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                
                // Placeholder evaluation result
                let result = RuntimeRemoteObject {
                    object_type: "undefined".to_string(),
                    value: None,
                    description: Some("undefined".to_string()),
                    object_id: None,
                };
                
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("result".into(), result.to_value()),
                ].into_iter().collect()))
            }
            "Runtime.getProperties" => {
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("result".into(), serde_value::Value::Seq(vec![])),
                ].into_iter().collect()))
            }
            _ => CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method)),
        }
    }
    
    fn handle_debugger(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "Debugger.enable" => {
                self.enable_domain("Debugger");
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("debuggerId".into(), serde_value::Value::String("fos-debugger".to_string())),
                ].into_iter().collect()))
            }
            "Debugger.disable" => {
                self.disable_domain("Debugger");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Debugger.setBreakpointByUrl" => {
                let url = cmd.params.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let line = cmd.params.get("lineNumber").and_then(|v| v.as_u64()).unwrap_or(0);
                
                let bp_id = format!("bp-{}:{}", url, line);
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("breakpointId".into(), serde_value::Value::String(bp_id)),
                    ("locations".into(), serde_value::Value::Seq(vec![])),
                ].into_iter().collect()))
            }
            "Debugger.removeBreakpoint" => {
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Debugger.pause" => {
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Debugger.resume" => {
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Debugger.stepOver" | "Debugger.stepInto" | "Debugger.stepOut" => {
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            _ => CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method)),
        }
    }
    
    fn handle_network(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "Network.enable" => {
                self.enable_domain("Network");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Network.disable" => {
                self.disable_domain("Network");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Network.setCacheDisabled" => {
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Network.emulateNetworkConditions" => {
                // Would apply throttle settings
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Network.getCookies" => {
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("cookies".into(), serde_value::Value::Seq(vec![])),
                ].into_iter().collect()))
            }
            _ => CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method)),
        }
    }
    
    fn handle_dom(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "DOM.enable" => {
                self.enable_domain("DOM");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "DOM.disable" => {
                self.disable_domain("DOM");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "DOM.getDocument" => {
                let root = DomNode {
                    node_id: 1,
                    node_type: 9,
                    node_name: "#document".to_string(),
                    local_name: "".to_string(),
                    node_value: "".to_string(),
                    child_node_count: Some(1),
                    children: None,
                    attributes: None,
                };
                
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("root".into(), root.to_value()),
                ].into_iter().collect()))
            }
            "DOM.requestChildNodes" => {
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            _ => CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method)),
        }
    }
    
    fn handle_css(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "CSS.enable" => {
                self.enable_domain("CSS");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "CSS.disable" => {
                self.disable_domain("CSS");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "CSS.getMatchedStylesForNode" => {
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("matchedCSSRules".into(), serde_value::Value::Seq(vec![])),
                    ("inherited".into(), serde_value::Value::Seq(vec![])),
                ].into_iter().collect()))
            }
            "CSS.getComputedStyleForNode" => {
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("computedStyle".into(), serde_value::Value::Seq(vec![])),
                ].into_iter().collect()))
            }
            _ => CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method)),
        }
    }
    
    fn handle_page(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "Page.enable" => {
                self.enable_domain("Page");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Page.disable" => {
                self.disable_domain("Page");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Page.getFrameTree" => {
                let frame = PageFrame {
                    id: "main".to_string(),
                    url: "about:blank".to_string(),
                    security_origin: "".to_string(),
                    mime_type: "text/html".to_string(),
                };
                
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("frameTree".into(), serde_value::Value::Map(vec![
                        ("frame".into(), frame.to_value()),
                        ("childFrames".into(), serde_value::Value::Seq(vec![])),
                    ].into_iter().collect())),
                ].into_iter().collect()))
            }
            "Page.reload" => {
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Page.navigate" => {
                let url = cmd.params.get("url").and_then(|v| v.as_str()).unwrap_or("");
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("frameId".into(), serde_value::Value::String("main".to_string())),
                ].into_iter().collect()))
            }
            _ => CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method)),
        }
    }
    
    fn handle_console(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "Console.enable" => {
                self.enable_domain("Console");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Console.disable" => {
                self.disable_domain("Console");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Console.clearMessages" => {
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            _ => CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method)),
        }
    }
    
    fn handle_profiler(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "Profiler.enable" => {
                self.enable_domain("Profiler");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Profiler.disable" => {
                self.disable_domain("Profiler");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Profiler.start" => {
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "Profiler.stop" => {
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("profile".into(), serde_value::Value::Map(vec![
                        ("nodes".into(), serde_value::Value::Seq(vec![])),
                        ("startTime".into(), serde_value::Value::F64(0.0)),
                        ("endTime".into(), serde_value::Value::F64(0.0)),
                    ].into_iter().collect())),
                ].into_iter().collect()))
            }
            _ => CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method)),
        }
    }
    
    fn handle_heap_profiler(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "HeapProfiler.enable" => {
                self.enable_domain("HeapProfiler");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "HeapProfiler.disable" => {
                self.disable_domain("HeapProfiler");
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            "HeapProfiler.takeHeapSnapshot" => {
                // Would trigger snapshot
                CdpResponse::success(cmd.id, serde_value::Value::Unit)
            }
            _ => CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method)),
        }
    }
    
    fn handle_target(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "Target.getTargets" => {
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("targetInfos".into(), serde_value::Value::Seq(vec![
                        serde_value::Value::Map(vec![
                            ("targetId".into(), serde_value::Value::String("main".to_string())),
                            ("type".into(), serde_value::Value::String("page".to_string())),
                            ("title".into(), serde_value::Value::String("fOS Browser".to_string())),
                            ("url".into(), serde_value::Value::String("about:blank".to_string())),
                        ].into_iter().collect()),
                    ])),
                ].into_iter().collect()))
            }
            "Target.attachToTarget" => {
                let session_id = self.create_session();
                CdpResponse::success(cmd.id, serde_value::Value::Map(vec![
                    ("sessionId".into(), serde_value::Value::String(session_id)),
                ].into_iter().collect()))
            }
            _ => CdpResponse::error(cmd.id, CdpError::method_not_found(&cmd.method)),
        }
    }
}

/// CDP session
#[derive(Debug)]
pub struct CdpSession {
    pub id: String,
    pub target_id: String,
}

impl CdpSession {
    pub fn new(id: String) -> Self {
        Self {
            id,
            target_id: "main".to_string(),
        }
    }
}

/// CDP command
#[derive(Debug, Clone)]
pub struct CdpCommand {
    pub id: u64,
    pub method: String,
    pub params: HashMap<String, serde_value::Value>,
    pub session_id: Option<String>,
}

impl CdpCommand {
    pub fn new(id: u64, method: &str) -> Self {
        Self {
            id,
            method: method.to_string(),
            params: HashMap::new(),
            session_id: None,
        }
    }
    
    pub fn with_param(mut self, key: &str, value: serde_value::Value) -> Self {
        self.params.insert(key.to_string(), value);
        self
    }
}

/// CDP response
#[derive(Debug, Clone)]
pub struct CdpResponse {
    pub id: u64,
    pub result: Option<serde_value::Value>,
    pub error: Option<CdpError>,
}

impl CdpResponse {
    pub fn success(id: u64, result: serde_value::Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }
    
    pub fn error(id: u64, error: CdpError) -> Self {
        Self {
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// CDP error
#[derive(Debug, Clone)]
pub struct CdpError {
    pub code: i32,
    pub message: String,
    pub data: Option<String>,
}

impl CdpError {
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {}", method),
            data: None,
        }
    }
    
    pub fn invalid_params(message: &str) -> Self {
        Self {
            code: -32602,
            message: message.to_string(),
            data: None,
        }
    }
    
    pub fn internal_error(message: &str) -> Self {
        Self {
            code: -32603,
            message: message.to_string(),
            data: None,
        }
    }
}

/// CDP event
#[derive(Debug, Clone)]
pub struct CdpEvent {
    pub method: String,
    pub params: serde_value::Value,
    pub session_id: Option<String>,
}

impl CdpEvent {
    pub fn new(method: &str, params: serde_value::Value) -> Self {
        Self {
            method: method.to_string(),
            params,
            session_id: None,
        }
    }
    
    // Common events
    
    pub fn console_message_added(level: &str, text: &str, url: Option<&str>, line: Option<u32>) -> Self {
        Self::new("Console.messageAdded", serde_value::Value::Map(vec![
            ("message".into(), serde_value::Value::Map(vec![
                ("level".into(), serde_value::Value::String(level.to_string())),
                ("text".into(), serde_value::Value::String(text.to_string())),
                ("url".into(), url.map(|u| serde_value::Value::String(u.to_string())).unwrap_or(serde_value::Value::Unit)),
                ("line".into(), line.map(|l| serde_value::Value::U32(l)).unwrap_or(serde_value::Value::Unit)),
            ].into_iter().collect())),
        ].into_iter().collect()))
    }
    
    pub fn debugger_paused(reason: &str, call_frames: Vec<serde_value::Value>) -> Self {
        Self::new("Debugger.paused", serde_value::Value::Map(vec![
            ("reason".into(), serde_value::Value::String(reason.to_string())),
            ("callFrames".into(), serde_value::Value::Seq(call_frames)),
        ].into_iter().collect()))
    }
    
    pub fn debugger_resumed() -> Self {
        Self::new("Debugger.resumed", serde_value::Value::Unit)
    }
    
    pub fn network_request_will_be_sent(request_id: &str, url: &str, method: &str) -> Self {
        Self::new("Network.requestWillBeSent", serde_value::Value::Map(vec![
            ("requestId".into(), serde_value::Value::String(request_id.to_string())),
            ("request".into(), serde_value::Value::Map(vec![
                ("url".into(), serde_value::Value::String(url.to_string())),
                ("method".into(), serde_value::Value::String(method.to_string())),
            ].into_iter().collect())),
            ("timestamp".into(), serde_value::Value::F64(current_time())),
        ].into_iter().collect()))
    }
    
    pub fn network_response_received(request_id: &str, url: &str, status: u16) -> Self {
        Self::new("Network.responseReceived", serde_value::Value::Map(vec![
            ("requestId".into(), serde_value::Value::String(request_id.to_string())),
            ("response".into(), serde_value::Value::Map(vec![
                ("url".into(), serde_value::Value::String(url.to_string())),
                ("status".into(), serde_value::Value::U16(status)),
            ].into_iter().collect())),
            ("timestamp".into(), serde_value::Value::F64(current_time())),
        ].into_iter().collect()))
    }
}

// === Helper types for CDP ===

/// Runtime remote object
#[derive(Debug)]
struct RuntimeRemoteObject {
    object_type: String,
    value: Option<serde_value::Value>,
    description: Option<String>,
    object_id: Option<String>,
}

impl RuntimeRemoteObject {
    fn to_value(&self) -> serde_value::Value {
        let mut map = vec![
            ("type".into(), serde_value::Value::String(self.object_type.clone())),
        ];
        
        if let Some(ref v) = self.value {
            map.push(("value".into(), v.clone()));
        }
        if let Some(ref d) = self.description {
            map.push(("description".into(), serde_value::Value::String(d.clone())));
        }
        if let Some(ref id) = self.object_id {
            map.push(("objectId".into(), serde_value::Value::String(id.clone())));
        }
        
        serde_value::Value::Map(map.into_iter().collect())
    }
}

/// DOM node for CDP
#[derive(Debug)]
struct DomNode {
    node_id: u64,
    node_type: u32,
    node_name: String,
    local_name: String,
    node_value: String,
    child_node_count: Option<u32>,
    children: Option<Vec<DomNode>>,
    attributes: Option<Vec<String>>,
}

impl DomNode {
    fn to_value(&self) -> serde_value::Value {
        let mut map = vec![
            ("nodeId".into(), serde_value::Value::U64(self.node_id)),
            ("nodeType".into(), serde_value::Value::U32(self.node_type)),
            ("nodeName".into(), serde_value::Value::String(self.node_name.clone())),
            ("localName".into(), serde_value::Value::String(self.local_name.clone())),
            ("nodeValue".into(), serde_value::Value::String(self.node_value.clone())),
        ];
        
        if let Some(count) = self.child_node_count {
            map.push(("childNodeCount".into(), serde_value::Value::U32(count)));
        }
        
        serde_value::Value::Map(map.into_iter().collect())
    }
}

/// Page frame for CDP
#[derive(Debug)]
struct PageFrame {
    id: String,
    url: String,
    security_origin: String,
    mime_type: String,
}

impl PageFrame {
    fn to_value(&self) -> serde_value::Value {
        serde_value::Value::Map(vec![
            ("id".into(), serde_value::Value::String(self.id.clone())),
            ("url".into(), serde_value::Value::String(self.url.clone())),
            ("securityOrigin".into(), serde_value::Value::String(self.security_origin.clone())),
            ("mimeType".into(), serde_value::Value::String(self.mime_type.clone())),
        ].into_iter().collect())
    }
}

/// Simple value type for CDP (standalone, no serde dependency)
pub mod serde_value {
    use std::collections::HashMap;
    
    #[derive(Debug, Clone)]
    pub enum Value {
        Unit,
        Bool(bool),
        I8(i8),
        I16(i16),
        I32(i32),
        I64(i64),
        U8(u8),
        U16(u16),
        U32(u32),
        U64(u64),
        F32(f32),
        F64(f64),
        String(String),
        Seq(Vec<Value>),
        Map(HashMap<String, Value>),
    }
    
    impl Value {
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::String(s) => Some(s),
                _ => None,
            }
        }
        
        pub fn as_u64(&self) -> Option<u64> {
            match self {
                Value::U64(n) => Some(*n),
                Value::U32(n) => Some(*n as u64),
                Value::U16(n) => Some(*n as u64),
                Value::U8(n) => Some(*n as u64),
                _ => None,
            }
        }
        
        pub fn as_bool(&self) -> Option<bool> {
            match self {
                Value::Bool(b) => Some(*b),
                _ => None,
            }
        }
    }
    
    impl From<&str> for Value {
        fn from(s: &str) -> Self {
            Value::String(s.to_string())
        }
    }
    
    impl From<String> for Value {
        fn from(s: String) -> Self {
            Value::String(s)
        }
    }
}

fn current_time() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cdp_server_creation() {
        let server = CdpServer::new();
        assert!(server.sessions.is_empty());
    }
    
    #[test]
    fn test_cdp_session_creation() {
        let mut server = CdpServer::new();
        let session_id = server.create_session();
        assert!(server.sessions.contains_key(&session_id));
    }
    
    #[test]
    fn test_runtime_enable() {
        let mut server = CdpServer::new();
        let cmd = CdpCommand::new(1, "Runtime.enable");
        let response = server.handle_command(cmd);
        
        assert!(response.error.is_none());
        assert!(server.is_domain_enabled("Runtime"));
    }
    
    #[test]
    fn test_unknown_method() {
        let mut server = CdpServer::new();
        let cmd = CdpCommand::new(1, "Unknown.method");
        let response = server.handle_command(cmd);
        
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }
}
