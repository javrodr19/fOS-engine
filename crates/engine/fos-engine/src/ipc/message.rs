//! IPC Messages
//!
//! Message types for inter-process communication.

/// Default inline message capacity (64 bytes for small messages)
const INLINE_CAPACITY: usize = 64;

/// IPC message variants
#[derive(Debug, Clone)]
pub enum IpcMessage {
    /// Small messages stored inline (<= 64 bytes)
    Inline(InlineMessage),
    /// Large data via shared memory region
    SharedMemory(SharedMemRef),
    /// File descriptor (Unix only)
    #[cfg(unix)]
    FileDescriptor(FileDescriptorRef),
    /// Typed message with header
    Typed(TypedMessage),
}

/// Inline message for small data
#[derive(Debug, Clone)]
pub struct InlineMessage {
    /// Data buffer (up to 64 bytes)
    data: [u8; INLINE_CAPACITY],
    /// Actual length
    len: u8,
}

impl InlineMessage {
    /// Create new inline message
    pub fn new(data: &[u8]) -> Option<Self> {
        if data.len() > INLINE_CAPACITY {
            return None;
        }
        
        let mut buf = [0u8; INLINE_CAPACITY];
        buf[..data.len()].copy_from_slice(data);
        
        Some(Self {
            data: buf,
            len: data.len() as u8,
        })
    }
    
    /// Get message data
    pub fn as_bytes(&self) -> &[u8] {
        &self.data[..self.len as usize]
    }
    
    /// Get message length
    pub fn len(&self) -> usize {
        self.len as usize
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Reference to shared memory region
#[derive(Debug, Clone)]
pub struct SharedMemRef {
    /// Shared memory handle ID
    pub handle_id: u32,
    /// Offset within the region
    pub offset: usize,
    /// Length of data
    pub len: usize,
}

impl SharedMemRef {
    /// Create new shared memory reference
    pub fn new(handle_id: u32, offset: usize, len: usize) -> Self {
        Self { handle_id, offset, len }
    }
}

/// Reference to file descriptor (Unix only)
#[cfg(unix)]
#[derive(Debug, Clone, Copy)]
pub struct FileDescriptorRef {
    /// Raw file descriptor
    pub fd: i32,
}

#[cfg(unix)]
impl FileDescriptorRef {
    /// Create from raw fd
    pub fn new(fd: i32) -> Self {
        Self { fd }
    }
}

/// Message type identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum MessageType {
    /// Ping/keepalive
    Ping = 1,
    /// Pong response
    Pong = 2,
    /// Navigate to URL
    Navigate = 10,
    /// Execute JavaScript
    ExecuteScript = 11,
    /// DOM update
    DomUpdate = 12,
    /// Layout complete
    LayoutComplete = 13,
    /// Paint layer
    PaintLayer = 20,
    /// Composite frame
    CompositeFrame = 21,
    /// Network request
    NetworkRequest = 30,
    /// Network response
    NetworkResponse = 31,
    /// Storage get
    StorageGet = 40,
    /// Storage set
    StorageSet = 41,
    /// Error
    Error = 255,
}

impl MessageType {
    /// From u16
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            1 => Some(Self::Ping),
            2 => Some(Self::Pong),
            10 => Some(Self::Navigate),
            11 => Some(Self::ExecuteScript),
            12 => Some(Self::DomUpdate),
            13 => Some(Self::LayoutComplete),
            20 => Some(Self::PaintLayer),
            21 => Some(Self::CompositeFrame),
            30 => Some(Self::NetworkRequest),
            31 => Some(Self::NetworkResponse),
            40 => Some(Self::StorageGet),
            41 => Some(Self::StorageSet),
            255 => Some(Self::Error),
            _ => None,
        }
    }
}

/// Typed message with header
#[derive(Debug, Clone)]
pub struct TypedMessage {
    /// Message type
    pub msg_type: MessageType,
    /// Request ID (for request/response correlation)
    pub request_id: u32,
    /// Payload data
    pub payload: Vec<u8>,
}

impl TypedMessage {
    /// Create new typed message
    pub fn new(msg_type: MessageType, request_id: u32, payload: Vec<u8>) -> Self {
        Self { msg_type, request_id, payload }
    }
    
    /// Create ping message
    pub fn ping(request_id: u32) -> Self {
        Self::new(MessageType::Ping, request_id, Vec::new())
    }
    
    /// Create pong message
    pub fn pong(request_id: u32) -> Self {
        Self::new(MessageType::Pong, request_id, Vec::new())
    }
    
    /// Create error message
    pub fn error(request_id: u32, message: &str) -> Self {
        Self::new(MessageType::Error, request_id, message.as_bytes().to_vec())
    }
}

impl IpcMessage {
    /// Create inline message
    pub fn inline(data: &[u8]) -> Option<Self> {
        InlineMessage::new(data).map(Self::Inline)
    }
    
    /// Create shared memory message
    pub fn shared(handle_id: u32, offset: usize, len: usize) -> Self {
        Self::SharedMemory(SharedMemRef::new(handle_id, offset, len))
    }
    
    /// Create typed message
    pub fn typed(msg_type: MessageType, request_id: u32, payload: Vec<u8>) -> Self {
        Self::Typed(TypedMessage::new(msg_type, request_id, payload))
    }
    
    /// Get rough size estimate
    pub fn size_estimate(&self) -> usize {
        match self {
            Self::Inline(m) => m.len(),
            Self::SharedMemory(r) => r.len,
            #[cfg(unix)]
            Self::FileDescriptor(_) => 0,
            Self::Typed(t) => t.payload.len() + 8, // header overhead
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_inline_message() {
        let data = b"Hello, IPC!";
        let msg = InlineMessage::new(data).unwrap();
        
        assert_eq!(msg.as_bytes(), data);
        assert_eq!(msg.len(), data.len());
    }
    
    #[test]
    fn test_inline_too_large() {
        let data = [0u8; 100];
        assert!(InlineMessage::new(&data).is_none());
    }
    
    #[test]
    fn test_typed_message() {
        let msg = TypedMessage::ping(42);
        
        assert_eq!(msg.msg_type, MessageType::Ping);
        assert_eq!(msg.request_id, 42);
        assert!(msg.payload.is_empty());
    }
    
    #[test]
    fn test_message_type_round_trip() {
        for val in [1u16, 2, 10, 11, 12, 13, 20, 21, 30, 31, 40, 41, 255] {
            let mt = MessageType::from_u16(val).unwrap();
            assert_eq!(mt as u16, val);
        }
    }
}
