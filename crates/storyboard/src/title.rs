//! A text clip's title as layers (L1; Paul, 2026-11-20: "an object, and the object had layers
//! like: text, background, effect, renderer"). [`title`] resolves the clip's style over its
//! theme into one model for the export, the Viewer, the Inspector and the CLI: the area it sits
//! in, a background (cards), a plate (overlays), the text, and the effects that move them, each
//! drawn by AssetCraft or by EffectCraft ([`Title::renderer`]).
//!
//! What's saved stays the clip's [`TextStyle`]: only what the clip changes from its theme. The
//! title is worked out from it, so a theme change re-styles every title that follows it.
//!
//! L1 builds the model; nothing draws from it yet (L2 the export, L3 the Viewer).

use serde::Serialize;

use crate::model::Clip;
use crate::text::{
    DesignPick, Look, TextAnimation, TextBox, TextEffect, TextExit, TextKind, TextPosition, ThemeId, Timing, UserLooks,
};

/// A text clip's title, resolved: its layers bottom to top, and the effects that move them.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Title {
    /// A card (a clip of its own) or an overlay (over the clip's shot).
    pub kind: TextKind,
    /// Where it sits and its size; `None` = where it falls by itself.
    pub area: Option<TextBox>,
    /// Under everything: a card's backdrop. Overlays have none (their shot shows).
    pub background: Option<BackgroundLayer>,
    /// The panel behind an overlay's words, if its theme has one.
    pub plate: Option<PlateLayer>,
    /// The words.
    pub text: TextLayer,
    /// What moves, in order. Empty: it stands still for the whole clip.
    pub effects: Vec<EffectLayer>,
    /// The theme's look the layers draw with (colours, type, decor, finish).
    #[serde(skip)]
    pub look: Look,
}

/// A card's background.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum BackgroundLayer {
    /// The theme's backdrop (its colour stops, grain, vignette) and its card decor.
    Theme,
    /// The clip's own picture under the theme's background colour at `alpha`, so it shows
    /// through.
    Scrim {
        /// How much of the theme's colour lies over the picture, 0–1.
        alpha: f32,
    },
}

/// An overlay's plate: a panel of the theme's colour behind the words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlateLayer {
    /// `0xRRGGBB`.
    pub color: u32,
    /// 0–255.
    pub alpha: u8,
}

/// The words.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextLayer {
    /// Where they sit when the title has no area of its own: a card centres them.
    pub position: TextPosition,
    /// Their height, a fraction of the frame's.
    pub size: f32,
    /// Shown in capitals.
    pub uppercase: bool,
    /// An effect draws them (a design takes the words as its Headline and Caption), not the
    /// text layer itself.
    pub by_effect: bool,
}

/// What an effect moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    /// The whole title.
    Whole,
    /// The words (an overlay's plate and the words' decor go with them).
    Text,
    /// The plate.
    Plate,
    /// The background.
    Background,
}

/// What an effect steps through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Per {
    /// All of it at once.
    Whole,
    /// A line at a time.
    Line,
    /// A word at a time.
    Word,
    /// A letter at a time.
    Letter,
}

/// A built-in effect EffectCraft draws: made in code, filled with the clip's words and theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BuiltEffect {
    /// Beams trace the banner's edge, the words decode.
    LaserTrace,
    /// The banner drops in as pieces, then folds out row by row.
    Stack,
}

impl BuiltEffect {
    /// Its design id (`banner-laser-trace`, `banner-stack`).
    pub fn id(self) -> &'static str {
        match self {
            BuiltEffect::LaserTrace => DesignPick::LaserTrace.id(),
            BuiltEffect::Stack => DesignPick::Stack.id(),
        }
    }
}

/// What an effect is and who draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum EffectKind {
    /// AssetCraft's moves: the in, what it does while up, the out (`timing`).
    InApp {
        /// The in.
        animation: TextAnimation,
        /// While up.
        effect: TextEffect,
        /// The out.
        exit: TextExit,
    },
    /// A built-in EffectCraft effect.
    Built {
        /// Which.
        effect: BuiltEffect,
    },
    /// A theme pack's own design for the title's role, made in EffectCraft.
    Design {
        /// The pack's theme.
        theme: ThemeId,
        /// `card` or `banner`.
        role: &'static str,
    },
}

