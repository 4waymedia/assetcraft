//! Clip motion in the Inspector (SB-3D1 step 2, `docs/design/clip-motion-handoff.md`): the
//! MOTION section — preset chips, Start / End frames, amounts, ease, the duration line with a
//! scrub that seeks the Cut inside the clip (D-M1), Clear — and the Strip card's glyph.
//! Everything the section shows comes from `Clip.motion`; every change is one
//! `storyboard.motion` call, so the CLI and tests see the same edits.
//!
//! Not here yet: *Set from Viewer* (step 4), *Apply to all* (step 5), and true start / end
//! renders of a model (step 3 wires `ModelView`; until then both frames show the hero's still
//! with the camera's numbers over it).

use crate::Navigator;
use crate::storyboard_view::{ACCENT, section};
use crate::thumbnails::Cell;
use assetcraft_storyboard::{Camera, Clip, Ease, Motion, MotionKind, MotionPreset, Storyboard, Ticks};
use assetcraft_thumbs::ThumbKey;
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;
use std::path::Path;

/// Start / End frame size (the design's 128 × 72) and the gap between them.
const FRAME: egui::Vec2 = vec2(128.0, 72.0);
const FRAME_GAP: f32 = 12.0;

/// An amount slider while it is being dragged: the clip and the value shown, committed as one
/// `storyboard.motion` (one undo step) when the drag ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MotionDrag {
    pub(crate) clip: u32,
    pub(crate) amount: Camera,
}

/// The MOTION section for `clip`, under SHOT.
pub(crate) fn inspector_section(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, clip: &Clip) {
    let kind = MotionKind::of_clip(clip);
    header(ui, kind);
    let Some(kind) = kind else {
        ui.weak("No shot to move");
        return;
    };
    let id = clip.id.0;
    let motion = clip.motion;
    let shown = motion.unwrap_or_default();

    // 1. Presets. "None" clears; a preset fills the amounts; Custom keeps them.
    let mut edit: Option<serde_json::Value> = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 4.0);
        if chip(ui, "None", motion.is_none(), true).clicked() && motion.is_some() {
            edit = Some(json!({ "clip": id, "clear": true }));
        }
        for preset in MotionPreset::ALL {
            let on = motion.is_some_and(|m| m.preset == preset);
            let allowed = preset.allows(kind);
            let r = chip(ui, preset.name(), on, allowed);
            if !allowed {
                r.on_disabled_hover_text("A turntable needs a 3D model");
            } else if r.clicked() && !on {
                edit = Some(json!({ "clip": id, "preset": preset.id() }));
            }
        }
    });
    ui.add_space(6.0);

    // 2. Start / End.
    ui.horizontal(|ui| {
        ui.weak("Start");
        let swap = ui
            .add_enabled(motion.is_some(), egui::Button::new("⇄ Swap").small())
            .on_hover_text("Run the motion backwards: End becomes Start");
        if swap.clicked() {
            edit = Some(json!({ "clip": id, "swap": true }));
        }
        ui.add_space((FRAME.x + FRAME_GAP - swap.rect.width() - 46.0).max(0.0));
        ui.weak("End");
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = FRAME_GAP;
        frame(nav, ui, clip, kind, shown.start, "Start");
        frame(nav, ui, clip, kind, shown.end(), "End");
    });
    ui.add_space(6.0);

    // 3. Amounts (the sliders), committed when a drag ends.
    ui.horizontal(|ui| {
        ui.weak("Amount");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new("End = Start + amount").font(FontId::monospace(10.0)).weak());
        });
    });
    let dragging = nav.state.storyboard.motion_drag.filter(|d| d.clip == id);
    let mut amount = dragging.map(|d| d.amount).unwrap_or(shown.amount);
    let mut changed = false;
    let mut released = false;
    for row in rows(kind) {
        let before = row.get(amount);
        let mut value = before;
        let r = ui
            .horizontal(|ui| {
                ui.add_sized([40.0, 18.0], egui::Label::new(row.label));
                let slider = egui::Slider::new(&mut value, row.range.clone())
                    .step_by(row.step)
                    .suffix(row.suffix)
                    .fixed_decimals(row.decimals)
                    .show_value(true);
                ui.add(slider)
            })
            .inner;
        // The slider snaps a preset's value to its step on the first frame (20.000002° → 20°,
        // 0.625× → 0.63×) and reports that as a change; only a move of at least a step is the
        // user's (Paul, 2026-11-05: Rise and Pull out "go to Custom").
        let moved = r.changed() && (value - before).abs() >= row.step * 0.5;
        if moved {
            amount = row.set(amount, value);
            changed = true;
        }
        if r.drag_stopped() {
            released = true;
        }
        if moved && !r.dragged() {
            // Typed or clicked into place: commit at once.
            released = true;
        }
    }
    if changed || dragging.is_some() {
        nav.state.storyboard.motion_drag = Some(MotionDrag { clip: id, amount });
    }
    if released {
        nav.state.storyboard.motion_drag = None;
        if amount != shown.amount {
            edit = Some(json!({ "clip": id, "amount": camera_json(amount) }));
        }
    }
    ui.add_space(6.0);

    // 4. Ease.
    ui.horizontal(|ui| {
        ui.add_sized([40.0, 18.0], egui::Label::new(RichText::new("Ease").weak()));
        let w = (ui.available_width() - 4.0) / Ease::ALL.len() as f32;
        egui::Frame::new().fill(Color32::from_gray(0x2a)).corner_radius(3).inner_margin(1.0).show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for ease in Ease::ALL {
                let on = shown.ease == ease;
                let b = egui::Button::new(RichText::new(ease.name()).color(if on {
                    Color32::WHITE
                } else {
                    ui.visuals().text_color()
                }))
                .fill(if on { ACCENT } else { Color32::TRANSPARENT })
                .stroke(Stroke::NONE)
                .min_size(vec2(w, 20.0));
                if ui.add(b).clicked() && !on {
                    edit = Some(json!({ "clip": id, "ease": ease.id() }));
                }
            }
        });
    });
    ui.add_space(6.0);

    // 5. Duration, and a scrub that seeks the Cut inside the clip (D-M1).
    let secs = clip.duration.as_secs_f64();
    ui.horizontal(|ui| {
        ui.weak("Duration");
        ui.label("Follows the clip ·");
        ui.label(RichText::new(format!("{secs:.1} s")).font(FontId::monospace(11.0)));
    });
    let start = clip_start(board, clip);
    let at = nav.session.transport().position;
    let inside = at.0.saturating_sub(start.0) as f64 / clip.duration.0.max(1) as f64;
    let mut t = if at >= start { inside.clamp(0.0, 1.0) as f32 } else { 0.0 };
    let scrub = ui.add(egui::Slider::new(&mut t, 0.0..=1.0).show_value(false).step_by(0.0));
    let scrub = scrub.on_hover_text("Drag to preview the motion");
    if scrub.changed() {
        let seconds = start.as_secs_f64() + f64::from(t) * secs;
        nav.run("storyboard.seek", json!({ "seconds": seconds }));
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("drag to preview").size(11.0).weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let text = if t > 0.0 { format!("Start ▸ End · {:.1} s", f64::from(t) * secs) } else { "0.0 s".into() };
            ui.label(RichText::new(text).font(FontId::monospace(10.0)).weak());
        });
    });
    ui.add_space(6.0);

    // 6. Footer.
    ui.separator();
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(motion.is_some(), egui::Button::new("Clear motion").frame(false)).clicked() {
                edit = Some(json!({ "clip": id, "clear": true }));
            }
        });
    });

    if let Some(p) = edit {
        nav.run("storyboard.motion", p);
    }
}

