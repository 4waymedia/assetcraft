//! Layout themes for social posts (frame-settings.md F5, L1 Split; Paul, 2026-11-17, D2: "split
//! screen theme will split the stage vertical/horizontal, and auto fit content to the one section
//! based on the setting").
//!
//! A split stage has two regions, A and B — stacked (top and bottom) or side by side (left and
//! right) — with A taking `share` per cent. Each clip belongs to one region (`Clip.region`, A when
//! unset) and is cropped into it by the board's fill and snap. While one region's clip plays, the
//! other holds its last clip's last frame (or, before its first clip, that clip's first). Title
//! cards always take the whole frame.
//!
//! A framed post (L2, for 4:5 and 1:1 feed posts) keeps the shots in a picture region between a
//! headline band above and a caption band below, coloured by a text theme.

use crate::{Clip, ShotType, Storyboard, TextTheme, Ticks};
use serde::{Deserialize, Serialize};

/// The split shares A can take, in per cent.
pub const SPLIT_SHARES: [u8; 3] = [50, 60, 70];

/// The gutter between the regions, as a fraction of the frame along the split.
pub const SPLIT_GUTTER: f32 = 0.004;

/// The largest band a framed post can have, in per cent of the frame's height.
pub const BAND_MAX: u8 = 35;

/// The longest headline or caption, in characters.
pub const POST_TEXT_MAX: usize = 200;

/// A board's stage layout. Without one the stage is the whole frame (L0 Full).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum Layout {
    /// Two regions (L1).
    #[serde(rename_all = "camelCase")]
    Split {
        /// Top and bottom; otherwise left and right.
        stack: bool,
        /// Region A's share of the frame, in per cent (one of [`SPLIT_SHARES`]).
        share: u8,
    },
    /// A picture between a headline band and a caption band (L2).
    #[serde(rename_all = "camelCase")]
    Framed {
        /// The headline band's height, in per cent of the frame (0 = none; at most [`BAND_MAX`]).
        top: u8,
        /// The caption band's height, in per cent.
        bottom: u8,
        /// The headline.
        #[serde(default)]
        headline: String,
        /// The caption.
        #[serde(default)]
        caption: String,
        /// The bands' colours and type.
        #[serde(default)]
        theme: TextTheme,
    },
}

impl Layout {
    /// Why this layout can't be used, if it can't.
    pub fn problem(&self) -> Option<String> {
        match self {
            Layout::Split { share, .. } if !SPLIT_SHARES.contains(share) => {
                Some(format!("a split's share must be one of {SPLIT_SHARES:?}"))
            }
            Layout::Split { .. } => None,
            Layout::Framed { top, bottom, headline, caption, .. } => {
                if *top > BAND_MAX || *bottom > BAND_MAX {
                    Some(format!("a band can be at most {BAND_MAX} % of the frame"))
                } else if headline.chars().count() > POST_TEXT_MAX || caption.chars().count() > POST_TEXT_MAX {
                    Some(format!("a headline or caption can be at most {POST_TEXT_MAX} characters"))
                } else {
                    None
                }
            }
        }
    }

    /// A framed post's bands and picture as `[x, y, w, h]` fractions: `(headline, picture,
    /// caption)`. `None` for any other layout.
    pub fn framed(&self) -> Option<[[f32; 4]; 3]> {
        let Layout::Framed { top, bottom, .. } = self else { return None };
        let (t, b) = (f32::from(*top) / 100.0, f32::from(*bottom) / 100.0);
        Some([[0.0, 0.0, 1.0, t], [0.0, t, 1.0, (1.0 - t - b).max(0.0)], [0.0, 1.0 - b, 1.0, b]])
    }

    /// The regions A and B as `[x, y, w, h]` fractions of the frame, the gutter between them. A
    /// framed post has one region, its picture (given twice).
    pub fn regions(&self) -> [[f32; 4]; 2] {
        match *self {
            Layout::Framed { .. } => {
                let p = self.framed().map_or([0.0, 0.0, 1.0, 1.0], |[_, p, _]| p);
                [p, p]
            }
            Layout::Split { stack, share } => {
                let a = f32::from(share.clamp(10, 90)) / 100.0;
                let g = SPLIT_GUTTER / 2.0;
                if stack {
                    [[0.0, 0.0, 1.0, a - g], [0.0, a + g, 1.0, 1.0 - a - g]]
                } else {
                    [[0.0, 0.0, a - g, 1.0], [a + g, 0.0, 1.0 - a - g, 1.0]]
                }
            }
        }
    }

    /// Its name in the UI.
    pub fn label(&self) -> String {
        match self {
            Layout::Split { stack: true, share } => format!("Split, stacked {share}/{}", 100 - share),
            Layout::Split { stack: false, share } => format!("Split, side by side {share}/{}", 100 - share),
            Layout::Framed { top, bottom, .. } => format!("Framed post, bands {top} % / {bottom} %"),
        }
    }
}

/// A region of a split stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Region {
    /// The first: top, or left. Clips without a region are here.
    #[default]
    A,
    /// The second: bottom, or right.
    B,
}

impl Region {
    /// Its index in [`Layout::regions`].
    pub fn index(self) -> usize {
        match self {
            Region::A => 0,
            Region::B => 1,
        }
    }

    /// Its letter.
    pub fn letter(self) -> &'static str {
        match self {
            Region::A => "A",
            Region::B => "B",
        }
    }
}

/// What a region shows at a moment: the clip's index and the seconds into it.
pub type RegionShot = Option<(usize, f64)>;

impl Clip {
    /// The region this clip plays in on a split stage.
    pub fn region(&self) -> Region {
        self.region.unwrap_or_default()
    }
}

