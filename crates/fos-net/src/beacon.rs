//! Beacon API
//!
//! Send analytics data without blocking.

/// Send a beacon (fire-and-forget HTTP POST)
pub fn send_beacon(url: &str, data: BeaconData) -> bool {
    // Would send async POST request
    // Returns true if queued successfully
    !url.is_empty() && data.is_valid()
}

/// Beacon data types
#[derive(Debug, Clone)]
pub enum BeaconData {
    Text(String),
    Blob { data: Vec<u8>, mime_type: String },
    FormData(Vec<(String, String)>),
    UrlSearchParams(String),
}

impl BeaconData {
    /// Check if data is valid for sending
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Text(s) => !s.is_empty(),
            Self::Blob { data, .. } => !data.is_empty(),
            Self::FormData(pairs) => !pairs.is_empty(),
            Self::UrlSearchParams(s) => !s.is_empty(),
        }
    }
    
    /// Get content type
    pub fn content_type(&self) -> &str {
        match self {
            Self::Text(_) => "text/plain;charset=UTF-8",
            Self::Blob { mime_type, .. } => mime_type,
            Self::FormData(_) => "multipart/form-data",
            Self::UrlSearchParams(_) => "application/x-www-form-urlencoded",
        }
    }
    
    /// Get body bytes
    pub fn body(&self) -> Vec<u8> {
        match self {
            Self::Text(s) => s.as_bytes().to_vec(),
            Self::Blob { data, .. } => data.clone(),
            Self::FormData(pairs) => {
                pairs.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join("&")
                    .into_bytes()
            }
            Self::UrlSearchParams(s) => s.as_bytes().to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_beacon_data_text() {
        let data = BeaconData::Text("analytics".into());
        
        assert!(data.is_valid());
        assert_eq!(data.content_type(), "text/plain;charset=UTF-8");
    }
    
    #[test]
    fn test_send_beacon() {
        let result = send_beacon("https://example.com/analytics", BeaconData::Text("test".into()));
        assert!(result);
    }
}
