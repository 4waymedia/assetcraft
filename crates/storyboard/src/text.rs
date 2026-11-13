//! Text clips: a clip's words shown either as a **Card** (a whole clip of its own: the theme's
//! background with the text animated in, the handoff's Title card) or as an **Overlay** (the
//! words drawn over the clip's shot, at a position, in the theme's style).
//!
//! The kind is derived, never stored twice:
//! - a [`ShotType::Ttl`] clip is a Card, and its text is its `description` (the scene title rule
//!   in [`crate::model`] stays true);
//! - any other clip with `overlay: Some(text)` is an Overlay.
//!
//! Switching kind keeps the clip's id, start and length, so the timeline never shifts and one
//! switch is one undo step. Card → Overlay turns the clip into a Wide shot that needs an image;
//! Overlay → Card drops the clip's images (a Card holds text only), and the report says how many.
//!
//! Theme and animation are data here ([`TextTheme::look`], [`TextAnimation::frame`]); the UI
//! only paints what they return, so the CLI and tests see the same frames.

use serde::{Deserialize, Serialize};

use crate::decor::{LookDecor, Pattern, Rules, TextFinish};

use crate::model::{ClipId, ShotType, Storyboard};

/// How a clip's text is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextKind {
    /// A whole clip: the theme's background, the text animated in.
    Card,
    /// Drawn over the clip's shot at [`TextStyle::position`].
    Overlay,
}

impl TextKind {
    /// Both kinds, in menu order.
    pub const ALL: [TextKind; 2] = [TextKind::Card, TextKind::Overlay];

    /// Id used in commands and documents.
    pub fn id(self) -> &'static str {
        match self {
            TextKind::Card => "card",
            TextKind::Overlay => "overlay",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            TextKind::Card => "Card",
            TextKind::Overlay => "Overlay",
        }
    }

    /// Parse an id (`card` / `overlay`).
    pub fn from_id(id: &str) -> Option<TextKind> {
        TextKind::ALL.into_iter().find(|k| k.id() == id)
    }

    /// The other kind (what a click on the kind chip switches to).
    pub fn other(self) -> TextKind {
        match self {
            TextKind::Card => TextKind::Overlay,
            TextKind::Overlay => TextKind::Card,
        }
    }
}

/// A text theme: colours, case, spacing and size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextTheme {
    /// Red card, white heavy capitals (the handoff's Title card).
    #[default]
    Bold,
    /// Near-black, light text, no plate.
    Minimal,
    /// Black, warm capitals with wide tracking.
    Cinematic,
    /// Off-white card, dark text.
    Paper,
    /// 1920s luxury: black, cream capitals, gold (themes §4 #5).
    #[serde(rename = "noir-gold")]
    NoirGold,
    /// Synthwave: deep purple, white, magenta and cyan (§4 #6).
    #[serde(rename = "neon-night")]
    NeonNight,
    /// Documentary, nature: moss, paper, ochre (§4 #8).
    #[serde(rename = "field-notes")]
    FieldNotes,
    /// How-to, tutorial: blueprint blue, white, yellow (§4 #13).
    Blueprint,
}

/// What a theme paints. Colours are `0xRRGGBB`; sizes are fractions of the frame height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// Card background.
    pub background: u32,
    /// Text colour.
    pub text: u32,
    /// Overlay plate behind the text, with its alpha (0–255); `None` = a soft shadow only.
    pub plate: Option<(u32, u8)>,
    /// Card text height as a fraction of the frame height.
    pub card_size: f32,
    /// Overlay text height as a fraction of the frame height.
    pub overlay_size: f32,
    /// Extra letter spacing as a fraction of the text height.
    pub tracking: f32,
    /// Show the text in capitals.
    pub uppercase: bool,
    /// What a card's background is: colour stops, grain, vignette (themes 1d).
    pub backdrop: Backdrop,
    /// The accent bar's colour — under a card's title, down an overlay plate's edge; `None` =
    /// no bar.
    pub accent: Option<u32>,
    /// Rules, corner marks, frame, pattern (themes 1e).
    pub decor: crate::decor::LookDecor,
    /// Shadow, outline or glow on the letters (themes 1f).
    pub finish: crate::decor::TextFinish,
    /// The theme's moves: what a text does unless its clip says otherwise (themes 1g).
    pub motion: Timing,
}

