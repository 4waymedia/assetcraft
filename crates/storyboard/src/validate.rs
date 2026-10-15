//! Loading a storyboard document as hostile input.
//!
//! [`Storyboard::from_json`] caps the byte size before parsing, then checks every count, length,
//! id and time against [`Limits`] and the model's invariants. A document that loads is safe for
//! the engine and UI to index without further checks.

use std::collections::HashSet;
use std::fmt;

use crate::model::{ShotType, Storyboard};
use crate::time::Ticks;

/// Current document format version.
pub const FORMAT_VERSION: u32 = 1;

/// Bounds for a loaded document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest document, in bytes.
    pub max_bytes: usize,
    /// Most clips (presets are 20–50; long boards stay well under this).
    pub max_clips: usize,
    /// Most scenes.
    pub max_scenes: usize,
    /// Most assets on one clip.
    pub max_assets_per_clip: usize,
    /// Most suggested angles on one clip.
    pub max_angles: usize,
    /// Most text overlays.
    pub max_text_clips: usize,
    /// Most beats in a beat grid.
    pub max_beats: usize,
    /// Most song sections.
    pub max_song_sections: usize,
    /// Most assets in a Story.
    pub max_story_assets: usize,
    /// Longest string, in bytes.
    pub max_string: usize,
    /// Longest clip, Cut or track.
    pub max_duration: Ticks,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_bytes: 16 << 20,
            max_clips: 5_000,
            max_scenes: 1_000,
            max_assets_per_clip: 256,
            max_angles: 32,
            max_text_clips: 5_000,
            max_beats: 200_000,
            max_song_sections: 256,
            max_story_assets: 100_000,
            max_string: 8 << 10,
            max_duration: Ticks(crate::time::TICKS_PER_SECOND * 6 * 3600),
        }
    }
}

/// Why a document was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardError {
    /// Larger than [`Limits::max_bytes`].
    TooLarge(usize),
    /// Not valid storyboard JSON.
    Parse(String),
    /// Written by a newer AssetCraft.
    NewerVersion(u32),
    /// A count, length or time is beyond [`Limits`].
    Limit(String),
    /// The model's invariants don't hold (duplicate ids, unknown scene, …).
    Invalid(String),
}

impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoardError::TooLarge(n) => write!(f, "storyboard file is too large ({n} bytes)"),
            BoardError::Parse(e) => write!(f, "not a storyboard file: {e}"),
            BoardError::NewerVersion(v) => {
                write!(f, "storyboard format {v} is newer than this AssetCraft ({FORMAT_VERSION})")
            }
            BoardError::Limit(e) => write!(f, "storyboard is too big: {e}"),
            BoardError::Invalid(e) => write!(f, "storyboard is damaged: {e}"),
        }
    }
}

impl std::error::Error for BoardError {}

impl Storyboard {
    /// Parse and validate a document with default [`Limits`].
    pub fn from_json(bytes: &[u8]) -> Result<Storyboard, BoardError> {
        Storyboard::from_json_with(bytes, &Limits::default())
    }

    /// Parse and validate a document.
    pub fn from_json_with(bytes: &[u8], limits: &Limits) -> Result<Storyboard, BoardError> {
        if bytes.len() > limits.max_bytes {
            return Err(BoardError::TooLarge(bytes.len()));
        }
        let board: Storyboard = serde_json::from_slice(bytes).map_err(|e| BoardError::Parse(e.to_string()))?;
        board.validate(limits)?;
        Ok(board)
    }

    /// Pretty JSON for saving.
    pub fn to_json(&self) -> Result<String, BoardError> {
        serde_json::to_string_pretty(self).map_err(|e| BoardError::Parse(e.to_string()))
    }