impl Storyboard {
    /// On a split stage, what regions A and B show at `at` (L1): the clip playing there, else the
    /// region's last clip held on its last frame, else its next clip's first frame, else nothing.
    /// `None` when the stage isn't split or `at` is on a title card (which takes the frame).
    pub fn split_shots(&self, at: Ticks) -> Option<[RegionShot; 2]> {
        let Some(Layout::Split { .. }) = self.layout else { return None };
        let starts = self.starts();
        let now = self.clips.iter().position(|c| self.clip_at(at).is_some_and(|x| x.id == c.id))?;
        if self.clips[now].shot_type == ShotType::Ttl {
            return None;
        }
        let shot = |c: &Clip| c.shot_type != ShotType::Ttl;
        let mut out: [RegionShot; 2] = [None, None];
        for region in [Region::A, Region::B] {
            let is = |c: &Clip| shot(c) && c.region() == region;
            let pick = if is(&self.clips[now]) {
                let start = starts.get(now).copied().unwrap_or(Ticks::ZERO);
                Some((now, Ticks(at.0.saturating_sub(start.0)).as_secs_f64()))
            } else if let Some(i) = (0..now).rev().find(|i| is(&self.clips[*i])) {
                // Held on its last frame.
                Some((i, (self.clips[i].duration.as_secs_f64() - 1e-3).max(0.0)))
            } else {
                (now + 1..self.clips.len()).find(|i| is(&self.clips[*i])).map(|i| (i, 0.0))
            };
            out[region.index()] = pick;
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClipId, SceneId};

    fn board(regions: &[(ShotType, Option<Region>)]) -> Storyboard {
        let mut b = Storyboard::new("Split");
        b.clips = regions
            .iter()
            .enumerate()
            .map(|(i, (shot, region))| {
                let mut c =
                    Clip::new(ClipId(i as u32 + 1), SceneId(1), *shot, Ticks::from_secs(2).unwrap_or(Ticks::ZERO));
                c.region = *region;
                c
            })
            .collect();
        b.layout = Some(Layout::Split { stack: true, share: 60 });
        b
    }

    fn secs(s: f64) -> Ticks {
        Ticks::from_secs_f64(s).unwrap_or(Ticks::ZERO)
    }

    #[test]
    fn regions_split_the_frame_with_a_gutter() {
        let [a, b] = Layout::Split { stack: true, share: 60 }.regions();
        assert!((a[3] - (0.6 - SPLIT_GUTTER / 2.0)).abs() < 1e-6 && a[2] == 1.0);
        assert!((b[1] - (0.6 + SPLIT_GUTTER / 2.0)).abs() < 1e-6 && (b[1] + b[3] - 1.0).abs() < 1e-6);
        let [a, b] = Layout::Split { stack: false, share: 50 }.regions();
        assert!(a[3] == 1.0 && b[3] == 1.0 && b[0] > a[0] + a[2]);
        assert!(Layout::Split { stack: true, share: 55 }.problem().is_some());
        assert!(Layout::Split { stack: true, share: 70 }.problem().is_none());
    }

    #[test]
    fn each_region_shows_its_clip_or_holds_its_last() {
        use Region::*;
        // A, B, A, title, B
        let b = board(&[
            (ShotType::Ws, None),
            (ShotType::Cu, Some(B)),
            (ShotType::Ms, Some(A)),
            (ShotType::Ttl, None),
            (ShotType::Cu, Some(B)),
        ]);
        // In clip 1 (A): B shows its first clip's first frame.
        assert_eq!(b.split_shots(secs(0.5)), Some([Some((0, 0.5)), Some((1, 0.0))]));
        // In clip 2 (B): A holds clip 1's last frame.
        let [a, bb] = b.split_shots(secs(2.5)).unwrap();
        assert_eq!(a.map(|s| s.0), Some(0));
        assert!(a.is_some_and(|s| s.1 > 1.99));
        assert_eq!(bb, Some((1, 0.5)));
        // In clip 3 (A): B holds clip 2.
        let [a, bb] = b.split_shots(secs(4.25)).unwrap();
        assert_eq!(a, Some((2, 0.25)));
        assert_eq!(bb.map(|s| s.0), Some(1));
        // A title card takes the whole frame.
        assert_eq!(b.split_shots(secs(6.5)), None);
        // Full stage: nothing to split.
        let mut full = b.clone();
        full.layout = None;
        assert_eq!(full.split_shots(secs(0.5)), None);
    }
}

#[cfg(test)]
mod framed_tests {
    use super::*;

    fn framed(top: u8, bottom: u8) -> Layout {
        Layout::Framed { top, bottom, headline: "Hi".into(), caption: String::new(), theme: TextTheme::Bold }
    }

    #[test]
    fn a_framed_post_has_bands_and_a_picture() {
        let [t, p, b] = framed(14, 12).framed().unwrap();
        assert!((t[3] - 0.14).abs() < 1e-6 && (p[1] - 0.14).abs() < 1e-6);
        assert!((p[3] - 0.74).abs() < 1e-6 && (b[1] - 0.88).abs() < 1e-6);
        assert_eq!(framed(0, 0).framed().map(|[_, p, _]| p), Some([0.0, 0.0, 1.0, 1.0]));
        assert_eq!(framed(14, 12).regions()[0], p, "its one region is the picture");
        assert!(Layout::Split { stack: true, share: 50 }.framed().is_none());
    }

    #[test]
    fn a_framed_post_is_checked() {
        assert!(framed(36, 0).problem().is_some());
        assert!(framed(35, 35).problem().is_none());
        let long = Layout::Framed {
            top: 10,
            bottom: 10,
            headline: "x".repeat(201),
            caption: String::new(),
            theme: TextTheme::Bold,
        };
        assert!(long.problem().is_some());
    }
}