/// A card's background (themes 1d, `docs/design/themes.md`): up to four colour stops along
/// an angle, film grain and darkened edges. One colour function for the export and the Viewer,
/// so both draw the same card.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backdrop {
    /// Colour stops, `0xRRGGBB`; the first `count` are used.
    pub stops: [u32; 4],
    /// How many stops (1 = solid, 2–4 = gradient).
    pub count: u8,
    /// Gradient direction in degrees: 0 = left to right, 90 = top to bottom.
    pub angle: f32,
    /// Film grain, 0–1.
    pub grain: f32,
    /// Darkened edges, 0–1.
    pub vignette: f32,
    /// From the centre out (first stop in the middle) instead of along `angle`.
    pub radial: bool,
}

impl Backdrop {
    /// One colour, nothing else.
    pub const fn solid(color: u32) -> Backdrop {
        Backdrop { stops: [color; 4], count: 1, angle: 0.0, grain: 0.0, vignette: 0.0, radial: false }
    }

    /// Two colours along `angle`, with grain and vignette.
    pub const fn gradient(from: u32, to: u32, angle: f32, grain: f32, vignette: f32) -> Backdrop {
        Backdrop { stops: [from, to, to, to], count: 2, angle, grain, vignette, radial: false }
    }

    /// Two colours from the centre (`inner`) to the corners (`outer`).
    pub const fn radial(inner: u32, outer: u32, grain: f32, vignette: f32) -> Backdrop {
        Backdrop { stops: [inner, outer, outer, outer], count: 2, angle: 0.0, grain, vignette, radial: true }
    }