/// `MOTION` with the kind at the right; dimmed when there is nothing to move.
fn header(ui: &mut Ui, kind: Option<MotionKind>) {
    ui.horizontal(|ui| {
        ui.set_opacity(if kind.is_some() { 1.0 } else { 0.5 });
        section(ui, "MOTION");
        if let Some(kind) = kind {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(kind.name()).size(11.0).weak());
            });
        }
    });
}

/// A preset chip: accent when lit, plain otherwise; dimmed when it doesn't apply.
fn chip(ui: &mut Ui, name: &str, on: bool, enabled: bool) -> egui::Response {
    let text = RichText::new(name).size(12.0).color(if on { Color32::WHITE } else { ui.visuals().text_color() });
    let b = egui::Button::new(text)
        .fill(if on { ACCENT } else { Color32::TRANSPARENT })
        .stroke(Stroke::NONE)
        .corner_radius(3)
        .min_size(vec2(0.0, 22.0));
    ui.add_enabled(enabled, b)
}

/// One 128 × 72 frame: the hero as the camera `cam` sees it. A still is drawn through its pan /
/// zoom window; a model shows its still with the camera's numbers over it until step 3.
fn frame(nav: &mut Navigator, ui: &mut Ui, clip: &Clip, kind: MotionKind, cam: Camera, label: &str) {
    let (rect, _) = ui.allocate_exact_size(FRAME, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0, Color32::BLACK);
    painter.rect_stroke(rect, 0, Stroke::new(1.0, Color32::from_gray(0x2e)), StrokeKind::Outside);
    if let Some(hero) = clip.asset_ids.first() {
        let ctx = ui.ctx().clone();
        let side = (FRAME.x * ctx.pixels_per_point()).ceil() as u32;
        if let Cell::Ready(texture, size) = nav.thumbs.get(&ctx, ThumbKey::new(Path::new(&hero.0), side), 0) {
            let target = crate::storyboard_view::contain(rect, size);
            let uv = match kind {
                MotionKind::PanZoom => window(cam),
                MotionKind::Orbit => Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            };
            ui.painter().image(texture, target, uv, Color32::WHITE);
        }
    }
    let caption = match kind {
        MotionKind::Orbit => {
            format!("{:.0}° · {:.0}° · {:.2}×", cam.yaw.to_degrees(), cam.pitch.to_degrees(), cam.zoom)
        }
        MotionKind::PanZoom => {
            format!("{:+.0} % · {:+.0} % · {:.2}×", cam.pan[0] * 100.0, cam.pan[1] * 100.0, cam.zoom)
        }
    };
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(caption, FontId::monospace(9.0), Color32::from_gray(0xdd));
    let band = Rect::from_min_size(rect.left_top() + vec2(3.0, 3.0), galley.size() + vec2(6.0, 2.0));
    painter.rect_filled(band, 2, Color32::from_black_alpha(150));
    painter.galley(band.min + vec2(3.0, 1.0), galley, Color32::from_gray(0xdd));
    ui.painter().text(
        rect.left_bottom() + vec2(3.0, -2.0),
        Align2::LEFT_BOTTOM,
        label,
        FontId::proportional(9.0),
        Color32::from_gray(0x8c),
    );
}

