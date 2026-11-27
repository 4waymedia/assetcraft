//! Text in a frame: the bundled Inter Regular (the UI's font, `assets/fonts/`, SIL OFL 1.1), or
//! a theme pack's display font (theme-packs.md TP4b), set with `ab_glyph` — word wrap at a width, extra letter spacing, lines centred or
//! left/right aligned as the Viewer's `text_view` does with egui's layout. Not a text engine:
//! no shaping, no bidi; a glyph the font lacks draws its `.notdef` box, as egui would show a
//! fallback.

use crate::draw::Rgba;
use ab_glyph::{Font, FontArc, FontVec, Glyph, GlyphId, PxScale, ScaleFont};

const INTER: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");

/// Line height as a multiple of the text height (egui's row height for Inter is close to this).
const LINE: f32 = 1.2;

/// The font a frame's words are set in (cheap to clone: the face is shared).
#[derive(Clone)]
pub struct Fonts {
    regular: FontArc,
    name: String,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fonts({})", self.name)
    }
}

/// One laid-out line: glyphs with their x positions, the line's width, and its words.
struct Line {
    glyphs: Vec<(GlyphId, f32)>,
    width: f32,
    words: Vec<String>,
}

/// A line of set text as drawn (FC-1): its words, where it starts and its width, its baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct SetLine {
    /// Its words, as drawn (capitals applied).
    pub words: Vec<String>,
    /// Its left edge, px.
    pub left: f32,
    /// Its width, px.
    pub width: f32,
    /// Its baseline, px from the top.
    pub baseline: f32,
}

/// A face's metrics at a text height (FC-1): what another renderer needs to set the same text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetMetrics {
    /// The em size in px that gives this text height (`height` is ascent − descent).
    pub em: f32,
    /// Ascent above the baseline, px.
    pub ascent: f32,
    /// Descent below it, px (positive).
    pub descent: f32,
    /// The face's own line gap, px.
    pub line_gap: f32,
    /// Baseline to baseline here, px.
    pub line_height: f32,
}

impl Fonts {
    /// The bundled font; `None` only if the embedded bytes are not a font (a build problem).
    pub fn bundled() -> Option<Fonts> {
        ab_glyph::FontRef::try_from_slice(INTER).ok().map(|f| Fonts { regular: FontArc::new(f), name: "Inter".into() })
    }

    /// Face `index` of a font file (a pack's display font, TP4b), named `name`; `None` when the
    /// bytes aren't a font.
    pub fn from_bytes(name: &str, bytes: Vec<u8>, index: u32) -> Option<Fonts> {
        FontVec::try_from_vec_and_index(bytes, index)
            .ok()
            .map(|f| Fonts { regular: FontArc::new(f), name: name.into() })
    }

    fn scaled(&self, height: f32) -> ab_glyph::PxScaleFont<&FontArc> {
        // `height` is the text height in pixels the Viewer asks for; egui sizes a font by its
        // point size, which for Inter is close to the pixel scale.
        self.regular.as_scaled(PxScale::from(height.max(1.0)))
    }

