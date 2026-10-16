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
}

impl TextTheme {
    /// Every theme, in menu order.
    pub const ALL: [TextTheme; 4] = [TextTheme::Bold, TextTheme::Minimal, TextTheme::Cinematic, TextTheme::Paper];

    /// Id used in commands and documents.
    pub fn id(self) -> &'static str {
        match self {
            TextTheme::Bold => "bold",
            TextTheme::Minimal => "minimal",
            TextTheme::Cinematic => "cinematic",
            TextTheme::Paper => "paper",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            TextTheme::Bold => "Bold",
            TextTheme::Minimal => "Minimal",
            TextTheme::Cinematic => "Cinematic",
            TextTheme::Paper => "Paper",
        }
    }

    /// Parse an id.
    pub fn from_id(id: &str) -> Option<TextTheme> {
        TextTheme::ALL.into_iter().find(|t| t.id() == id)
    }

    /// What the theme paints.
    pub fn look(self) -> Look {
        match self {
            TextTheme::Bold => Look {
                background: 0xe5243b,
                text: 0xffffff,
                plate: Some((0xe5243b, 235)),
                card_size: 0.11,
                overlay_size: 0.06,
                tracking: 0.0,
                uppercase: true,
            },
            TextTheme::Minimal => Look {
                background: 0x0f1217,
                text: 0xe9ebee,
                plate: None,
                card_size: 0.075,
                overlay_size: 0.05,
                tracking: 0.02,
                uppercase: false,
            },
            TextTheme::Cinematic => Look {
                background: 0x000000,
                text: 0xf2e6c9,
                plate: Some((0x000000, 150)),
                card_size: 0.07,
                overlay_size: 0.045,
                tracking: 0.35,
                uppercase: true,
            },
            TextTheme::Paper => Look {
                background: 0xf3efe6,
                text: 0x1d1d1f,
                plate: Some((0xf3efe6, 230)),
                card_size: 0.085,
                overlay_size: 0.05,
                tracking: 0.0,
                uppercase: false,
            },
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

/// How a clip's text looks: theme, animation, and (for an Overlay) position.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextStyle {
    /// Theme.
    #[serde(default)]
    pub theme: TextTheme,
    /// Intro animation.
    #[serde(default)]
    pub animation: TextAnimation,
    /// Overlay position (a Card centres its text).
    #[serde(default)]
    pub position: TextPosition,
}

impl TextStyle {
    /// Whether this is the default style (left out of saved documents).
    pub fn is_default(&self) -> bool {
        *self == TextStyle::default()
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
    /// Images dropped because a Card holds text only.
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
            change.removed_assets = clip.asset_ids.len();
            clip.asset_ids.clear();
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
    animation: Option<TextAnimation>,
    position: Option<TextPosition>,
) -> Result<bool, TextError> {
    let clip = board.clips.iter_mut().find(|c| c.id == id).ok_or(TextError::NoClip(id))?;
    if clip.text_kind().is_none() {
        return Err(TextError::NoText(id));
    }
    let before = clip.text_style;
    let s = &mut clip.text_style;
    s.theme = theme.unwrap_or(s.theme);
    s.animation = animation.unwrap_or(s.animation);
    s.position = position.unwrap_or(s.position);
    Ok(clip.text_style != before)
}