/// The part of a still a camera shows, as texture coordinates: centred at the pan, `1 / zoom`
/// wide, kept inside the picture.
pub(crate) fn window(cam: Camera) -> Rect {
    let half = 0.5 / cam.zoom.max(1.0);
    let cx = (0.5 + cam.pan[0]).clamp(half, 1.0 - half);
    let cy = (0.5 + cam.pan[1]).clamp(half, 1.0 - half);
    Rect::from_min_max(pos2(cx - half, cy - half), pos2(cx + half, cy + half))
}

/// One amount slider: what it reads and writes on a [`Camera`], in the Inspector's units.
struct Row {
    label: &'static str,
    range: std::ops::RangeInclusive<f64>,
    step: f64,
    suffix: &'static str,
    decimals: usize,
    field: Field,
}

#[derive(Clone, Copy)]
enum Field {
    YawDeg,
    PitchDeg,
    Zoom,
    PanX,
    PanY,
}

impl Row {
    fn get(&self, c: Camera) -> f64 {
        match self.field {
            Field::YawDeg => f64::from(c.yaw.to_degrees()),
            Field::PitchDeg => f64::from(c.pitch.to_degrees()),
            Field::Zoom => f64::from(c.zoom),
            Field::PanX => f64::from(c.pan[0]) * 100.0,
            Field::PanY => f64::from(c.pan[1]) * 100.0,
        }
    }

    fn set(&self, mut c: Camera, v: f64) -> Camera {
        let v = v as f32;
        match self.field {
            Field::YawDeg => c.yaw = v.to_radians(),
            Field::PitchDeg => c.pitch = v.to_radians(),
            Field::Zoom => c.zoom = v,
            Field::PanX => c.pan[0] = v / 100.0,
            Field::PanY => c.pan[1] = v / 100.0,
        }
        c
    }
}

/// The three rows for a kind (the design's ranges).
fn rows(kind: MotionKind) -> [Row; 3] {
    match kind {
        MotionKind::Orbit => [
            Row { label: "Turn", range: -360.0..=360.0, step: 1.0, suffix: "°", decimals: 0, field: Field::YawDeg },
            Row { label: "Rise", range: -80.0..=80.0, step: 1.0, suffix: "°", decimals: 0, field: Field::PitchDeg },
            Row { label: "Zoom", range: 0.5..=4.0, step: 0.01, suffix: "×", decimals: 2, field: Field::Zoom },
        ],
        MotionKind::PanZoom => [
            Row { label: "Pan X", range: -50.0..=50.0, step: 1.0, suffix: " %", decimals: 0, field: Field::PanX },
            Row { label: "Pan Y", range: -50.0..=50.0, step: 1.0, suffix: " %", decimals: 0, field: Field::PanY },
            Row { label: "Zoom", range: 1.0..=3.0, step: 0.01, suffix: "×", decimals: 2, field: Field::Zoom },
        ],
    }
}

/// A camera as `storyboard.motion` takes it (degrees).
fn camera_json(c: Camera) -> serde_json::Value {
    json!({ "yaw": c.yaw.to_degrees(), "pitch": c.pitch.to_degrees(), "zoom": c.zoom, "pan": c.pan })
}

/// Where `clip` starts on the Cut.
fn clip_start(board: &Storyboard, clip: &Clip) -> Ticks {
    board.index_of(clip.id).and_then(|i| board.starts().get(i).copied()).unwrap_or(Ticks::ZERO)
}

