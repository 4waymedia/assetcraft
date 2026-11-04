//! Clip motion (SB-3D1, `docs/design/clip-motion-handoff.md`): how the camera moves over a
//! clip. A 3D model clip orbits (yaw · pitch · zoom); a still pans and zooms (Ken Burns). Both
//! use one [`Camera`], and the motion is defined over the clip's *duration*, never in seconds,
//! so a Re-cut keeps it right.
//!
//! `End = Start + amount`: the [`Motion`] stores the start camera and the relative amount, so a
//! preset reads the same on every clip. [`Motion::at`] gives the camera at a point `0..=1` of
//! the clip, eased. The UI only paints what it returns; the CLI and tests see the same frames.
//!
//! Angles are radians in the data (the viewer's `View`); the Inspector and the command show
//! degrees. Pan is a fraction of the frame (`0.12` = 12 %).

use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};
use std::fmt;

use crate::model::{Clip, ClipId, ShotType, Storyboard};

/// What kind of motion a clip's shot can have, from its hero asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotionKind {
    /// A 3D model: orbit camera (yaw, pitch, zoom).
    Orbit,
    /// A still: pan and zoom.
    PanZoom,
}

impl MotionKind {
    /// Display name, as the Inspector's MOTION header shows it.
    pub fn name(self) -> &'static str {
        match self {
            MotionKind::Orbit => "Orbit camera",
            MotionKind::PanZoom => "Pan and zoom",
        }
    }

    /// The kind for a hero asset path (`.glb`, `.gltf`, `.obj` orbit; anything else pans).
    pub fn of_path(path: &str) -> MotionKind {
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if matches!(ext.as_str(), "glb" | "gltf" | "obj") { MotionKind::Orbit } else { MotionKind::PanZoom }
    }

    /// The kind for a clip: its hero's, or `None` for a title card or an empty shot.
    pub fn of_clip(clip: &Clip) -> Option<MotionKind> {
        if clip.shot_type == ShotType::Ttl {
            return None;
        }
        clip.asset_ids.first().map(|a| MotionKind::of_path(&a.0))
    }
}

/// A named motion. The amounts are [`Motion::preset`]'s; editing any amount makes it `Custom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MotionPreset {
    /// A full turn (orbit only).
    Turntable,
    /// Zoom in.
    PushIn,
    /// Zoom out.
    PullOut,
    /// Orbit: turn left; still: pan left.
    PanLeft,
    /// Orbit: turn right; still: pan right.
    PanRight,
    /// Orbit: look from higher; still: pan up.
    Rise,
    /// Amounts set by hand.
    Custom,
}

impl MotionPreset {
    /// All presets, in chip order.
    pub const ALL: [MotionPreset; 7] = [
        MotionPreset::Turntable,
        MotionPreset::PushIn,
        MotionPreset::PullOut,
        MotionPreset::PanLeft,
        MotionPreset::PanRight,
        MotionPreset::Rise,
        MotionPreset::Custom,
    ];

