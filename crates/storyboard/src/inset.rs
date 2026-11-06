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

    /// The scenes that can carry an inset: those with at least one clip that isn't a title card.
    pub fn inset_scenes(&self) -> impl Iterator<Item = &crate::Scene> {
        self.scenes.iter().filter(|s| self.inset_span(s.id).next().is_some())
    }
}
