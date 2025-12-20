//! Blob and File APIs
//!
//! Binary data handling for web content.

use std::sync::{Arc, Mutex};

/// Blob - immutable raw binary data
#[derive(Debug, Clone)]
pub struct Blob {
    data: Arc<Vec<u8>>,
    mime_type: String,
}

impl Blob {
    /// Create a new blob
    pub fn new(parts: Vec<BlobPart>, options: BlobOptions) -> Self {
        let mut data = Vec::new();
        for part in parts {
            match part {
                BlobPart::String(s) => data.extend(s.as_bytes()),
                BlobPart::Bytes(b) => data.extend(b),
                BlobPart::Blob(blob) => data.extend(blob.as_bytes()),
            }
        }
        Self {
            data: Arc::new(data),
            mime_type: options.mime_type.unwrap_or_default(),
        }
    }
    
    /// Get size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
    
    /// Get MIME type
    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }
    
    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    
    /// Slice the blob
    pub fn slice(&self, start: usize, end: Option<usize>, content_type: Option<&str>) -> Blob {
        let end = end.unwrap_or(self.data.len()).min(self.data.len());
        let start = start.min(end);
        
        Blob {
            data: Arc::new(self.data[start..end].to_vec()),
            mime_type: content_type.unwrap_or(&self.mime_type).to_string(),
        }
    }
    
    /// Convert to text
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.data).to_string()
    }
    
    /// Convert to array buffer (as bytes)
    pub fn array_buffer(&self) -> Vec<u8> {
        self.data.to_vec()
    }
}

/// Blob part for construction
pub enum BlobPart {
    String(String),
    Bytes(Vec<u8>),
    Blob(Blob),
}

/// Blob options
#[derive(Debug, Clone, Default)]
pub struct BlobOptions {
    pub mime_type: Option<String>,
    pub endings: LineEndings,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineEndings {
    #[default]
    Transparent,
    Native,
}

/// File - extends Blob with filename
#[derive(Debug, Clone)]
pub struct File {
    blob: Blob,
    name: String,
    last_modified: u64, // Unix timestamp ms
}

impl File {
    pub fn new(parts: Vec<BlobPart>, name: &str, options: FileOptions) -> Self {
        let blob = Blob::new(parts, BlobOptions {
            mime_type: options.mime_type,
            ..Default::default()
        });
        Self {
            blob,
            name: name.to_string(),
            last_modified: options.last_modified.unwrap_or(0),
        }
    }
    
    pub fn name(&self) -> &str {
        &self.name
    }
    
    pub fn last_modified(&self) -> u64 {
        self.last_modified
    }
    
    pub fn size(&self) -> usize {
        self.blob.size()
    }
    
    pub fn mime_type(&self) -> &str {
        self.blob.mime_type()
    }
    
    pub fn as_blob(&self) -> &Blob {
        &self.blob
    }
}

/// File options
#[derive(Debug, Clone, Default)]
pub struct FileOptions {
    pub mime_type: Option<String>,
    pub last_modified: Option<u64>,
}

/// FileReader - async blob reading
#[derive(Debug)]
pub struct FileReader {
    result: Arc<Mutex<Option<FileReaderResult>>>,
    state: FileReaderState,
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum FileReaderResult {
    ArrayBuffer(Vec<u8>),
    Text(String),
    DataUrl(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileReaderState {
    Empty,
    Loading,
    Done,
}

impl FileReader {
    pub fn new() -> Self {
        Self {
            result: Arc::new(Mutex::new(None)),
            state: FileReaderState::Empty,
            error: None,
        }
    }
    
    pub fn ready_state(&self) -> FileReaderState {
        self.state
    }
    
    pub fn result(&self) -> Option<FileReaderResult> {
        self.result.lock().unwrap().clone()
    }
    
    pub fn read_as_array_buffer(&mut self, blob: &Blob) {
        self.state = FileReaderState::Loading;
        *self.result.lock().unwrap() = Some(FileReaderResult::ArrayBuffer(blob.array_buffer()));
        self.state = FileReaderState::Done;
    }
    
    pub fn read_as_text(&mut self, blob: &Blob) {
        self.state = FileReaderState::Loading;
        *self.result.lock().unwrap() = Some(FileReaderResult::Text(blob.text()));
        self.state = FileReaderState::Done;
    }
    
    pub fn read_as_data_url(&mut self, blob: &Blob) {
        self.state = FileReaderState::Loading;
        let base64 = base64_encode(blob.as_bytes());
        let data_url = format!("data:{};base64,{}", blob.mime_type(), base64);
        *self.result.lock().unwrap() = Some(FileReaderResult::DataUrl(data_url));
        self.state = FileReaderState::Done;
    }
    
    pub fn abort(&mut self) {
        self.state = FileReaderState::Done;
        *self.result.lock().unwrap() = None;
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    
    for chunk in data.chunks(3) {
        let n = match chunk.len() {
            3 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32),
            2 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8),
            1 => (chunk[0] as u32) << 16,
            _ => 0,
        };
        
        result.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        
        if chunk.len() > 2 {
            result.push(CHARS[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    
    result
}

impl Default for FileReader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_blob() {
        let blob = Blob::new(
            vec![BlobPart::String("Hello".into())],
            BlobOptions::default(),
        );
        
        assert_eq!(blob.size(), 5);
        assert_eq!(blob.text(), "Hello");
    }
    
    #[test]
    fn test_file() {
        let file = File::new(
            vec![BlobPart::String("content".into())],
            "test.txt",
            FileOptions::default(),
        );
        
        assert_eq!(file.name(), "test.txt");
        assert_eq!(file.size(), 7);
    }
}