    /// Id used in commands and documents.
    pub fn id(self) -> &'static str {
        match self {
            MotionPreset::Turntable => "turntable",
            MotionPreset::PushIn => "pushIn",
            MotionPreset::PullOut => "pullOut",
            MotionPreset::PanLeft => "panLeft",
            MotionPreset::PanRight => "panRight",
            MotionPreset::Rise => "rise",
            MotionPreset::Custom => "custom",
        }
    }

    /// Display name (the chip).
    pub fn name(self) -> &'static str {
        match self {
            MotionPreset::Turntable => "Turntable",
            MotionPreset::PushIn => "Push in",
            MotionPreset::PullOut => "Pull out",
            MotionPreset::PanLeft => "Pan left",
            MotionPreset::PanRight => "Pan right",
            MotionPreset::Rise => "Rise",
            MotionPreset::Custom => "Custom",
        }
    }

    /// Parse an id.
    pub fn from_id(id: &str) -> Option<MotionPreset> {
        MotionPreset::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Whether the preset applies to `kind` (Turntable is orbit only).
    pub fn allows(self, kind: MotionKind) -> bool {
        !(self == MotionPreset::Turntable && kind == MotionKind::PanZoom)
    }

    /// The preset's amount and ease for `kind` (`Custom`: no change, the identity amount).
    fn amount(self, kind: MotionKind) -> (Camera, Ease) {
        let a = Camera::NONE;
        match (self, kind) {
            (MotionPreset::Turntable, _) => (Camera { yaw: TAU, ..a }, Ease::Linear),
            (MotionPreset::PushIn, _) => (Camera { zoom: 1.6, ..a }, Ease::InOut),
            (MotionPreset::PullOut, _) => (Camera { zoom: 1.0 / 1.6, ..a }, Ease::InOut),
            (MotionPreset::PanLeft, MotionKind::Orbit) => (Camera { yaw: -PI / 6.0, ..a }, Ease::Out),
            (MotionPreset::PanRight, MotionKind::Orbit) => (Camera { yaw: PI / 6.0, ..a }, Ease::Out),
            (MotionPreset::Rise, MotionKind::Orbit) => (Camera { pitch: PI / 9.0, ..a }, Ease::InOut),
            (MotionPreset::PanLeft, MotionKind::PanZoom) => (Camera { pan: [-0.12, 0.0], ..a }, Ease::Out),
            (MotionPreset::PanRight, MotionKind::PanZoom) => (Camera { pan: [0.12, 0.0], ..a }, Ease::Out),
            (MotionPreset::Rise, MotionKind::PanZoom) => (Camera { pan: [0.0, -0.12], ..a }, Ease::InOut),
            (MotionPreset::Custom, _) => (a, Ease::InOut),
        }
    }
}

/// How the motion is timed over the clip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Ease {
    /// Constant speed.
    #[default]
    Linear,
    /// Slow start and end (smoothstep).
    InOut,
    /// Fast start, settling at the end.
    Out,
}

impl Ease {
    /// All eases, in control order.
    pub const ALL: [Ease; 3] = [Ease::Linear, Ease::InOut, Ease::Out];

    /// Id used in commands and documents.
    pub fn id(self) -> &'static str {
        match self {
            Ease::Linear => "linear",
            Ease::InOut => "inOut",
            Ease::Out => "out",
        }
    }

    /// Display name (the segment).
    pub fn name(self) -> &'static str {
        match self {
            Ease::Linear => "Linear",
            Ease::InOut => "Ease in-out",
            Ease::Out => "Ease out",
        }
    }

    /// Parse an id.
    pub fn from_id(id: &str) -> Option<Ease> {
        Ease::ALL.into_iter().find(|e| e.id() == id)
    }

    /// Progress `0..=1` for a point `t` of the clip (`t` outside is clamped).
    pub fn apply(self, t: f32) -> f32 {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        match self {
            Ease::Linear => t,
            Ease::InOut => t * t * (3.0 - 2.0 * t),
            Ease::Out => 1.0 - (1.0 - t) * (1.0 - t),
        }
    }
}

/// A camera: orbit angles and zoom for a model, pan and zoom for a still. As a *start* it is
/// where the clip begins; as an *amount* it is what is added over the clip (zoom multiplies).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Camera {
    /// Turn around the vertical axis, radians (orbit).
    pub yaw: f32,
    /// Elevation, radians; positive looks down from above (orbit).
    pub pitch: f32,
    /// Magnification; 1 fits the shot. As an amount: a factor.
    pub zoom: f32,
    /// Offset of the frame as a fraction of its size, +x right, +y down (still).
    pub pan: [f32; 2],
}

impl Default for Camera {
    fn default() -> Self {
        Camera::NONE
    }
}

impl Camera {
    /// No turn, no pan, zoom 1: the shot as framed. Also the amount that changes nothing.
    pub const NONE: Camera = Camera { yaw: 0.0, pitch: 0.0, zoom: 1.0, pan: [0.0, 0.0] };

    /// This camera moved by `amount` (angles and pan add, zoom multiplies).
    pub fn plus(self, amount: Camera) -> Camera {
        Camera {
            yaw: self.yaw + amount.yaw,
            pitch: self.pitch + amount.pitch,
            zoom: self.zoom * amount.zoom,
            pan: [self.pan[0] + amount.pan[0], self.pan[1] + amount.pan[1]],
        }
    }

