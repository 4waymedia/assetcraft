//! The storyboard document.
//!
//! Field names follow the design handoff's data model (`Storyboard`, `Clip`, `Scene`, `Track`,
//! `Story`, `TextClip`), serialized in camelCase. Differences, on purpose:
//! - a clip's position is its index in [`Storyboard::clips`], not a stored `index` that could
//!   disagree with it;
//! - durations are [`Ticks`], and a clip's beat count is derived from the track's [`Bpm`];
//! - a scene's title is the text of its first clip with text (its title card's description, or
//!   the overlay of a title switched to an Overlay), not a second copy of the text.

use serde::{Deserialize, Serialize};

use crate::text::{TextKind, TextStyle};
use crate::time::{Bpm, Ticks};

/// Stable clip id, unique within a storyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClipId(pub u32);

/// Stable scene id, unique within a storyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SceneId(pub u32);

/// An asset reference: the file's path as the Organizer shows it (UTF-8, rooted). Later stages
/// may add a catalog id; the path stays the readable fallback.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(pub String);

/// Shot type, with the design's codes and colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShotType {
    /// Establishing.
    #[serde(rename = "EST")]
    Est,
    /// Wide.
    #[serde(rename = "WS")]
    Ws,
    /// Medium.
    #[serde(rename = "MS")]
    Ms,
    /// Close-up.
    #[serde(rename = "CU")]
    Cu,
    /// Insert.
    #[serde(rename = "INS")]
    Ins,
    /// Title card: a text clip opening a scene or ending the Cut.
    #[serde(rename = "TTL")]
    Ttl,
}

impl ShotType {
    /// Every shot type, in the order the Add buttons show them.
    pub const ALL: [ShotType; 6] =
        [ShotType::Est, ShotType::Ws, ShotType::Ms, ShotType::Cu, ShotType::Ins, ShotType::Ttl];

    /// Short code shown on tiles (`EST`, `WS`, …).
    pub fn code(self) -> &'static str {
        match self {
            ShotType::Est => "EST",
            ShotType::Ws => "WS",
            ShotType::Ms => "MS",
            ShotType::Cu => "CU",
            ShotType::Ins => "INS",
            ShotType::Ttl => "TTL",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            ShotType::Est => "Establishing",
            ShotType::Ws => "Wide",
            ShotType::Ms => "Medium",
            ShotType::Cu => "Close-up",
            ShotType::Ins => "Insert",
            ShotType::Ttl => "Title card",
        }
    }

    /// Tile fill as `0xRRGGBB` (`None` = transparent, the Insert style).
    pub fn fill_rgb(self) -> Option<u32> {
        match self {
            ShotType::Est => Some(0x0f1217),
            ShotType::Ws => Some(0x2b313b),
            ShotType::Ms => Some(0x5f6673),
            ShotType::Cu => Some(0xe9ebee),
            ShotType::Ins => None,
            ShotType::Ttl => Some(0xe5243b),
        }
    }

    /// Whether Quick fill places shots in a clip of this type (title cards are skipped: their
    /// one optional asset is a background, set by dropping on the card).
    pub fn accepts_assets(self) -> bool {
        self != ShotType::Ttl
    }

    /// Whether an asset can be dropped on a clip of this type: a shot, or a title card's
    /// background (Paul, 2026-10-17, finding B5: "a title over a video").
    pub fn accepts_drop(self) -> bool {
        true
    }
}

/// Transition into or out of a clip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Transition {
    /// Straight cut.
    #[default]
    Cut,
    /// Cross fade.
    Fade,
    /// Fade in from black.
    #[serde(rename = "Fade in")]
    FadeIn,
    /// Fade out to black.
    #[serde(rename = "Fade out")]
    FadeOut,
    /// White flash.
    Flash,
    /// Smash cut.
    #[serde(rename = "Smash cut")]
    SmashCut,
    /// Whip pan.
    Whip,
}

