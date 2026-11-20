//! The storyboard as frames (SF5-3). [`Compositor::frame`] draws the Cut at a time as one RGBA
//! picture, the way the Strip Viewer shows it (`ui-egui/src/strip_view.rs::show_clip`,
//! `motion_view::show`, `text_view::paint_text`), without a window:
//!
//! - a **still** is fitted into the frame (letterboxed) and, with motion, seen through its
//!   pan/zoom window at `Motion::at(t)`;
//! - a **model** is rendered by the mesh rasterizer at full quality from the clip's camera
//!   (the thumbnails' three-quarter view plus the motion), over the Viewer's ground;
//! - a **title card** fills the frame with its theme (or its background asset under a scrim)
//!   and sets its words; an **overlay** sets its words over the shot, on a plate or a shadow;
//!   intro animations (fade, rise, zoom, typewriter) run as in the Viewer;
//! - an **empty clip** shows "No shot assigned" and its brief, as the Viewer does.
//!
//! Layer 4: reads assets through `Services` (a fake in tests), decodes them through
//! `assetcraft-thumbs` (its limits apply; a still is decoded at up to [`STILL_MAX_SIDE`]) and
//! keeps the last few decoded stills and loaded models so a clip's frames share one decode.
//! Not drawn yet: transitions (every edit is a cut), the Viewer's badge and progress line
//! (those are UI, not picture). Text is set with the bundled Inter through `ab_glyph`.

#![forbid(unsafe_code)]

mod draw;
mod text;

pub use draw::Rgba;

use assetcraft_fs::Services;
use assetcraft_mesh::{RenderOptions, View};
use assetcraft_storyboard::{
    AnimFrame, Camera, Clip, Corner, Inset, InsetKind, InsetSize, MotionKind, ShotType, Storyboard, TextKind, Ticks,
};
use assetcraft_thumbs::model::{Model, load_model, render_view};
use assetcraft_thumbs::{DecodeOptions, ThumbError, ThumbKey};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Longest side a still is decoded at (a 4K export zooms into it at most 3×).
pub const STILL_MAX_SIDE: u32 = 4096;
/// Decoded stills kept, in bytes.
pub const STILL_CACHE_BYTES: usize = 1 << 30;
/// Loaded models kept.
pub const MODEL_CACHE: usize = 4;
/// The Viewer's ground, under a model with a transparent background.
pub const GROUND: [u8; 3] = [0x19, 0x19, 0x19];

/// Why a frame could not be drawn. Assets that fail stay failed for the compositor's life
/// (`Rgba` is still produced: the clip shows as the Viewer shows a failed asset, empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimaticError {
    /// Width or height out of range (even, 16..=8192).
    BadSize(u32, u32),
    /// The font could not be read (a build problem, never the user's).
    Font,
}

impl fmt::Display for AnimaticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSize(w, h) => write!(f, "frame size {w}×{h} out of range (even, 16..=8192)"),
            Self::Font => write!(f, "the bundled font could not be read"),
        }
    }
}

impl std::error::Error for AnimaticError {}

/// What went wrong with one asset, as reported after an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetProblem {
    /// The asset.
    pub path: PathBuf,
    /// The decoder's reason.
    pub reason: String,
}

/// Draws frames of one or more storyboards at a fixed size.
pub struct Compositor {
    services: Arc<dyn Services>,
    width: u32,
    height: u32,
    look: RenderOptions,
    decode: DecodeOptions,
    fonts: text::Fonts,
    stills: HashMap<PathBuf, Arc<Rgba>>,
    stills_order: Vec<PathBuf>,
    stills_bytes: usize,
    models: HashMap<PathBuf, Arc<Model>>,
    models_order: Vec<PathBuf>,
    /// How a shot is cropped into the frame (F3): the board's fill and snap at this frame's size;
    /// none (an older board) fits the whole picture in, as before.
    crop: Option<assetcraft_storyboard::FrameSpec>,
    /// Videos being read (P3), most recently used last; at most [`OPEN_VIDEOS`].
    videos: Vec<(PathBuf, assetcraft_media::VideoReader)>,
    failed: HashMap<PathBuf, String>,
    /// The user's loaded themes, for text naming one (TP2a-2); empty = built-ins only.
    user_looks: assetcraft_storyboard::UserLooks,
    /// Theme packs' designs (TP2a-3): a card or overlay naming a pack theme with a design for
    /// its role draws through it.
    designs: std::sync::Arc<Vec<ThemeDesign>>,
}

