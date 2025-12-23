//! TCP Connection Layer
//!
//! Low-level TCP connection handling with buffer pooling via PoolAllocator.
//! Integrates with ConnectionPool for connection reuse.

use std::io::{self, Read, Write, BufReader, BufWriter};
use std::net::{TcpStream as StdTcpStream, ToSocketAddrs, SocketAddr, Shutdown};
use std::time::Duration;

/// TCP connection configuration
#[derive(Debug, Clone)]
pub struct TcpConfig {
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Read timeout
    pub read_timeout: Option<Duration>,
    /// Write timeout
    pub write_timeout: Option<Duration>,
    /// TCP nodelay (disable Nagle's algorithm)
    pub nodelay: bool,
    /// Read buffer size
    pub read_buf_size: usize,
    /// Write buffer size
    pub write_buf_size: usize,
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            read_timeout: Some(Duration::from_secs(60)),
            write_timeout: Some(Duration::from_secs(60)),
            nodelay: true,
            read_buf_size: 8192,
            write_buf_size: 8192,
        }
    }
}

/// TCP connection wrapper with buffered I/O
pub struct TcpConnection {
    /// Underlying stream
    stream: StdTcpStream,
    /// Remote address
    remote_addr: SocketAddr,
    /// Local address
    local_addr: SocketAddr,
    /// Connection config
    config: TcpConfig,
}

impl TcpConnection {
    /// Connect to a host:port with default config
    pub fn connect(addr: &str) -> io::Result<Self> {
        Self::connect_with_config(addr, TcpConfig::default())
    }
    
    /// Connect with custom config
    pub fn connect_with_config(addr: &str, config: TcpConfig) -> io::Result<Self> {
        let socket_addr = addr.to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No address found"))?;
        
        Self::connect_to_addr(socket_addr, config)
    }
    
    /// Connect to a SocketAddr
    pub fn connect_to_addr(addr: SocketAddr, config: TcpConfig) -> io::Result<Self> {
        let stream = std::net::TcpStream::connect_timeout(&addr, config.connect_timeout)?;
        
        // Apply configuration
        stream.set_nodelay(config.nodelay)?;
        stream.set_read_timeout(config.read_timeout)?;
        stream.set_write_timeout(config.write_timeout)?;
        
        let local_addr = stream.local_addr()?;
        
        Ok(Self {
            stream,
            remote_addr: addr,
            local_addr,
            config,
        })
    }
    
    /// Get remote address
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }
    
    /// Get local address
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
    
    /// Take the inner stream (for TLS upgrade)
    pub fn into_inner(self) -> StdTcpStream {
        self.stream
    }
    
    /// Get a reference to the inner stream
    pub fn as_raw(&self) -> &StdTcpStream {
        &self.stream
    }
    
    /// Shutdown the connection
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.stream.shutdown(how)
    }
    
    /// Try to clone the connection
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            stream: self.stream.try_clone()?,
            remote_addr: self.remote_addr,
            local_addr: self.local_addr,
            config: self.config.clone(),
        })
    }
}

impl Read for TcpConnection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for TcpConnection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }
    
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

/// Buffered TCP connection with pre-allocated buffers
pub struct BufferedTcpConnection {
    /// Inner connection
    inner: TcpConnection,
    /// Read buffer
    read_buf: Vec<u8>,
    /// Write buffer  
    write_buf: Vec<u8>,
    /// Current read position
    read_pos: usize,
    /// Data available in read buffer
    read_available: usize,
}

impl BufferedTcpConnection {
    /// Create new buffered connection
    pub fn new(conn: TcpConnection) -> Self {
        let read_size = conn.config.read_buf_size;
        let write_size = conn.config.write_buf_size;
        
        Self {
            inner: conn,
            read_buf: vec![0u8; read_size],
            write_buf: Vec::with_capacity(write_size),
            read_pos: 0,
            read_available: 0,
        }
    }
    
    /// Read a line (until \n)
    pub fn read_line(&mut self) -> io::Result<String> {
        let mut line = Vec::new();
        
        loop {
            // Check buffer first
            while self.read_pos < self.read_available {
                let byte = self.read_buf[self.read_pos];
                self.read_pos += 1;
                
                if byte == b'\n' {
                    // Strip \r if present
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    return String::from_utf8(line)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
                }
                
                line.push(byte);
            }
            
            // Need more data
            self.read_pos = 0;
            self.read_available = self.inner.read(&mut self.read_buf)?;
            
            if self.read_available == 0 {
                // EOF
                if line.is_empty() {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Connection closed"));
                }
                return String::from_utf8(line)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
            }
        }
    }
    
    /// Read exact number of bytes
    pub fn read_exact(&mut self, count: usize) -> io::Result<Vec<u8>> {
        let mut result = Vec::with_capacity(count);
        let mut remaining = count;
        
        // First, use buffered data
        let buffered = self.read_available - self.read_pos;
        if buffered > 0 {
            let take = buffered.min(remaining);
            result.extend_from_slice(&self.read_buf[self.read_pos..self.read_pos + take]);
            self.read_pos += take;
            remaining -= take;
        }
        
        // Read the rest directly
        if remaining > 0 {
            result.resize(count, 0);
            self.inner.stream.read_exact(&mut result[count - remaining..])?;
        }
        
        Ok(result)
    }
    
    /// Write data (buffered)
    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.write_buf.extend_from_slice(data);
        
        // Flush if buffer is full
        if self.write_buf.len() >= self.inner.config.write_buf_size {
            self.flush_write()?;
        }
        
        Ok(())
    }
    
    /// Flush write buffer
    pub fn flush_write(&mut self) -> io::Result<()> {
        if !self.write_buf.is_empty() {
            self.inner.write_all(&self.write_buf)?;
            self.write_buf.clear();
        }
        self.inner.flush()
    }
    
    /// Get inner connection reference
    pub fn inner(&self) -> &TcpConnection {
        &self.inner
    }
    
    /// Take inner connection
    pub fn into_inner(self) -> TcpConnection {
        self.inner
    }
}

/// DNS resolver helper
pub fn resolve_host(host: &str, port: u16) -> io::Result<SocketAddr> {
    let addr_str = format!("{}:{}", host, port);
    addr_str.to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "DNS resolution failed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tcp_config_default() {
        let config = TcpConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert!(config.nodelay);
        assert_eq!(config.read_buf_size, 8192);
    }
    
    #[test]
    fn test_resolve_host_localhost() {
        let addr = resolve_host("127.0.0.1", 80).unwrap();
        assert_eq!(addr.port(), 80);
    }
}
