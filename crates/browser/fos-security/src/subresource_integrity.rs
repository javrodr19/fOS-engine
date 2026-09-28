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
        // hash-expression = algorithm "-" base64-value ["?" option-expression]
        let value = value.split('?').next().unwrap_or(value);
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
        let algorithm = match algorithm {
            IntegrityAlgorithm::Sha256 => &ring::digest::SHA256,
            IntegrityAlgorithm::Sha384 => &ring::digest::SHA384,
            IntegrityAlgorithm::Sha512 => &ring::digest::SHA512,
        };
        ring::digest::digest(algorithm, content).as_ref().to_vec()
    }
}

/// SRI validation result
#[derive(Debug, Clone, PartialEq)]
pub enum SriResult {
    Valid,
    Invalid { algorithm: IntegrityAlgorithm, expected: String, got: String },
    Skipped,
}

/// Decode base64 or base64url (padding optional); `None` on invalid input
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut result = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.trim_end_matches('=').bytes() {
        let val = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
        }
    }
    Some(result)
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    /// Example from the SRI specification
    const SCRIPT: &[u8] = b"alert('Hello, world.');";
    const SCRIPT_SHA384: &str = "sha384-H8BRh8j48O9oYatfu5AZzq6A9RINhZO5H16dQZngK7T62em8MUt1FLm52t+eX6xO";
    const SCRIPT_SHA256: &str = "sha256-qznLcsROx4GACP2dm0UCKCzCG+HiZ1guq6ZZDob/Tng=";
    
    #[test]
    fn test_integrity_parse() {
        let meta = IntegrityMetadata::parse(&format!("{} {}", SCRIPT_SHA256, SCRIPT_SHA384));
        assert_eq!(meta.values.len(), 2);
        assert_eq!(meta.strongest_algorithm(), Some(IntegrityAlgorithm::Sha384));
        
        // Options after '?' are ignored
        assert_eq!(IntegrityMetadata::parse(&format!("{}?ct=text/js", SCRIPT_SHA384)).values.len(), 1);
        
        // Wrong digest length, invalid base64 and unknown algorithms are dropped
        assert!(IntegrityMetadata::parse("sha256-abc123 sha384-de!f md5-AAAA").is_empty());
    }
    
    #[test]
    fn test_validate_with_real_digests() {
        let validator = SriValidator::new();
        
        let meta = IntegrityMetadata::parse(SCRIPT_SHA384);
        assert_eq!(validator.validate(SCRIPT, &meta), SriResult::Valid);
        assert!(matches!(validator.validate(b"alert('tampered');", &meta), SriResult::Invalid { .. }));
        
        // Only the strongest algorithm is checked
        let meta = IntegrityMetadata::parse(&format!("{} sha384-{}", SCRIPT_SHA256, "A".repeat(64)));
        assert!(matches!(validator.validate(SCRIPT, &meta), SriResult::Invalid { .. }));
        
        let sha512 = "sha512-Q2bFTOhEALkN8hOms2FKTDLy7eugP2zFZ1T8LCvX42Fp3WoNr3bjZSAHeOsHrbV1Fu9/A0EzCinRE7Af1ofPrw==";
        assert_eq!(validator.validate(SCRIPT, &IntegrityMetadata::parse(sha512)), SriResult::Valid);
        
        // No usable metadata: the resource is not blocked
        assert_eq!(validator.validate(SCRIPT, &IntegrityMetadata::parse("md5-xyz")), SriResult::Skipped);
    }
    
    #[test]
    fn test_base64url_is_accepted() {
        let url_safe = SCRIPT_SHA256.replace('+', "-").replace('/', "_");
        assert_eq!(SriValidator::new().validate(SCRIPT, &IntegrityMetadata::parse(&url_safe)), SriResult::Valid);
    }
    
    #[test]
    fn test_algorithm_parse() {
        assert_eq!(IntegrityAlgorithm::parse("sha256"), Some(IntegrityAlgorithm::Sha256));
        assert_eq!(IntegrityAlgorithm::parse("SHA384"), Some(IntegrityAlgorithm::Sha384));
    }
}
