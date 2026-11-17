//! The Cut's frame (frame-settings.md F1): its size, frame rate and how a picture is cropped
//! into it, picked from presets for film and social or set by hand.
//!
//! Paul, 2026-11-17: "support for normal film ratios, square ratio, and mobile vertical
//! sizes... ratio, pixel dimensions, frame rate", and the setup's "Default Crop: Fill Vertical,
//! Fill Horizontal, Snap-to: top, bottom, left, right". A board without a frame is
//! [`FrameSpec::default`], today's 1920 × 1080 at 24 fps, so every older board means what it did.

use serde::{Deserialize, Serialize};

/// The frame rates a Cut can run at: whole numbers the tick rate divides exactly.
pub const FRAME_RATES: [u32; 5] = [24, 25, 30, 50, 60];

/// The smallest and largest side, in pixels (the encoder's limits).
pub const FRAME_SIDE_RANGE: (u32, u32) = (16, 8192);

/// How a picture of another shape is fitted into the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fill {
    /// The picture's height fills the frame: a wide shot in a vertical frame loses its sides.
    #[default]
    Vertical,
    /// The picture's width fills the frame: a wide shot in a vertical frame keeps its sides,
    /// with bands above and below.
    Horizontal,
}

impl Fill {
    /// Its name in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Fill::Vertical => "Fill vertical",
            Fill::Horizontal => "Fill horizontal",
        }
    }
}

/// Where a cropped or banded picture sits along the side it doesn't fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Snap {
    /// In the middle.
    #[default]
    Center,
    /// Against the top.
    Top,
    /// Against the bottom.
    Bottom,
    /// Against the left.
    Left,
    /// Against the right.
    Right,
}

impl Snap {
    /// Every snap, in UI order.
    pub const ALL: [Snap; 5] = [Snap::Center, Snap::Top, Snap::Bottom, Snap::Left, Snap::Right];

    /// Its name in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Snap::Center => "Center",
            Snap::Top => "Top",
            Snap::Bottom => "Bottom",
            Snap::Left => "Left",
            Snap::Right => "Right",
        }
    }

    /// Where the picture sits on each axis, 0 (left / top) to 1 (right / bottom).
    pub fn align(self) -> (f32, f32) {
        match self {
            Snap::Center => (0.5, 0.5),
            Snap::Top => (0.5, 0.0),
            Snap::Bottom => (0.5, 1.0),
            Snap::Left => (0.0, 0.5),
            Snap::Right => (1.0, 0.5),
        }
    }
}

/// The Cut's frame: pixel size, frame rate, the default crop, and the preset it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameSpec {
    /// Width in pixels: even, within [`FRAME_SIDE_RANGE`].
    pub width: u32,
    /// Height in pixels: even, within [`FRAME_SIDE_RANGE`].
    pub height: u32,
    /// Frames a second: one of [`FRAME_RATES`].
    pub fps: u32,
    /// How a picture of another shape fills the frame.
    #[serde(default)]
    pub fill: Fill,
    /// Where it sits along the other side.
    #[serde(default)]
    pub snap: Snap,
    /// The preset's id, if it came from one ([`FRAME_PRESETS`]); none when set by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
}

impl Default for FrameSpec {
    /// Today's frame: HD 16:9 at 24 fps, filled vertically from the centre.
    fn default() -> Self {
        FrameSpec { width: 1920, height: 1080, fps: 24, fill: Fill::Vertical, snap: Snap::Center, preset: None }
    }
}

impl FrameSpec {
    /// Width over height.
    pub fn ratio(&self) -> f64 {
        f64::from(self.width) / f64::from(self.height.max(1))
    }

    /// The ratio as people say it: `16:9`, `9:16`, `1:1`, `4:5`, `2.39:1`.
    pub fn ratio_label(&self) -> String {
        let (w, h) = (self.width.max(1), self.height.max(1));
        let g = gcd(w, h);
        let (a, b) = (w / g, h / g);
        if a <= 32 && b <= 32 { format!("{a}:{b}") } else { format!("{:.2}:1", self.ratio()) }
    }

    /// Why this frame can't be used, if it can't.
    pub fn problem(&self) -> Option<String> {
        let (lo, hi) = FRAME_SIDE_RANGE;
        for (name, v) in [("width", self.width), ("height", self.height)] {
            if !(lo..=hi).contains(&v) {
                return Some(format!("the {name} must be {lo} to {hi} pixels"));
            }
            if v % 2 != 0 {
                return Some(format!("the {name} must be even (the encoder works in pairs of pixels)"));
            }
        }
        if !FRAME_RATES.contains(&self.fps) {
            return Some(format!("the frame rate must be one of {FRAME_RATES:?}"));
        }
        if self.preset.as_ref().is_some_and(|p| p.len() > 64) {
            return Some("the preset's id is too long".into());
        }
        None
    }