    /// The colour at `(u, v)` (0–1 across and down the frame), vignette applied, without grain,
    /// as linear 0–255 channels.
    pub fn color_at(&self, u: f32, v: f32) -> [f32; 3] {
        let n = usize::from(self.count.clamp(1, 4));
        let mut c = channels(self.stops[0]);
        if n > 1 {
            let t = if self.radial {
                let (dx, dy) = ((u - 0.5) * 2.0, (v - 0.5) * 2.0);
                (dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2
            } else {
                let (sin, cos) = self.angle.to_radians().sin_cos();
                let reach = cos.abs() + sin.abs();
                0.5 + ((u - 0.5) * cos + (v - 0.5) * sin) / reach.max(1e-3)
            };
            let t = t.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (t.floor() as usize).min(n - 2);
            let f = t - i as f32;
            let (a, b) = (channels(self.stops[i]), channels(self.stops[i + 1]));
            for k in 0..3 {
                c[k] = a[k] + (b[k] - a[k]) * f;
            }
        }
        if self.vignette > 0.0 {
            let (dx, dy) = ((u - 0.5) * 2.0, (v - 0.5) * 2.0);
            let r = ((dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2).clamp(0.0, 1.0);
            let edge = ((r - 0.35) / 0.65).clamp(0.0, 1.0);
            let dark = 1.0 - self.vignette.clamp(0.0, 1.0) * 0.85 * edge * edge * (3.0 - 2.0 * edge);
            for ch in &mut c {
                *ch *= dark;
            }
        }
        c
    }

    /// Grain at pixel `(x, y)`: an offset to add to each channel (same for every frame, so a
    /// still card doesn't shimmer and exports stay byte-identical).
    pub fn grain_at(&self, x: u32, y: u32) -> f32 {
        if self.grain <= 0.0 {
            return 0.0;
        }
        let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
        let n = (h & 0xffff) as f32 / 65_535.0 * 2.0 - 1.0;
        n * self.grain.clamp(0.0, 1.0) * 28.0
    }
}

fn channels(c: u32) -> [f32; 3] {
    [((c >> 16) & 0xff) as f32, ((c >> 8) & 0xff) as f32, (c & 0xff) as f32]
}

/// Where the accent bar goes, in pixels `[x, y, w, h]`, for text whose block is `block`
/// (`[left, top, width, height]`) in a frame `frame_h` tall. A card's bar sits centred under
/// the title; an overlay's runs down the plate's left edge (`pad` = the plate's padding), or
/// under the text when there is no plate.
pub fn accent_bar(kind: TextKind, block: [f32; 4], frame_h: f32, pad: [f32; 2], plate: bool) -> [f32; 4] {
    let [left, top, w, h] = block;
    let thick = (frame_h * 0.008).max(2.0);
    match kind {
        TextKind::Card => {
            let bw = frame_h * 0.12;
            [left + w / 2.0 - bw / 2.0, top + h + frame_h * 0.03, bw, thick]
        }
        TextKind::Overlay if plate => [left - pad[0], top - pad[1], thick * 1.5, h + 2.0 * pad[1]],
        TextKind::Overlay => [left, top + h + thick, (frame_h * 0.06).min(w), thick],
    }
}

impl TextTheme {
    /// Every theme, in menu order.
    pub const ALL: [TextTheme; 8] = [
        TextTheme::Bold,
        TextTheme::Minimal,
        TextTheme::Cinematic,
        TextTheme::Paper,
        TextTheme::NoirGold,
        TextTheme::NeonNight,
        TextTheme::FieldNotes,
        TextTheme::Blueprint,
    ];

    /// Id used in commands and documents.
    pub fn id(self) -> &'static str {
        match self {
            TextTheme::Bold => "bold",
            TextTheme::Minimal => "minimal",
            TextTheme::Cinematic => "cinematic",
            TextTheme::Paper => "paper",
            TextTheme::NoirGold => "noir-gold",
            TextTheme::NeonNight => "neon-night",
            TextTheme::FieldNotes => "field-notes",
            TextTheme::Blueprint => "blueprint",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            TextTheme::Bold => "Bold",
            TextTheme::Minimal => "Minimal",
            TextTheme::Cinematic => "Cinematic",
            TextTheme::Paper => "Paper",
            TextTheme::NoirGold => "Noir Gold",
            TextTheme::NeonNight => "Neon Night",
            TextTheme::FieldNotes => "Field Notes",
            TextTheme::Blueprint => "Blueprint",
        }
    }

    /// Parse an id.
    pub fn from_id(id: &str) -> Option<TextTheme> {
        TextTheme::ALL.into_iter().find(|t| t.id() == id)
    }

    /// What the theme paints.
    pub fn look(self) -> Look {
        let mut look = match self {
            TextTheme::Bold => Look {
                background: 0xe5243b,
                text: 0xffffff,
                plate: Some((0xe5243b, 235)),
                card_size: 0.11,
                overlay_size: 0.06,
                tracking: 0.0,
                uppercase: true,
                backdrop: Backdrop::gradient(0xe5243b, 0xa8172a, 135.0, 0.0, 0.2),
                accent: Some(0x111111),
                decor: LookDecor::NONE,
                finish: TextFinish::Shadow,
                motion: Timing {
                    animation: TextAnimation::Fade,
                    duration_ms: None,
                    effect: TextEffect::None,
                    exit: TextExit::Fade,
                },
            },
            TextTheme::Minimal => Look {
                background: 0x0f1217,
                text: 0xe9ebee,
                plate: None,
                card_size: 0.075,
                overlay_size: 0.05,
                tracking: 0.02,
                uppercase: false,
                backdrop: Backdrop { vignette: 0.35, ..Backdrop::solid(0x0f1217) },
                accent: Some(0xcfcfcf),
                decor: LookDecor::NONE,
                finish: TextFinish::Shadow,
                motion: Timing {
                    animation: TextAnimation::Fade,
                    duration_ms: None,
                    effect: TextEffect::None,
                    exit: TextExit::Fade,
                },
            },
            TextTheme::Cinematic => Look {
                background: 0x000000,
                text: 0xf2e6c9,
                plate: Some((0x000000, 150)),
                card_size: 0.07,
                overlay_size: 0.045,
                tracking: 0.35,
                uppercase: true,
                backdrop: Backdrop { grain: 0.35, ..Backdrop::solid(0x000000) },
                accent: None,
                decor: LookDecor::NONE,
                finish: TextFinish::None,
                motion: Timing {
                    animation: TextAnimation::Fade,
                    duration_ms: None,
                    effect: TextEffect::Push,
                    exit: TextExit::Fade,
                },
            },
            TextTheme::Paper => Look {
                background: 0xf3efe6,
                text: 0x1d1d1f,
                plate: Some((0xf3efe6, 230)),
                card_size: 0.085,
                overlay_size: 0.05,
                tracking: 0.0,
                uppercase: false,
                backdrop: Backdrop::gradient(0xf3efe6, 0xe4dccb, 90.0, 0.15, 0.15),
                accent: Some(0xa4472d),
                decor: LookDecor { frame: true, ..LookDecor::NONE },
                finish: TextFinish::None,
                motion: Timing {
                    animation: TextAnimation::Rise,
                    duration_ms: None,
                    effect: TextEffect::None,
                    exit: TextExit::Sink,
                },
            },
            TextTheme::NoirGold => Look {
                background: 0x0b0b0d,
                text: 0xf3e9d2,
                plate: Some((0x0b0b0d, 190)),
                card_size: 0.075,
                overlay_size: 0.045,
                tracking: 0.25,
                uppercase: true,
                backdrop: Backdrop::radial(0x24242b, 0x0b0b0d, 0.2, 0.4),
                accent: Some(0xc9a24a),
                decor: LookDecor { rules: Rules::Both, corners: true, ..LookDecor::NONE },
                finish: TextFinish::None,
                motion: Timing {
                    animation: TextAnimation::Fade,
                    duration_ms: None,
                    effect: TextEffect::Push,
                    exit: TextExit::Fade,
                },
            },
            TextTheme::NeonNight => Look {
                background: 0x1a0b2e,
                text: 0xffffff,
                plate: Some((0x1a0b2e, 210)),
                card_size: 0.10,
                overlay_size: 0.055,
                tracking: 0.06,
                uppercase: true,
                backdrop: Backdrop::gradient(0x2c1350, 0x0c0418, 90.0, 0.05, 0.35),
                accent: Some(0xff2bd6),
                decor: LookDecor { pattern: Pattern::Scanlines, frame: true, ..LookDecor::NONE },
                finish: TextFinish::Glow,
                motion: Timing {
                    animation: TextAnimation::Zoom,
                    duration_ms: None,
                    effect: TextEffect::Pulse,
                    exit: TextExit::Zoom,
                },
            },
            TextTheme::FieldNotes => Look {
                background: 0x3c4a2c,
                text: 0xf1ead8,
                plate: Some((0x3c4a2c, 225)),
                card_size: 0.08,
                overlay_size: 0.05,
                tracking: 0.0,
                uppercase: false,
                backdrop: Backdrop::gradient(0x4c5a38, 0x2e3822, 90.0, 0.25, 0.35),
                accent: Some(0xc58a2b),
                decor: LookDecor { pattern: Pattern::Dots, rules: Rules::Below, ..LookDecor::NONE },
                finish: TextFinish::Shadow,
                motion: Timing {
                    animation: TextAnimation::Rise,
                    duration_ms: None,
                    effect: TextEffect::Drift,
                    exit: TextExit::Fade,
                },
            },
            TextTheme::Blueprint => Look {
                background: 0x1e4f8f,
                text: 0xffffff,
                plate: Some((0x163c6e, 225)),
                card_size: 0.08,
                overlay_size: 0.048,
                tracking: 0.08,
                uppercase: true,
                backdrop: Backdrop::gradient(0x2463a8, 0x153a6a, 90.0, 0.0, 0.25),
                accent: Some(0xffd23f),
                decor: LookDecor { pattern: Pattern::Grid, corners: true, frame: true, ..LookDecor::NONE },
                finish: TextFinish::None,
                motion: Timing {
                    animation: TextAnimation::Typewriter,
                    duration_ms: None,
                    effect: TextEffect::None,
                    exit: TextExit::Erase,
                },
            },
        };
        // Rules, corners and frame are drawn in the accent.
        look.decor.color = self.accent();
        look
    }

    /// The theme's highlight colour (`0xRRGGBB`): the picker's swatch, later lower-third bars.
    pub fn accent(self) -> u32 {
        match self {
            TextTheme::Bold => 0x111111,
            TextTheme::Minimal => 0xcfcfcf,
            TextTheme::Cinematic => 0xd9a441,
            TextTheme::Paper => 0xa4472d,
            TextTheme::NoirGold => 0xc9a24a,
            TextTheme::NeonNight => 0xff2bd6,
            TextTheme::FieldNotes => 0xc58a2b,
            TextTheme::Blueprint => 0xffd23f,
        }
    }
}

/// How the text comes in at the start of its clip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAnimation {
    /// Shown at once.
    None,
    /// Fades in.
    #[default]
    Fade,
    /// Fades in while rising into place.
    Rise,
    /// Fades in while settling from 125% to 100%.
    Zoom,
    /// Types on, one character at a time.
    Typewriter,
}