    /// Lay `text` out at `height` with `tracking` extra pixels per glyph, wrapping at `wrap`.
    fn layout(&self, text: &str, height: f32, tracking: f32, wrap: f32) -> Vec<Line> {
        let font = self.scaled(height);
        let space = font.h_advance(font.glyph_id(' ')) + tracking;
        let mut lines: Vec<Line> = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = Line { glyphs: Vec::new(), width: 0.0, words: Vec::new() };
            let mut x = 0.0f32;
            for word in paragraph.split(' ') {
                let glyphs: Vec<(GlyphId, f32)> = word.chars().map(|c| (font.glyph_id(c), 0.0)).collect();
                let word_w: f32 =
                    glyphs.iter().map(|(g, _)| font.h_advance(*g) + tracking).sum::<f32>() - tracking.min(0.0);
                let needs_space = !line.glyphs.is_empty();
                let start = if needs_space { x + space } else { x };
                if needs_space && start + word_w > wrap.max(10.0) {
                    line.width = x;
                    lines
                        .push(std::mem::replace(&mut line, Line { glyphs: Vec::new(), width: 0.0, words: Vec::new() }));
                    x = 0.0;
                } else {
                    x = start;
                }
                line.words.push(word.to_owned());
                for (g, _) in glyphs {
                    line.glyphs.push((g, x));
                    x += font.h_advance(g) + tracking;
                }
            }
            line.width = (x - tracking).max(0.0);
            lines.push(line);
        }
        lines
    }

    /// The family's name (the bundled face's is "Inter").
    pub fn family(&self) -> &str {
        &self.name
    }

    /// The face's metrics at text height `height` (FC-1).
    pub fn metrics(&self, height: f32) -> SetMetrics {
        let font = self.scaled(height);
        let upem = self.regular.units_per_em().unwrap_or(1000.0).max(1.0);
        let unscaled = self.regular.height_unscaled().max(1.0);
        SetMetrics {
            em: height.max(1.0) * upem / unscaled,
            ascent: font.ascent(),
            descent: -font.descent(),
            line_gap: font.line_gap(),
            line_height: height * LINE,
        }
    }

    /// `text` set as [`draw_aligned`](Self::draw_aligned) sets it with its block's top-left at
    /// `(left, top)`: each line's words, left edge, width and baseline (FC-1: what another
    /// renderer is told, so it sets the same lines in the same places).
    #[allow(clippy::too_many_arguments)]
    pub fn set_lines(
        &self,
        text: &str,
        height: f32,
        tracking: f32,
        wrap: f32,
        left: f32,
        top: f32,
        align: i8,
    ) -> Vec<SetLine> {
        let lines = self.layout(text, height, tracking, wrap);
        let block_w = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        let font = self.scaled(height);
        lines
            .into_iter()
            .enumerate()
            .map(|(i, line)| SetLine {
                left: left
                    + match align {
                        ..=-1 => 0.0,
                        0 => (block_w - line.width) / 2.0,
                        1.. => block_w - line.width,
                    },
                baseline: top
                    + i as f32 * height * LINE
                    + (height * LINE - (font.ascent() - font.descent())) / 2.0
                    + font.ascent(),
                width: line.width,
                words: line.words,
            })
            .collect()
    }

    /// The block size `[w, h]` the text takes.
    pub fn measure(&self, text: &str, height: f32, tracking: f32, wrap: f32) -> [f32; 2] {
        let lines = self.layout(text, height, tracking, wrap);
        let w = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        [w, lines.len() as f32 * height * LINE]
    }

    /// Draw `text` with its block's top-left at `(left, top)`; each line centred inside the
    /// block (the Viewer centres wrapped lines too), colour at `alpha`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        out: &mut Rgba,
        text: &str,
        height: f32,
        tracking: f32,
        wrap: f32,
        left: f32,
        top: f32,
        rgb: [u8; 3],
        alpha: f32,
    ) {
        self.draw_aligned(out, text, height, tracking, wrap, left, top, rgb, alpha, 0);
    }

    /// [`draw`](Self::draw) with each line aligned in the block: `-1` left, `0` centred, `1`
    /// right (a title's area, L4).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_aligned(
        &self,
        out: &mut Rgba,
        text: &str,
        height: f32,
        tracking: f32,
        wrap: f32,
        left: f32,
        top: f32,
        rgb: [u8; 3],
        alpha: f32,
        align: i8,
    ) {
        if alpha <= 0.0 {
            return;
        }
        let lines = self.layout(text, height, tracking, wrap);
        let block_w = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        let font = self.scaled(height);
        let ascent = font.ascent();
        for (i, line) in lines.iter().enumerate() {
            let line_left = left
                + match align {
                    ..=-1 => 0.0,
                    0 => (block_w - line.width) / 2.0,
                    1.. => block_w - line.width,
                };
            let baseline =
                top + i as f32 * height * LINE + (height * LINE - (font.ascent() - font.descent())) / 2.0 + ascent;
            for (id, x) in &line.glyphs {
                let glyph: Glyph = id.with_scale_and_position(font.scale(), ab_glyph::point(line_left + x, baseline));
                let Some(outlined) = font.outline_glyph(glyph) else { continue };
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, cov| {
                    let px = bounds.min.x + gx as f32;
                    let py = bounds.min.y + gy as f32;
                    if px >= 0.0 && py >= 0.0 {
                        blend(out, px as u32, py as u32, rgb, cov * alpha);
                    }
                });
            }
        }
    }

    /// Draw `text` centred on `(ax, ay)` as fractions of the frame (the empty clip's words).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_centered(
        &self,
        out: &mut Rgba,
        text: &str,
        height: f32,
        rgb: [u8; 3],
        alpha: f32,
        ax: f32,
        ay: f32,
        wrap: f32,
    ) {
        let block = self.measure(text, height, 0.0, wrap);
        let left = out.width as f32 * ax - block[0] / 2.0;
        let top = out.height as f32 * ay - block[1] / 2.0;
        self.draw(out, text, height, 0.0, wrap, left, top, rgb, alpha);
    }
}

fn blend(out: &mut Rgba, x: u32, y: u32, rgb: [u8; 3], alpha: f32) {
    out.blend(x, y, rgb, alpha);
}
