//! Section insets (SR2, `docs/storyboard-plan.md` § Sections): a second picture lane per
//! section — the user's talking video, b-roll, an interview, a graphic — drawn in a corner over
//! the section's clips. Paul (2026-11-06): "the storyboard should still display as normal, with
//! the inset covering the spread between each title… That is where the user can insert b-roll,
//! interview, or other video."
//!
//! An inset belongs to a [`Scene`](crate::Scene) and spans that section's clips after its title
//! card (a title card and the credits never carry one). A slot may be empty (`asset: None`) so a
//! template can leave the slots to fill later. A video inset's length is the section's length —
//! the section is resized to the recording when it lands (engine).

use serde::{Deserialize, Serialize};

use crate::model::{AssetId, ShotType, Storyboard};

/// What the inset shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsetKind {
    /// A video: the host, b-roll, an interview. Its audio is mixed in when `mix_audio` is on.
    #[default]
    Video,
    /// A still: a diagram, a part, a label.
    Graphic,
}

/// The corner the inset sits in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    /// Top left.
    TopLeft,
    /// Top right.
    TopRight,
    /// Bottom left.
    BottomLeft,
    /// Bottom right (the default: out of the way of lower-third overlays on the left).
    #[default]
    BottomRight,
}

/// How big the inset is: a share of the frame's width.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsetSize {
    /// 22 % of the frame's width.
    Small,
    /// 30 %.
    #[default]
    Medium,
    /// 40 %.
    Large,
}

impl InsetSize {
    /// Width as a share of the frame's width.
    pub fn share(self) -> f32 {
        match self {
            InsetSize::Small => 0.22,
            InsetSize::Medium => 0.30,
            InsetSize::Large => 0.40,
        }
    }
}

impl Corner {
    /// Every corner, in chip order (TL · TR · BL · BR).
    pub const ALL: [Corner; 4] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight];

    /// The chip's letters.
    pub fn code(self) -> &'static str {
        match self {
            Corner::TopLeft => "TL",
            Corner::TopRight => "TR",
            Corner::BottomLeft => "BL",
            Corner::BottomRight => "BR",
        }
    }
}

/// A section's inset lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inset {
    /// Video or graphic.
    #[serde(default)]
    pub kind: InsetKind,
    /// The file, or `None` for an empty slot to fill later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetId>,
    /// Which corner.
    #[serde(default)]
    pub corner: Corner,
    /// How big.
    #[serde(default)]
    pub size: InsetSize,
    /// Mix the inset video's audio in (the ducking rule lowers the music only then).
    #[serde(default)]
    pub mix_audio: bool,
    /// The inset full-frame and the section's clips in the corner instead.
    #[serde(default)]
    pub swap: bool,
    /// The music's level under this section while the inset's audio plays, in dB
    /// ([`MUSIC_DB_RANGE`]; SR4 ducking). Only used with `mix_audio`.
    #[serde(default = "default_music_db", skip_serializing_if = "is_default_music_db")]
    pub music_db: i8,
    /// The inset's own sound, in dB ([`VOICE_DB_RANGE`]).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub voice_db: i8,
    /// What belongs in the slot, in the user's words ("B-roll: close on the hands"), shown on
    /// the INSET row; a planning note, not drawn in the picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The music under a voice, in dB, by default: low enough to talk over, still there.
pub const DEFAULT_MUSIC_DB: i8 = -12;
/// Music level range, in dB (−40 is as good as silent).
pub const MUSIC_DB_RANGE: (i8, i8) = (-40, 0);
/// Voice level range, in dB.
pub const VOICE_DB_RANGE: (i8, i8) = (-24, 12);

fn default_music_db() -> i8 {
    DEFAULT_MUSIC_DB
}
fn is_default_music_db(v: &i8) -> bool {
    *v == DEFAULT_MUSIC_DB
}
fn is_zero(v: &i8) -> bool {
    *v == 0
}

impl Inset {
    /// An empty slot: video, bottom right, medium, audio out.
    pub fn slot(kind: InsetKind) -> Inset {
        Inset {
            kind,
            asset: None,
            corner: Corner::default(),
            size: InsetSize::default(),
            mix_audio: false,
            swap: false,
            music_db: DEFAULT_MUSIC_DB,
            voice_db: 0,
            note: None,
        }
    }

    /// Nothing to show yet.
    pub fn is_empty(&self) -> bool {
        self.asset.is_none()
    }
}

impl Storyboard {
    /// The clips an inset of `scene` covers, in Cut order: the section's clips after its title
    /// cards (a title card never carries an inset).
    pub fn inset_span(&self, scene: crate::SceneId) -> impl Iterator<Item = &crate::Clip> {
        self.clips.iter().filter(move |c| c.scene_id == scene && c.shot_type != ShotType::Ttl)
    }

    /// Where `scene`'s inset sits in the Cut: from its first shot for as long as its shots run
    /// (title cards excluded); `None` without shots.
    pub fn inset_window(&self, scene: crate::SceneId) -> Option<(crate::Ticks, crate::Ticks)> {
        let starts = self.starts();
        let mut first: Option<crate::Ticks> = None;
        let mut length = crate::Ticks::ZERO;
        for (i, c) in self.clips.iter().enumerate() {
            if c.scene_id == scene && c.shot_type != ShotType::Ttl {
                if first.is_none() {
                    first = starts.get(i).copied();
                }
                length = length.checked_add(c.duration)?;
            }
        }
        first.map(|f| (f, length))
    }

    /// The scenes that can carry an inset: those with at least one clip that isn't a title card.
    pub fn inset_scenes(&self) -> impl Iterator<Item = &crate::Scene> {
        self.scenes.iter().filter(|s| self.inset_span(s.id).next().is_some())
    }
}