/// One frame of a text animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimFrame {
    /// Opacity, 0–1.
    pub alpha: f32,
    /// Vertical offset as a fraction of the frame height (positive = below its place).
    pub offset_y: f32,
    /// Scale, 1 = at rest.
    pub scale: f32,
    /// Fraction of the characters shown, 0–1.
    pub reveal: f32,
}

impl AnimFrame {
    /// At rest: fully shown.
    pub const REST: AnimFrame = AnimFrame { alpha: 1.0, offset_y: 0.0, scale: 1.0, reveal: 1.0 };
    /// Gone: after the text's out.
    pub const HIDDEN: AnimFrame = AnimFrame { alpha: 0.0, offset_y: 0.0, scale: 1.0, reveal: 0.0 };

    /// Whether nothing of the text shows.
    pub fn is_hidden(&self) -> bool {
        self.alpha <= 0.0 || self.reveal <= 0.0
    }

    /// How many of `chars` characters to draw (at least one once anything shows).
    pub fn visible_chars(&self, chars: usize) -> usize {
        if self.reveal >= 1.0 {
            return chars;
        }
        let n = (chars as f32 * self.reveal.max(0.0)).ceil() as usize;
        n.min(chars)
    }
}

impl TextAnimation {
    /// Every animation, in menu order.
    pub const ALL: [TextAnimation; 5] =
        [TextAnimation::None, TextAnimation::Fade, TextAnimation::Rise, TextAnimation::Zoom, TextAnimation::Typewriter];