/// The Strip card's glyph for a clip, if it moves.
pub(crate) fn glyph(clip: &Clip) -> Option<&'static str> {
    clip.motion.as_ref().and_then(Motion::glyph)
}

/// "n clips with motion" for the status bar, or `None`.
pub(crate) fn status(board: &Storyboard) -> Option<String> {
    let n = board.clips.iter().filter(|c| glyph(c).is_some()).count();
    (n > 0).then(|| format!("{n} clip{} with motion", if n == 1 { "" } else { "s" }))
}

// ---- Viewer playback (steps 3, 3b and 3c) ---------------------------------------------------

/// Frames of the Cut per second: how finely a clip's motion is sampled for playback.
pub(crate) const PLAY_FPS: f32 = 24.0;
/// Largest frame rendered for the Viewer, in pixels on a side.
const PLAY_MAX_SIDE: f32 = 1024.0;
/// Most bytes of rendered frames the cache keeps (RGBA, in memory; the shown one is uploaded).
const CACHE_BYTES: usize = 768 << 20;
/// Most heroes kept loaded at once (each is its own render thread and a mesh in memory).
const MAX_MODELS: usize = 4;

/// What a clip's cached frames were rendered for: a change in any of these drops them.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FrameKey {
    motion: Motion,
    size: [u32; 2],
    prefs: assetcraft_engine::ModelPrefs,
    /// The clip's first frame on the Cut (a Re-cut moves it; the frames stay valid but the
    /// distance-from-playhead changes, so the key carries it).
    base: u32,
    count: u32,
}

/// One clip's rendered frames.
struct ClipFrames {
    key: FrameKey,
    frames: std::collections::BTreeMap<u32, assetcraft_thumbs::model::Image>,
}

/// A render job started with Return: every model clip with motion, in Cut order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderJob {
    /// The clips to finish, in order.
    clips: Vec<u32>,
    /// The frames they have between them.
    pub(crate) total: u32,
}

/// The Viewer's playback frames (FilmCraft's `frames.rs` shape, steps 3b–3c): the motion is
/// deterministic, so the heroes' `ModelView` threads render **full-quality** frames — the same
/// render a paused Viewer shows — ahead of the playhead into a byte-budgeted cache, on-screen
/// frame first, then the next ones; the Viewer draws the nearest cached frame and never renders
/// itself. Return starts a [`RenderJob`] over the whole Cut, so Play is smooth from the first
/// frame; Esc stops it. A clip's frames are dropped when its motion, the Viewer size or the look
/// changes; otherwise the frame farthest from the playhead goes first when the cache is full.
#[derive(Default)]
pub(crate) struct MotionPlayer {
    models: std::collections::HashMap<std::path::PathBuf, assetcraft_thumbs::model::ModelView>,
    /// Heroes in the order they were last used (the front is dropped first).
    recent: Vec<std::path::PathBuf>,
    clips: std::collections::HashMap<u32, ClipFrames>,
    bytes: usize,
    /// The frame being rendered: hero, clip, index.
    pending: Option<(std::path::PathBuf, u32, u32)>,
    texture: Option<egui::TextureHandle>,
    /// The (clip, index) the texture holds.
    shown: Option<(u32, u32)>,
    pub(crate) job: Option<RenderJob>,
    /// The camera of the first frame asked for (tests: the on-screen frame comes first).
    #[cfg(test)]
    pub(crate) first_view: Option<assetcraft_thumbs::model::View>,
}

impl std::fmt::Debug for MotionPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MotionPlayer({} models, {} frames, {} MiB)", self.models.len(), self.frame_total(), self.bytes >> 20)
    }
}

impl MotionPlayer {
    /// Whether a rendered frame is being shown (tests).
    #[cfg(test)]
    pub(crate) fn has_frame(&self) -> bool {
        self.texture.is_some()
    }

    /// Frames cached for `clip` (tests).
    #[cfg(test)]
    pub(crate) fn cached(&self, clip: u32) -> usize {
        self.clips.get(&clip).map_or(0, |c| c.frames.len())
    }

    fn frame_total(&self) -> usize {
        self.clips.values().map(|c| c.frames.len()).sum()
    }

    /// Bytes a clip's frames would take.
    fn frames_in(&self, clip: u32) -> Option<&ClipFrames> {
        self.clips.get(&clip)
    }

