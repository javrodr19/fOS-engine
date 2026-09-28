//! TCP Connection Layer
//!
//! Low-level TCP connection handling with buffer pooling via PoolAllocator.
//! Integrates with ConnectionPool for connection reuse.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{TcpStream as StdTcpStream, ToSocketAddrs, SocketAddr, Shutdown};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

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
    ///
    /// All resolved addresses are tried using Happy Eyeballs (RFC 8305), so a
    /// host whose IPv6 (or IPv4) path is broken still connects quickly over
    /// the other family instead of waiting for the full connect timeout.
    pub fn connect_with_config(addr: &str, config: TcpConfig) -> io::Result<Self> {
        let addrs: Vec<SocketAddr> = addr.to_socket_addrs()?.collect();
        let stream = connect_happy_eyeballs(&addrs, config.connect_timeout)?;
        Self::from_stream(stream, config)
    }

    /// Connect to a SocketAddr
    pub fn connect_to_addr(addr: SocketAddr, config: TcpConfig) -> io::Result<Self> {
        let stream = std::net::TcpStream::connect_timeout(&addr, config.connect_timeout)?;
        Self::from_stream(stream, config)
    }

    fn from_stream(stream: StdTcpStream, config: TcpConfig) -> io::Result<Self> {
        let addr = stream.peer_addr()?;

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

/// Delay between staggered connection attempts (RFC 8305 §5)
const CONNECTION_ATTEMPT_DELAY: Duration = Duration::from_millis(250);

/// Order addresses by alternating address families, starting with the
/// family the resolver preferred (RFC 8305 §4)
fn interleave_families(addrs: &[SocketAddr]) -> Vec<SocketAddr> {
    let first_is_v6 = addrs.first().map_or(true, |a| a.is_ipv6());
    let (mut primary, mut secondary): (VecDeque<SocketAddr>, VecDeque<SocketAddr>) =
        addrs.iter().copied().partition(|a| a.is_ipv6() == first_is_v6);

    let mut ordered = Vec::with_capacity(addrs.len());
    while !primary.is_empty() || !secondary.is_empty() {
        ordered.extend(primary.pop_front());
        ordered.extend(secondary.pop_front());
    }
    ordered
}

/// Race connection attempts across all addresses (Happy Eyeballs v2).
///
/// Attempts start 250ms apart, or immediately when the previous attempt
/// fails; the first successful connection wins. Losing attempts finish in
/// the background and their sockets are closed.
fn connect_happy_eyeballs(addrs: &[SocketAddr], timeout: Duration) -> io::Result<StdTcpStream> {
    match addrs {
        [] => return Err(io::Error::new(io::ErrorKind::NotFound, "No address found")),
        [single] => return StdTcpStream::connect_timeout(single, timeout),
        _ => {}
    }

    let ordered = interleave_families(addrs);
    let deadline = Instant::now() + timeout;
    let (tx, rx) = mpsc::channel::<io::Result<StdTcpStream>>();
    let mut started = 0;
    let mut finished = 0;
    let mut last_err = None;

    while finished < ordered.len() {
        let now = Instant::now();
        if now >= deadline {
            break;
        }

        if started < ordered.len() {
            let addr = ordered[started];
            let remaining = deadline - now;
            let tx = tx.clone();
            let spawned = thread::Builder::new()
                .name("fos-connect".into())
                .stack_size(64 * 1024)
                .spawn(move || {
                    let _ = tx.send(StdTcpStream::connect_timeout(&addr, remaining));
                });
            if let Err(e) = spawned {
                last_err = Some(e);
                finished += 1;
            }
            started += 1;
        }

        let wait = if started < ordered.len() {
            CONNECTION_ATTEMPT_DELAY
        } else {
            deadline.saturating_duration_since(Instant::now())
        };

        match rx.recv_timeout(wait) {
            Ok(Ok(stream)) => return Ok(stream),
            Ok(Err(e)) => {
                finished += 1;
                last_err = Some(e);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Err(last_err.unwrap_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "connection timed out")))
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

    #[test]
    fn test_interleave_families() {
        let v6a: SocketAddr = "[::1]:1".parse().unwrap();
        let v6b: SocketAddr = "[::1]:2".parse().unwrap();
        let v4a: SocketAddr = "127.0.0.1:3".parse().unwrap();
        let v4b: SocketAddr = "127.0.0.1:4".parse().unwrap();

        let ordered = interleave_families(&[v6a, v6b, v4a, v4b]);
        assert_eq!(ordered, vec![v6a, v4a, v6b, v4b]);

        let ordered = interleave_families(&[v4a, v6a, v6b]);
        assert_eq!(ordered, vec![v4a, v6a, v6b]);
    }

    #[test]
    fn test_happy_eyeballs_falls_back_to_working_address() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let good = listener.local_addr().unwrap();

        // A port with nothing listening: bind, read the port, then close
        let closed = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };

        let start = Instant::now();
        let stream = connect_happy_eyeballs(&[closed, good], Duration::from_secs(5)).unwrap();
        assert_eq!(stream.peer_addr().unwrap(), good);
        // The refused attempt must not cost the full timeout
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn test_happy_eyeballs_no_addresses() {
        assert!(connect_happy_eyeballs(&[], Duration::from_secs(1)).is_err());
    }
}