    /// Id used in commands and documents.
    pub fn id(self) -> &'static str {
        match self {
            TextAnimation::None => "none",
            TextAnimation::Fade => "fade",
            TextAnimation::Rise => "rise",
            TextAnimation::Zoom => "zoom",
            TextAnimation::Typewriter => "typewriter",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            TextAnimation::None => "None",
            TextAnimation::Fade => "Fade in",
            TextAnimation::Rise => "Rise",
            TextAnimation::Zoom => "Zoom",
            TextAnimation::Typewriter => "Typewriter",
        }
    }

    /// Parse an id.
    pub fn from_id(id: &str) -> Option<TextAnimation> {
        TextAnimation::ALL.into_iter().find(|a| a.id() == id)
    }

    /// Length of the intro in seconds.
    pub fn seconds(self) -> f64 {
        match self {
            TextAnimation::None => 0.0,
            TextAnimation::Fade | TextAnimation::Rise => 0.5,
            TextAnimation::Zoom => 0.6,
            TextAnimation::Typewriter => 1.2,
        }
    }

    /// The frame `elapsed` seconds into the clip. Before the start counts as the start; past
    /// the intro (or a non-finite time) is at rest.
    pub fn frame(self, elapsed: f64) -> AnimFrame {
        let len = self.seconds();
        if len <= 0.0 || !elapsed.is_finite() || elapsed >= len {
            return AnimFrame::REST;
        }
        let x = (elapsed.max(0.0) / len) as f32;
        let eased = 1.0 - (1.0 - x).powi(3);
        match self {
            TextAnimation::None => AnimFrame::REST,
            TextAnimation::Fade => AnimFrame { alpha: eased, ..AnimFrame::REST },
            TextAnimation::Rise => AnimFrame { alpha: eased, offset_y: 0.06 * (1.0 - eased), ..AnimFrame::REST },
            TextAnimation::Zoom => AnimFrame { alpha: eased, scale: 1.25 - 0.25 * eased, ..AnimFrame::REST },
            TextAnimation::Typewriter => AnimFrame { reveal: x, ..AnimFrame::REST },
        }
    }
}

/// What the text does while it is up, between its in and its out (Paul, 2026-11-08: "theme,
/// transition in, duration, effect, transition out, position").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextEffect {
    /// Holds still.
    #[default]
    None,
    /// Grows slowly (100 % to 108 %) for as long as it is up.
    Push,
    /// Floats slowly upward.
    Drift,
    /// Breathes: a gentle swell every 1.6 s.
    Pulse,
}

impl TextEffect {
    /// Every effect, in menu order.
    pub const ALL: [TextEffect; 4] = [TextEffect::None, TextEffect::Push, TextEffect::Drift, TextEffect::Pulse];

