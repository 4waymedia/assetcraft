//! Theme decor (themes 1e, `docs/design/themes.md`): the shapes a theme draws besides its
//! background and words — rules around a card's title, corner marks, a frame, and a pattern
//! (grid, dots, scanlines) on the card. Paul, 2026-11-10: "I want to create some themes that
//! are more than just a solid color and font size".
//!
//! Everything comes out as plain rectangles ([`DecorRect`]) in pixels, so the export and the
//! Viewer draw exactly the same shapes from one function each: [`card_decor`] (behind the
//! words, steady) and [`text_decor`] (with the words: it fades in and out with them).

use serde::{Deserialize, Serialize};

use crate::text::{Look, TextKind};

/// Lines around a card's title.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rules {
    /// None (the accent bar, if the theme has one, sits under the title instead).
    #[default]
    None,
    /// One line under the title.
    Below,
    /// One line above, one below.
    Both,
    /// Two thin lines under the title.
    Double,
}

/// A pattern over the card's background, in the text colour, faint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Pattern {
    /// Plain.
    #[default]
    None,
    /// Squares, like drafting paper.
    Grid,
    /// A dot grid.
    Dots,
    /// Fine horizontal lines, like an old screen.
    Scanlines,
}

/// The decor a [`Look`] draws. `color` is the theme's accent (rules, corners, frame).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LookDecor {
    /// Lines around a card's title.
    pub rules: Rules,
    /// L-shaped marks in the card's four corners.
    pub corners: bool,
    /// A thin border inset from the card's edge.
    pub frame: bool,
    /// A pattern on the card.
    pub pattern: Pattern,
    /// Rules, corners and frame colour, `0xRRGGBB`.
    pub color: u32,
}

impl LookDecor {
    /// No decor.
    pub const NONE: LookDecor =
        LookDecor { rules: Rules::None, corners: false, frame: false, pattern: Pattern::None, color: 0 };
}

/// One shape to draw: a filled rectangle in pixels, its colour and opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecorRect {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
    /// `0xRRGGBB`.
    pub color: u32,
    /// 0–1.
    pub alpha: f32,
}

/// Most pattern shapes on one card (a bound for a huge frame).
pub const MAX_PATTERN_SHAPES: usize = 4096;

/// A card's steady decor in a `w`×`h` frame: the pattern, the frame, the corner marks.
/// Drawn right after the backdrop, before the words.
pub fn card_decor(look: &Look, w: f32, h: f32) -> Vec<DecorRect> {
    let mut out = Vec::new();
    if !(w.is_finite() && h.is_finite()) || w < 8.0 || h < 8.0 {
        return out;
    }
    let d = look.decor;
    let ink = look.text;
    let hair = (h / 540.0).max(1.0);
    match d.pattern {
        Pattern::None => {}
        Pattern::Grid => {
            let step = h / 10.0;
            let mut x = (w / 2.0) % step;
            while x < w && out.len() < MAX_PATTERN_SHAPES {
                out.push(DecorRect { x: x - hair / 2.0, y: 0.0, w: hair, h, color: ink, alpha: 0.10 });
                x += step;
            }
            let mut y = (h / 2.0) % step;
            while y < h && out.len() < MAX_PATTERN_SHAPES {
                out.push(DecorRect { x: 0.0, y: y - hair / 2.0, w, h: hair, color: ink, alpha: 0.10 });
                y += step;
            }
        }
        Pattern::Dots => {
            let step = h / 24.0;
            let size = (h / 360.0).max(2.0);
            let mut y = step / 2.0;
            'rows: while y < h {
                let mut x = step / 2.0;
                while x < w {
                    if out.len() >= MAX_PATTERN_SHAPES {
                        break 'rows;
                    }
                    out.push(DecorRect {
                        x: x - size / 2.0,
                        y: y - size / 2.0,
                        w: size,
                        h: size,
                        color: ink,
                        alpha: 0.14,
                    });
                    x += step;
                }
                y += step;
            }
        }
        Pattern::Scanlines => {
            let step = (h / 180.0).max(3.0);
            let mut y = 0.0;
            while y < h && out.len() < MAX_PATTERN_SHAPES {
                out.push(DecorRect { x: 0.0, y, w, h: hair, color: ink, alpha: 0.07 });
                y += step;
            }
        }
    }
    let line = (h * 0.004).max(2.0);
    if d.frame {
        let inset = h * 0.04;
        let (x0, y0, x1, y1) = (inset, inset, w - inset, h - inset);
        let thin = (h * 0.0025).max(1.0);
        for r in [
            [x0, y0, x1 - x0, thin],
            [x0, y1 - thin, x1 - x0, thin],
            [x0, y0, thin, y1 - y0],
            [x1 - thin, y0, thin, y1 - y0],
        ] {
            out.push(DecorRect { x: r[0], y: r[1], w: r[2], h: r[3], color: d.color, alpha: 0.85 });
        }
    }
    if d.corners {
        let inset = h * if d.frame { 0.065 } else { 0.05 };
        let arm = h * 0.05;
        for (cx, cy, sx, sy) in [
            (inset, inset, 1.0, 1.0),
            (w - inset, inset, -1.0, 1.0),
            (inset, h - inset, 1.0, -1.0),
            (w - inset, h - inset, -1.0, -1.0),
        ] {
            // The horizontal arm, then the vertical one, both growing inward from the corner.
            let hx = if sx > 0.0 { cx } else { cx - arm };
            let hy = if sy > 0.0 { cy } else { cy - line };
            out.push(DecorRect { x: hx, y: hy, w: arm, h: line, color: d.color, alpha: 1.0 });
            let vx = if sx > 0.0 { cx } else { cx - line };
            let vy = if sy > 0.0 { cy } else { cy - arm };
            out.push(DecorRect { x: vx, y: vy, w: line, h: arm, color: d.color, alpha: 1.0 });
        }
    }
    out
}