    /// The hero's render thread, started if needed; the least recently used one is dropped
    /// past [`MAX_MODELS`]. `None` while it is still loading.
    fn model(
        &mut self,
        services: &std::sync::Arc<dyn assetcraft_fs::Services>,
        path: &Path,
    ) -> Option<&mut assetcraft_thumbs::model::ModelView> {
        if !self.models.contains_key(path) {
            while self.models.len() >= MAX_MODELS && !self.recent.is_empty() {
                let old = self.recent.remove(0);
                if self.pending.as_ref().is_some_and(|(p, _, _)| *p == old) {
                    self.pending = None;
                }
                self.models.remove(&old);
            }
            let view = assetcraft_thumbs::model::ModelView::open(std::sync::Arc::clone(services), path.to_path_buf());
            self.models.insert(path.to_path_buf(), view);
        }
        self.recent.retain(|p| p != path);
        self.recent.push(path.to_path_buf());
        let model = self.models.get_mut(path)?;
        // The load state only moves on a poll; a frame surfacing here belongs to a request
        // that was dropped (its clip's key changed), so it is let go.
        let _ = model.poll();
        matches!(model.state(), assetcraft_thumbs::model::ModelState::Ready { .. }).then_some(model)
    }

    /// Keep `clip`'s frames only while they were rendered for `key`.
    fn keep_for(&mut self, clip: u32, key: FrameKey) {
        let stale = self.clips.get(&clip).is_some_and(|c| c.key != key);
        if stale {
            self.drop_clip(clip);
        }
        self.clips.entry(clip).or_insert_with(|| ClipFrames { key, frames: std::collections::BTreeMap::new() });
        if self.pending.as_ref().is_some_and(|(_, c, _)| *c == clip) && stale {
            self.pending = None;
        }
    }

    fn drop_clip(&mut self, clip: u32) {
        if let Some(c) = self.clips.remove(&clip) {
            self.bytes = self.bytes.saturating_sub(c.frames.values().map(|i| i.rgba.len()).sum());
        }
        if self.shown.is_some_and(|(c, _)| c == clip) {
            self.shown = None;
        }
    }

    /// Make room for one more frame: drop the cached frame farthest, on the Cut, from the
    /// playhead at frame `now`.
    fn evict_for(&mut self, now: u32, frame_bytes: usize) {
        while self.bytes + frame_bytes > CACHE_BYTES && self.frame_total() > 0 {
            let farthest = self
                .clips
                .iter()
                .flat_map(|(id, c)| c.frames.keys().map(move |i| ((*id, *i), (c.key.base + i).abs_diff(now))))
                .max_by_key(|(_, d)| *d)
                .map(|(k, _)| k);
            let Some((clip, index)) = farthest else { break };
            if let Some(image) = self.clips.get_mut(&clip).and_then(|c| c.frames.remove(&index)) {
                self.bytes = self.bytes.saturating_sub(image.rgba.len());
            }
        }
    }

    /// The next frame worth rendering for the clip on screen: the one wanted, else the first
    /// missing one after it, else the first missing one before it (a scrub back) — `None`
    /// when the cache is full or the clip complete.
    fn next_for(&self, clip: u32, want: u32, frame_bytes: usize) -> Option<u32> {
        let c = self.clips.get(&clip)?;
        if !c.frames.contains_key(&want) {
            return Some(want);
        }
        if self.bytes + frame_bytes > CACHE_BYTES {
            return None;
        }
        (want + 1..c.key.count).chain(0..want).find(|i| !c.frames.contains_key(i))
    }

    /// The cached frame to show for `want`: itself, else the nearest before it, else after.
    fn nearest(&self, clip: u32, want: u32) -> Option<u32> {
        let c = self.clips.get(&clip)?;
        if c.frames.contains_key(&want) {
            return Some(want);
        }
        c.frames.range(..want).next_back().map(|(i, _)| *i).or_else(|| c.frames.range(want..).next().map(|(i, _)| *i))
    }
}

/// How many playback frames a clip has at [`PLAY_FPS`] (at least one).
pub(crate) fn frame_count(clip: &Clip) -> u32 {
    ((clip.duration.as_secs_f64() * f64::from(PLAY_FPS)).ceil() as u32).max(1)
}

/// The frame index at `t` of the clip.
pub(crate) fn frame_index(clip: &Clip, t: f32) -> u32 {
    let count = frame_count(clip);
    ((t * count as f32).floor() as u32).min(count - 1)
}

/// Where frame `index` sits in the clip, `0..=1`.
pub(crate) fn frame_time(clip: &Clip, index: u32) -> f32 {
    let count = frame_count(clip);
    if count <= 1 { 0.0 } else { index as f32 / (count - 1) as f32 }
}

/// The Viewer's frame size in pixels for a frame rect.
fn frame_size(frame: Rect, pixels_per_point: f32) -> [u32; 2] {
    let px = frame.size() * pixels_per_point;
    let scale = (PLAY_MAX_SIDE / px.x.max(px.y).max(1.0)).min(1.0);
    [(px.x * scale).round().clamp(16.0, PLAY_MAX_SIDE) as u32, (px.y * scale).round().clamp(16.0, PLAY_MAX_SIDE) as u32]
}

/// A clip's first playback frame on the Cut.
fn clip_base(board: &Storyboard, clip: &Clip) -> u32 {
    (clip_start(board, clip).as_secs_f64() * f64::from(PLAY_FPS)).round() as u32
}