    /// Id used in commands and documents.
    pub fn id(self) -> &'static str {
        match self {
            TextEffect::None => "none",
            TextEffect::Push => "push",
            TextEffect::Drift => "drift",
            TextEffect::Pulse => "pulse",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            TextEffect::None => "None",
            TextEffect::Push => "Slow push",
            TextEffect::Drift => "Drift up",
            TextEffect::Pulse => "Pulse",
        }
    }

    /// Parse an id.
    pub fn from_id(id: &str) -> Option<TextEffect> {
        TextEffect::ALL.into_iter().find(|e| e.id() == id)
    }

    /// Apply the effect to `f`, `elapsed` seconds into a text that is up for `shown` seconds.
    fn apply(self, f: AnimFrame, elapsed: f64, shown: f64) -> AnimFrame {
        let t = if shown > 0.0 { (elapsed / shown).clamp(0.0, 1.0) as f32 } else { 0.0 };
        match self {
            TextEffect::None => f,
            TextEffect::Push => AnimFrame { scale: f.scale * (1.0 + 0.08 * t), ..f },
            TextEffect::Drift => AnimFrame { offset_y: f.offset_y - 0.03 * t, ..f },
            TextEffect::Pulse => {
                let phase = (elapsed.max(0.0) * std::f64::consts::TAU / 1.6).sin() as f32;
                AnimFrame { scale: f.scale * (1.0 + 0.025 * phase), ..f }
            }
        }
    }
}

/// How the text leaves at the end of its duration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextExit {
    /// Gone in one frame (a cut).
    #[default]
    None,
    /// Fades out.
    Fade,
    /// Fades out while sinking (Rise, reversed).
    Sink,
    /// Fades out while shrinking.
    Zoom,
    /// Un-types, last character first (Typewriter, reversed).
    Erase,
}

impl TextExit {
    /// Every exit, in menu order.
    pub const ALL: [TextExit; 5] = [TextExit::None, TextExit::Fade, TextExit::Sink, TextExit::Zoom, TextExit::Erase];

    /// Id used in commands and documents.
    pub fn id(self) -> &'static str {
        match self {
            TextExit::None => "none",
            TextExit::Fade => "fade",
            TextExit::Sink => "sink",
            TextExit::Zoom => "zoom",
            TextExit::Erase => "erase",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            TextExit::None => "Cut",
            TextExit::Fade => "Fade out",
            TextExit::Sink => "Sink",
            TextExit::Zoom => "Zoom out",
            TextExit::Erase => "Erase",
        }
    }

    /// Parse an id.
    pub fn from_id(id: &str) -> Option<TextExit> {
        TextExit::ALL.into_iter().find(|e| e.id() == id)
    }

    /// Length of the exit in seconds.
    pub fn seconds(self) -> f64 {
        match self {
            TextExit::None => 0.0,
            TextExit::Fade | TextExit::Sink => 0.5,
            TextExit::Zoom => 0.4,
            TextExit::Erase => 0.8,
        }
    }

    /// The frame `x` (0–1) of the way through the exit.
    fn frame(self, x: f32) -> AnimFrame {
        let x = x.clamp(0.0, 1.0);
        let e = x * x;
        match self {
            TextExit::None => AnimFrame::REST,
            TextExit::Fade => AnimFrame { alpha: 1.0 - e, ..AnimFrame::REST },
            TextExit::Sink => AnimFrame { alpha: 1.0 - e, offset_y: 0.06 * e, ..AnimFrame::REST },
            TextExit::Zoom => AnimFrame { alpha: 1.0 - e, scale: 1.0 - 0.25 * e, ..AnimFrame::REST },
            TextExit::Erase => AnimFrame { reveal: 1.0 - x, ..AnimFrame::REST },
        }
    }
}

/// Shortest text duration, in milliseconds.
pub const MIN_TEXT_MS: u32 = 500;
/// Longest text duration, in milliseconds (ten minutes; a clip is clamped to its own length).
pub const MAX_TEXT_MS: u32 = 600_000;

/// Where an Overlay sits in the frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextPosition {
    /// Lower third, left aligned.
    #[default]
    LowerThirdLeft,
    /// Lower third, right aligned.
    LowerThirdRight,
    /// Centre of the frame.
    Center,
    /// Top, centred.
    Top,
    /// Bottom, centred.
    Bottom,
}

impl TextPosition {
    /// Every position, in menu order.
    pub const ALL: [TextPosition; 5] = [
        TextPosition::LowerThirdLeft,
        TextPosition::LowerThirdRight,
        TextPosition::Center,
        TextPosition::Top,
        TextPosition::Bottom,
    ];

