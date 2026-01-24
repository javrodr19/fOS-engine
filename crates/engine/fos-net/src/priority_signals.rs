//! RFC 9218 Priority Signals
//!
//! Extensible prioritization for HTTP/2 and HTTP/3 per RFC 9218.
//! Provides urgency (0-7) and incremental flags for resource prioritization.

use std::fmt;

/// Priority signal per RFC 9218
/// 
/// Urgency levels:
/// - 0: Highest priority (render-blocking)
/// - 1-2: High priority (CSS, fonts)
/// - 3-4: Normal priority (async scripts, images)
/// - 5-6: Low priority (prefetch)
/// - 7: Lowest priority (speculative)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrioritySignal {
    /// Urgency level (0-7, lower = more urgent)
    urgency: u8,
    /// Whether the resource can be incrementally processed
    incremental: bool,
}

impl Default for PrioritySignal {
    fn default() -> Self {
        Self {
            urgency: 3,
            incremental: false,
        }
    }
}

impl PrioritySignal {
    /// Create a new priority signal
    pub const fn new(urgency: u8, incremental: bool) -> Self {
        Self {
            urgency: if urgency > 7 { 7 } else { urgency },
            incremental,
        }
    }
    
    /// Highest priority (urgency 0, non-incremental)
    pub const fn highest() -> Self {
        Self { urgency: 0, incremental: false }
    }
    
    /// Critical priority for render-blocking resources
    pub const fn critical() -> Self {
        Self { urgency: 0, incremental: false }
    }
    
    /// High priority for important resources
    pub const fn high() -> Self {
        Self { urgency: 1, incremental: false }
    }
    
    /// Normal priority
    pub const fn normal() -> Self {
        Self { urgency: 3, incremental: false }
    }
    
    /// Low priority
    pub const fn low() -> Self {
        Self { urgency: 5, incremental: true }
    }
    
    /// Lowest priority for speculative resources
    pub const fn lowest() -> Self {
        Self { urgency: 7, incremental: true }
    }
    
    /// Get urgency level (0-7)
    pub const fn urgency(&self) -> u8 {
        self.urgency
    }
    
    /// Check if incremental processing is enabled
    pub const fn is_incremental(&self) -> bool {
        self.incremental
    }
    
    /// Set urgency level
    pub fn with_urgency(mut self, urgency: u8) -> Self {
        self.urgency = urgency.min(7);
        self
    }
    
    /// Set incremental flag
    pub fn with_incremental(mut self, incremental: bool) -> Self {
        self.incremental = incremental;
        self
    }
    
    /// Serialize to HTTP Priority header value
    /// Format: u=N, i (where N is urgency, i is optional incremental flag)
    pub fn to_header_value(&self) -> String {
        if self.incremental {
            format!("u={}, i", self.urgency)
        } else {
            format!("u={}", self.urgency)
        }
    }
    
    /// Parse from HTTP Priority header value
    pub fn from_header_value(value: &str) -> Option<Self> {
        let mut urgency = 3u8; // Default per RFC
        let mut incremental = false;
        
        for part in value.split(',') {
            let part = part.trim();
            if part.starts_with("u=") {
                if let Ok(u) = part[2..].trim().parse::<u8>() {
                    urgency = u.min(7);
                }
            } else if part == "i" {
                incremental = true;
            }
        }
        
        Some(Self { urgency, incremental })
    }
    
    /// Serialize to HTTP/2 PRIORITY frame format
    /// Returns (exclusive, stream_dependency, weight)
    pub fn to_h2_priority(&self) -> (bool, u32, u8) {
        // Map urgency 0-7 to weight 256-1 (inverse relationship)
        let weight = 256 - (self.urgency as u16 * 32).min(255) as u8;
        (false, 0, weight)
    }
    
    /// Create from HTTP/2 PRIORITY frame
    pub fn from_h2_priority(weight: u8) -> Self {
        // Map weight 1-256 to urgency 7-0
        let urgency = 7 - (weight / 32).min(7);
        Self {
            urgency,
            incremental: false,
        }
    }
}

