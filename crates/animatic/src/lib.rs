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
use assetcraft_storyboard::{AnimFrame, Camera, Clip, MotionKind, ShotType, Storyboard, TextKind, Ticks};
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
    failed: HashMap<PathBuf, String>,
}

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
            failed: HashMap::new(),
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
        let starts = board.starts();
        let found = board.clip_at(at).and_then(|c| {
            let i = board.clips.iter().position(|x| x.id == c.id)?;
            starts.get(i).map(|s| (c, *s))
        });
        match found {
            Some((clip, start)) => self.clip_frame(clip, Ticks(at.0.saturating_sub(start.0)).as_secs_f64()),
            None => Rgba::filled(self.width, self.height, [0, 0, 0]),
        }
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
                match (clip.motion, MotionKind::of_clip(clip)) {
                    (Some(motion), Some(MotionKind::Orbit)) => self.model_frame(&mut out, path, Some(motion.at(t))),
                    (Some(motion), Some(MotionKind::PanZoom)) => {
                        if let Some(image) = self.still(path) {
                            draw::contain_window(&mut out, &image, window(motion.at(t)));
                        }
                    }
                    _ => {
                        if assetcraft_thumbs::model::is_model(format_of(path)) {
                            self.model_frame(&mut out, path, None);
                        } else if let Some(image) = self.still(path) {
                            draw::contain_window(&mut out, &image, [0.0, 0.0, 1.0, 1.0]);
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
        let look = style.theme.look();
        let anim = style.animation.frame(elapsed);
        if kind == TextKind::Card {
            let bg = rgb(look.background);
            if clip.background().is_some() {
                draw::tint(out, bg, 0.35);
            } else {
                draw::fill(out, bg);
            }
        }
        if words.trim().is_empty() {
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
        self.fonts.draw(out, &shown, height, tracking, wrap, left, top, color, anim.alpha);
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