/// A theme pack's design for one role, ready to fill (theme-packs.md TP2a-3).
#[derive(Debug, Clone)]
pub struct ThemeDesign {
    /// The pack theme.
    pub theme: assetcraft_storyboard::ThemeId,
    /// `card` (a title card) or `banner` (an overlay).
    pub role: String,
    /// The design's file, for problems.
    pub path: PathBuf,
    /// The design.
    pub design: std::sync::Arc<assetcraft_titles::Design>,
    /// The theme's colours and fonts; the clip's words are added when drawn.
    pub fill: assetcraft_titles::Fill,
}

/// The design role a text kind draws through.
pub fn design_role(kind: TextKind) -> &'static str {
    match kind {
        TextKind::Card => "card",
        TextKind::Overlay => "banner",
    }
}

/// How many videos a compositor keeps open at once.
const OPEN_VIDEOS: usize = 4;

impl fmt::Debug for Compositor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Compositor")
            .field("size", &(self.width, self.height))
            .field("stills", &self.stills.len())
            .field("models", &self.models.len())
            .field("failed", &self.failed.len())
            .finish()
    }
}

impl Compositor {
    /// A compositor drawing `width × height` frames. `look` is the model display (mode,
    /// background, lighting; size and quality are set per frame); `decode` the format switches.
    pub fn new(
        services: Arc<dyn Services>,
        width: u32,
        height: u32,
        look: RenderOptions,
        decode: DecodeOptions,
    ) -> Result<Self, AnimaticError> {
        if !(16..=8192).contains(&width) || !(16..=8192).contains(&height) || width % 2 == 1 || height % 2 == 1 {
            return Err(AnimaticError::BadSize(width, height));
        }
        Ok(Self {
            services,
            width,
            height,
            look,
            decode,
            fonts: text::Fonts::bundled().ok_or(AnimaticError::Font)?,
            stills: HashMap::new(),
            stills_order: Vec::new(),
            stills_bytes: 0,
            models: HashMap::new(),
            models_order: Vec::new(),
            crop: None,
            videos: Vec::new(),
            failed: HashMap::new(),
            user_looks: assetcraft_storyboard::UserLooks::default(),
            designs: std::sync::Arc::new(Vec::new()),
        })
    }

    /// Frame size.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Assets that would not decode, with the reason, in the order they failed.
    pub fn problems(&self) -> Vec<AssetProblem> {
        let mut v: Vec<AssetProblem> =
            self.failed.iter().map(|(p, r)| AssetProblem { path: p.clone(), reason: r.clone() }).collect();
        v.sort_by(|a, b| a.path.cmp(&b.path));
        v
    }

    /// The Cut at `at`: the clip under the playhead, `elapsed` seconds into it; black past the
    /// end or with no clips.
    pub fn frame(&mut self, board: &Storyboard, at: Ticks) -> Rgba {
        // A split stage (F5, L1): each region its own shot, cropped into it.
        if let (Some(layout), Some(shots)) = (board.layout.as_ref(), board.split_shots(at)) {
            return self.split_frame(board, layout, shots);
        }
        // A framed post (F5, L2): bands around the shot, unless a title card takes the frame.
        if let Some(layout @ assetcraft_storyboard::Layout::Framed { .. }) = board.layout.as_ref()
            && let Some(clip) = board.clip_at(at).filter(|c| c.shot_type != ShotType::Ttl)
        {
            let start = board.index_of(clip.id).and_then(|i| board.starts().get(i).copied()).unwrap_or(Ticks::ZERO);
            let elapsed = Ticks(at.0.saturating_sub(start.0)).as_secs_f64();
            return self.framed_frame(clip, elapsed, layout);
        }
        let starts = board.starts();
        let found = board.clip_at(at).and_then(|c| {
            let i = board.clips.iter().position(|x| x.id == c.id)?;
            starts.get(i).map(|s| (c, *s))
        });
        match found {
            Some((clip, start)) => {
                let elapsed = Ticks(at.0.saturating_sub(start.0)).as_secs_f64();
                let inset = board.scenes.iter().find(|s| s.id == clip.scene_id).and_then(|s| s.inset.as_ref());
                self.section_frame(clip, elapsed, inset)
            }
            None => Rgba::filled(self.width, self.height, [0, 0, 0]),
        }
    }

