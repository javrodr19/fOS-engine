//! Streaming Responses
//!
//! Support for reading HTTP response bodies incrementally with
//! chunked transfer encoding and progress tracking.

use std::io::{self, Read, BufRead, BufReader};

/// Streaming body state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// Not started reading
    Pending,
    /// Currently streaming
    Reading,
    /// All data received
    Complete,
    /// Error occurred
    Error,
}

/// Body encoding type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferEncoding {
    /// Content-Length specified
    ContentLength(u64),
    /// Chunked transfer encoding
    Chunked,
    /// Read until connection close
    UntilClose,
}

/// Streaming body reader
#[derive(Debug)]
pub struct StreamingBody<R: Read> {
    reader: BufReader<R>,
    encoding: TransferEncoding,
    bytes_read: u64,
    state: StreamState,
    /// Internal buffer for chunk data
    chunk_remaining: usize,
}

impl<R: Read> StreamingBody<R> {
    /// Create new streaming body with content-length
    pub fn with_content_length(reader: R, length: u64) -> Self {
        Self {
            reader: BufReader::new(reader),
            encoding: TransferEncoding::ContentLength(length),
            bytes_read: 0,
            state: StreamState::Pending,
            chunk_remaining: 0,
        }
    }
    
    /// Create new streaming body with chunked encoding
    pub fn chunked(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            encoding: TransferEncoding::Chunked,
            bytes_read: 0,
            state: StreamState::Pending,
            chunk_remaining: 0,
        }
    }
    
    /// Create streaming body that reads until connection close
    pub fn until_close(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            encoding: TransferEncoding::UntilClose,
            bytes_read: 0,
            state: StreamState::Pending,
            chunk_remaining: 0,
        }
    }
    
    /// Get current state
    pub fn state(&self) -> StreamState {
        self.state
    }
    
    /// Get bytes read so far
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
    
    /// Get total bytes if known
    pub fn total_bytes(&self) -> Option<u64> {
        match self.encoding {
            TransferEncoding::ContentLength(len) => Some(len),
            _ => None,
        }
    }
    
    /// Get progress as percentage (0.0 - 1.0)
    pub fn progress(&self) -> Option<f64> {
        self.total_bytes().map(|total| {
            if total == 0 {
                1.0
            } else {
                self.bytes_read as f64 / total as f64
            }
        })
    }
    
    /// Check if complete
    pub fn is_complete(&self) -> bool {
        self.state == StreamState::Complete
    }
    
    /// Read next chunk into buffer, returns bytes read
    pub fn read_chunk(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.state == StreamState::Complete {
            return Ok(0);
        }
        
        self.state = StreamState::Reading;
        
        let bytes = match self.encoding {
            TransferEncoding::ContentLength(total) => {
                let remaining = total - self.bytes_read;
                if remaining == 0 {
                    self.state = StreamState::Complete;
                    return Ok(0);
                }
                
                let to_read = buf.len().min(remaining as usize);
                let n = self.reader.read(&mut buf[..to_read])?;
                
                if n == 0 {
                    self.state = StreamState::Error;
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Connection closed before content-length reached",
                    ));
                }
                
                n
            }
            TransferEncoding::Chunked => {
                self.read_chunked(buf)?
            }
            TransferEncoding::UntilClose => {
                let n = self.reader.read(buf)?;
                if n == 0 {
                    self.state = StreamState::Complete;
                }
                n
            }
        };
        
        self.bytes_read += bytes as u64;
        
        // Check completion for content-length
        if let TransferEncoding::ContentLength(total) = self.encoding {
            if self.bytes_read >= total {
                self.state = StreamState::Complete;
            }
        }
        
        Ok(bytes)
    }
    
    /// Read from chunked encoding
    fn read_chunked(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // If we have remaining chunk data, read from it
        if self.chunk_remaining > 0 {
            let to_read = buf.len().min(self.chunk_remaining);
            let n = self.reader.read(&mut buf[..to_read])?;
            self.chunk_remaining -= n;
            
            // If chunk finished, consume trailing CRLF
            if self.chunk_remaining == 0 {
                let mut crlf = [0u8; 2];
                self.reader.read_exact(&mut crlf)?;
            }
            
            return Ok(n);
        }
        
        // Read next chunk size
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        let line = line.trim();
        
        // Parse chunk size (may include extension after ;)
        let size_str = line.split(';').next().unwrap_or(line);
        let chunk_size = usize::from_str_radix(size_str, 16)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        
        // Zero chunk means end
        if chunk_size == 0 {
            // Consume trailing headers and final CRLF
            loop {
                let mut trailer = String::new();
                self.reader.read_line(&mut trailer)?;
                if trailer.trim().is_empty() {
                    break;
                }
            }
            self.state = StreamState::Complete;
            return Ok(0);
        }
        
        self.chunk_remaining = chunk_size;
        
        // Now read from the chunk
        let to_read = buf.len().min(self.chunk_remaining);
        let n = self.reader.read(&mut buf[..to_read])?;
        self.chunk_remaining -= n;
        
        // If chunk finished, consume trailing CRLF
        if self.chunk_remaining == 0 {
            let mut crlf = [0u8; 2];
            self.reader.read_exact(&mut crlf)?;
        }
        
        Ok(n)
    }
    
    /// Read entire body to vec
    pub fn read_to_end(&mut self) -> io::Result<Vec<u8>> {
        let mut data = Vec::new();
        let mut buf = [0u8; 8192];
        
        loop {
            let n = self.read_chunk(&mut buf)?;
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n]);
        }
        
        Ok(data)
    }
}