    /// Id used in commands and documents.
    pub fn id(self) -> &'static str {
        match self {
            TextPosition::LowerThirdLeft => "lowerThirdLeft",
            TextPosition::LowerThirdRight => "lowerThirdRight",
            TextPosition::Center => "center",
            TextPosition::Top => "top",
            TextPosition::Bottom => "bottom",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            TextPosition::LowerThirdLeft => "Lower third · left",
            TextPosition::LowerThirdRight => "Lower third · right",
            TextPosition::Center => "Center",
            TextPosition::Top => "Top",
            TextPosition::Bottom => "Bottom",
        }
    }

    /// Parse an id.
    pub fn from_id(id: &str) -> Option<TextPosition> {
        TextPosition::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Anchor point in the frame as fractions `(x, y)` of its width and height, and the text's
    /// horizontal alignment there (-1 left, 0 centre, 1 right).
    pub fn anchor(self) -> (f32, f32, i8) {
        match self {
            TextPosition::LowerThirdLeft => (0.06, 0.78, -1),
            TextPosition::LowerThirdRight => (0.94, 0.78, 1),
            TextPosition::Center => (0.5, 0.5, 0),
            TextPosition::Top => (0.5, 0.12, 0),
            TextPosition::Bottom => (0.5, 0.88, 0),
        }
    }
}

/// How a clip's text looks: theme, timing, and (for an Overlay) position. The timing fields
/// are the clip's own choices; one left empty follows the theme ([`Look::motion`], themes 1g),
/// so changing the theme changes the moves too. [`TextStyle::timing`] resolves them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextStyle {
    /// Theme.
    #[serde(default)]
    pub theme: TextTheme,
    /// The in; `None` = the theme's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<TextAnimation>,
    /// Overlay position (a Card centres its text).
    #[serde(default)]
    pub position: TextPosition,
    /// How long the text stays up from the clip's start, in milliseconds, its in and out
    /// included: `Some(0)` = until the clip ends; `None` = the theme's. Longer than the clip
    /// counts as the clip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    /// What it does while it is up; `None` = the theme's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<TextEffect>,
    /// How it leaves; `None` = the theme's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<TextExit>,
}

/// A text's moves, resolved: the clip's choices over its theme's ([`TextStyle::timing`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Timing {
    /// The in.
    pub animation: TextAnimation,
    /// Milliseconds up from the clip's start; `None` = until the clip ends.
    pub duration_ms: Option<u32>,
    /// While up.
    pub effect: TextEffect,
    /// The out.
    pub exit: TextExit,
}

impl Timing {
    /// Seconds the text is up in a clip `clip_secs` long: its duration, or the clip.
    pub fn shown_secs(&self, clip_secs: f64) -> f64 {
        let clip = if clip_secs.is_finite() { clip_secs.max(0.0) } else { 0.0 };
        self.duration_ms.map_or(clip, |ms| (f64::from(ms) / 1000.0).min(clip))
    }

    /// The whole life of the text, `elapsed` seconds into a clip `clip_secs` long: the in, the
    /// effect while it is up, the out at the end of its duration, then nothing
    /// ([`AnimFrame::HIDDEN`]). A non-finite `elapsed` is at rest (thumbnails, stills). The out
    /// never starts before the in has finished, so a short duration shortens the out.
    pub fn frame_at(&self, elapsed: f64, clip_secs: f64) -> AnimFrame {
        if !elapsed.is_finite() {
            return AnimFrame::REST;
        }
        let shown = self.shown_secs(clip_secs);
        if elapsed >= shown && (self.duration_ms.is_some() || self.exit != TextExit::None) {
            return AnimFrame::HIDDEN;
        }
        let intro = self.animation.seconds().min(shown);
        let out_len = self.exit.seconds().min((shown - intro).max(0.0));
        let out_start = shown - out_len;
        let base = if out_len > 0.0 && elapsed >= out_start {
            self.exit.frame(((elapsed - out_start) / out_len) as f32)
        } else {
            self.animation.frame(elapsed)
        };
        self.effect.apply(base, elapsed, shown)
    }
}

impl TextStyle {
    /// Whether this is the default style (left out of saved documents).
    pub fn is_default(&self) -> bool {
        *self == TextStyle::default()
    }

    /// The moves this text makes: each of the clip's own choices, or else its theme's.
    pub fn timing(&self) -> Timing {
        let theme = self.theme.look().motion;
        Timing {
            animation: self.animation.unwrap_or(theme.animation),
            duration_ms: match self.duration_ms {
                None => theme.duration_ms,
                Some(0) => None,
                Some(ms) => Some(ms),
            },
            effect: self.effect.unwrap_or(theme.effect),
            exit: self.exit.unwrap_or(theme.exit),
        }
    }