    /// `clip` with its section's inset (SR2c): the shot as [`clip_frame`](Self::clip_frame),
    /// and the inset in its corner — a graphic drawn at full quality, a video as a poster box
    /// with its name (FilmCraft renders the real video from the OTIO). With `swap`, the inset
    /// takes the frame and the shot sits in the corner. Title cards never carry one.
    pub fn section_frame(&mut self, clip: &Clip, elapsed: f64, inset: Option<&Inset>) -> Rgba {
        let shot = self.clip_frame(clip, elapsed);
        let Some(inset) = inset.filter(|i| i.asset.is_some() && clip.shot_type != ShotType::Ttl) else {
            return shot;
        };
        if inset.swap {
            let mut out = Rgba::filled(self.width, self.height, [0, 0, 0]);
            let full = [0.0, 0.0, self.width as f32, self.height as f32];
            self.paint_inset(&mut out, inset, full);
            let corner = self.corner_rect(inset.corner, inset.size);
            let [x, y, w, h] = corner;
            draw::rect(&mut out, x - 2.0, y - 2.0, w + 4.0, h + 4.0, [0x10, 0x10, 0x10], 1.0);
            draw::contain_in(&mut out, &shot, corner);
            out
        } else {
            let mut out = shot;
            let corner = self.corner_rect(inset.corner, inset.size);
            let [x, y, w, h] = corner;
            draw::rect(&mut out, x - 2.0, y - 2.0, w + 4.0, h + 4.0, [0x10, 0x10, 0x10], 1.0);
            self.paint_inset(&mut out, inset, corner);
            out
        }
    }

    /// Where an inset of `size` sits in `corner`: 16:9, `size.share()` of the frame's width,
    /// 2.5 % of the width in from the edges.
    fn corner_rect(&self, corner: Corner, size: InsetSize) -> [f32; 4] {
        let (fw, fh) = (self.width as f32, self.height as f32);
        let w = (fw * size.share()).max(8.0);
        let h = (w * 9.0 / 16.0).min(fh * 0.6);
        let m = fw * 0.025;
        let x = match corner {
            Corner::TopLeft | Corner::BottomLeft => m,
            Corner::TopRight | Corner::BottomRight => fw - m - w,
        };
        let y = match corner {
            Corner::TopLeft | Corner::TopRight => m,
            Corner::BottomLeft | Corner::BottomRight => fh - m - h,
        };
        [x, y, w, h]
    }

    /// The inset's picture into `target`: a graphic as itself; a video as a dark poster box
    /// with a play mark and the file's name.
    fn paint_inset(&mut self, out: &mut Rgba, inset: &Inset, target: [f32; 4]) {
        let Some(asset) = &inset.asset else { return };
        let path = Path::new(asset.0.as_str());
        let [x, y, w, h] = target;
        draw::rect(out, x, y, w, h, [0x1e, 0x1e, 0x22], 1.0);
        let graphic = inset.kind == InsetKind::Graphic
            || !matches!(format_of(path).category(), assetcraft_probe::Category::Video);
        if graphic && let Some(image) = self.still(path) {
            draw::contain_in(out, &image, target);
            return;
        }
        // A video: the poster box. (Decoding video is FilmCraft's; the Viewer shows where it
        // goes and the export carries the box so the cut is readable without it.)
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (fw, fh) = (self.width as f32, self.height as f32);
        let cx = (x + w / 2.0) / fw;
        let cy = (y + h / 2.0) / fh;
        self.fonts.draw_centered(
            out,
            "VIDEO",
            (h * 0.16).max(7.0),
            [0xd0, 0xd0, 0xd0],
            1.0,
            cx,
            cy - (h * 0.1) / fh,
            w,
        );
        self.fonts.draw_centered(
            out,
            &name,
            (h * 0.11).max(6.0),
            [0xa0, 0xa0, 0xa0],
            1.0,
            cx,
            cy + (h * 0.12) / fh,
            w * 0.9,
        );
    }

