//! IPC Channel
//!
//! Platform-specific IPC channel implementation using OS primitives.
//! - Unix: Unix domain sockets
//! - Windows: Named pipes

use std::io::{self, Read, Write};

#[cfg(unix)]
use std::os::unix::net::{UnixStream, UnixListener};
#[cfg(windows)]
use std::os::windows::io::RawHandle;

/// IPC channel state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    /// Not connected
    Disconnected,
    /// Connecting
    Connecting,
    /// Connected and ready
    Connected,
    /// Error state
    Error,
}

/// IPC channel for inter-process communication
#[derive(Debug)]
pub struct IpcChannel {
    #[cfg(unix)]
    socket: Option<UnixStream>,
    #[cfg(windows)]
    pipe: Option<WindowsPipe>,
    /// Channel path/name
    path: String,
    /// Current state
    state: ChannelState,
    /// Read buffer
    read_buffer: Vec<u8>,
    /// Write buffer
    write_buffer: Vec<u8>,
}

/// Windows named pipe wrapper (placeholder for cross-platform support)
#[cfg(windows)]
#[derive(Debug)]
struct WindowsPipe {
    handle: RawHandle,
}

impl IpcChannel {
    /// Create new channel (not connected)
    pub fn new(path: &str) -> Self {
        Self {
            #[cfg(unix)]
            socket: None,
            #[cfg(windows)]
            pipe: None,
            path: path.to_string(),
            state: ChannelState::Disconnected,
            read_buffer: Vec::with_capacity(4096),
            write_buffer: Vec::with_capacity(4096),
        }
    }
    
    /// Get channel path
    pub fn path(&self) -> &str {
        &self.path
    }
    
    /// Get current state
    pub fn state(&self) -> ChannelState {
        self.state
    }
    
    /// Is connected?
    pub fn is_connected(&self) -> bool {
        self.state == ChannelState::Connected
    }
    
    /// Connect to server (client side)
    #[cfg(unix)]
    pub fn connect(&mut self) -> io::Result<()> {
        self.state = ChannelState::Connecting;
        
        match UnixStream::connect(&self.path) {
            Ok(socket) => {
                socket.set_nonblocking(true)?;
                self.socket = Some(socket);
                self.state = ChannelState::Connected;
                Ok(())
            }
            Err(e) => {
                self.state = ChannelState::Error;
                Err(e)
            }
        }
    }
    
    #[cfg(windows)]
    pub fn connect(&mut self) -> io::Result<()> {
        // Windows named pipe implementation would go here
        self.state = ChannelState::Error;
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows pipes not yet implemented"))
    }
    
    /// Create from existing stream (server side, after accept)
    #[cfg(unix)]
    pub fn from_stream(stream: UnixStream, path: &str) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            socket: Some(stream),
            path: path.to_string(),
            state: ChannelState::Connected,
            read_buffer: Vec::with_capacity(4096),
            write_buffer: Vec::with_capacity(4096),
        })
    }
    
    /// Send data
    #[cfg(unix)]
    pub fn send(&mut self, data: &[u8]) -> io::Result<usize> {
        if let Some(ref mut socket) = self.socket {
            socket.write(data)
        } else {
            Err(io::Error::new(io::ErrorKind::NotConnected, "Channel not connected"))
        }
    }
    
    #[cfg(windows)]
    pub fn send(&mut self, _data: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows pipes not yet implemented"))
    }
    
    /// Receive data (non-blocking)
    #[cfg(unix)]
    pub fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(ref mut socket) = self.socket {
            match socket.read(buf) {
                Ok(n) => Ok(n),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(0),
                Err(e) => Err(e),
            }
        } else {
            Err(io::Error::new(io::ErrorKind::NotConnected, "Channel not connected"))
        }
    }
    
    #[cfg(windows)]
    pub fn recv(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows pipes not yet implemented"))
    }
    
    /// Close the channel
    pub fn close(&mut self) {
        #[cfg(unix)]
        {
            self.socket = None;
        }
        #[cfg(windows)]
        {
            self.pipe = None;
        }
        self.state = ChannelState::Disconnected;
    }
}

/// IPC server listener
#[derive(Debug)]
pub struct IpcListener {
    #[cfg(unix)]
    listener: UnixListener,
    /// Listener path
    path: String,
}

impl IpcListener {
    /// Bind to path
    #[cfg(unix)]
    pub fn bind(path: &str) -> io::Result<Self> {
        // Remove existing socket file
        let _ = std::fs::remove_file(path);
        
        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        
        Ok(Self {
            listener,
            path: path.to_string(),
        })
    }
    
    #[cfg(windows)]
    pub fn bind(path: &str) -> io::Result<Self> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows pipes not yet implemented"))
    }
    
    /// Accept connection (non-blocking)
    #[cfg(unix)]
    pub fn accept(&self) -> io::Result<Option<IpcChannel>> {
        match self.listener.accept() {
            Ok((stream, _addr)) => {
                let channel = IpcChannel::from_stream(stream, &self.path)?;
                Ok(Some(channel))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
    
    #[cfg(windows)]
    pub fn accept(&self) -> io::Result<Option<IpcChannel>> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows pipes not yet implemented"))
    }
    
    /// Get listener path
    pub fn path(&self) -> &str {
        &self.path
    }
}

#[cfg(unix)]
impl Drop for IpcListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_channel_new() {
        let channel = IpcChannel::new("/tmp/test.sock");
        assert_eq!(channel.state(), ChannelState::Disconnected);
        assert!(!channel.is_connected());
    }
    
    #[cfg(unix)]
    #[test]
    fn test_listener_and_connect() {
        let path = "/tmp/fos-ipc-test.sock";
        
        // Create listener
        let listener = IpcListener::bind(path).unwrap();
        
        // Connect client
        let mut client = IpcChannel::new(path);
        client.connect().unwrap();
        assert!(client.is_connected());
        
        // Accept on server
        let server = listener.accept().unwrap();
        assert!(server.is_some());
    }
    
    #[cfg(unix)]
    #[test]
    fn test_send_recv() {
        let path = "/tmp/fos-ipc-test2.sock";
        
        let listener = IpcListener::bind(path).unwrap();
        
        let mut client = IpcChannel::new(path);
        client.connect().unwrap();
        
        // Small delay to let connection establish
        std::thread::sleep(std::time::Duration::from_millis(10));
        
        let mut server = listener.accept().unwrap().unwrap();
        
        // Send from client
        let msg = b"Hello, IPC!";
        client.send(msg).unwrap();
        
        // Receive on server
        std::thread::sleep(std::time::Duration::from_millis(10));
        let mut buf = [0u8; 64];
        let n = server.recv(&mut buf).unwrap();
        
        assert_eq!(&buf[..n], msg);
    }
}