    /// The camera a fraction `k` of the way from `self` towards `self.plus(amount)`.
    fn towards(self, amount: Camera, k: f32) -> Camera {
        Camera {
            yaw: self.yaw + amount.yaw * k,
            pitch: self.pitch + amount.pitch * k,
            zoom: self.zoom * amount.zoom.powf(k),
            pan: [self.pan[0] + amount.pan[0] * k, self.pan[1] + amount.pan[1] * k],
        }
    }

    fn finite(self) -> bool {
        self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.zoom.is_finite()
            && self.pan.iter().all(|v| v.is_finite())
    }

    /// Whether this is a usable *start* for `kind`: finite, pitch within ±80°, zoom in
    /// `0.25..=16` (orbit) or `1..=4` (still), pan within ±50 % (still).
    fn valid_start(self, kind: MotionKind) -> bool {
        let pitch_ok = self.pitch.abs() <= MAX_PITCH;
        match kind {
            MotionKind::Orbit => self.finite() && pitch_ok && (0.25..=16.0).contains(&self.zoom),
            MotionKind::PanZoom => {
                self.finite() && (1.0..=4.0).contains(&self.zoom) && self.pan.iter().all(|v| v.abs() <= 0.5)
            }
        }
    }

    /// Whether this is a usable *amount* for `kind` (the Inspector's slider ranges): turn
    /// within ±360°, rise within ±80°, zoom factor `0.5..=4` (orbit) or `1/3..=3` (still), pan
    /// within ±50 %.
    fn valid_amount(self, kind: MotionKind) -> bool {
        let angles_ok = self.yaw.abs() <= TAU && self.pitch.abs() <= MAX_PITCH;
        let pan_ok = self.pan.iter().all(|v| v.abs() <= 0.5);
        match kind {
            MotionKind::Orbit => self.finite() && angles_ok && (0.5..=4.0).contains(&self.zoom),
            MotionKind::PanZoom => self.finite() && pan_ok && (1.0 / 3.0..=3.0).contains(&self.zoom),
        }
    }
}

/// Largest |pitch| as a start or an amount (80°).
pub const MAX_PITCH: f32 = 80.0 * PI / 180.0;

/// A clip's camera motion: from `start`, by `amount`, timed by `ease`, over the clip.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Motion {
    /// The chip that is lit.
    pub preset: MotionPreset,
    /// Where the clip begins.
    pub start: Camera,
    /// What is added over the clip (`End = Start + amount`).
    pub amount: Camera,
    /// Timing.
    pub ease: Ease,
}

impl Default for Motion {
    fn default() -> Self {
        Motion { preset: MotionPreset::Custom, start: Camera::NONE, amount: Camera::NONE, ease: Ease::InOut }
    }
}

impl Motion {
    /// The preset's motion for `kind`, from `start`.
    pub fn preset(preset: MotionPreset, kind: MotionKind, start: Camera) -> Motion {
        let (amount, ease) = preset.amount(kind);
        Motion { preset, start, amount, ease }
    }

    /// Where the clip ends.
    pub fn end(&self) -> Camera {
        self.start.plus(self.amount)
    }

    /// The camera at a point `t` of the clip (`0` = start, `1` = end; outside is clamped).
    pub fn at(&self, t: f32) -> Camera {
        self.start.towards(self.amount, self.ease.apply(t))
    }

    /// The same motion run backwards: start at the old end, amount undone.
    pub fn swapped(&self) -> Motion {
        let amount = Camera {
            yaw: -self.amount.yaw,
            pitch: -self.amount.pitch,
            zoom: if self.amount.zoom > 0.0 { 1.0 / self.amount.zoom } else { 1.0 },
            pan: [-self.amount.pan[0], -self.amount.pan[1]],
        };
        Motion { preset: MotionPreset::Custom, start: self.end(), amount, ease: self.ease }
    }

