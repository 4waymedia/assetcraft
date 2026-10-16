//! A preset's shape: the choices the Start screen exposes on top of a pacing profile (S2b, mock-up
//! `docs/design/start-storyboard-v2.dc.html`): title cards, tighten into drops, end card, the
//! transition between scenes, text overlays, and each style's shot-type mix.
//!
//! [`Shape`] holds overrides (`None` = the style's own choice); [`Pacing::shape`] gives the style's
//! defaults and [`Shape::resolve`] merges them. The generator reads only the resolved values.

use serde::{Deserialize, Serialize};

use crate::generate::Pacing;
use crate::model::{ShotType, Transition};

/// Where title cards go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Titles {
    /// A title at the start of every scene.
    #[serde(rename = "every")]
    EveryScene,
    /// The opening title, then a title only on scenes with a drop (or a chorus).
    #[serde(rename = "key")]
    KeyMoments,
    /// The opening title only.
    #[serde(rename = "opening")]
    OpeningOnly,
    /// No title cards (the end card has its own switch).
    #[serde(rename = "none")]
    None,
}

impl Titles {
    /// Every option, in menu order.
    pub const ALL: [Titles; 4] = [Titles::EveryScene, Titles::KeyMoments, Titles::OpeningOnly, Titles::None];

    /// Id used in commands.
    pub fn id(self) -> &'static str {
        match self {
            Titles::EveryScene => "every",
            Titles::KeyMoments => "key",
            Titles::OpeningOnly => "opening",
            Titles::None => "none",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            Titles::EveryScene => "Every scene",
            Titles::KeyMoments => "Key moments",
            Titles::OpeningOnly => "Opening only",
            Titles::None => "None",
        }
    }

    /// Parse an id.
    pub fn from_id(id: &str) -> Option<Titles> {
        Titles::ALL.into_iter().find(|t| t.id() == id)
    }
}

/// How one scene hands over to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SceneTransition {
    /// Straight cut.
    Cut,
    /// Cross fade.
    Crossfade,
    /// Fade to black and back.
    Dip,
    /// White flash.
    Flash,
}

impl SceneTransition {
    /// Every option, in menu order.
    pub const ALL: [SceneTransition; 4] =
        [SceneTransition::Cut, SceneTransition::Crossfade, SceneTransition::Dip, SceneTransition::Flash];

    /// Id used in commands.
    pub fn id(self) -> &'static str {
        match self {
            SceneTransition::Cut => "cut",
            SceneTransition::Crossfade => "crossfade",
            SceneTransition::Dip => "dip",
            SceneTransition::Flash => "flash",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            SceneTransition::Cut => "Cut",
            SceneTransition::Crossfade => "Crossfade",
            SceneTransition::Dip => "Dip",
            SceneTransition::Flash => "Flash",
        }
    }

    /// Parse an id.
    pub fn from_id(id: &str) -> Option<SceneTransition> {
        SceneTransition::ALL.into_iter().find(|t| t.id() == id)
    }

    /// `(out of the last clip, into the first clip)` at a scene boundary.
    pub fn transitions(self) -> (Transition, Transition) {
        match self {
            SceneTransition::Cut => (Transition::Cut, Transition::Cut),
            SceneTransition::Crossfade => (Transition::Cut, Transition::Fade),
            SceneTransition::Dip => (Transition::FadeOut, Transition::FadeIn),
            SceneTransition::Flash => (Transition::Cut, Transition::Flash),
        }
    }
}

/// Overrides of a style's shape; `None` keeps the style's choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shape {
    /// Title cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub titles: Option<Titles>,
    /// Shots tighten toward each drop (and through the track).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tighten: Option<bool>,
    /// An end title card over the final bars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_card: Option<bool>,
    /// Transition between scenes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<SceneTransition>,
    /// Scene titles (after the opening) as text over a shot instead of cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlays: Option<bool>,
}

/// A style's shape with every choice made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedShape {
    /// Title cards.
    pub titles: Titles,
    /// Tighten into drops.
    pub tighten: bool,
    /// End title card.
    pub end_card: bool,
    /// Transition between scenes.
    pub transition: SceneTransition,
    /// Scene titles as overlays.
    pub overlays: bool,
}

impl Shape {
    /// The overrides on top of `pacing`'s defaults.
    pub fn resolve(&self, pacing: Pacing) -> ResolvedShape {
        let d = pacing.shape();
        ResolvedShape {
            titles: self.titles.unwrap_or(d.titles),
            tighten: self.tighten.unwrap_or(d.tighten),
            end_card: self.end_card.unwrap_or(d.end_card),
            transition: self.transition.unwrap_or(d.transition),
            overlays: self.overlays.unwrap_or(d.overlays),
        }
    }

    /// Whether nothing is overridden.
    pub fn is_default(&self) -> bool {
        *self == Shape::default()
    }
}

/// Relative weights of Establishing, Wide, Medium, Close-up and Insert shots for a style
/// (title cards are placed separately).
pub(crate) fn shot_weights(pacing: Pacing) -> [(ShotType, u32); 5] {
    use ShotType::{Cu, Est, Ins, Ms, Ws};
    let w = match pacing {
        Pacing::Trailer => [1, 2, 3, 3, 1],
        Pacing::Energetic => [0, 1, 3, 3, 2],
        Pacing::Cinematic => [3, 4, 2, 1, 1],
        Pacing::Calm => [2, 3, 2, 1, 2],
        Pacing::Family => [0, 1, 3, 4, 1],
        Pacing::Hero => [1, 2, 2, 4, 1],
    };
    [(Est, w[0]), (Ws, w[1]), (Ms, w[2]), (Cu, w[3]), (Ins, w[4])]
}

/// A deterministic shot type: the scene's first shot sets the place (Establishing on the opening
/// or a quiet scene, else Wide); a Hero cut on a drop lands on a Close-up; the rest follow the
/// style's mix, picked by a hash of the position so the same input always gives the same board.
pub(crate) fn pick_shot(pacing: Pacing, scene: usize, k: usize, energy: f32, on_drop: bool) -> ShotType {
    let weights = shot_weights(pacing);
    if on_drop && pacing == Pacing::Hero {
        return ShotType::Cu;
    }
    if k == 0 {
        let has_est = weights[0].1 > 0;
        return if has_est && (scene == 0 || energy < 0.5) { ShotType::Est } else { ShotType::Ws };
    }
    let total: u32 = weights.iter().map(|(_, w)| w).sum();
    // splitmix64 of the position.
    let mut x = (scene as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (k as u64).wrapping_add(0xD1B5_4A32_D192_ED03);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    let mut r = (x % u64::from(total.max(1))) as u32;
    for (t, w) in weights {
        if r < w {
            return t;
        }
        r -= w;
    }
    ShotType::Ms
}