    /// `clip` as the Viewer shows it `elapsed` seconds in (`f64::INFINITY` = at rest).
    pub fn clip_frame(&mut self, clip: &Clip, elapsed: f64) -> Rgba {
        let mut out = Rgba::filled(self.width, self.height, [0, 0, 0]);
        if clip.shot_type == ShotType::Ttl {
            if let Some(bg) = clip.background()
                && let Some(image) = self.still(Path::new(&bg.0))
            {
                draw::cover(&mut out, &image);
            }
            self.paint_text(&mut out, clip, elapsed);
            return out;
        }
        match clip.asset_ids.first() {
            Some(hero) => {
                let path = Path::new(hero.0.as_str());
                let t = progress(clip, elapsed);
                // A video plays from the clip's in-point (P3, C6); a still is itself.
                let at = clip.source_in.as_secs_f64() + if elapsed.is_finite() { elapsed.max(0.0) } else { 0.0 };
                match (clip.motion, MotionKind::of_clip(clip)) {
                    (Some(motion), Some(MotionKind::Orbit)) => self.model_frame(&mut out, path, Some(motion.at(t))),
                    (Some(motion), Some(MotionKind::PanZoom)) => {
                        if let Some(image) = self.picture(path, at) {
                            draw::contain_window(&mut out, &image, window(motion.at(t)));
                        }
                    }
                    _ => {
                        if assetcraft_thumbs::model::is_model(format_of(path)) {
                            self.model_frame(&mut out, path, None);
                        } else if let Some(image) = self.picture(path, at) {
                            self.place(&mut out, &image);
                        }
                    }
                }
            }
            None => {
                let h = self.height as f32;
                let grey = [0x80, 0x80, 0x80];
                let dim = [0x8a, 0x8a, 0x8a];
                self.fonts.draw_centered(
                    &mut out,
                    "No shot assigned",
                    h * 0.041,
                    grey,
                    1.0,
                    0.5,
                    0.5 - 0.026,
                    self.width as f32 * 0.8,
                );
                self.fonts.draw_centered(
                    &mut out,
                    &clip.description,
                    h * 0.031,
                    dim,
                    1.0,
                    0.5,
                    0.5 + 0.036,
                    self.width as f32 * 0.8,
                );
            }
        }
        self.paint_text(&mut out, clip, elapsed);
        out
    }

    fn model_frame(&mut self, out: &mut Rgba, path: &Path, cam: Option<Camera>) {
        let Some(model) = self.model(path) else { return };
        let view = cam.map_or_else(View::default, orbit_view);
        draw::fill(out, GROUND);
        // The viewer's renderer caps a frame at `PREVIEW_MAX_SIDE` (2048) a side: a 4K export
        // renders at the capped size *in the frame's aspect* and is scaled to fill the frame
        // (finding SF5-E2, Paul 2026-11-06: "the 3d render is no longer centered and is on the
        // left side" — the 2048 × 1080 picture had been dropped at the top-left).
        let (rw, rh) = render_size(self.width, self.height);
        match render_view(&model, view, rw, rh, false, &self.look) {
            Ok(image) => {
                let picture = Rgba { width: image.width, height: image.height, data: image.rgba };
                if (picture.width, picture.height) == (self.width, self.height) {
                    draw::over(out, &picture, 0, 0);
                } else {
                    draw::contain_window(out, &picture, [0.0, 0.0, 1.0, 1.0]);
                }
            }
            Err(e) => {
                self.failed.insert(path.to_path_buf(), e.to_string());
            }
        }
    }

