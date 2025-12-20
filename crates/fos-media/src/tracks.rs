//! Media Tracks
//!
//! TextTrack, AudioTrack, VideoTrack.

/// Text track kind
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextTrackKind {
    Subtitles,
    #[default]
    Captions,
    Descriptions,
    Chapters,
    Metadata,
}

/// Text track mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextTrackMode {
    #[default]
    Disabled,
    Hidden,
    Showing,
}

/// Text track
#[derive(Debug, Clone)]
pub struct TextTrack {
    pub id: String,
    pub kind: TextTrackKind,
    pub label: String,
    pub language: String,
    pub mode: TextTrackMode,
    pub cues: Vec<TextTrackCue>,
    pub active_cues: Vec<usize>,
}

/// Text track cue
#[derive(Debug, Clone)]
pub struct TextTrackCue {
    pub id: String,
    pub start_time: f64,
    pub end_time: f64,
    pub pause_on_exit: bool,
    pub text: String,
}

impl TextTrack {
    pub fn new(kind: TextTrackKind, label: &str, language: &str) -> Self {
        Self {
            id: String::new(),
            kind,
            label: label.to_string(),
            language: language.to_string(),
            mode: TextTrackMode::Disabled,
            cues: Vec::new(),
            active_cues: Vec::new(),
        }
    }
    
    pub fn add_cue(&mut self, cue: TextTrackCue) {
        self.cues.push(cue);
    }
    
    pub fn remove_cue(&mut self, id: &str) {
        self.cues.retain(|c| c.id != id);
    }
    
    pub fn update_active(&mut self, current_time: f64) {
        self.active_cues = self.cues.iter()
            .enumerate()
            .filter(|(_, c)| c.start_time <= current_time && c.end_time > current_time)
            .map(|(i, _)| i)
            .collect();
    }
}

/// Audio track
#[derive(Debug, Clone)]
pub struct AudioTrack {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub language: String,
    pub enabled: bool,
}

impl AudioTrack {
    pub fn new(id: &str, label: &str, language: &str) -> Self {
        Self {
            id: id.to_string(),
            kind: "main".to_string(),
            label: label.to_string(),
            language: language.to_string(),
            enabled: true,
        }
    }
}

/// Video track
#[derive(Debug, Clone)]
pub struct VideoTrack {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub language: String,
    pub selected: bool,
}

impl VideoTrack {
    pub fn new(id: &str, label: &str, language: &str) -> Self {
        Self {
            id: id.to_string(),
            kind: "main".to_string(),
            label: label.to_string(),
            language: language.to_string(),
            selected: true,
        }
    }
}

/// Track list
#[derive(Debug, Clone, Default)]
pub struct TextTrackList {
    pub tracks: Vec<TextTrack>,
}

impl TextTrackList {
    pub fn new() -> Self { Self::default() }
    pub fn length(&self) -> usize { self.tracks.len() }
    pub fn get(&self, index: usize) -> Option<&TextTrack> { self.tracks.get(index) }
    pub fn get_by_id(&self, id: &str) -> Option<&TextTrack> {
        self.tracks.iter().find(|t| t.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_text_track() {
        let mut track = TextTrack::new(TextTrackKind::Subtitles, "English", "en");
        track.add_cue(TextTrackCue {
            id: "1".into(),
            start_time: 0.0,
            end_time: 5.0,
            pause_on_exit: false,
            text: "Hello".into(),
        });
        
        track.update_active(2.5);
        assert_eq!(track.active_cues.len(), 1);
    }
}