impl fmt::Display for PrioritySignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// Resource type with associated priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourcePriority {
    /// HTML document - highest priority
    Document,
    /// Render-blocking CSS
    BlockingStyle,
    /// Fonts visible above the fold
    VisibleFont,
    /// Async/deferred JavaScript
    AsyncScript,
    /// Images in viewport
    ViewportImage,
    /// Below-fold images
    LazyImage,
    /// Prefetch resources
    Prefetch,
    /// Speculative/prerender
    Speculative,
}

impl ResourcePriority {
    /// Get the default priority signal for this resource type
    pub const fn to_signal(&self) -> PrioritySignal {
        match self {
            Self::Document => PrioritySignal::new(0, false),
            Self::BlockingStyle => PrioritySignal::new(1, false),
            Self::VisibleFont => PrioritySignal::new(2, false),
            Self::AsyncScript => PrioritySignal::new(3, true),
            Self::ViewportImage => PrioritySignal::new(4, true),
            Self::LazyImage => PrioritySignal::new(5, true),
            Self::Prefetch => PrioritySignal::new(7, true),
            Self::Speculative => PrioritySignal::new(7, true),
        }
    }
}

/// Priority scheduler for managing request ordering
#[derive(Debug, Default)]
pub struct PriorityScheduler {
    /// Pending requests by urgency level
    queues: [Vec<ScheduledRequest>; 8],
    /// Total pending count
    pending: usize,
}

/// A scheduled request with priority
#[derive(Debug, Clone)]
pub struct ScheduledRequest {
    /// Request ID
    pub id: u64,
    /// Stream ID (for HTTP/2, HTTP/3)
    pub stream_id: Option<u32>,
    /// Priority signal
    pub priority: PrioritySignal,
    /// URL for the request
    pub url: String,
}

impl ScheduledRequest {
    /// Create a new scheduled request
    pub fn new(id: u64, url: String, priority: PrioritySignal) -> Self {
        Self {
            id,
            stream_id: None,
            url,
            priority,
        }
    }
    
    /// Set stream ID
    pub fn with_stream_id(mut self, stream_id: u32) -> Self {
        self.stream_id = Some(stream_id);
        self
    }
}

impl PriorityScheduler {
    /// Create a new priority scheduler
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add a request to the scheduler
    pub fn push(&mut self, request: ScheduledRequest) {
        let urgency = request.priority.urgency() as usize;
        self.queues[urgency].push(request);
        self.pending += 1;
    }
    
    /// Pop the highest priority request
    pub fn pop(&mut self) -> Option<ScheduledRequest> {
        for queue in &mut self.queues {
            if let Some(req) = queue.pop() {
                self.pending -= 1;
                return Some(req);
            }
        }
        None
    }
    
    /// Get the next request without removing it
    pub fn peek(&self) -> Option<&ScheduledRequest> {
        for queue in &self.queues {
            if let Some(req) = queue.last() {
                return Some(req);
            }
        }
        None
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.pending == 0
    }
    
    /// Get pending count
    pub fn len(&self) -> usize {
        self.pending
    }
    
    /// Get count by urgency level
    pub fn count_by_urgency(&self) -> [usize; 8] {
        let mut counts = [0usize; 8];
        for (i, queue) in self.queues.iter().enumerate() {
            counts[i] = queue.len();
        }
        counts
    }
    
    /// Update priority for a request
    pub fn update_priority(&mut self, id: u64, new_priority: PrioritySignal) -> bool {
        // Find and remove from current queue
        let mut found_request = None;
        for queue in &mut self.queues {
            if let Some(pos) = queue.iter().position(|r| r.id == id) {
                found_request = Some(queue.remove(pos));
                break;
            }
        }
        
        // Re-insert with new priority
        if let Some(mut request) = found_request {
            request.priority = new_priority;
            let urgency = new_priority.urgency() as usize;
            self.queues[urgency].push(request);
            true
        } else {
            false
        }
    }
    