    /// The Strip card's glyph: ↻ for a turn, ⤢ for a zoom, ⇆ for a pan, `None` when nothing
    /// moves.
    pub fn glyph(&self) -> Option<&'static str> {
        let a = self.amount;
        if a.yaw != 0.0 || a.pitch != 0.0 {
            Some("↻")
        } else if a.zoom != 1.0 {
            Some("⤢")
        } else if a.pan != [0.0, 0.0] {
            Some("⇆")
        } else {
            None
        }
    }
}

/// Why a motion edit was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MotionError {
    /// No clip has this id.
    NoClip(ClipId),
    /// A title card, or a shot with no asset: nothing to move.
    NoShot(ClipId),
    /// The preset doesn't apply to the clip's kind (Turntable on a still).
    NotForKind(MotionPreset, MotionKind),
    /// A start or amount outside the ranges for the clip's kind, or not finite.
    OutOfRange(&'static str),
}

impl fmt::Display for MotionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MotionError::NoClip(id) => write!(f, "no clip #{}", id.0),
            MotionError::NoShot(id) => write!(f, "clip #{} has no shot to move", id.0),
            MotionError::NotForKind(p, k) => write!(f, "{} does not apply to {}", p.name(), k.name().to_lowercase()),
            MotionError::OutOfRange(what) => write!(f, "`{what}` is out of range for this clip"),
        }
    }
}

impl std::error::Error for MotionError {}

/// What to change on a clip's motion; `None` keeps a field. A `preset` other than `Custom`
/// replaces the amount and ease with the preset's; `start`, `amount` and `ease` given alongside
/// apply on top (an amount or ease on top of a preset makes it `Custom`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MotionEdit {
    /// The chip clicked.
    pub preset: Option<MotionPreset>,
    /// A new start camera (Set from Viewer).
    pub start: Option<Camera>,
    /// A new amount (the sliders).
    pub amount: Option<Camera>,
    /// A new timing.
    pub ease: Option<Ease>,
}

/// Apply `edit` to clip `id`'s motion; a clip without motion starts from the shot as framed.
/// Returns whether anything changed. Refused for a title card, an empty shot, a preset that
/// doesn't fit the kind, or a value out of range; nothing changes on refusal.
pub fn set_motion(board: &mut Storyboard, id: ClipId, edit: MotionEdit) -> Result<bool, MotionError> {
    let clip = board.clips.iter_mut().find(|c| c.id == id).ok_or(MotionError::NoClip(id))?;
    let kind = MotionKind::of_clip(clip).ok_or(MotionError::NoShot(id))?;
    let before = clip.motion;
    let mut m = before.unwrap_or_default();
    if let Some(preset) = edit.preset {
        if !preset.allows(kind) {
            return Err(MotionError::NotForKind(preset, kind));
        }
        if preset != MotionPreset::Custom {
            m = Motion::preset(preset, kind, m.start);
        } else {
            m.preset = MotionPreset::Custom;
        }
    }
    if let Some(start) = edit.start {
        if !start.valid_start(kind) {
            return Err(MotionError::OutOfRange("start"));
        }
        m.start = start;
    }
    if let Some(amount) = edit.amount {
        if !amount.valid_amount(kind) {
            return Err(MotionError::OutOfRange("amount"));
        }
        if amount != m.amount {
            m.preset = MotionPreset::Custom;
        }
        m.amount = amount;
    }
    if let Some(ease) = edit.ease {
        if ease != m.ease {
            m.preset = MotionPreset::Custom;
        }
        m.ease = ease;
    }
    clip.motion = Some(m);
    Ok(before != Some(m))
}

/// Remove clip `id`'s motion. Returns whether it had one.
pub fn clear_motion(board: &mut Storyboard, id: ClipId) -> Result<bool, MotionError> {
    let clip = board.clips.iter_mut().find(|c| c.id == id).ok_or(MotionError::NoClip(id))?;
    Ok(clip.motion.take().is_some())
}

/// Replace the start and end of clip `id`'s motion with each other (the Swap button).
pub fn swap_motion(board: &mut Storyboard, id: ClipId) -> Result<bool, MotionError> {
    let clip = board.clips.iter_mut().find(|c| c.id == id).ok_or(MotionError::NoClip(id))?;
    match clip.motion {
        Some(m) => {
            clip.motion = Some(m.swapped());
            Ok(true)
        }
        None => Ok(false),
    }
}
