//! FileReader API
//!
//! Implementation of JavaScript FileReader for reading File/Blob contents.

/// FileReader ready state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileReaderState {
    #[default]
    Empty = 0,
    Loading = 1,
    Done = 2,
}

/// FileReader result type
#[derive(Debug, Clone)]
pub enum FileReaderResult {
    ArrayBuffer(Vec<u8>),
    BinaryString(String),
    DataUrl(String),
    Text(String),
}

/// FileReader error
#[derive(Debug, Clone, thiserror::Error)]
pub enum FileReaderError {
    #[error("File not found")]
    NotFound,
    
    #[error("File not readable")]
    NotReadable,
    
    #[error("Read aborted")]
    Abort,
    
    #[error("Security error")]
    Security,
    
    #[error("Encoding error")]
    EncodingError,
}

/// FileReader object
#[derive(Debug, Default)]
pub struct FileReader {
    /// Current state
    pub ready_state: FileReaderState,
    /// Result after reading
    pub result: Option<FileReaderResult>,
    /// Error if any
    pub error: Option<FileReaderError>,
    /// Read pending
    reading: bool,
}

impl FileReader {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Read file as ArrayBuffer
    pub fn read_as_array_buffer(&mut self, data: &[u8]) -> Result<(), FileReaderError> {
        if self.reading {
            return Err(FileReaderError::NotReadable);
        }
        
        self.ready_state = FileReaderState::Loading;
        self.reading = true;
        
        // Simulated async read (in real impl, would be async)
        self.result = Some(FileReaderResult::ArrayBuffer(data.to_vec()));
        self.ready_state = FileReaderState::Done;
        self.reading = false;
        
        Ok(())
    }
    
    /// Read file as binary string
    pub fn read_as_binary_string(&mut self, data: &[u8]) -> Result<(), FileReaderError> {
        if self.reading {
            return Err(FileReaderError::NotReadable);
        }
        
        self.ready_state = FileReaderState::Loading;
        self.reading = true;
        
        // Convert bytes to string (each byte as char)
        let s: String = data.iter().map(|&b| b as char).collect();
        
        self.result = Some(FileReaderResult::BinaryString(s));
        self.ready_state = FileReaderState::Done;
        self.reading = false;
        
        Ok(())
    }
    
    /// Read file as data URL
    pub fn read_as_data_url(&mut self, data: &[u8], mime_type: &str) -> Result<(), FileReaderError> {
        if self.reading {
            return Err(FileReaderError::NotReadable);
        }
        
        self.ready_state = FileReaderState::Loading;
        self.reading = true;
        
        // Encode as base64 data URL
        let base64 = base64_encode(data);
        let url = format!("data:{};base64,{}", mime_type, base64);
        
        self.result = Some(FileReaderResult::DataUrl(url));
        self.ready_state = FileReaderState::Done;
        self.reading = false;
        
        Ok(())
    }
    
    /// Read file as text
    pub fn read_as_text(&mut self, data: &[u8], encoding: Option<&str>) -> Result<(), FileReaderError> {
        if self.reading {
            return Err(FileReaderError::NotReadable);
        }
        
        self.ready_state = FileReaderState::Loading;
        self.reading = true;
        
        // Default to UTF-8
        let _encoding = encoding.unwrap_or("utf-8");
        
        let text = String::from_utf8(data.to_vec())
            .map_err(|_| FileReaderError::EncodingError)?;
        
        self.result = Some(FileReaderResult::Text(text));
        self.ready_state = FileReaderState::Done;
        self.reading = false;
        
        Ok(())
    }
    
    /// Abort reading
    pub fn abort(&mut self) {
        if self.reading {
            self.reading = false;
            self.ready_state = FileReaderState::Done;
            self.result = None;
            self.error = Some(FileReaderError::Abort);
        }
    }
}

/// Simple base64 encoder
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    
    let mut result = String::new();
    
    for chunk in data.chunks(3) {
        let b0 = chunk.get(0).copied().unwrap_or(0);
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        
        result.push(ALPHABET[(b0 >> 2) as usize] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4 | (b1 >> 4)) as usize] as char);
        
        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2 | (b2 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }
        
        if chunk.len() > 2 {
            result.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_read_as_array_buffer() {
        let mut reader = FileReader::new();
        let data = b"Hello, World!";
        
        reader.read_as_array_buffer(data).unwrap();
        
        assert_eq!(reader.ready_state, FileReaderState::Done);
        match &reader.result {
            Some(FileReaderResult::ArrayBuffer(buf)) => {
                assert_eq!(buf, data);
            }
            _ => panic!("Expected ArrayBuffer"),
        }
    }
    
    #[test]
    fn test_read_as_text() {
        let mut reader = FileReader::new();
        let data = b"Hello, World!";
        
        reader.read_as_text(data, None).unwrap();
        
        match &reader.result {
            Some(FileReaderResult::Text(s)) => {
                assert_eq!(s, "Hello, World!");
            }
            _ => panic!("Expected Text"),
        }
    }
    
    #[test]
    fn test_read_as_data_url() {
        let mut reader = FileReader::new();
        let data = b"Hello";
        
        reader.read_as_data_url(data, "text/plain").unwrap();
        
        match &reader.result {
            Some(FileReaderResult::DataUrl(url)) => {
                assert!(url.starts_with("data:text/plain;base64,"));
            }
            _ => panic!("Expected DataUrl"),
        }
    }
    
    #[test]
    fn test_abort() {
        let mut reader = FileReader::new();
        reader.ready_state = FileReaderState::Loading;
        reader.reading = true;
        
        reader.abort();
        
        assert_eq!(reader.ready_state, FileReaderState::Done);
        assert!(matches!(reader.error, Some(FileReaderError::Abort)));
    }
}
