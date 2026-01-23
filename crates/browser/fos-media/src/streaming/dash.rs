//! DASH (Dynamic Adaptive Streaming over HTTP)
//!
//! MPD manifest parsing and segment handling.

use super::{Manifest, Variant, Segment, StreamingResult, StreamingError};
use std::time::Duration;

/// DASH Manifest (MPD)
#[derive(Debug, Clone)]
pub struct DashManifest {
    pub media_presentation_duration: Option<Duration>,
    pub min_buffer_time: Duration,
    pub is_live: bool,
    pub periods: Vec<Period>,
}

/// MPD Period
#[derive(Debug, Clone)]
pub struct Period { pub id: String, pub start: Duration, pub duration: Option<Duration>, pub adaptation_sets: Vec<AdaptationSet> }

/// Adaptation Set
#[derive(Debug, Clone)]
pub struct AdaptationSet {
    pub id: u32, pub content_type: String, pub mime_type: String, pub codecs: String,
    pub width: Option<u32>, pub height: Option<u32>, pub frame_rate: Option<f64>,
    pub representations: Vec<Representation>,
}

/// Representation (quality level)
#[derive(Debug, Clone)]
pub struct Representation {
    pub id: String, pub bandwidth: u64, pub width: Option<u32>, pub height: Option<u32>,
    pub codecs: Option<String>, pub segment_template: Option<SegmentTemplate>,
    pub segment_list: Option<SegmentList>, pub base_url: Option<String>,
}

/// Segment Template
#[derive(Debug, Clone)]
pub struct SegmentTemplate {
    pub initialization: String, pub media: String, pub timescale: u32,
    pub duration: u32, pub start_number: u64,
}

/// Segment List
#[derive(Debug, Clone)]
pub struct SegmentList { pub duration: u32, pub timescale: u32, pub segments: Vec<String> }

impl DashManifest {
    /// Parse MPD manifest (simplified XML parsing)
    pub fn parse(content: &str, _base_url: &str) -> StreamingResult<Self> {
        let mut manifest = Self { media_presentation_duration: None, min_buffer_time: Duration::from_secs(2), is_live: false, periods: Vec::new() };
        
        // Simple attribute extraction
        if content.contains("type=\"dynamic\"") { manifest.is_live = true; }
        
        if let Some(dur_str) = Self::extract_attr(content, "mediaPresentationDuration") {
            manifest.media_presentation_duration = Some(Self::parse_duration(&dur_str));
        }
        
        if let Some(buf_str) = Self::extract_attr(content, "minBufferTime") {
            manifest.min_buffer_time = Self::parse_duration(&buf_str);
        }
        
        // Extract periods (simplified)
        let mut period_id = 0;
        for period_content in content.split("<Period").skip(1) {
            let end = period_content.find("</Period>").unwrap_or(period_content.len());
            let period_xml = &period_content[..end];
            
            let mut period = Period { id: format!("period{}", period_id), start: Duration::ZERO, duration: None, adaptation_sets: Vec::new() };
            period_id += 1;
            
            // Extract adaptation sets
            for as_content in period_xml.split("<AdaptationSet").skip(1) {
                let as_end = as_content.find("</AdaptationSet>").unwrap_or(as_content.len());
                let as_xml = &as_content[..as_end];
                
                let content_type = Self::extract_attr(as_xml, "contentType").unwrap_or_else(|| {
                    if as_xml.contains("video") { "video".into() } else { "audio".into() }
                });
                let mime_type = Self::extract_attr(as_xml, "mimeType").unwrap_or_default();
                let codecs = Self::extract_attr(as_xml, "codecs").unwrap_or_default();
                
                let mut adaptation_set = AdaptationSet {
                    id: period.adaptation_sets.len() as u32, content_type, mime_type, codecs,
                    width: None, height: None, frame_rate: None, representations: Vec::new(),
                };
                
                // Extract representations
                for rep_content in as_xml.split("<Representation").skip(1) {
                    let rep_end = rep_content.find("/>").or_else(|| rep_content.find("</Representation>")).unwrap_or(rep_content.len());
                    let rep_xml = &rep_content[..rep_end];
                    
                    let id = Self::extract_attr(rep_xml, "id").unwrap_or_default();
                    let bandwidth: u64 = Self::extract_attr(rep_xml, "bandwidth").and_then(|s| s.parse().ok()).unwrap_or(0);
                    let width: Option<u32> = Self::extract_attr(rep_xml, "width").and_then(|s| s.parse().ok());
                    let height: Option<u32> = Self::extract_attr(rep_xml, "height").and_then(|s| s.parse().ok());
                    
                    adaptation_set.representations.push(Representation {
                        id, bandwidth, width, height, codecs: None,
                        segment_template: None, segment_list: None, base_url: None,
                    });
                }
                
                period.adaptation_sets.push(adaptation_set);
            }
            
            manifest.periods.push(period);
        }
        
        Ok(manifest)
    }
    
    fn extract_attr(xml: &str, name: &str) -> Option<String> {
        let pattern = format!("{}=\"", name);
        xml.find(&pattern).and_then(|start| {
            let value_start = start + pattern.len();
            xml[value_start..].find('"').map(|end| xml[value_start..value_start + end].to_string())
        })
    }
    
    fn parse_duration(iso: &str) -> Duration {
        // Parse ISO 8601 duration (PT1H2M3.4S)
        let s = iso.trim_start_matches("PT");
        let mut secs = 0.0f64;
        let mut num = String::new();
        for c in s.chars() {
            match c {
                'H' => { secs += num.parse::<f64>().unwrap_or(0.0) * 3600.0; num.clear(); }
                'M' => { secs += num.parse::<f64>().unwrap_or(0.0) * 60.0; num.clear(); }
                'S' => { secs += num.parse::<f64>().unwrap_or(0.0); num.clear(); }
                _ => num.push(c),
            }
        }
        Duration::from_secs_f64(secs)
    }
    
    pub fn to_manifest(&self) -> Manifest {
        let mut variants = Vec::new();
        for period in &self.periods {
            for as_ in &period.adaptation_sets {
                if as_.content_type == "video" {
                    for rep in &as_.representations {
                        variants.push(Variant {
                            bandwidth: rep.bandwidth, width: rep.width, height: rep.height,
                            codecs: rep.codecs.clone().unwrap_or_else(|| as_.codecs.clone()), url: String::new(),
                        });
                    }
                }
            }
        }
        Manifest { duration: self.media_presentation_duration, is_live: self.is_live, variants }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_duration() { assert_eq!(DashManifest::parse_duration("PT1H30M"), Duration::from_secs(5400)); }
}