/// One unit of the storyboard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Clip {
    /// Stable id.
    pub id: ClipId,
    /// Scene this clip belongs to.
    pub scene_id: SceneId,
    /// Shot type.
    pub shot_type: ShotType,
    /// What the viewer sees (for a title card: the title text).
    #[serde(default)]
    pub description: String,
    /// Time on the Cut.
    pub duration: Ticks,
    /// Suggested angles (AI-suggested, user-editable chips).
    #[serde(default)]
    pub suggested_angles: Vec<String>,
    /// Transition in.
    #[serde(default)]
    pub transition_in: Transition,
    /// Transition out.
    #[serde(default)]
    pub transition_out: Transition,
    /// Assigned assets; the first is the hero frame. Always empty for title cards.
    #[serde(default)]
    pub asset_ids: Vec<AssetId>,
    /// Start locked to a downbeat (◆ marker).
    #[serde(default)]
    pub sync_to_downbeat: bool,
    /// Free notes.
    #[serde(default)]
    pub notes: String,
    /// Text drawn over the shot (an Overlay; see [`crate::text`]). A title card's text is its
    /// description, so a title card never has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<String>,
    /// How the clip's text looks: theme, animation, overlay position.
    #[serde(default, skip_serializing_if = "TextStyle::is_default")]
    pub text_style: TextStyle,
}

impl Clip {
    /// A new clip with defaults.
    pub fn new(id: ClipId, scene_id: SceneId, shot_type: ShotType, duration: Ticks) -> Clip {
        Clip {
            id,
            scene_id,
            shot_type,
            description: String::new(),
            duration,
            suggested_angles: Vec::new(),
            transition_in: Transition::Cut,
            transition_out: Transition::Cut,
            asset_ids: Vec::new(),
            sync_to_downbeat: false,
            notes: String::new(),
            overlay: None,
            text_style: TextStyle::default(),
        }
    }

    /// How the clip shows text, if it has any: a title card is a Card, a shot with overlay text
    /// is an Overlay.
    pub fn text_kind(&self) -> Option<TextKind> {
        if self.shot_type == ShotType::Ttl {
            Some(TextKind::Card)
        } else if self.overlay.is_some() {
            Some(TextKind::Overlay)
        } else {
            None
        }
    }

    /// The clip's text, if it has any.
    pub fn text(&self) -> Option<&str> {
        match self.text_kind()? {
            TextKind::Card => Some(&self.description),
            TextKind::Overlay => self.overlay.as_deref(),
        }
    }

    /// A title card's background asset, if it has one.
    pub fn background(&self) -> Option<&AssetId> {
        if self.shot_type == ShotType::Ttl { self.asset_ids.first() } else { None }
    }

    /// Whether Quick fill should place an asset here.
    pub fn is_empty_shot(&self) -> bool {
        self.shot_type.accepts_assets() && self.asset_ids.is_empty()
    }
}

/// A run of clips starting with a title card (marked § in the Shot map).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scene {
    /// Stable id.
    pub id: SceneId,
    /// Song section this scene is aligned to (`Intro`, `Verse`, …), if any.
    #[serde(default)]
    pub song_section: Option<String>,
}

/// A named span of the track (Intro, Verse, Pre-chorus, Chorus, Outro).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SongSection {
    /// Name.
    pub name: String,
    /// Start on the track.
    pub start: Ticks,
    /// End on the track (after `start`).
    pub end: Ticks,
}

/// The music file a storyboard is timed to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Track {
    /// Audio file (an asset reference).
    pub id: AssetId,
    /// Display name.
    pub name: String,
    /// Tempo.
    pub bpm: Bpm,
    /// Length of the track.
    pub duration: Ticks,
    /// Detected beats, ascending (may be empty: then beats are `bpm` from zero).
    #[serde(default)]
    pub beat_grid: Vec<Ticks>,
    /// Song sections, ascending and non-overlapping.
    #[serde(default)]
    pub song_sections: Vec<SongSection>,
    /// How the audio file plays on the Cut when it was fitted to a length (Fit to length,
    /// `docs/track-analysis.md` §6.5); `None` plays the file from its start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<TrackEdit>,
}

/// An audio edit: which parts of the file play, in order, and how they join and end. Frames are
/// at `sampleRate`, the rate the cue map was measured at; playback rescales them to the decoded
/// file's rate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackEdit {
    /// Rate the frames are counted at.
    pub sample_rate: u32,
    /// Source ranges `[start, end)` played back to back.
    pub segments: Vec<[u64; 2]>,
    /// Equal-power crossfade centred on each splice, in frames (0 = butt splice).
    #[serde(default)]
    pub crossfade_frames: u64,
    /// Fade to silence `[start, frames]` on the output timeline, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_out: Option<[u64; 2]>,
    /// Length of the edited track in frames.
    pub length_frames: u64,
}