    /// Whether it is taller than wide.
    pub fn is_vertical(&self) -> bool {
        self.height > self.width
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

/// A named frame for a kind of film or a social placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePreset {
    /// Its stable id, e.g. `vertical`.
    pub id: &'static str,
    /// Its group in the UI: `Film`, `Square` or `Social`.
    pub group: &'static str,
    /// Its name in the UI.
    pub name: &'static str,
    /// Where it is used.
    pub for_: &'static str,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Frames a second.
    pub fps: u32,
    /// Its default crop.
    pub fill: Fill,
}

impl FramePreset {
    /// The frame it gives, snapped to the centre.
    pub fn spec(&self) -> FrameSpec {
        FrameSpec {
            width: self.width,
            height: self.height,
            fps: self.fps,
            fill: self.fill,
            snap: Snap::Center,
            preset: Some(self.id.to_owned()),
        }
    }
}

/// The presets (frame-settings.md §3). Social sizes from current platform guides; the frame
/// rates are AssetCraft's defaults and can be changed per board.
pub const FRAME_PRESETS: [FramePreset; 9] = [
    FramePreset {
        id: "hd",
        group: "Film",
        name: "HD 16:9",
        for_: "Film, TV, web",
        width: 1920,
        height: 1080,
        fps: 24,
        fill: Fill::Vertical,
    },
    FramePreset {
        id: "uhd",
        group: "Film",
        name: "4K UHD 16:9",
        for_: "Film, TV, web",
        width: 3840,
        height: 2160,
        fps: 24,
        fill: Fill::Vertical,
    },
    FramePreset {
        id: "flat",
        group: "Film",
        name: "Flat 1.85:1",
        for_: "Cinema (flat)",
        width: 1998,
        height: 1080,
        fps: 24,
        fill: Fill::Horizontal,
    },
    FramePreset {
        id: "scope",
        group: "Film",
        name: "Scope 2.39:1",
        for_: "Cinema (scope), trailers",
        width: 2048,
        height: 858,
        fps: 24,
        fill: Fill::Horizontal,
    },
    FramePreset {
        id: "academy",
        group: "Film",
        name: "4:3",
        for_: "Classic TV, archival",
        width: 1440,
        height: 1080,
        fps: 24,
        fill: Fill::Vertical,
    },
    FramePreset {
        id: "square",
        group: "Square",
        name: "Square 1:1",
        for_: "Instagram, Facebook, LinkedIn, X feeds",
        width: 1080,
        height: 1080,
        fps: 30,
        fill: Fill::Vertical,
    },
    FramePreset {
        id: "portrait",
        group: "Social",
        name: "Feed portrait 4:5",
        for_: "Instagram, Facebook, LinkedIn feeds",
        width: 1080,
        height: 1350,
        fps: 30,
        fill: Fill::Vertical,
    },
    FramePreset {
        id: "vertical",
        group: "Social",
        name: "Vertical 9:16",
        for_: "Reels, TikTok, YouTube Shorts, Stories",
        width: 1080,
        height: 1920,
        fps: 30,
        fill: Fill::Vertical,
    },
    FramePreset {
        id: "youtube",
        group: "Social",
        name: "YouTube 16:9",
        for_: "YouTube, X",
        width: 1920,
        height: 1080,
        fps: 30,
        fill: Fill::Vertical,
    },
];

/// The preset with id `id`.
pub fn frame_preset(id: &str) -> Option<&'static FramePreset> {
    FRAME_PRESETS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_a_usable_frame() {
        for p in FRAME_PRESETS {
            assert_eq!(p.spec().problem(), None, "{}", p.id);
            assert_eq!(frame_preset(p.id), Some(&p));
        }
        let ids: std::collections::HashSet<_> = FRAME_PRESETS.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), FRAME_PRESETS.len(), "ids are unique");
    }

    #[test]
    fn ratios_read_as_people_say_them() {
        let label = |id| frame_preset(id).map(|p| p.spec().ratio_label());
        assert_eq!(label("hd").as_deref(), Some("16:9"));
        assert_eq!(label("vertical").as_deref(), Some("9:16"));
        assert_eq!(label("square").as_deref(), Some("1:1"));
        assert_eq!(label("portrait").as_deref(), Some("4:5"));
        assert_eq!(label("academy").as_deref(), Some("4:3"));
        assert_eq!(label("scope").as_deref(), Some("2.39:1"));
        assert_eq!(label("flat").as_deref(), Some("1.85:1"));
        assert!(frame_preset("vertical").is_some_and(|p| p.spec().is_vertical()));
    }

    #[test]
    fn a_bad_frame_says_why() {
        let f = |width, height, fps| FrameSpec { width, height, fps, ..FrameSpec::default() };
        assert!(f(1081, 1920, 30).problem().is_some_and(|p| p.contains("even")));
        assert!(f(8, 1920, 30).problem().is_some_and(|p| p.contains("16 to 8192")));
        assert!(f(9000, 1920, 30).problem().is_some());
        assert!(f(1080, 1920, 29).problem().is_some_and(|p| p.contains("frame rate")));
        assert_eq!(f(1080, 1920, 60).problem(), None);
        assert_eq!(FrameSpec::default().problem(), None);
    }
}