/// The playhead as a frame number on the Cut.
fn playhead_frame(at: Ticks) -> u32 {
    (at.as_secs_f64() * f64::from(PLAY_FPS)).round() as u32
}

/// The key `clip`'s frames must have been rendered for, now.
fn key_for(
    board: &Storyboard,
    clip: &Clip,
    motion: Motion,
    size: [u32; 2],
    prefs: assetcraft_engine::ModelPrefs,
) -> FrameKey {
    FrameKey { motion, size, prefs, base: clip_base(board, clip), count: frame_count(clip) }
}

/// Collect a finished frame, if one landed, into its clip's cache.
fn collect(player: &mut MotionPlayer, now: u32) {
    let Some((path, clip, index)) = player.pending.clone() else { return };
    let Some(model) = player.models.get_mut(&path) else {
        player.pending = None;
        return;
    };
    let Some(got) = model.poll() else { return };
    player.pending = None;
    let Some(c) = player.clips.get_mut(&clip) else { return };
    if got.image.width != c.key.size[0] || got.image.height != c.key.size[1] {
        return;
    }
    let bytes = got.image.rgba.len();
    player.evict_for(now, bytes);
    if let Some(c) = player.clips.get_mut(&clip) {
        c.frames.insert(index, got.image);
        player.bytes += bytes;
    }
}

/// Ask the hero's thread for frame `index` of `clip`.
fn request(
    nav: &mut Navigator,
    clip: &Clip,
    motion: &Motion,
    index: u32,
    size: [u32; 2],
    prefs: assetcraft_engine::ModelPrefs,
) -> bool {
    let Some(hero) = clip.asset_ids.first() else { return false };
    let path = Path::new(hero.0.as_str()).to_path_buf();
    let services = nav.session.services();
    let look = crate::model_preview::look(prefs);
    let view = orbit_view(motion.at(frame_time(clip, index)));
    let Some(player) = nav.state.storyboard.motion_player.as_mut() else { return false };
    let Some(model) = player.model(&services, &path) else { return false };
    model.request_look(view, size[0], size[1], false, &look);
    player.pending = Some((path, clip.id.0, index));
    #[cfg(test)]
    {
        player.first_view.get_or_insert(view);
    }
    true
}

/// Every model clip with motion, in Cut order.
fn motion_clips(board: &Storyboard) -> Vec<Clip> {
    board
        .clips
        .iter()
        .filter(|c| c.motion.is_some() && MotionKind::of_clip(c) == Some(MotionKind::Orbit))
        .cloned()
        .collect()
}

/// Return: render every model clip's frames for the whole Cut (or stop the job that is
/// running). The Viewer must have been drawn once for the frame size to be known.
pub(crate) fn toggle_render_job(nav: &mut Navigator) {
    let Some(board) = nav.session.storyboard().cloned() else { return };
    let player = nav.state.storyboard.motion_player.get_or_insert_with(MotionPlayer::default);
    if player.job.take().is_some() {
        nav.inform("Render stopped");
        return;
    }
    let clips = motion_clips(&board);
    if clips.is_empty() {
        nav.inform("Nothing to render: no 3D clip has motion");
        return;
    }
    let total: u32 = clips.iter().map(frame_count).sum();
    player.job = Some(RenderJob { clips: clips.iter().map(|c| c.id.0).collect(), total });
    nav.inform(format!("Rendering {total} frames · Return or Esc stops"));
}

