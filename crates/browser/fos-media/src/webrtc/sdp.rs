//! SDP (Session Description Protocol)
//!
//! SDP parsing and generation for WebRTC.

use std::collections::HashMap;

/// SDP Session Description
#[derive(Debug, Clone, Default)]
pub struct SessionDescription {
    pub version: u8,
    pub origin: Origin,
    pub session_name: String,
    pub timing: (u64, u64),
    pub media_descriptions: Vec<MediaDescription>,
    pub attributes: HashMap<String, String>,
}

/// SDP Origin
#[derive(Debug, Clone, Default)]
pub struct Origin { pub username: String, pub session_id: u64, pub session_version: u64, pub net_type: String, pub addr_type: String, pub address: String }

/// Media Description
#[derive(Debug, Clone)]
pub struct MediaDescription {
    pub media_type: String,
    pub port: u16,
    pub protocol: String,
    pub formats: Vec<String>,
    pub connection: Option<String>,
    pub attributes: HashMap<String, String>,
    pub rtpmap: HashMap<u8, RtpMap>,
    pub ice_ufrag: Option<String>,
    pub ice_pwd: Option<String>,
    pub fingerprint: Option<String>,
    pub candidates: Vec<String>,
}

/// RTP Map
#[derive(Debug, Clone)]
pub struct RtpMap { pub payload_type: u8, pub encoding_name: String, pub clock_rate: u32, pub channels: Option<u8> }

impl SessionDescription {
    /// Parse SDP string
    pub fn parse(sdp: &str) -> Option<Self> {
        let mut desc = Self::default();
        let mut current_media: Option<MediaDescription> = None;
        
        for line in sdp.lines() {
            let line = line.trim();
            if line.len() < 2 || line.chars().nth(1) != Some('=') { continue; }
            
            let (key, value) = (&line[0..1], &line[2..]);
            
            match key {
                "v" => desc.version = value.parse().unwrap_or(0),
                "o" => {
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() >= 6 {
                        desc.origin = Origin {
                            username: parts[0].into(), session_id: parts[1].parse().unwrap_or(0),
                            session_version: parts[2].parse().unwrap_or(0), net_type: parts[3].into(),
                            addr_type: parts[4].into(), address: parts[5].into(),
                        };
                    }
                }
                "s" => desc.session_name = value.into(),
                "t" => {
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() >= 2 {
                        desc.timing = (parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0));
                    }
                }
                "m" => {
                    if let Some(media) = current_media.take() { desc.media_descriptions.push(media); }
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() >= 4 {
                        current_media = Some(MediaDescription {
                            media_type: parts[0].into(), port: parts[1].parse().unwrap_or(0),
                            protocol: parts[2].into(), formats: parts[3..].iter().map(|s| s.to_string()).collect(),
                            connection: None, attributes: HashMap::new(), rtpmap: HashMap::new(),
                            ice_ufrag: None, ice_pwd: None, fingerprint: None, candidates: Vec::new(),
                        });
                    }
                }
                "c" => { if let Some(ref mut m) = current_media { m.connection = Some(value.into()); } }
                "a" => {
                    let (name, val) = value.split_once(':').unwrap_or((value, ""));
                    if let Some(ref mut m) = current_media {
                        match name {
                            "rtpmap" => {
                                if let Some((pt_str, rest)) = val.split_once(' ') {
                                    let pt: u8 = pt_str.parse().unwrap_or(0);
                                    let parts: Vec<&str> = rest.split('/').collect();
                                    if parts.len() >= 2 {
                                        m.rtpmap.insert(pt, RtpMap {
                                            payload_type: pt, encoding_name: parts[0].into(),
                                            clock_rate: parts[1].parse().unwrap_or(0),
                                            channels: parts.get(2).and_then(|s| s.parse().ok()),
                                        });
                                    }
                                }
                            }
                            "ice-ufrag" => m.ice_ufrag = Some(val.into()),
                            "ice-pwd" => m.ice_pwd = Some(val.into()),
                            "fingerprint" => m.fingerprint = Some(val.into()),
                            "candidate" => m.candidates.push(val.into()),
                            _ => { m.attributes.insert(name.into(), val.into()); }
                        }
                    } else {
                        desc.attributes.insert(name.into(), val.into());
                    }
                }
                _ => {}
            }
        }
        
        if let Some(media) = current_media { desc.media_descriptions.push(media); }
        Some(desc)
    }
    
    /// Generate SDP string
    pub fn generate(&self) -> String {
        let mut sdp = String::new();
        sdp.push_str(&format!("v={}\r\n", self.version));
        sdp.push_str(&format!("o={} {} {} {} {} {}\r\n", self.origin.username, self.origin.session_id, self.origin.session_version, self.origin.net_type, self.origin.addr_type, self.origin.address));
        sdp.push_str(&format!("s={}\r\n", self.session_name));
        sdp.push_str(&format!("t={} {}\r\n", self.timing.0, self.timing.1));
        
        for (k, v) in &self.attributes { sdp.push_str(&format!("a={}:{}\r\n", k, v)); }
        
        for m in &self.media_descriptions {
            sdp.push_str(&format!("m={} {} {} {}\r\n", m.media_type, m.port, m.protocol, m.formats.join(" ")));
            if let Some(ref c) = m.connection { sdp.push_str(&format!("c={}\r\n", c)); }
            for (pt, rtp) in &m.rtpmap {
                let channels = rtp.channels.map(|c| format!("/{}", c)).unwrap_or_default();
                sdp.push_str(&format!("a=rtpmap:{} {}/{}{}\r\n", pt, rtp.encoding_name, rtp.clock_rate, channels));
            }
            if let Some(ref u) = m.ice_ufrag { sdp.push_str(&format!("a=ice-ufrag:{}\r\n", u)); }
            if let Some(ref p) = m.ice_pwd { sdp.push_str(&format!("a=ice-pwd:{}\r\n", p)); }
            if let Some(ref f) = m.fingerprint { sdp.push_str(&format!("a=fingerprint:{}\r\n", f)); }
            for c in &m.candidates { sdp.push_str(&format!("a=candidate:{}\r\n", c)); }
        }
        sdp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse() {
        let sdp = "v=0\r\no=- 123 1 IN IP4 0.0.0.0\r\ns=-\r\nt=0 0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=rtpmap:111 opus/48000/2\r\n";
        let desc = SessionDescription::parse(sdp).unwrap();
        assert_eq!(desc.media_descriptions.len(), 1);
    }
}
