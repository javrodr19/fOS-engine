//! Password manager integration
//!
//! Secure credential storage for browser autofill.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// A stored credential
#[derive(Debug, Clone)]
pub struct Credential {
    pub id: u64,
    pub origin: String,
    pub username: String,
    /// Encrypted password (base64)
    pub password_encrypted: String,
    pub created: u64,
    pub last_used: u64,
}

/// Password manager
#[derive(Debug)]
pub struct PasswordManager {
    credentials: HashMap<u64, Credential>,
    next_id: u64,
    storage_path: Option<PathBuf>,
    /// Simple XOR key for obfuscation (NOT secure encryption)
    /// Real implementation would use proper encryption
    obfuscation_key: [u8; 32],
}

impl Default for PasswordManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordManager {
    pub fn new() -> Self {
        Self {
            credentials: HashMap::new(),
            next_id: 1,
            storage_path: None,
            obfuscation_key: [0x42; 32], // Placeholder key
        }
    }
    
    /// Create with storage
    pub fn with_storage(path: PathBuf) -> Self {
        let mut mgr = Self::new();
        mgr.storage_path = Some(path);
        mgr.load();
        mgr
    }
    
    /// Save a credential
    pub fn save_credential(&mut self, origin: &str, username: &str, password: &str) -> u64 {
        // Compute obfuscation first to avoid borrow issues
        let encrypted = self.obfuscate(password);
        let now = Self::now();
        
        // Check if credential already exists for this origin/username
        if let Some(existing) = self.find_credential(origin, username) {
            let id = existing.id;
            if let Some(cred) = self.credentials.get_mut(&id) {
                cred.password_encrypted = encrypted;
                cred.last_used = now;
            }
            return id;
        }
        
        let id = self.next_id;
        self.next_id += 1;
        
        let credential = Credential {
            id,
            origin: origin.to_string(),
            username: username.to_string(),
            password_encrypted: encrypted,
            created: now,
            last_used: now,
        };
        
        self.credentials.insert(id, credential);
        id
    }
    
    /// Find credentials for an origin
    pub fn find_for_origin(&self, origin: &str) -> Vec<&Credential> {
        self.credentials.values()
            .filter(|c| c.origin == origin)
            .collect()
    }
    
    /// Find specific credential
    pub fn find_credential(&self, origin: &str, username: &str) -> Option<&Credential> {
        self.credentials.values()
            .find(|c| c.origin == origin && c.username == username)
    }
    
    /// Get decrypted password
    pub fn get_password(&self, id: u64) -> Option<String> {
        self.credentials.get(&id)
            .map(|c| self.deobfuscate(&c.password_encrypted))
    }
    
    /// Delete a credential
    pub fn delete(&mut self, id: u64) -> bool {
        self.credentials.remove(&id).is_some()
    }
    
    /// Delete all credentials for origin
    pub fn delete_for_origin(&mut self, origin: &str) {
        self.credentials.retain(|_, c| c.origin != origin);
    }
    
    /// Get all stored origins
    pub fn get_origins(&self) -> Vec<&str> {
        let mut origins: Vec<_> = self.credentials.values()
            .map(|c| c.origin.as_str())
            .collect();
        origins.sort();
        origins.dedup();
        origins
    }
    
    /// Update last used time
    pub fn mark_used(&mut self, id: u64) {
        if let Some(cred) = self.credentials.get_mut(&id) {
            cred.last_used = Self::now();
        }
    }
    
    /// Simple obfuscation (XOR) - NOT SECURE, just for demo
    fn obfuscate(&self, password: &str) -> String {
        let bytes: Vec<u8> = password.bytes()
            .enumerate()
            .map(|(i, b)| b ^ self.obfuscation_key[i % 32])
            .collect();
        base64_encode(&bytes)
    }
    
    fn deobfuscate(&self, encrypted: &str) -> String {
        let bytes = base64_decode(encrypted);
        let plain: Vec<u8> = bytes.iter()
            .enumerate()
            .map(|(i, b)| b ^ self.obfuscation_key[i % 32])
            .collect();
        String::from_utf8_lossy(&plain).to_string()
    }
    
    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    
    /// Save to disk
    pub fn save(&self) {
        let Some(path) = &self.storage_path else { return };
        
        let mut data = String::new();
        for cred in self.credentials.values() {
            data.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                cred.id,
                cred.origin,
                cred.username,
                cred.password_encrypted,
                cred.created,
                cred.last_used
            ));
        }
        
        let _ = fs::write(path, data);
    }
    
    /// Load from disk
    pub fn load(&mut self) {
        let Some(path) = &self.storage_path else { return };
        
        let data = match fs::read_to_string(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        
        for line in data.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 6 {
                let id: u64 = parts[0].parse().unwrap_or(0);
                let credential = Credential {
                    id,
                    origin: parts[1].to_string(),
                    username: parts[2].to_string(),
                    password_encrypted: parts[3].to_string(),
                    created: parts[4].parse().unwrap_or(0),
                    last_used: parts[5].parse().unwrap_or(0),
                };
                self.credentials.insert(id, credential);
                self.next_id = self.next_id.max(id + 1);
            }
        }
    }
    
    pub fn len(&self) -> usize {
        self.credentials.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.credentials.is_empty()
    }
}

// Simple base64 encoding/decoding
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        
        result.push(CHARS[(b0 >> 2) & 0x3F] as char);
        result.push(CHARS[((b0 << 4) | (b1 >> 4)) & 0x3F] as char);
        
        if chunk.len() > 1 {
            result.push(CHARS[((b1 << 2) | (b2 >> 6)) & 0x3F] as char);
        } else {
            result.push('=');
        }
        
        if chunk.len() > 2 {
            result.push(CHARS[b2 & 0x3F] as char);
        } else {
            result.push('=');
        }
    }
    
    result
}

fn base64_decode(s: &str) -> Vec<u8> {
    const DECODE: [i8; 128] = [
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,62,-1,-1,-1,63,
        52,53,54,55,56,57,58,59,60,61,-1,-1,-1,-1,-1,-1,
        -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14,
        15,16,17,18,19,20,21,22,23,24,25,-1,-1,-1,-1,-1,
        -1,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,
        41,42,43,44,45,46,47,48,49,50,51,-1,-1,-1,-1,-1,
    ];
    
    let mut result = Vec::new();
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
    
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 { break; }
        
        let b0 = DECODE[chunk[0] as usize] as u8;
        let b1 = DECODE[chunk[1] as usize] as u8;
        result.push((b0 << 2) | (b1 >> 4));
        
        if chunk.len() > 2 {
            let b2 = DECODE[chunk[2] as usize] as u8;
            result.push((b1 << 4) | (b2 >> 2));
            
            if chunk.len() > 3 {
                let b3 = DECODE[chunk[3] as usize] as u8;
                result.push((b2 << 6) | b3);
            }
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_password_storage() {
        let mut mgr = PasswordManager::new();
        
        let id = mgr.save_credential("https://example.com", "user@test.com", "secret123");
        
        let creds = mgr.find_for_origin("https://example.com");
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].username, "user@test.com");
        
        let password = mgr.get_password(id).unwrap();
        assert_eq!(password, "secret123");
    }
    
    #[test]
    fn test_base64() {
        let original = "Hello, World!";
        let encoded = base64_encode(original.as_bytes());
        let decoded = base64_decode(&encoded);
        assert_eq!(String::from_utf8(decoded).unwrap(), original);
    }
}