/// One effect: what it moves, a piece at a time, what it is, and how long the title is up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectLayer {
    /// What it moves.
    pub target: Target,
    /// What it steps through.
    pub per: Per,
    /// What it is.
    pub kind: EffectKind,
    /// How long the title is up, in milliseconds from the clip's start; `None` = the clip.
    pub duration_ms: Option<u32>,
}

impl EffectLayer {
    /// Who draws it.
    pub fn renderer(&self) -> Renderer {
        match self.kind {
            EffectKind::InApp { .. } => Renderer::InApp,
            EffectKind::Built { .. } | EffectKind::Design { .. } => Renderer::EffectCraft,
        }
    }
}

/// Who draws a title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Renderer {
    /// AssetCraft's renderer: quick, drawn live.
    InApp,
    /// EffectCraft's, in-process: built-in and pack designs.
    EffectCraft,
}

impl Title {
    /// Who draws it: EffectCraft when any effect is one of its designs, else AssetCraft.
    pub fn renderer(&self) -> Renderer {
        if self.effects.iter().any(|e| e.renderer() == Renderer::EffectCraft) {
            Renderer::EffectCraft
        } else {
            Renderer::InApp
        }
    }
}

/// The design role a text kind draws through.
pub fn design_role(kind: TextKind) -> &'static str {
    match kind {
        TextKind::Card => "card",
        TextKind::Overlay => "banner",
    }
}

/// How much of a card's theme colour lies over its own picture.
pub const SCRIM_ALPHA: f32 = 0.35;

/// `clip`'s title, if it has text: its style over its theme (`looks` for user themes).
/// `can_draw` says whether a design effect (a built-in, or a pack's design for a role) is
/// loaded; one that isn't leaves the theme's words, as the renderers do.
pub fn title(clip: &Clip, looks: &UserLooks, can_draw: impl Fn(&EffectKind) -> bool) -> Option<Title> {
    let kind = clip.text_kind()?;
    let style = clip.text_style;
    let look = style.look_with(looks);
    let timing: Timing = style.timing_with(looks);
    let role = design_role(kind);
    // What draws it: the clip's pick; else its theme pack's own design for the role.
    let design = match style.design {
        Some(DesignPick::None) => None,
        Some(DesignPick::LaserTrace) => Some(EffectKind::Built { effect: BuiltEffect::LaserTrace }),
        Some(DesignPick::Stack) => Some(EffectKind::Built { effect: BuiltEffect::Stack }),
        None => style.theme_id.map(|theme| EffectKind::Design { theme, role }),
    }
    .filter(|d| can_draw(d));
    let background = (kind == TextKind::Card).then(|| {
        if clip.background().is_some() { BackgroundLayer::Scrim { alpha: SCRIM_ALPHA } } else { BackgroundLayer::Theme }
    });
    // A design brings its own panel; the theme's plate is for its words.
    let plate = match (kind, &design) {
        (TextKind::Overlay, None) => look.plate.map(|(color, alpha)| PlateLayer { color, alpha }),
        _ => None,
    };
    let text = TextLayer {
        position: if kind == TextKind::Card { TextPosition::Center } else { style.position },
        size: if kind == TextKind::Card { look.card_size } else { look.overlay_size },
        uppercase: look.uppercase,
        by_effect: design.is_some(),
    };
    let effects = match design {
        // A design brings its own moves (R-T7); the clip's duration still sets how long it's up.
        Some(kind) => {
            vec![EffectLayer { target: Target::Whole, per: Per::Whole, kind, duration_ms: timing.duration_ms }]
        }
        None => {
            let moves = timing.animation != TextAnimation::None
                || timing.effect != TextEffect::None
                || timing.exit != TextExit::None
                || timing.duration_ms.is_some();
            if moves {
                let per = if timing.animation == TextAnimation::Typewriter { Per::Letter } else { Per::Whole };
                vec![EffectLayer {
                    target: Target::Text,
                    per,
                    kind: EffectKind::InApp { animation: timing.animation, effect: timing.effect, exit: timing.exit },
                    duration_ms: timing.duration_ms,
                }]
            } else {
                Vec::new()
            }
        }
    };
    Some(Title { kind, area: style.area, background, plate, text, effects, look })
}