    /// Seconds the text is up in a clip `clip_secs` long ([`Timing::shown_secs`]).
    pub fn shown_secs(&self, clip_secs: f64) -> f64 {
        self.timing().shown_secs(clip_secs)
    }

    /// The text's frame `elapsed` seconds into a clip `clip_secs` long ([`Timing::frame_at`]).
    pub fn frame_at(&self, elapsed: f64, clip_secs: f64) -> AnimFrame {
        self.timing().frame_at(elapsed, clip_secs)
    }
}

/// Why a text edit was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextError {
    /// No such clip.
    NoClip(ClipId),
    /// The clip has no text to switch or style.
    NoText(ClipId),
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextError::NoClip(id) => write!(f, "no clip #{}", id.0),
            TextError::NoText(id) => write!(f, "clip #{} has no text (only title cards and overlays do)", id.0),
        }
    }
}

impl std::error::Error for TextError {}

/// What [`set_text_kind`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindChange {
    /// The kind before.
    pub from: TextKind,
    /// The kind now.
    pub to: TextKind,
    /// Images dropped because a Card keeps at most one (its background).
    pub removed_assets: usize,
}

impl KindChange {
    /// Whether anything changed.
    pub fn changed(&self) -> bool {
        self.from != self.to
    }
}

/// Description given to a shot made from a Card, until the user writes one.
pub const SHOT_UNDER_TITLE: &str = "Shot under the title";

/// Present clip `id`'s text as `kind`, keeping its id, start and length.
pub fn set_text_kind(board: &mut Storyboard, id: ClipId, kind: TextKind) -> Result<KindChange, TextError> {
    let clip = board.clips.iter_mut().find(|c| c.id == id).ok_or(TextError::NoClip(id))?;
    let from = clip.text_kind().ok_or(TextError::NoText(id))?;
    let mut change = KindChange { from, to: kind, removed_assets: 0 };
    match (from, kind) {
        (TextKind::Card, TextKind::Overlay) => {
            clip.overlay = Some(std::mem::take(&mut clip.description));
            clip.description = SHOT_UNDER_TITLE.to_owned();
            clip.shot_type = ShotType::Ws;
        }
        (TextKind::Overlay, TextKind::Card) => {
            clip.description = clip.overlay.take().unwrap_or_default();
            clip.shot_type = ShotType::Ttl;
            // The shot's hero frame stays as the card's background (finding B5); extra angles go.
            change.removed_assets = clip.asset_ids.len().saturating_sub(1);
            clip.asset_ids.truncate(1);
        }
        _ => {}
    }
    Ok(change)
}

/// Change clip `id`'s text style; `None` keeps a field. Returns whether anything changed.
pub fn set_text_style(
    board: &mut Storyboard,
    id: ClipId,
    theme: Option<TextTheme>,
    animation: Option<Option<TextAnimation>>,
    position: Option<TextPosition>,
) -> Result<bool, TextError> {
    let clip = board.clips.iter_mut().find(|c| c.id == id).ok_or(TextError::NoClip(id))?;
    if clip.text_kind().is_none() {
        return Err(TextError::NoText(id));
    }
    let before = clip.text_style;
    let s = &mut clip.text_style;
    s.theme = theme.unwrap_or(s.theme);
    if let Some(a) = animation {
        s.animation = a;
    }
    s.position = position.unwrap_or(s.position);
    Ok(clip.text_style != before)
}

/// Clip `id`'s text timing. Each part: `None` leaves it as it is, `Some(None)` follows the
/// theme again, `Some(Some(v))` sets the clip's own. `duration` in ms, `0` = until the clip
/// ends. Returns whether anything changed.
pub fn set_text_timing(
    board: &mut Storyboard,
    id: ClipId,
    duration: Option<Option<u32>>,
    effect: Option<Option<TextEffect>>,
    exit: Option<Option<TextExit>>,
) -> Result<bool, TextError> {
    let clip = board.clips.iter_mut().find(|c| c.id == id).ok_or(TextError::NoClip(id))?;
    if clip.text_kind().is_none() {
        return Err(TextError::NoText(id));
    }
    let before = clip.text_style;
    let s = &mut clip.text_style;
    if let Some(d) = duration {
        s.duration_ms = d.map(|ms| if ms == 0 { 0 } else { ms.clamp(MIN_TEXT_MS, MAX_TEXT_MS) });
    }
    if let Some(e) = effect {
        s.effect = e;
    }
    if let Some(e) = exit {
        s.exit = e;
    }
    Ok(clip.text_style != before)
}
