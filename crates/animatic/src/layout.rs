//! A title as layers with their places, as this compositor draws it (FC-1; D-SF8: Send to
//! FilmCraft builds each in-app title as a FilmCraft graphic clip from these numbers, so what
//! FilmCraft sets is what the Viewer and the export show — GC-8 to GC-10 in
//! `docs/sibling-apis/filmcraft-graphic-clips.md`).
//!
//! [`Compositor::title_layout`] takes the same steps as `paint_text` at rest (the words' face,
//! their fitted size, the lines they break into, the plate, the decor, the finish, a card's
//! background) and hands them back instead of drawing them. A title an EffectCraft design draws
//! has no layout here: it goes as its render (SF1b-1).

use crate::text::{SetLine, SetMetrics};
use crate::{Compositor, Renderer, Title, shown_text, title_of};
use assetcraft_storyboard::{AnimFrame, Backdrop, BackgroundLayer, Clip, FinishPass, TextFinish, TextKind};

/// A rectangle and its fill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRect {
    /// Left, px.
    pub x: f32,
    /// Top, px.
    pub y: f32,
    /// Width, px.
    pub w: f32,
    /// Height, px.
    pub h: f32,
    /// `0xRRGGBB`.
    pub color: u32,
    /// 0–1.
    pub alpha: f32,
}

/// A card's background.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutBackground {
    /// The theme's backdrop (stops, angle, grain) and its card decor.
    Backdrop {
        /// The backdrop.
        backdrop: Backdrop,
        /// Its steady decor: pattern, frame, corner marks.
        decor: Vec<LayoutRect>,
    },
    /// The clip's own picture under the theme's colour.
    Scrim {
        /// The colour.
        color: u32,
        /// How much of it, 0–1.
        alpha: f32,
    },
}

/// The words as set.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutWords {
    /// The face's family ("Inter", or a pack's display font).
    pub family: String,
    /// Text height, px (ascent − descent: what this compositor sizes by).
    pub height: f32,
    /// The face's metrics there (its em size is what an editor calls the font size).
    pub metrics: SetMetrics,
    /// Extra space after every glyph, px.
    pub tracking: f32,
    /// Each line: its words as typed (not upper-cased), as drawn, its left edge, width and
    /// baseline.
    pub lines: Vec<LayoutLine>,
    /// `-1` left, `0` centred, `1` right.
    pub align: i8,
    /// `0xRRGGBB`.
    pub color: u32,
    /// Drawn in capitals.
    pub uppercase: bool,
    /// Shadow, outline or glow, and the copies that make it.
    pub finish: TextFinish,
    /// The finish's copies under the words.
    pub passes: Vec<FinishPass>,
    /// The block the lines fill, `[left, top, width, height]`.
    pub block: [f32; 4],
}

/// One line of words.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutLine {
    /// Its words as typed.
    pub typed: String,
    /// Its words as drawn.
    pub drawn: String,
    /// Left edge, px.
    pub left: f32,
    /// Width, px.
    pub width: f32,
    /// Baseline, px.
    pub baseline: f32,
}

/// An in-app title at rest, as layers in a `width × height` frame, bottom to top.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleLayout {
    /// Frame width.
    pub width: u32,
    /// Frame height.
    pub height: u32,
    /// Card or overlay.
    pub kind: TextKind,
    /// A card's background.
    pub background: Option<LayoutBackground>,
    /// An overlay's plate.
    pub plate: Option<LayoutRect>,
    /// The decor that moves with the words (a card's rules, an accent bar).
    pub decor: Vec<LayoutRect>,
    /// The words.
    pub words: LayoutWords,
}

fn rect(r: &assetcraft_storyboard::DecorRect) -> LayoutRect {
    LayoutRect { x: r.x, y: r.y, w: r.w, h: r.h, color: r.color, alpha: r.alpha }
}

impl Compositor {
    /// `clip`'s title at rest as layers (FC-1); `None` for a clip without words, one an
    /// EffectCraft design draws, or words that are empty.
    pub fn title_layout(&mut self, clip: &Clip) -> Option<TitleLayout> {
        let words = clip.text()?;
        if words.trim().is_empty() {
            return None;
        }
        let title: Title = title_of(&self.designs, &self.user_looks, clip)?;
        if title.renderer() == Renderer::EffectCraft {
            return None;
        }
        let fonts = self.fonts_for(&clip.text_style);
        let rest = AnimFrame::REST;
        let l = match title.area {
            Some(area) => self.words_in(&title, area, words, &rest, &fonts),
            None => self.words_placed(&title, words, &rest, &fonts),
        };
        let look = &title.look;
        let (fw, fh) = (self.width as f32, self.height as f32);
        let background = title.background.map(|bg| match bg {
            BackgroundLayer::Theme => LayoutBackground::Backdrop {
                backdrop: look.backdrop,
                decor: assetcraft_storyboard::card_decor(look, fw, fh).iter().map(rect).collect(),
            },
            BackgroundLayer::Scrim { alpha } => LayoutBackground::Scrim { color: look.background, alpha },
        });
        let plate = match (title.kind, title.plate) {
            (TextKind::Overlay, Some(p)) => Some(LayoutRect {
                x: l.plate[0],
                y: l.plate[1],
                w: l.plate[2],
                h: l.plate[3],
                color: p.color,
                alpha: f32::from(p.alpha) / 255.0,
            }),
            _ => None,
        };
        let pad = [l.height * 0.45, l.height * 0.25];
        let decor = assetcraft_storyboard::text_decor(look, title.kind, l.at, fh, pad).iter().map(rect).collect();
        // An overlay without a plate gets the words' drop shadow from paint_words: a pass too.
        let mut passes = assetcraft_storyboard::finish_passes(look, l.height);
        if title.kind == TextKind::Overlay && title.plate.is_none() {
            passes.insert(0, FinishPass { dx: 1.0, dy: 1.5, color: 0x000000, alpha: 0.63 });
        }
        let drawn = shown_text(words, title.text.uppercase, &rest);
        let set: Vec<SetLine> = fonts.set_lines(&drawn, l.height, l.tracking, l.wrap, l.at[0], l.at[1], l.align);
        // The words as typed, broken where the drawn ones are (capitals change no breaks).
        let mut typed_words = words.split('\n').flat_map(|p| p.split(' ')).map(str::to_owned);
        let lines = set
            .into_iter()
            .map(|line| {
                let typed: Vec<String> = typed_words.by_ref().take(line.words.len()).collect();
                LayoutLine {
                    typed: typed.join(" "),
                    drawn: line.words.join(" "),
                    left: line.left,
                    width: line.width,
                    baseline: line.baseline,
                }
            })
            .collect();
        let packed = |c: [u8; 3]| (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
        Some(TitleLayout {
            width: self.width,
            height: self.height,
            kind: title.kind,
            background,
            plate,
            decor,
            words: LayoutWords {
                family: fonts.family().to_owned(),
                height: l.height,
                metrics: fonts.metrics(l.height),
                tracking: l.tracking,
                lines,
                align: l.align,
                color: packed(l.color),
                uppercase: title.text.uppercase,
                finish: look.finish,
                passes,
                block: l.at,
            },
        })
    }
}