    fn paint_text(&mut self, out: &mut Rgba, clip: &Clip, elapsed: f64) {
        let (Some(kind), Some(words)) = (clip.text_kind(), clip.text()) else { return };
        let style = clip.text_style;
        let look = style.look_with(&self.user_looks);
        let anim = style.frame_at_with(&self.user_looks, elapsed, clip.duration.as_secs_f64());
        if kind == TextKind::Card {
            if clip.background().is_some() {
                // Over a background asset the theme colour is a scrim, so the image shows.
                draw::tint(out, rgb(look.background), 0.35);
            } else {
                draw::backdrop(out, &look.backdrop);
                for r in assetcraft_storyboard::card_decor(&look, out.width as f32, out.height as f32) {
                    draw::rect(out, r.x, r.y, r.w, r.h, rgb(r.color), r.alpha);
                }
            }
        }
        // A pack design for this role draws instead of the theme's words (TP2a-3); if it can't,
        // the words draw as before and the export lists why.
        if let Some(d) = self.design_for(style.theme_id, kind) {
            match self.draw_design(out, &d, words, elapsed) {
                Ok(()) => return,
                Err(e) => {
                    self.failed.insert(d.path.clone(), e);
                }
            }
        }
        // Past its duration (and its out) the words are gone; a Card keeps its background.
        if words.trim().is_empty() || anim.is_hidden() {
            return;
        }
        let (w, h) = (self.width as f32, self.height as f32);
        let size = match kind {
            TextKind::Card => look.card_size,
            TextKind::Overlay => look.overlay_size,
        };
        let height = (h * size * anim.scale).clamp(6.0, h);
        let shown = shown_text(words, look.uppercase, &anim);
        let full = shown_text(words, look.uppercase, &AnimFrame::REST);
        let wrap = w * if kind == TextKind::Card { 0.8 } else { 0.6 };
        let tracking = look.tracking * height;
        let color = rgb(look.text);
        let (ax, ay, align) = match kind {
            TextKind::Card => (0.5, 0.5, 0),
            TextKind::Overlay => style.position.anchor(),
        };
        // Place by the finished text's size so a typewriter doesn't drift as it types.
        let block = self.fonts.measure(&full, height, tracking, wrap);
        let anchor_x = w * ax;
        let left = match align {
            -1 => anchor_x,
            1 => anchor_x - block[0],
            _ => anchor_x - block[0] / 2.0,
        };
        let top = h * ay - block[1] / 2.0 + anim.offset_y * h;
        if kind == TextKind::Overlay {
            let pad = [height * 0.45, height * 0.25];
            match look.plate {
                Some((fill, alpha)) => {
                    let a = (f32::from(alpha) / 255.0 * anim.alpha).clamp(0.0, 1.0);
                    draw::rect(
                        out,
                        left - pad[0],
                        top - pad[1],
                        block[0] + 2.0 * pad[0],
                        block[1] + 2.0 * pad[1],
                        rgb(fill),
                        a,
                    );
                }
                None => {
                    self.fonts.draw(
                        out,
                        &shown,
                        height,
                        tracking,
                        wrap,
                        left + 1.0,
                        top + 1.5,
                        [0, 0, 0],
                        0.63 * anim.alpha,
                    );
                }
            }
        }
        let pad = [height * 0.45, height * 0.25];
        for r in assetcraft_storyboard::text_decor(&look, kind, [left, top, block[0], block[1]], h, pad) {
            draw::rect(out, r.x, r.y, r.w, r.h, rgb(r.color), r.alpha * anim.alpha);
        }
        // Shadow, outline or glow: the same words, under them (themes 1f).
        for p in assetcraft_storyboard::finish_passes(&look, height) {
            let a = p.alpha * anim.alpha;
            self.fonts.draw(out, &shown, height, tracking, wrap, left + p.dx, top + p.dy, rgb(p.color), a);
        }
        self.fonts.draw(out, &shown, height, tracking, wrap, left, top, color, anim.alpha);
    }

    /// Draw text that names a user theme with that theme's look (TP2a-2).
    pub fn with_user_looks(mut self, looks: assetcraft_storyboard::UserLooks) -> Self {
        self.user_looks = looks;
        self
    }

    /// Draw cards and overlays whose pack theme has a design through it (TP2a-3).
    pub fn with_designs(mut self, designs: std::sync::Arc<Vec<ThemeDesign>>) -> Self {
        self.designs = designs;
        self
    }

    /// This compositor cropping shots by `frame`'s fill and snap (F3: the board's frame, as the
    /// Viewer shows it); `None` fits the whole picture in, as older boards always did.
    pub fn with_crop(mut self, frame: Option<&assetcraft_storyboard::FrameSpec>) -> Self {
        self.crop =
            frame.map(|f| assetcraft_storyboard::FrameSpec { width: self.width, height: self.height, ..f.clone() });
        self
    }

    /// The pack design a text with theme `theme` and kind `kind` draws through, if any.
    fn design_for(&self, theme: Option<assetcraft_storyboard::ThemeId>, kind: TextKind) -> Option<ThemeDesign> {
        let theme = theme?;
        let role = design_role(kind);
        self.designs.iter().find(|d| d.theme == theme && d.role == role).cloned()
    }