    /// Cancel a request
    pub fn cancel(&mut self, id: u64) -> bool {
        for queue in &mut self.queues {
            if let Some(pos) = queue.iter().position(|r| r.id == id) {
                queue.remove(pos);
                self.pending -= 1;
                return true;
            }
        }
        false
    }
    
    /// Clear all pending requests
    pub fn clear(&mut self) {
        for queue in &mut self.queues {
            queue.clear();
        }
        self.pending = 0;
    }
}

/// QUIC/HTTP3 priority frame encoder
#[derive(Debug, Default)]
pub struct H3PriorityEncoder;

impl H3PriorityEncoder {
    /// Encode PRIORITY_UPDATE frame for HTTP/3
    pub fn encode_priority_update(stream_id: u64, priority: &PrioritySignal) -> Vec<u8> {
        let mut buf = Vec::new();
        
        // Frame type: PRIORITY_UPDATE (0x0f)
        buf.push(0x0f);
        
        // Encode prioritized element ID (stream ID) as varint
        Self::encode_varint(&mut buf, stream_id);
        
        // Encode priority field value
        let value = priority.to_header_value();
        Self::encode_varint(&mut buf, value.len() as u64);
        buf.extend_from_slice(value.as_bytes());
        
        buf
    }
    
    /// Decode PRIORITY_UPDATE frame
    pub fn decode_priority_update(data: &[u8]) -> Option<(u64, PrioritySignal)> {
        if data.is_empty() {
            return None;
        }
        
        let mut pos = 0;
        
        // Skip frame type
        if data[pos] != 0x0f {
            return None;
        }
        pos += 1;
        
        // Decode stream ID
        let (stream_id, len) = Self::decode_varint(&data[pos..])?;
        pos += len;
        
        // Decode priority field value length
        let (value_len, len) = Self::decode_varint(&data[pos..])?;
        pos += len;
        
        // Decode priority field value
        if pos + value_len as usize > data.len() {
            return None;
        }
        let value = std::str::from_utf8(&data[pos..pos + value_len as usize]).ok()?;
        let priority = PrioritySignal::from_header_value(value)?;
        
        Some((stream_id, priority))
    }
    
    fn encode_varint(buf: &mut Vec<u8>, mut value: u64) {
        if value < 64 {
            buf.push(value as u8);
        } else if value < 16384 {
            buf.push(0x40 | (value >> 8) as u8);
            buf.push(value as u8);
        } else if value < 1073741824 {
            buf.push(0x80 | (value >> 24) as u8);
            buf.push((value >> 16) as u8);
            buf.push((value >> 8) as u8);
            buf.push(value as u8);
        } else {
            buf.push(0xc0 | (value >> 56) as u8);
            for i in (0..7).rev() {
                buf.push((value >> (i * 8)) as u8);
            }
        }
    }
    
