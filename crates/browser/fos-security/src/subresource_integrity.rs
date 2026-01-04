//! Subresource Integrity (SRI)
//!
//! Hash validation for external resources.

/// Integrity hash algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityAlgorithm {
    Sha256,
    Sha384,
    Sha512,
}

impl IntegrityAlgorithm {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "sha256" => Some(Self::Sha256),
            "sha384" => Some(Self::Sha384),
            "sha512" => Some(Self::Sha512),
            _ => None,
        }
    }
    
    pub fn prefix(&self) -> &'static str {
        match self { Self::Sha256 => "sha256", Self::Sha384 => "sha384", Self::Sha512 => "sha512" }
    }
    
    pub fn digest_length(&self) -> usize {
        match self { Self::Sha256 => 32, Self::Sha384 => 48, Self::Sha512 => 64 }
    }
}

/// Parsed integrity value
#[derive(Debug, Clone)]
pub struct IntegrityValue {
    pub algorithm: IntegrityAlgorithm,
    pub hash: Vec<u8>,
}

impl IntegrityValue {
    pub fn parse(value: &str) -> Option<Self> {
        let (algo, hash) = value.split_once('-')?;
        let algorithm = IntegrityAlgorithm::parse(algo)?;
        let hash = base64_decode(hash)?;
        if hash.len() != algorithm.digest_length() { return None; }
        Some(Self { algorithm, hash })
    }
}

/// Integrity metadata (multiple hashes)
#[derive(Debug, Clone, Default)]
pub struct IntegrityMetadata {
    pub values: Vec<IntegrityValue>,
}

impl IntegrityMetadata {
    pub fn parse(integrity: &str) -> Self {
        let values = integrity.split_whitespace()
            .filter_map(IntegrityValue::parse)
            .collect();
        Self { values }
    }
    
    pub fn is_empty(&self) -> bool { self.values.is_empty() }
    
    pub fn strongest_algorithm(&self) -> Option<IntegrityAlgorithm> {
        self.values.iter().map(|v| v.algorithm).max_by_key(|a| a.digest_length())
    }
}

/// SRI validator
#[derive(Debug, Default)]
pub struct SriValidator {
    enabled: bool,
}

impl SriValidator {
    pub fn new() -> Self { Self { enabled: true } }
    pub fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }
    
    /// Validate resource against integrity attribute
    pub fn validate(&self, content: &[u8], integrity: &IntegrityMetadata) -> SriResult {
        if !self.enabled || integrity.is_empty() { return SriResult::Skipped; }
        
        // Group by algorithm and check strongest
        let strongest = integrity.strongest_algorithm().unwrap();
        let computed = self.compute_hash(content, strongest);
        
        for value in &integrity.values {
            if value.algorithm == strongest && value.hash == computed {
                return SriResult::Valid;
            }
        }
        SriResult::Invalid { algorithm: strongest, expected: integrity.values.iter()
            .find(|v| v.algorithm == strongest).map(|v| hex_encode(&v.hash)).unwrap_or_default(),
            got: hex_encode(&computed) }
    }
    
    fn compute_hash(&self, content: &[u8], algorithm: IntegrityAlgorithm) -> Vec<u8> {
        // Simple hash computation placeholder
        match algorithm {
            IntegrityAlgorithm::Sha256 => sha256(content),
            IntegrityAlgorithm::Sha384 => sha384(content),
            IntegrityAlgorithm::Sha512 => sha512(content),
        }
    }
}

/// SRI validation result
#[derive(Debug, Clone, PartialEq)]
pub enum SriResult {
    Valid,
    Invalid { algorithm: IntegrityAlgorithm, expected: String, got: String },
    Skipped,
}

// Simple hash implementations (placeholder - would use crypto library)
fn sha256(data: &[u8]) -> Vec<u8> {
    let mut hash = vec![0u8; 32];
    let mut state = 0u64;
    for (i, b) in data.iter().enumerate() {
        state = state.wrapping_add(*b as u64).wrapping_mul(0x517cc1b727220a95);
        hash[i % 32] ^= (state >> ((i % 8) * 8)) as u8;
    }
    hash
}

fn sha384(data: &[u8]) -> Vec<u8> {
    let mut hash = vec![0u8; 48];
    for (i, b) in data.iter().enumerate() { hash[i % 48] ^= b.wrapping_add(i as u8); }
    hash
}

fn sha512(data: &[u8]) -> Vec<u8> {
    let mut hash = vec![0u8; 64];
    for (i, b) in data.iter().enumerate() { hash[i % 64] ^= b.wrapping_add(i as u8); }
    hash
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        if c == b'=' { break; }
        let val = TABLE.iter().position(|&x| x == c)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 { bits -= 8; result.push((buf >> bits) as u8); }
    }
    Some(result)
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_integrity_parse() {
        let meta = IntegrityMetadata::parse("sha256-abc123 sha384-def456");
        assert!(!meta.is_empty());
    }
    
    #[test]
    fn test_algorithm_parse() {
        assert_eq!(IntegrityAlgorithm::parse("sha256"), Some(IntegrityAlgorithm::Sha256));
        assert_eq!(IntegrityAlgorithm::parse("SHA384"), Some(IntegrityAlgorithm::Sha384));
    }
}