    /// Fill `d` with the clip's words (first line → Headline, the rest → Caption) and draw its
    /// frame `elapsed` seconds in (at rest: half way, the hold), placed by the board's crop.
    fn draw_design(&self, out: &mut Rgba, d: &ThemeDesign, words: &str, elapsed: f64) -> Result<(), String> {
        let mut lines = words.lines().map(str::trim).filter(|l| !l.is_empty());
        let headline = lines.next().unwrap_or_default().to_string();
        let caption = lines.collect::<Vec<_>>().join(" ");
        let mut fill = d.fill.clone().text(assetcraft_titles::Slot::Headline, &headline);
        if !caption.is_empty() {
            fill = fill.text(assetcraft_titles::Slot::Caption, &caption);
        }
        let (dw, dh) = d.design.size();
        let (fw, fh) = (self.width as f32, self.height as f32);
        let [x, y, w, _] = match &self.crop {
            Some(f) => {
                let [x, y, w, h] = f.place(dw as f32, dh as f32);
                [x * fw, y * fh, w * fw, h * fh]
            }
            None => draw::contain_rect(self.width, self.height, (dw, dh)),
        };
        let scale = f64::from(w) / f64::from(dw.max(1));
        let secs = if elapsed.is_finite() { elapsed } else { d.design.seconds() / 2.0 };
        let frame = d.design.render(&fill, secs, scale).map_err(|e| e.to_string())?;
        let image = Rgba { width: frame.width, height: frame.height, data: frame.rgba };
        draw::over(out, &image, x.round() as i64, y.round() as i64);
        Ok(())
    }

    /// A shot's picture into the frame by the crop (`FrameSpec::place`, the Viewer's rule).
    fn place(&self, out: &mut Rgba, image: &Rgba) {
        match &self.crop {
            Some(f) => {
                let [x, y, w, h] = f.place(image.width as f32, image.height as f32);
                let (fw, fh) = (self.width as f32, self.height as f32);
                draw::window_into(out, image, [x * fw, y * fh, w * fw, h * fh]);
            }
            None => draw::contain_window(out, image, [0.0, 0.0, 1.0, 1.0]),
        }
    }

    /// A split stage's frame (F5, L1): ground, then each region's shot — a still or a video's
    /// frame — cropped into its region by the board's fill and snap. A region with nothing to
    /// show stays ground; models and camera moves show their still there for now.
    fn split_frame(
        &mut self,
        board: &Storyboard,
        layout: &assetcraft_storyboard::Layout,
        shots: [assetcraft_storyboard::RegionShot; 2],
    ) -> Rgba {
        let mut out = Rgba::filled(self.width, self.height, [0, 0, 0]);
        let (fw, fh) = (self.width as f32, self.height as f32);
        for (rect, shot) in layout.regions().into_iter().zip(shots) {
            let Some((i, elapsed)) = shot else { continue };
            let Some(clip) = board.clips.get(i) else { continue };
            let Some(hero) = clip.asset_ids.first() else { continue };
            let at = clip.source_in.as_secs_f64() + elapsed.max(0.0);
            let Some(image) = self.picture(Path::new(hero.0.as_str()), at) else { continue };
            let region = [rect[0] * fw, rect[1] * fh, rect[2] * fw, rect[3] * fh];
            // The board's crop at the region's own shape.
            let spec = assetcraft_storyboard::FrameSpec {
                width: (region[2].round() as u32).max(2),
                height: (region[3].round() as u32).max(2),
                ..self.crop.clone().unwrap_or_default()
            };
            let [x, y, w, h] = spec.place(image.width as f32, image.height as f32);
            let target = [region[0] + x * region[2], region[1] + y * region[3], w * region[2], h * region[3]];
            draw::window_clipped(&mut out, &image, target, region);
        }
        out
    }