/// Streaming body iterator
pub struct StreamIterator<R: Read> {
    body: StreamingBody<R>,
    chunk_size: usize,
}

impl<R: Read> StreamIterator<R> {
    /// Create iterator with default chunk size (8KB)
    pub fn new(body: StreamingBody<R>) -> Self {
        Self {
            body,
            chunk_size: 8192,
        }
    }
    
    /// Create iterator with custom chunk size
    pub fn with_chunk_size(body: StreamingBody<R>, chunk_size: usize) -> Self {
        Self { body, chunk_size }
    }
}

impl<R: Read> Iterator for StreamIterator<R> {
    type Item = io::Result<Vec<u8>>;
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.body.is_complete() {
            return None;
        }
        
        let mut buf = vec![0u8; self.chunk_size];
        match self.body.read_chunk(&mut buf) {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some(Ok(buf))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

/// Progress callback type
pub type ProgressCallback = Box<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Streaming body with progress callback
pub struct ProgressBody<R: Read> {
    inner: StreamingBody<R>,
    callback: Option<ProgressCallback>,
}

impl<R: Read> ProgressBody<R> {
    /// Create with progress callback
    pub fn new(body: StreamingBody<R>, callback: ProgressCallback) -> Self {
        Self {
            inner: body,
            callback: Some(callback),
        }
    }
    
    /// Read chunk with progress notification
    pub fn read_chunk(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read_chunk(buf)?;
        
        if let Some(callback) = &self.callback {
            callback(self.inner.bytes_read(), self.inner.total_bytes());
        }
        
        Ok(n)
    }
}

/// Parse Transfer-Encoding and Content-Length from headers
pub fn detect_encoding(headers: &[(String, String)]) -> TransferEncoding {
    for (name, value) in headers {
        let name_lower = name.to_lowercase();
        
        if name_lower == "transfer-encoding" {
            if value.to_lowercase().contains("chunked") {
                return TransferEncoding::Chunked;
            }
        }
        
        if name_lower == "content-length" {
            if let Ok(len) = value.parse::<u64>() {
                return TransferEncoding::ContentLength(len);
            }
        }
    }
    
    TransferEncoding::UntilClose
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    
    #[test]
    fn test_content_length_stream() {
        let data = b"Hello, World!";
        let mut body = StreamingBody::with_content_length(
            Cursor::new(data.to_vec()),
            data.len() as u64,
        );
        
        assert_eq!(body.total_bytes(), Some(13));
        
        let mut buf = [0u8; 5];
        let n = body.read_chunk(&mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"Hello");
        assert_eq!(body.bytes_read(), 5);
        
        let result = body.read_to_end().unwrap();
        assert_eq!(result, b", World!");
        assert!(body.is_complete());
    }
    
    #[test]
    fn test_chunked_stream() {
        // Chunked encoded body
        let chunked = b"5\r\nHello\r\n7\r\n, World\r\n0\r\n\r\n";
        let mut body = StreamingBody::chunked(Cursor::new(chunked.to_vec()));
        
        let result = body.read_to_end().unwrap();
        assert_eq!(result, b"Hello, World");
        assert!(body.is_complete());
    }
    
    #[test]
    fn test_progress() {
        let data = vec![0u8; 100];
        let body = StreamingBody::with_content_length(
            Cursor::new(data),
            100,
        );
        
        let mut progress_body = ProgressBody::new(body, Box::new(|read, total| {
            println!("Progress: {}/{:?}", read, total);
        }));
        
        let mut buf = [0u8; 25];
        progress_body.read_chunk(&mut buf).unwrap();
        assert_eq!(progress_body.inner.bytes_read(), 25);
    }
    
    #[test]
    fn test_stream_iterator() {
        let data = b"0123456789".to_vec();
        let body = StreamingBody::with_content_length(
            Cursor::new(data),
            10,
        );
        
        let iter = StreamIterator::with_chunk_size(body, 4);
        let chunks: Vec<_> = iter.map(|r| r.unwrap()).collect();
        
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], b"0123");
        assert_eq!(chunks[1], b"4567");
        assert_eq!(chunks[2], b"89");
    }
    
    #[test]
    fn test_detect_encoding() {
        let headers = vec![
            ("Content-Type".to_string(), "text/html".to_string()),
            ("Transfer-Encoding".to_string(), "chunked".to_string()),
        ];
        assert_eq!(detect_encoding(&headers), TransferEncoding::Chunked);
        
        let headers = vec![
            ("Content-Length".to_string(), "1024".to_string()),
        ];
        assert_eq!(detect_encoding(&headers), TransferEncoding::ContentLength(1024));
    }
}
