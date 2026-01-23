//! HLS (HTTP Live Streaming)
//!
//! M3U8 manifest parsing and segment fetching.

use super::{Manifest, Variant, Segment, StreamingResult, StreamingError};
use std::time::Duration;

/// HLS Manifest (M3U8)
#[derive(Debug, Clone)]
pub struct HlsManifest {
    pub version: u8,
    pub target_duration: Duration,
    pub media_sequence: u64,
    pub is_live: bool,
    pub variants: Vec<HlsVariant>,
    pub segments: Vec<HlsSegment>,
}

/// HLS Variant stream
#[derive(Debug, Clone)]
pub struct HlsVariant { pub bandwidth: u64, pub resolution: Option<(u32, u32)>, pub codecs: String, pub url: String }

/// HLS Segment
#[derive(Debug, Clone)]
pub struct HlsSegment { pub url: String, pub duration: Duration, pub sequence: u64, pub discontinuity: bool, pub byte_range: Option<(u64, u64)> }

impl HlsManifest {
    /// Parse M3U8 manifest
    pub fn parse(content: &str, base_url: &str) -> StreamingResult<Self> {
        let mut manifest = Self { version: 3, target_duration: Duration::from_secs(6), media_sequence: 0, is_live: true, variants: Vec::new(), segments: Vec::new() };
        
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() || !lines[0].starts_with("#EXTM3U") {
            return Err(StreamingError::Parse("Not a valid M3U8".into()));
        }
        
        let mut i = 1;
        let mut current_duration = 0.0f64;
        let mut sequence = 0u64;
        
        while i < lines.len() {
            let line = lines[i].trim();
            
            if line.starts_with("#EXT-X-VERSION:") {
                manifest.version = line[15..].parse().unwrap_or(3);
            } else if line.starts_with("#EXT-X-TARGETDURATION:") {
                let secs: u64 = line[22..].parse().unwrap_or(6);
                manifest.target_duration = Duration::from_secs(secs);
            } else if line.starts_with("#EXT-X-MEDIA-SEQUENCE:") {
                manifest.media_sequence = line[22..].parse().unwrap_or(0);
                sequence = manifest.media_sequence;
            } else if line.starts_with("#EXT-X-ENDLIST") {
                manifest.is_live = false;
            } else if line.starts_with("#EXT-X-STREAM-INF:") {
                // Master playlist variant
                let attrs = line[18..].to_string();
                let bandwidth = Self::parse_attr(&attrs, "BANDWIDTH").and_then(|s| s.parse().ok()).unwrap_or(0);
                let resolution = Self::parse_attr(&attrs, "RESOLUTION").and_then(|s| {
                    let parts: Vec<&str> = s.split('x').collect();
                    if parts.len() == 2 { Some((parts[0].parse().ok()?, parts[1].parse().ok()?)) } else { None }
                });
                let codecs = Self::parse_attr(&attrs, "CODECS").unwrap_or_default().replace('"', "");
                
                i += 1;
                if i < lines.len() && !lines[i].starts_with('#') {
                    let url = Self::resolve_url(base_url, lines[i].trim());
                    manifest.variants.push(HlsVariant { bandwidth, resolution, codecs, url });
                }
            } else if line.starts_with("#EXTINF:") {
                let dur_str = line[8..].split(',').next().unwrap_or("0");
                current_duration = dur_str.parse().unwrap_or(0.0);
                
                i += 1;
                if i < lines.len() && !lines[i].starts_with('#') {
                    let url = Self::resolve_url(base_url, lines[i].trim());
                    manifest.segments.push(HlsSegment {
                        url, duration: Duration::from_secs_f64(current_duration),
                        sequence, discontinuity: false, byte_range: None,
                    });
                    sequence += 1;
                }
            }
            i += 1;
        }
        
        Ok(manifest)
    }
    
    fn parse_attr(attrs: &str, name: &str) -> Option<String> {
        let prefix = format!("{}=", name);
        attrs.split(',').find(|a| a.trim().starts_with(&prefix)).map(|a| {
            a.trim()[prefix.len()..].trim_matches('"').to_string()
        })
    }
    
    fn resolve_url(base: &str, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") { path.to_string() }
        else if path.starts_with('/') { format!("{}{}", base.split('/').take(3).collect::<Vec<_>>().join("/"), path) }
        else { format!("{}/{}", base.rsplit_once('/').map(|(b, _)| b).unwrap_or(base), path) }
    }
    
    pub fn to_manifest(&self) -> Manifest {
        Manifest {
            duration: if self.is_live { None } else { Some(self.segments.iter().map(|s| s.duration).sum()) },
            is_live: self.is_live, variants: self.variants.iter().map(|v| Variant {
                bandwidth: v.bandwidth, width: v.resolution.map(|r| r.0), height: v.resolution.map(|r| r.1),
                codecs: v.codecs.clone(), url: v.url.clone(),
            }).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse() {
        let m3u8 = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\nseg0.ts\n#EXT-X-ENDLIST";
        let manifest = HlsManifest::parse(m3u8, "http://example.com/").unwrap();
        assert_eq!(manifest.segments.len(), 1);
        assert!(!manifest.is_live);
    }
}