    /// A framed post's frame (F5, L2): the headline band, the shot cropped into the picture
    /// region by the board's fill and snap, the caption band; the bands in the layout's theme.
    fn framed_frame(&mut self, clip: &Clip, elapsed: f64, layout: &assetcraft_storyboard::Layout) -> Rgba {
        let mut out = Rgba::filled(self.width, self.height, [0, 0, 0]);
        let assetcraft_storyboard::Layout::Framed { headline, caption, theme, .. } = layout else { return out };
        let Some([top, picture, bottom]) = layout.framed() else { return out };
        let look = theme.look();
        let (fw, fh) = (self.width as f32, self.height as f32);
        let px = |r: [f32; 4]| [r[0] * fw, r[1] * fh, r[2] * fw, r[3] * fh];
        let (top, picture, bottom) = (px(top), px(picture), px(bottom));
        for band in [top, bottom] {
            draw::rect(&mut out, band[0], band[1], band[2], band[3], rgb(look.background), 1.0);
        }
        if let Some(hero) = clip.asset_ids.first() {
            let at = clip.source_in.as_secs_f64() + elapsed.max(0.0);
            if let Some(image) = self.picture(Path::new(hero.0.as_str()), at) {
                let spec = assetcraft_storyboard::FrameSpec {
                    width: (picture[2].round() as u32).max(2),
                    height: (picture[3].round() as u32).max(2),
                    ..self.crop.clone().unwrap_or_default()
                };
                let [x, y, w, h] = spec.place(image.width as f32, image.height as f32);
                let target = [picture[0] + x * picture[2], picture[1] + y * picture[3], w * picture[2], h * picture[3]];
                draw::window_clipped(&mut out, &image, target, picture);
            }
        }
        let case = |s: &str| if look.uppercase { s.to_uppercase() } else { s.to_owned() };
        for (band, words, scale) in [(top, headline, 0.42), (bottom, caption, 0.3)] {
            let words = case(words.trim());
            if words.is_empty() || band[3] < 4.0 {
                continue;
            }
            let height = (band[3] * scale).min(fh * 0.06).max(6.0);
            let wrap = band[2] * 0.9;
            let block = self.fonts.measure(&words, height, look.tracking * height, wrap);
            let left = band[0] + (band[2] - block[0]) / 2.0;
            let topy = band[1] + (band[3] - block[1]) / 2.0;
            self.fonts.draw(&mut out, &words, height, look.tracking * height, wrap, left, topy, rgb(look.text), 1.0);
        }
        out
    }

    /// A clip's picture: a video's frame `secs` in, else the still.
    fn picture(&mut self, path: &Path, secs: f64) -> Option<Arc<Rgba>> {
        if assetcraft_media::is_video(path) { self.video(path, secs) } else { self.still(path) }
    }

    /// The frame `secs` into the video at `path` (P3; Paul, 2026-11-15: "mp4 videos do not
    /// display/play"), read on this thread through FilmCraft's codecs at the frame's size. A
    /// video that won't open or decode is reported once, as a still is, and drawn as black.
    fn video(&mut self, path: &Path, secs: f64) -> Option<Arc<Rgba>> {
        if self.failed.contains_key(path) {
            return None;
        }
        let at = match self.videos.iter().position(|(p, _)| p == path) {
            Some(i) => i,
            None => match assetcraft_media::VideoReader::open(path) {
                Ok(r) => {
                    if self.videos.len() >= OPEN_VIDEOS {
                        self.videos.remove(0);
                    }
                    self.videos.push((path.to_path_buf(), r));
                    self.videos.len() - 1
                }
                Err(why) => {
                    self.failed.insert(path.to_path_buf(), why);
                    return None;
                }
            },
        };
        // Most recently used last.
        let entry = self.videos.remove(at);
        self.videos.push(entry);
        let side = self.width.max(self.height);
        let reader = &mut self.videos.last_mut()?.1;
        match reader.frame(secs, side) {
            Ok(f) => Some(Arc::new(Rgba { width: f.width, height: f.height, data: f.rgba.clone() })),
            Err(why) => {
                self.failed.insert(path.to_path_buf(), why);
                None
            }
        }
    }

    fn still(&mut self, path: &Path) -> Option<Arc<Rgba>> {
        if let Some(image) = self.stills.get(path) {
            return Some(image.clone());
        }
        if self.failed.contains_key(path) {
            return None;
        }
        let key = ThumbKey { path: path.to_path_buf(), max_side: STILL_MAX_SIDE, modified: None };
        match assetcraft_thumbs::render_with(self.services.as_ref(), &key, &self.decode) {
            Ok(thumb) => {
                let image = Arc::new(Rgba { width: thumb.width, height: thumb.height, data: thumb.rgba });
                self.stills_bytes += image.data.len();
                self.stills.insert(path.to_path_buf(), image.clone());
                self.stills_order.push(path.to_path_buf());
                while self.stills_bytes > STILL_CACHE_BYTES && self.stills_order.len() > 1 {
                    let old = self.stills_order.remove(0);
                    if let Some(gone) = self.stills.remove(&old) {
                        self.stills_bytes = self.stills_bytes.saturating_sub(gone.data.len());
                    }
                }
                Some(image)
            }
            Err(e) => {
                self.failed.insert(path.to_path_buf(), reason(&e));
                None
            }
        }
    }