/// The decor that moves with the words, for text whose block is `block` (`[left, top, width,
/// height]`) in a frame `frame_h` tall: a card's rules (or its accent bar when it has no
/// rules), an overlay's accent bar. `pad` is the overlay plate's padding.
pub fn text_decor(look: &Look, kind: TextKind, block: [f32; 4], frame_h: f32, pad: [f32; 2]) -> Vec<DecorRect> {
    let mut out = Vec::new();
    let [left, top, bw, bh] = block;
    let rule = (frame_h * 0.003).max(1.0);
    let gap = frame_h * 0.03;
    let wide = bw + frame_h * 0.08;
    let x = left + bw / 2.0 - wide / 2.0;
    let color = look.decor.color;
    let rules = if kind == TextKind::Card { look.decor.rules } else { Rules::None };
    match rules {
        Rules::None => {
            if let Some(accent) = look.accent {
                let [x, y, w, h] = crate::text::accent_bar(kind, block, frame_h, pad, look.plate.is_some());
                out.push(DecorRect { x, y, w, h, color: accent, alpha: 1.0 });
            }
        }
        Rules::Below => out.push(DecorRect { x, y: top + bh + gap, w: wide, h: rule, color, alpha: 1.0 }),
        Rules::Both => {
            out.push(DecorRect { x, y: top - gap - rule, w: wide, h: rule, color, alpha: 1.0 });
            out.push(DecorRect { x, y: top + bh + gap, w: wide, h: rule, color, alpha: 1.0 });
        }
        Rules::Double => {
            out.push(DecorRect { x, y: top + bh + gap, w: wide, h: rule, color, alpha: 1.0 });
            out.push(DecorRect { x, y: top + bh + gap + rule * 3.0, w: wide, h: rule, color, alpha: 1.0 });
        }
    }
    out
}

/// How a theme finishes its letters (themes 1f): drawn as extra passes of the same words
/// under them ([`finish_passes`]), so the export and the Viewer match.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextFinish {
    /// Plain letters.
    #[default]
    None,
    /// A soft drop shadow, down and right.
    Shadow,
    /// A thin edge in the opposite tone (dark round light text, light round dark).
    Outline,
    /// A halo in the accent colour (neon).
    Glow,
}

/// One extra pass of the words: drawn `dx`, `dy` pixels off, in `color` at `alpha`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FinishPass {
    /// Offset right, px.
    pub dx: f32,
    /// Offset down, px.
    pub dy: f32,
    /// `0xRRGGBB`.
    pub color: u32,
    /// 0–1, times the words' own alpha.
    pub alpha: f32,
}

/// The passes to draw before the words for `look`'s finish, for text `height` px tall.
pub fn finish_passes(look: &Look, height: f32) -> Vec<FinishPass> {
    let mut out = Vec::new();
    if !height.is_finite() || height <= 0.0 {
        return out;
    }
    let ring = |out: &mut Vec<FinishPass>, r: f32, steps: u32, color: u32, alpha: f32| {
        for i in 0..steps {
            let a = i as f32 / steps as f32 * std::f32::consts::TAU;
            out.push(FinishPass { dx: r * a.cos(), dy: r * a.sin(), color, alpha });
        }
    };
    match look.finish {
        TextFinish::None => {}
        TextFinish::Shadow => {
            let d = (height * 0.05).max(1.0);
            out.push(FinishPass { dx: d * 1.8, dy: d * 1.8, color: 0x000000, alpha: 0.22 });
            out.push(FinishPass { dx: d, dy: d, color: 0x000000, alpha: 0.5 });
        }
        TextFinish::Outline => {
            let r = (height * 0.035).max(1.0);
            let lum = crate::theme::luminance(look.text);
            let edge = if lum > 0.4 { 0x000000 } else { 0xffffff };
            ring(&mut out, r, 12, edge, 1.0);
        }
        TextFinish::Glow => {
            let c = look.decor.color;
            ring(&mut out, (height * 0.12).max(3.0), 16, c, 0.05);
            ring(&mut out, (height * 0.07).max(2.0), 12, c, 0.09);
            ring(&mut out, (height * 0.03).max(1.0), 8, c, 0.22);
        }
    }
    out
}