/// Named tag grouping assets for one storyboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Story {
    /// Name (the tag).
    pub name: String,
    /// Tagged assets.
    #[serde(default)]
    pub asset_ids: Vec<AssetId>,
}

/// A text overlay on the T1 track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextClip {
    /// Start on the Cut.
    pub start: Ticks,
    /// Length.
    pub duration: Ticks,
    /// Text.
    pub text: String,
    /// Placement label, e.g. `Lower third · left`, `Center`.
    #[serde(default)]
    pub placement: String,
}

/// The storyboard document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Storyboard {
    /// Document format version ([`crate::FORMAT_VERSION`]).
    pub version: u32,
    /// Name.
    pub name: String,
    /// Format (e.g. `Movie trailer`), if started from a preset.
    #[serde(default)]
    pub format: Option<String>,
    /// Preset within the format (e.g. `Prebuilt trailer`).
    #[serde(default)]
    pub preset: Option<String>,
    /// The track the storyboard is timed to.
    #[serde(default)]
    pub track: Option<Track>,
    /// The Story whose tagged assets feed this storyboard.
    #[serde(default)]
    pub story: Option<Story>,
    /// Scenes, in order.
    #[serde(default)]
    pub scenes: Vec<Scene>,
    /// Clips, in Cut order.
    #[serde(default)]
    pub clips: Vec<Clip>,
    /// Text overlays.
    #[serde(default)]
    pub text_clips: Vec<TextClip>,
}

impl Storyboard {
    /// An empty storyboard.
    pub fn new(name: impl Into<String>) -> Storyboard {
        Storyboard {
            version: crate::FORMAT_VERSION,
            name: name.into(),
            format: None,
            preset: None,
            track: None,
            story: None,
            scenes: Vec::new(),
            clips: Vec::new(),
            text_clips: Vec::new(),
        }
    }

    /// Total length of the Cut (saturating).
    pub fn total(&self) -> Ticks {
        Ticks(self.clips.iter().fold(0u64, |acc, c| acc.saturating_add(c.duration.0)))
    }

    /// Position of a clip in Cut order.
    pub fn index_of(&self, id: ClipId) -> Option<usize> {
        self.clips.iter().position(|c| c.id == id)
    }

    /// A clip by id.
    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }

    /// Start time of each clip on the Cut, in Cut order (saturating).
    pub fn starts(&self) -> Vec<Ticks> {
        let mut at = 0u64;
        self.clips
            .iter()
            .map(|c| {
                let s = Ticks(at);
                at = at.saturating_add(c.duration.0);
                s
            })
            .collect()
    }

    /// The clip playing at `t` (the Viewer during "Watch your Cut").
    pub fn clip_at(&self, t: Ticks) -> Option<&Clip> {
        let mut at = 0u64;
        for c in &self.clips {
            let end = at.saturating_add(c.duration.0);
            if t.0 >= at && t.0 < end {
                return Some(c);
            }
            at = end;
        }
        None
    }

    /// A scene's title: the text of its first clip with text (a title card, or a shot whose
    /// title was switched to an overlay).
    pub fn scene_title(&self, scene: SceneId) -> Option<&str> {
        self.clips.iter().filter(|c| c.scene_id == scene).find_map(Clip::text)
    }

    /// Number of distinct assets assigned (status bar: "n assets assigned").
    pub fn assets_assigned(&self) -> usize {
        let mut all: Vec<&AssetId> = self.clips.iter().flat_map(|c| c.asset_ids.iter()).collect();
        all.sort();
        all.dedup();
        all.len()
    }

    /// `(filled, fillable)`: clips with an asset vs clips that accept assets (Quick fill progress).
    pub fn fill_progress(&self) -> (usize, usize) {
        let fillable = self.clips.iter().filter(|c| c.shot_type.accepts_assets());
        let (mut filled, mut total) = (0, 0);
        for c in fillable {
            total += 1;
            if !c.asset_ids.is_empty() {
                filled += 1;
            }
        }
        (filled, total)
    }

    /// The next unused clip id.
    pub fn next_clip_id(&self) -> Option<ClipId> {
        match self.clips.iter().map(|c| c.id.0).max() {
            None => Some(ClipId(1)),
            Some(m) => m.checked_add(1).map(ClipId),
        }
    }
}