    fn model(&mut self, path: &Path) -> Option<Arc<Model>> {
        if let Some(model) = self.models.get(path) {
            return Some(model.clone());
        }
        if self.failed.contains_key(path) {
            return None;
        }
        let loaded = assetcraft_probe::probe(self.services.as_ref(), path)
            .map_err(|e| e.to_string())
            .and_then(|probe| load_model(self.services.as_ref(), &probe).map_err(|e| reason(&e)));
        match loaded {
            Ok(model) => {
                let model = Arc::new(model);
                self.models.insert(path.to_path_buf(), model.clone());
                self.models_order.push(path.to_path_buf());
                while self.models_order.len() > MODEL_CACHE {
                    let old = self.models_order.remove(0);
                    self.models.remove(&old);
                }
                Some(model)
            }
            Err(e) => {
                self.failed.insert(path.to_path_buf(), e);
                None
            }
        }
    }
}

fn reason(e: &ThumbError) -> String {
    e.to_string()
}

fn format_of(path: &Path) -> assetcraft_probe::Format {
    assetcraft_probe::Format::from_extension(path.extension().and_then(|e| e.to_str()).unwrap_or_default())
        .unwrap_or(assetcraft_probe::Format::Unknown)
}

/// Where the clip is, `0..=1`, from the seconds elapsed in it.
fn progress(clip: &Clip, elapsed: f64) -> f32 {
    if !elapsed.is_finite() || clip.duration.0 == 0 {
        return 0.0;
    }
    (elapsed / clip.duration.as_secs_f64()).clamp(0.0, 1.0) as f32
}

/// The size a model frame is rendered at: the frame itself when the renderer takes it, else
/// the largest size of the same aspect within [`PREVIEW_MAX_SIDE`] (the picture is then scaled
/// up to the frame).
pub fn render_size(width: u32, height: u32) -> (u32, u32) {
    let cap = assetcraft_thumbs::model::PREVIEW_MAX_SIDE;
    if width <= cap && height <= cap {
        return (width.max(1), height.max(1));
    }
    let scale = f64::from(cap) / f64::from(width.max(height).max(1));
    let w = ((f64::from(width) * scale).round() as u32).clamp(1, cap);
    let h = ((f64::from(height) * scale).round() as u32).clamp(1, cap);
    // Even sides keep chroma subsampling happy downstream.
    (w & !1, h & !1)
}

/// A still's window at a camera: `[u0, v0, u1, v1]` of the picture, kept inside it
/// (`motion_view::window`).
pub fn window(cam: Camera) -> [f32; 4] {
    let half = 0.5 / cam.zoom.max(1.0);
    let cx = (0.5 + cam.pan[0]).clamp(half, 1.0 - half);
    let cy = (0.5 + cam.pan[1]).clamp(half, 1.0 - half);
    [cx - half, cy - half, cx + half, cy + half]
}

/// A model's view at a camera: the thumbnails' three-quarter view plus the motion
/// (`motion_view::orbit_view`).
pub fn orbit_view(cam: Camera) -> View {
    let base = View::default();
    View { yaw: base.yaw + cam.yaw, pitch: base.pitch + cam.pitch, zoom: base.zoom * cam.zoom }
        .orbit(0.0, 0.0)
        .zoomed(1.0)
}

fn shown_text(words: &str, uppercase: bool, anim: &AnimFrame) -> String {
    let text = if uppercase { words.to_uppercase() } else { words.to_owned() };
    let n = anim.visible_chars(text.chars().count());
    text.chars().take(n).collect()
}

fn rgb(packed: u32) -> [u8; 3] {
    [(packed >> 16) as u8, (packed >> 8) as u8, packed as u8]
}

#[cfg(test)]
mod tests;