    fn decode_varint(data: &[u8]) -> Option<(u64, usize)> {
        if data.is_empty() {
            return None;
        }
        
        let first = data[0];
        let prefix = first >> 6;
        
        match prefix {
            0 => Some((first as u64, 1)),
            1 if data.len() >= 2 => {
                let value = ((first & 0x3f) as u64) << 8 | data[1] as u64;
                Some((value, 2))
            }
            2 if data.len() >= 4 => {
                let value = ((first & 0x3f) as u64) << 24
                    | (data[1] as u64) << 16
                    | (data[2] as u64) << 8
                    | data[3] as u64;
                Some((value, 4))
            }
            3 if data.len() >= 8 => {
                let value = ((first & 0x3f) as u64) << 56
                    | (data[1] as u64) << 48
                    | (data[2] as u64) << 40
                    | (data[3] as u64) << 32
                    | (data[4] as u64) << 24
                    | (data[5] as u64) << 16
                    | (data[6] as u64) << 8
                    | data[7] as u64;
                Some((value, 8))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_priority_signal_default() {
        let signal = PrioritySignal::default();
        assert_eq!(signal.urgency(), 3);
        assert!(!signal.is_incremental());
    }
    
    #[test]
    fn test_priority_signal_levels() {
        assert_eq!(PrioritySignal::highest().urgency(), 0);
        assert_eq!(PrioritySignal::critical().urgency(), 0);
        assert_eq!(PrioritySignal::high().urgency(), 1);
        assert_eq!(PrioritySignal::normal().urgency(), 3);
        assert_eq!(PrioritySignal::low().urgency(), 5);
        assert_eq!(PrioritySignal::lowest().urgency(), 7);
    }
    
    #[test]
    fn test_priority_signal_header() {
        let signal = PrioritySignal::new(2, true);
        assert_eq!(signal.to_header_value(), "u=2, i");
        
        let signal = PrioritySignal::new(4, false);
        assert_eq!(signal.to_header_value(), "u=4");
    }
    
    #[test]
    fn test_priority_signal_parse() {
        let signal = PrioritySignal::from_header_value("u=2, i").unwrap();
        assert_eq!(signal.urgency(), 2);
        assert!(signal.is_incremental());
        
        let signal = PrioritySignal::from_header_value("u=5").unwrap();
        assert_eq!(signal.urgency(), 5);
        assert!(!signal.is_incremental());
    }
    
    #[test]
    fn test_priority_clamping() {
        let signal = PrioritySignal::new(10, false);
        assert_eq!(signal.urgency(), 7); // Clamped to max
    }
    
    #[test]
    fn test_resource_priority() {
        assert_eq!(ResourcePriority::Document.to_signal().urgency(), 0);
        assert_eq!(ResourcePriority::BlockingStyle.to_signal().urgency(), 1);
        assert_eq!(ResourcePriority::Prefetch.to_signal().urgency(), 7);
        assert!(ResourcePriority::AsyncScript.to_signal().is_incremental());
    }
    
    #[test]
    fn test_priority_scheduler() {
        let mut scheduler = PriorityScheduler::new();
        
        scheduler.push(ScheduledRequest::new(1, "low.js".into(), PrioritySignal::low()));
        scheduler.push(ScheduledRequest::new(2, "high.css".into(), PrioritySignal::high()));
        scheduler.push(ScheduledRequest::new(3, "doc.html".into(), PrioritySignal::critical()));
        
        assert_eq!(scheduler.len(), 3);
        
        // Should pop in priority order
        assert_eq!(scheduler.pop().unwrap().id, 3); // Critical first
        assert_eq!(scheduler.pop().unwrap().id, 2); // High second
        assert_eq!(scheduler.pop().unwrap().id, 1); // Low last
        
        assert!(scheduler.is_empty());
    }
    
    #[test]
    fn test_priority_update() {
        let mut scheduler = PriorityScheduler::new();
        scheduler.push(ScheduledRequest::new(1, "test.js".into(), PrioritySignal::low()));
        
        // Update priority
        assert!(scheduler.update_priority(1, PrioritySignal::high()));
        
        let req = scheduler.pop().unwrap();
        assert_eq!(req.priority.urgency(), 1);
    }
    
    #[test]
    fn test_h3_priority_encoder() {
        let priority = PrioritySignal::new(2, true);
        let encoded = H3PriorityEncoder::encode_priority_update(4, &priority);
        
        let (stream_id, decoded) = H3PriorityEncoder::decode_priority_update(&encoded).unwrap();
        assert_eq!(stream_id, 4);
        assert_eq!(decoded.urgency(), 2);
        assert!(decoded.is_incremental());
    }
    
    #[test]
    fn test_h2_priority_conversion() {
        let signal = PrioritySignal::new(0, false);
        let (exclusive, dep, weight) = signal.to_h2_priority();
        assert!(!exclusive);
        assert_eq!(dep, 0);
        assert!(weight > 200); // High weight for urgency 0
        
        let signal = PrioritySignal::new(7, true);
        let (_, _, weight) = signal.to_h2_priority();
        assert!(weight < 50); // Low weight for urgency 7
    }
}