/// Keep the render job and the cache moving: called every frame the Viewer is drawn, for the
/// Viewer's `frame` rect, with the playhead at `at`. Renders the clip on screen first (if it is
/// a model with motion), then the job's clips in order.
pub(crate) fn tick(nav: &mut Navigator, ui: &Ui, frame: Rect, board: &Storyboard, at: Ticks, on_screen: Option<&Clip>) {
    let ctx = ui.ctx().clone();
    let prefs = nav.session.model();
    let size = frame_size(frame, ctx.pixels_per_point());
    let now = playhead_frame(at);
    let frame_bytes = size[0] as usize * size[1] as usize * 4;
    let player = nav.state.storyboard.motion_player.get_or_insert_with(MotionPlayer::default);
    collect(player, now);
    // Drop frames of clips that no longer exist or lost their motion.
    let live: std::collections::HashSet<u32> = motion_clips(board).iter().map(|c| c.id.0).collect();
    for id in player.clips.keys().copied().collect::<Vec<_>>() {
        if !live.contains(&id) {
            player.drop_clip(id);
        }
    }
    if let Some(job) = player.job.as_mut() {
        job.clips.retain(|c| live.contains(c));
    }
    if player.pending.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        return;
    }
    // 1. The clip on screen.
    if let Some(clip) = on_screen
        && let Some(motion) = clip.motion
        && MotionKind::of_clip(clip) == Some(MotionKind::Orbit)
    {
        let key = key_for(board, clip, motion, size, prefs);
        player.keep_for(clip.id.0, key);
        let want =
            frame_index(clip, progress(clip, (at.as_secs_f64() - clip_start(board, clip).as_secs_f64()).max(0.0)));
        if let Some(index) = player.next_for(clip.id.0, want, frame_bytes) {
            if request(nav, clip, &motion, index, size, prefs) {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(50)); // still loading
            }
            return;
        }
    }
    // 2. The job: the first clip with a missing frame, in Cut order.
    let Some(job) = player.job.clone() else { return };
    for id in &job.clips {
        let Some(clip) = board.clip(assetcraft_storyboard::ClipId(*id)).cloned() else { continue };
        let Some(motion) = clip.motion else { continue };
        let key = key_for(board, &clip, motion, size, prefs);
        let Some(player) = nav.state.storyboard.motion_player.as_mut() else { return };
        player.keep_for(*id, key);
        if player.bytes + frame_bytes > CACHE_BYTES {
            player.job = None;
            let mib = player.bytes >> 20;
            nav.inform(format!("Render cache full at {mib} MiB: {} frames ready", player_total(nav)));
            return;
        }
        let missing = player.frames_in(*id).and_then(|c| (0..c.key.count).find(|i| !c.frames.contains_key(i)));
        if let Some(index) = missing {
            if request(nav, &clip, &motion, index, size, prefs) {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
            return;
        }
    }
    // Every job clip is complete.
    if let Some(player) = nav.state.storyboard.motion_player.as_mut()
        && let Some(job) = player.job.take()
    {
        let mib = player.bytes >> 20;
        nav.inform(format!("Render cache ready: {} frames, {mib} MiB", job.total));
    }
}

fn player_total(nav: &Navigator) -> usize {
    nav.state.storyboard.motion_player.as_ref().map_or(0, MotionPlayer::frame_total)
}

/// The render job's progress for the Viewer, `(done, total)`: frames its clips have in the
/// cache, out of the frames they need.
pub(crate) fn job_progress(nav: &Navigator) -> Option<(u32, u32)> {
    let player = nav.state.storyboard.motion_player.as_ref()?;
    let job = player.job.as_ref()?;
    let done: usize = job.clips.iter().map(|c| player.clips.get(c).map_or(0, |f| f.frames.len())).sum();
    Some(((done as u32).min(job.total), job.total))
}

/// The camera's orbit view: an offset from the shot as the thumbnails frame it (the 3/4 view).
pub(crate) fn orbit_view(cam: Camera) -> assetcraft_thumbs::model::View {
    let base = assetcraft_thumbs::model::View::default();
    assetcraft_thumbs::model::View {
        yaw: base.yaw + cam.yaw,
        pitch: base.pitch + cam.pitch,
        zoom: base.zoom * cam.zoom,
    }
    .orbit(0.0, 0.0)
    .zoomed(1.0)
}

/// Where the clip is, `0..=1`, from the seconds elapsed in it (`f64::INFINITY`, at rest, is the
/// start).
pub(crate) fn progress(clip: &Clip, elapsed: f64) -> f32 {
    if !elapsed.is_finite() || clip.duration.0 == 0 {
        return 0.0;
    }
    (elapsed / clip.duration.as_secs_f64()).clamp(0.0, 1.0) as f32
}