    /// Check counts, lengths, ids and times.
    pub fn validate(&self, l: &Limits) -> Result<(), BoardError> {
        if self.version > FORMAT_VERSION {
            return Err(BoardError::NewerVersion(self.version));
        }
        if self.version == 0 {
            return Err(BoardError::Invalid("version 0".into()));
        }
        count("clips", self.clips.len(), l.max_clips)?;
        count("scenes", self.scenes.len(), l.max_scenes)?;
        count("text clips", self.text_clips.len(), l.max_text_clips)?;
        text("name", &self.name, l)?;
        for s in [&self.format, &self.preset].into_iter().flatten() {
            text("format/preset", s, l)?;
        }

        let mut scene_ids = HashSet::new();
        for s in &self.scenes {
            if !scene_ids.insert(s.id) {
                return Err(BoardError::Invalid(format!("scene {} appears twice", s.id.0)));
            }
            if let Some(n) = &s.song_section {
                text("song section", n, l)?;
            }
        }

        let mut clip_ids = HashSet::new();
        let mut total = Ticks::ZERO;
        for c in &self.clips {
            if !clip_ids.insert(c.id) {
                return Err(BoardError::Invalid(format!("clip #{} appears twice", c.id.0)));
            }
            if !scene_ids.contains(&c.scene_id) {
                return Err(BoardError::Invalid(format!("clip #{} names unknown scene {}", c.id.0, c.scene_id.0)));
            }
            if c.shot_type == ShotType::Ttl && !c.asset_ids.is_empty() {
                return Err(BoardError::Invalid(format!("title card #{} has assets", c.id.0)));
            }
            if c.duration > l.max_duration {
                return Err(BoardError::Limit(format!("clip #{} is longer than the limit", c.id.0)));
            }
            total = total
                .checked_add(c.duration)
                .filter(|t| *t <= l.max_duration)
                .ok_or_else(|| BoardError::Limit("the Cut is longer than the limit".into()))?;
            count("assets on a clip", c.asset_ids.len(), l.max_assets_per_clip)?;
            count("suggested angles", c.suggested_angles.len(), l.max_angles)?;
            text("description", &c.description, l)?;
            text("notes", &c.notes, l)?;
            for a in &c.suggested_angles {
                text("angle", a, l)?;
            }
            for a in &c.asset_ids {
                text("asset", &a.0, l)?;
            }
        }

        for t in &self.text_clips {
            text("text", &t.text, l)?;
            text("placement", &t.placement, l)?;
            let end = t.start.checked_add(t.duration);
            if end.is_none_or(|e| e > l.max_duration) {
                return Err(BoardError::Limit("a text overlay ends beyond the limit".into()));
            }
        }

        if let Some(track) = &self.track {
            text("track", &track.id.0, l)?;
            text("track name", &track.name, l)?;
            if track.duration > l.max_duration {
                return Err(BoardError::Limit("the track is longer than the limit".into()));
            }
            count("beats", track.beat_grid.len(), l.max_beats)?;
            if track.beat_grid.windows(2).any(|w| w[0] >= w[1])
                || track.beat_grid.last().is_some_and(|b| *b > track.duration)
            {
                return Err(BoardError::Invalid("beat grid is not ascending within the track".into()));
            }
            count("song sections", track.song_sections.len(), l.max_song_sections)?;
            let mut prev_end = Ticks::ZERO;
            for s in &track.song_sections {
                text("song section", &s.name, l)?;
                if s.start >= s.end || s.start < prev_end || s.end > track.duration {
                    return Err(BoardError::Invalid(format!("song section {:?} is out of order", s.name)));
                }
                prev_end = s.end;
            }
        }

        if let Some(story) = &self.story {
            text("story", &story.name, l)?;
            count("story assets", story.asset_ids.len(), l.max_story_assets)?;
            for a in &story.asset_ids {
                text("asset", &a.0, l)?;
            }
        }
        Ok(())
    }
}

fn count(what: &str, n: usize, max: usize) -> Result<(), BoardError> {
    if n > max { Err(BoardError::Limit(format!("{n} {what} (limit {max})"))) } else { Ok(()) }
}

fn text(what: &str, s: &str, l: &Limits) -> Result<(), BoardError> {
    if s.len() > l.max_string {
        Err(BoardError::Limit(format!("{what} is {} bytes (limit {})", s.len(), l.max_string)))
    } else {
        Ok(())
    }
}