/// Draw `clip`'s hero in `frame` as its motion shows it `elapsed` seconds in. Returns `false`
/// when the clip has no motion or no hero, so the caller draws it the usual way.
pub(crate) fn show(
    nav: &mut Navigator,
    ui: &Ui,
    frame: Rect,
    clip: &Clip,
    thumb: Option<(egui::TextureId, egui::Vec2)>,
    elapsed: f64,
) -> bool {
    let (Some(motion), Some(kind)) = (clip.motion, MotionKind::of_clip(clip)) else {
        return false;
    };
    let t = progress(clip, elapsed);
    let cam = motion.at(t);
    let painter = ui.painter();
    match kind {
        MotionKind::PanZoom => {
            if let Some((texture, size)) = thumb {
                let target = crate::storyboard_view::contain(frame, size);
                painter.image(texture, target, window(cam), Color32::WHITE);
                // At the start, show where the motion ends: the end framing, dashed.
                if t == 0.0 && motion.amount != Camera::NONE {
                    let w = window(motion.end());
                    let start_w = window(cam);
                    // Map the end window into the drawn picture's coordinates.
                    let to = |u: f32, v: f32| {
                        pos2(
                            target.left() + (u - start_w.min.x) / start_w.width() * target.width(),
                            target.top() + (v - start_w.min.y) / start_w.height() * target.height(),
                        )
                    };
                    let corners =
                        [to(w.min.x, w.min.y), to(w.max.x, w.min.y), to(w.max.x, w.max.y), to(w.min.x, w.max.y)];
                    for i in 0..4 {
                        painter.add(egui::Shape::dashed_line(
                            &[corners[i], corners[(i + 1) % 4]],
                            Stroke::new(1.5, crate::storyboard_view::SELECT),
                            5.0,
                            4.0,
                        ));
                    }
                }
            }
        }
        MotionKind::Orbit => {
            let drew = orbit_frame(nav, ui, frame, clip, t);
            if !drew && let Some((texture, size)) = thumb {
                let target = crate::storyboard_view::contain(frame, size);
                painter.image(texture, target, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
        }
    }
    badge(ui, frame, kind, &motion, cam, t);
    progress_line(ui, frame, clip, t);
    true
}

/// Draw the cached frame nearest to the clip's point `t`. `false` until one exists (the caller
/// draws the still). Rendering itself happens in [`tick`].
fn orbit_frame(nav: &mut Navigator, ui: &Ui, frame: Rect, clip: &Clip, t: f32) -> bool {
    let ctx = ui.ctx().clone();
    let Some(player) = nav.state.storyboard.motion_player.as_mut() else { return false };
    let want = frame_index(clip, t);
    let Some(index) = player.nearest(clip.id.0, want) else { return false };
    if player.shown != Some((clip.id.0, index))
        && let Some(image) = player.clips.get(&clip.id.0).and_then(|c| c.frames.get(&index))
    {
        let color =
            egui::ColorImage::from_rgba_unmultiplied([image.width as usize, image.height as usize], &image.rgba);
        match &mut player.texture {
            Some(texture) => texture.set(color, egui::TextureOptions::LINEAR),
            None => {
                player.texture = Some(ctx.load_texture("assetcraft-motion-player", color, egui::TextureOptions::LINEAR))
            }
        }
        player.shown = Some((clip.id.0, index));
    }
    match &player.texture {
        Some(texture) => {
            let target = crate::widgets::fit_at_most(frame, texture.size_vec2() / ctx.pixels_per_point(), 1.0);
            ui.painter().image(
                texture.id(),
                target,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            true
        }
        None => false,
    }
}

/// The mono badge top-left: the camera now, as an offset from Start.
fn badge(ui: &Ui, frame: Rect, kind: MotionKind, motion: &Motion, cam: Camera, t: f32) {
    let s = motion.start;
    let text = match kind {
        MotionKind::Orbit => format!(
            "TURN {:.0}° · RISE {:.0}° · {:.2}×",
            (cam.yaw - s.yaw).to_degrees(),
            (cam.pitch - s.pitch).to_degrees(),
            cam.zoom / s.zoom.max(1e-6)
        ),
        MotionKind::PanZoom if t == 0.0 => {
            let e = motion.end();
            format!("START · END FRAME {:.2}×", e.zoom)
        }
        MotionKind::PanZoom => format!(
            "PAN {:+.0} % {:+.0} % · {:.2}×",
            (cam.pan[0] - s.pan[0]) * 100.0,
            (cam.pan[1] - s.pan[1]) * 100.0,
            cam.zoom
        ),
    };
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(text, FontId::monospace(10.0), Color32::from_gray(0xe0));
    let band = Rect::from_min_size(frame.left_top() + vec2(8.0, 8.0), galley.size() + vec2(12.0, 4.0));
    painter.rect_filled(band, 2, Color32::from_black_alpha(150));
    painter.galley(band.min + vec2(6.0, 2.0), galley, Color32::from_gray(0xe0));
}

/// "Start ▸ End", the elapsed line and "1.2 / 2.4 s" along the frame's bottom edge.
fn progress_line(ui: &Ui, frame: Rect, clip: &Clip, t: f32) {
    let painter = ui.painter();
    let y = frame.bottom() - 10.0;
    let left = painter.text(
        pos2(frame.left() + 16.0, y),
        Align2::LEFT_CENTER,
        "Start ▸ End",
        FontId::monospace(10.0),
        Color32::from_gray(0x8c),
    );
    let secs = clip.duration.as_secs_f64();
    let right = painter.text(
        pos2(frame.right() - 16.0, y),
        Align2::RIGHT_CENTER,
        format!("{:.1} / {secs:.1} s", f64::from(t) * secs),
        FontId::monospace(10.0),
        Color32::from_gray(0x8c),
    );
    let x0 = left.right() + 8.0;
    let x1 = right.left() - 8.0;
    if x1 > x0 {
        painter.rect_filled(Rect::from_min_max(pos2(x0, y - 1.0), pos2(x1, y + 1.0)), 0, Color32::from_gray(0x3a));
        painter.rect_filled(Rect::from_min_max(pos2(x0, y - 1.0), pos2(x0 + (x1 - x0) * t, y + 1.0)), 0, ACCENT);
    }
}
