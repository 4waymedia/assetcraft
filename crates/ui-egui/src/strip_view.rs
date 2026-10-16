//! Storyboard Strip view (design 2b) and Quick fill (design 4a), plus Watch your Cut playback.
//!
//! Stage 4 steps 4–5. Draws only:
//! - **Strip:** Content panel (drag assets), Viewer, Inspector, full-width timeline. Dropping an
//!   asset on a clip or on the Viewer runs `storyboard.place {asset, clip}`.
//! - **Quick fill:** Content panel with the NEXT card; *clicking* an asset places it in the
//!   target clip: the selected clip when one is selected (REPLACING if it already has a shot),
//!   else the next empty clip. The engine then selects the next empty clip, so clicks run on in
//!   order. Timeline controls: `n / N filled`, Undo, Auto-fill rest, Clear. When every clip is
//!   filled the Viewer offers ▶ Watch your Cut.
//! - **Playback:** `storyboard.play` / `.pause {at}` / `.seek` record the transport and start or
//!   stop the track's music; while it plays, the playhead follows the audio clock (or egui's
//!   clock from the recorded start, [`SbUi`], when there is no music), the Viewer shows the clip
//!   under it, and reaching the end runs `storyboard.pause {at: total}`.
//! - **Re-cut:** clip widths animate over 0.4 s to their new lengths, and the result shows as a
//!   toast over the timeline for 3.8 s.
//!
//! The Content panel lists the files of the folder open in the Organizer (from the session's
//! listing: the UI never reads the disk), with the Organizer's thumbnail keys.

use crate::Navigator;
use crate::storyboard_view::{
    ACCENT, INSPECTOR_W, SCENE_RED, SELECT, TILE_BORDER, contain, cover, dashed_rect, inspector, rgb, uv_for_cover,
};
use crate::thumbnails::Cell;
use assetcraft_engine::BoardView;
use assetcraft_fs::EntryKind;
use assetcraft_storyboard::{Clip, ClipId, ShotType, Storyboard, Ticks, next_empty};
use assetcraft_thumbs::ThumbKey;
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Content panel widths (Strip, Quick fill) and grid columns.
const CONTENT_W: f32 = 340.0;
const FILL_CONTENT_W: f32 = 360.0;
const CONTENT_COLS: usize = 4;
/// Timeline scale and strip height.
pub(crate) const PX_PER_SEC: f32 = 44.0;
const STRIP_H: f32 = 150.0;
const RULER_H: f32 = 20.0;
const MARKER_H: f32 = 18.0;
const CONTROLS_H: f32 = 30.0;
const TIMELINE_H: f32 = RULER_H + MARKER_H + STRIP_H + 24.0;
const NEXT_FILL: Color32 = Color32::from_rgb(0x1f, 0x33, 0x50);
/// Re-cut width animation and toast time (design 4a).
const RECUT_ANIM_SECS: f32 = 0.4;
const TOAST_SECS: f64 = 3.8;

/// What an asset drag carries: the file path, as the Organizer lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DragAsset(pub String);

/// Storyboard view plumbing kept by the window (not application state).
#[derive(Debug, Default)]
pub(crate) struct SbUi {
    /// egui time when the current playback started (`None` when not playing).
    pub(crate) play_started: Option<f64>,
    /// Re-cut toast and when it was first drawn.
    pub(crate) toast: Option<(String, Option<f64>)>,
}

/// Where the playhead is now. While the track plays it is the audio clock (so pictures follow
/// the music); otherwise, or until the music comes in, it runs on egui's clock from the recorded
/// start. Reaching the end of the Cut pauses there. Also keeps the engine's track audio in step
/// (`Session::poll_audio`).
pub(crate) fn playhead(nav: &mut Navigator, ctx: &egui::Context) -> Ticks {
    let transport = nav.session.transport();
    let total = nav.session.storyboard().map(Storyboard::total).unwrap_or(Ticks::ZERO);
    if !transport.playing {
        nav.state.storyboard.play_started = None;
        let at = transport.position.min(total);
        nav.session.poll_audio(at);
        return at;
    }
    let now = ctx.input(|i| i.time);
    let started = *nav.state.storyboard.play_started.get_or_insert(now);
    let elapsed = Ticks::from_secs_f64((now - started).max(0.0)).unwrap_or(Ticks::ZERO);
    let mut at = transport.position.checked_add(elapsed).unwrap_or(total);
    nav.session.poll_audio(at.min(total));
    if let Some(clock) = nav.session.audio_clock() {
        // Follow the music, and keep the window's clock aligned in case the music stops.
        at = clock;
        let since = (clock.as_secs_f64() - transport.position.as_secs_f64()).max(0.0);
        nav.state.storyboard.play_started = Some(now - since);
    }
    if at >= total {
        nav.run("storyboard.pause", json!({ "at": total.as_secs_f64() }));
        return total;
    }
    ctx.request_repaint();
    at
}

/// ⏮ ▶/⏸ and the live timecode, for the mode bar.
pub(crate) fn transport_controls(nav: &mut Navigator, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let at = playhead(nav, &ctx);
    let total = nav.session.storyboard().map(Storyboard::total).unwrap_or(Ticks::ZERO);
    let playing = nav.session.transport().playing;
    if ui.button("⏮").on_hover_text("Rewind").clicked() {
        nav.run("storyboard.seek", json!({ "seconds": 0 }));
    }
    let (label, tip) = if playing { ("⏸", "Pause (Space)") } else { ("▶", "Watch your Cut (Space)") };
    if ui.button(label).on_hover_text(tip).clicked() {
        toggle_play(nav, at);
    }
    ui.label(RichText::new(format!("{} / {}", at.timecode(), total.timecode())).font(FontId::monospace(12.0)));
}

/// Space in Storyboard mode, and the ▶/⏸ button.
pub(crate) fn toggle_play(nav: &mut Navigator, at: Ticks) {
    if nav.session.transport().playing {
        nav.run("storyboard.pause", json!({ "at": at.as_secs_f64() }));
    } else {
        nav.run("storyboard.play", json!({}));
    }
}

type Index = HashMap<PathBuf, Option<SystemTime>>;

/// Modification times of the open folder's files, so storyboard thumbnails share the
/// Organizer's cache keys.
fn modified_index(nav: &Navigator) -> Index {
    nav.session.items().iter().filter(|e| e.kind == EntryKind::File).map(|e| (e.path.clone(), e.modified)).collect()
}

fn thumb(nav: &mut Navigator, ui: &Ui, path: &Path, side_pt: f32, index: &Index) -> Cell {
    let ctx = ui.ctx().clone();
    let side = (side_pt * ctx.pixels_per_point()).ceil() as u32;
    let modified = index.get(path).copied().flatten();
    nav.thumbs.get(&ctx, ThumbKey::new(path, side).with_modified(modified), 1)
}

/// Strip or Quick fill, by the session's view.
pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    let Some(board) = nav.session.storyboard().cloned() else { return };
    let fill = nav.session.board_view() == BoardView::Fill;
    let index = modified_index(nav);
    let at = playhead(nav, ui.ctx());
    let height = if fill { TIMELINE_H + CONTROLS_H } else { TIMELINE_H };
    egui::Panel::bottom("sb-timeline").exact_size(height).resizable(false).show(ui, |ui| {
        if fill {
            fill_controls(nav, ui, &board);
        }
        timeline(nav, ui, &board, &index, at);
    });
    let width = if fill { FILL_CONTENT_W } else { CONTENT_W };
    egui::Panel::left("sb-content").exact_size(width).resizable(false).show(ui, |ui| {
        content_panel(nav, ui, &board, &index, fill);
    });
    if !fill {
        egui::Panel::right("sb-inspector").exact_size(INSPECTOR_W).resizable(false).show(ui, |ui| {
            inspector(nav, ui);
        });
    }
    egui::CentralPanel::default().show(ui, |ui| viewer(nav, ui, &board, &index, at, fill));
    toast(nav, ui.ctx(), height);
}

// ---- Content panel -------------------------------------------------------------------------

/// The clip a Quick fill click goes to: the selected clip if it takes shots, else the next
/// empty one. `true` when that clip already has a shot (REPLACING).
fn fill_target(nav: &Navigator, board: &Storyboard) -> Option<(ClipId, bool)> {
    let selected = nav.session.selected_clip().and_then(|id| board.clip(id)).filter(|c| c.shot_type.accepts_assets());
    match selected {
        Some(c) => Some((c.id, !c.asset_ids.is_empty())),
        None => next_empty(board).map(|id| (id, false)),
    }
}

fn content_panel(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, index: &Index, fill: bool) {
    ui.horizontal(|ui| {
        let _ = ui.selectable_label(true, "Content");
        ui.add_enabled(false, egui::Button::selectable(false, "Effects")).on_disabled_hover_text("Later (Stage 5)");
        ui.add_enabled(false, egui::Button::selectable(false, "Titles")).on_disabled_hover_text("Later (step 6)");
    });
    if fill {
        next_card(nav, ui, board);
    }
    let folder = nav.session.location().label();
    ui.weak(format!("From the Organizer: {folder}"));
    ui.separator();
    let files: Vec<PathBuf> =
        nav.session.items().iter().filter(|e| e.kind == EntryKind::File).map(|e| e.path.clone()).collect();
    if files.is_empty() {
        ui.add_space(12.0);
        ui.weak("No files here. Open a folder in the Organizer to fill clips from it.");
        return;
    }
    let mut usage: HashMap<&str, usize> = HashMap::new();
    for c in &board.clips {
        for a in &c.asset_ids {
            *usage.entry(a.0.as_str()).or_default() += 1;
        }
    }
    let target = fill_target(nav, board);
    let gap = 6.0;
    let cell = ((ui.available_width() - gap * (CONTENT_COLS as f32 - 1.0)) / CONTENT_COLS as f32).floor().max(32.0);
    let mut clicked: Option<String> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for row in files.chunks(CONTENT_COLS) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for path in row {
                    let used = usage.get(path.to_string_lossy().as_ref()).copied();
                    if asset_cell(nav, ui, path, cell, used, index, fill) {
                        clicked = Some(path.to_string_lossy().into_owned());
                    }
                }
            });
            ui.add_space(gap);
        }
    });
    if let Some(asset) = clicked {
        match target {
            Some((clip, _)) => nav.run("storyboard.place", json!({ "asset": asset, "clip": clip.0 })),
            None => nav.run("storyboard.place", json!({ "asset": asset })),
        };
    }
}

/// The NEXT card (design 4a): which clip the next click fills, and its brief.
fn next_card(nav: &Navigator, ui: &mut Ui, board: &Storyboard) {
    let target = fill_target(nav, board);
    egui::Frame::NONE.fill(NEXT_FILL).stroke(Stroke::new(1.0, ACCENT)).corner_radius(6).inner_margin(8.0).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            match target.and_then(|(id, replacing)| board.clip(id).map(|c| (c, replacing))) {
                Some((clip, replacing)) => {
                    let n = board.index_of(clip.id).map(|i| i + 1).unwrap_or(0);
                    let head = if replacing { "REPLACING" } else { "NEXT" };
                    ui.label(
                        RichText::new(format!(
                            "{head} · #{n:02} · {} · {:.1}s",
                            clip.shot_type.name(),
                            clip.duration.as_secs_f64()
                        ))
                        .font(FontId::monospace(11.0))
                        .color(Color32::WHITE),
                    );
                    if !clip.description.is_empty() {
                        ui.label(RichText::new(&clip.description).color(Color32::from_gray(0xdd)));
                    }
                    ui.label(RichText::new("Click an asset to place it").small().color(Color32::from_gray(0x8a)));
                }
                None => {
                    let (_, n) = board.fill_progress();
                    ui.label(RichText::new(format!("All {n} clips filled")).color(Color32::WHITE));
                    ui.label(RichText::new("Click a clip in the timeline to replace its shot").small());
                }
            }
        },
    );
    ui.add_space(6.0);
}

/// Widget id of a Quick fill asset cell.
pub(crate) fn asset_cell_id(path: &Path) -> egui::Id {
    egui::Id::new(("sb-fill-asset", path))
}

/// Widget id of a clip in the timeline.
pub(crate) fn strip_clip_id(id: ClipId) -> egui::Id {
    egui::Id::new(("sb-strip-clip", id.0))
}

/// One asset cell. Strip: a drag source. Quick fill: a click target (returns `true` when clicked).
fn asset_cell(
    nav: &mut Navigator,
    ui: &mut Ui,
    path: &Path,
    side: f32,
    used: Option<usize>,
    index: &Index,
    clickable: bool,
) -> bool {
    let name = assetcraft_fs::display_name(path);
    let cell = thumb(nav, ui, path, side, index);
    let draw = |ui: &mut Ui| {
        let (rect, hover) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
        // A stable id (the path) so tests and future keyboard focus can find the cell.
        let response = if clickable { ui.interact(rect, asset_cell_id(path), Sense::click()) } else { hover };
        let painter = ui.painter();
        painter.rect_filled(rect, 4, Color32::from_gray(0x15));
        match &cell {
            Cell::Ready(texture, size) => {
                let target = cover(rect, *size);
                painter.image(*texture, rect, uv_for_cover(rect, target), Color32::WHITE);
            }
            Cell::Loading => {}
            Cell::Failed(_) => {
                let ext = path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
                painter.text(rect.center(), Align2::CENTER_CENTER, ext, FontId::monospace(10.0), Color32::GRAY);
            }
        }
        if let Some(n) = used {
            let chip = Rect::from_min_size(rect.right_top() + vec2(-26.0, 3.0), vec2(23.0, 14.0));
            painter.rect_filled(chip, 7, ACCENT);
            painter.text(chip.center(), Align2::CENTER_CENTER, format!("×{n}"), FontId::monospace(9.0), Color32::WHITE);
        }
        if response.contains_pointer() {
            painter.rect_stroke(rect, 4, Stroke::new(2.0, SELECT), StrokeKind::Inside);
        }
        response
    };
    if clickable {
        let response = draw(ui).on_hover_text(format!("{name}\nClick to place in the target clip"));
        return response.clicked();
    }
    let payload = DragAsset(path.to_string_lossy().into_owned());
    let inner = ui.dnd_drag_source(egui::Id::new(("sb-asset", path)), payload, |ui| {
        let _ = draw(ui);
    });
    inner.response.on_hover_text(format!("{name}\nDrag onto a clip in the timeline, or onto the Viewer"));
    false
}

// ---- Viewer --------------------------------------------------------------------------------

fn viewer(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, index: &Index, at: Ticks, fill: bool) {
    let avail = ui.available_rect_before_wrap().shrink(8.0);
    let w = avail.width().min(avail.height() * 16.0 / 9.0).max(16.0);
    let frame = Rect::from_center_size(avail.center(), vec2(w, w * 9.0 / 16.0));
    let response = ui.interact(frame, ui.id().with("sb-viewer"), Sense::hover());
    ui.painter().rect_filled(frame, 0, Color32::BLACK);
    let playing = nav.session.transport().playing;
    let paused_mid = !playing && at > Ticks::ZERO && at < board.total();
    if playing || paused_mid {
        if let Some(clip) = board.clip_at(at).cloned() {
            let start = board.index_of(clip.id).and_then(|i| board.starts().get(i).copied()).unwrap_or(Ticks::ZERO);
            let elapsed = at.as_secs_f64() - start.as_secs_f64();
            show_clip(nav, ui, frame, &clip, index, elapsed);
        }
        return;
    }
    if fill {
        fill_brief(nav, ui, frame, board);
        return;
    }
    let selected = nav.session.selected_clip().and_then(|id| board.clip(id)).cloned();
    match &selected {
        None => {
            ui.painter().text(
                frame.center(),
                Align2::CENTER_CENTER,
                "Select a clip",
                FontId::proportional(14.0),
                Color32::GRAY,
            );
        }
        Some(clip) => show_clip(nav, ui, frame, clip, index, f64::INFINITY),
    }
    // Dropping on the Viewer places into the selected clip.
    if let Some(clip) = selected.filter(|c| c.shot_type.accepts_assets()) {
        if response.dnd_hover_payload::<DragAsset>().is_some() {
            ui.painter().rect_stroke(frame, 0, Stroke::new(2.0, SELECT), StrokeKind::Outside);
        }
        if let Some(asset) = response.dnd_release_payload::<DragAsset>() {
            nav.run("storyboard.place", json!({ "asset": asset.0, "clip": clip.id.0 }));
        }
    }
}

/// A clip as the Cut shows it, `elapsed` seconds in (`f64::INFINITY` = at rest): a Card, or the
/// hero frame (or "No shot assigned") with any Overlay text on top.
fn show_clip(nav: &mut Navigator, ui: &mut Ui, frame: Rect, clip: &Clip, index: &Index, elapsed: f64) {
    if clip.shot_type == ShotType::Ttl {
        crate::text_view::paint_text(ui.painter(), frame, clip, elapsed);
        return;
    }
    match clip.asset_ids.first() {
        Some(hero) => {
            if let Cell::Ready(texture, size) = thumb(nav, ui, Path::new(&hero.0), 512.0, index) {
                let target = contain(frame, size);
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                ui.painter().image(texture, target, uv, Color32::WHITE);
            }
        }
        None => {
            let painter = ui.painter();
            painter.text(
                frame.center() - vec2(0.0, 10.0),
                Align2::CENTER_CENTER,
                "No shot assigned",
                FontId::proportional(16.0),
                Color32::GRAY,
            );
            painter.text(
                frame.center() + vec2(0.0, 14.0),
                Align2::CENTER_CENTER,
                clip.description.as_str(),
                FontId::proportional(12.0),
                Color32::from_gray(0x8a),
            );
        }
    }
    crate::text_view::paint_text(ui.painter(), frame, clip, elapsed);
}

/// Quick fill Viewer: the target clip's brief in a dashed frame, or "All n clips filled" with
/// ▶ Watch your Cut.
fn fill_brief(nav: &mut Navigator, ui: &mut Ui, frame: Rect, board: &Storyboard) {
    let (filled, fillable) = board.fill_progress();
    if filled == fillable && fillable > 0 {
        let painter = ui.painter();
        painter.text(
            frame.center() - vec2(0.0, 24.0),
            Align2::CENTER_CENTER,
            format!("All {fillable} clips filled"),
            FontId::proportional(22.0),
            Color32::WHITE,
        );
        let button = Rect::from_center_size(frame.center() + vec2(0.0, 20.0), vec2(180.0, 34.0));
        let watch = egui::Button::new(RichText::new("▶ Watch your Cut").color(Color32::WHITE).size(15.0)).fill(ACCENT);
        if ui.put(button, watch).clicked() {
            nav.run("storyboard.play", json!({ "from": 0 }));
        }
        return;
    }
    let Some((id, replacing)) = fill_target(nav, board) else { return };
    let Some(clip) = board.clip(id) else { return };
    let inner = frame.shrink(24.0);
    dashed_rect(ui, inner, SELECT);
    let n = board.index_of(id).map(|i| i + 1).unwrap_or(0);
    let painter = ui.painter();
    let head = if replacing { "REPLACING" } else { "NEXT SHOT" };
    painter.text(
        inner.left_top() + vec2(16.0, 14.0),
        Align2::LEFT_TOP,
        format!("{head} · #{n:02} · {}", clip.shot_type.name()),
        FontId::monospace(12.0),
        SELECT,
    );
    let desc = if clip.description.is_empty() { "No description yet".to_owned() } else { clip.description.clone() };
    let galley = painter.layout(desc, FontId::proportional(20.0), Color32::from_gray(0xf0), inner.width() - 32.0);
    let y = inner.top() + 40.0;
    let h = galley.size().y;
    painter.galley(pos2(inner.left() + 16.0, y), galley, Color32::WHITE);
    let angles = if clip.suggested_angles.is_empty() {
        "Suggested angles: none yet (AI suggestions come with Stage 5)".to_owned()
    } else {
        format!("Suggested angles: {}", clip.suggested_angles.join(" · "))
    };
    painter.text(
        pos2(inner.left() + 16.0, y + h + 12.0),
        Align2::LEFT_TOP,
        angles,
        FontId::proportional(12.0),
        Color32::from_gray(0x8a),
    );
}

// ---- Timeline ------------------------------------------------------------------------------

/// Quick fill controls above the timeline: `n / N filled`, progress, Undo, Auto-fill rest, Clear.
fn fill_controls(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard) {
    let (filled, fillable) = board.fill_progress();
    ui.horizontal(|ui| {
        ui.set_height(CONTROLS_H - 6.0);
        ui.label(RichText::new(format!("{filled} / {fillable} filled")).font(FontId::monospace(11.0)));
        let (bar, _) = ui.allocate_exact_size(vec2(180.0, 4.0), Sense::hover());
        ui.painter().rect_filled(bar, 2, Color32::from_gray(0x2c));
        let frac = if fillable == 0 { 0.0 } else { filled as f32 / fillable as f32 };
        let shown = ui.ctx().animate_value_with_time(ui.id().with("sb-fill-progress"), frac, 0.25);
        ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * shown, bar.height())), 2, ACCENT);
        ui.add_space(8.0);
        if ui.add_enabled(nav.session.can_undo_storyboard(), egui::Button::new("Undo")).clicked() {
            nav.run("storyboard.undo", json!({}));
        }
        let autofill = ui
            .add_enabled(filled < fillable, egui::Button::new("Auto-fill rest"))
            .on_hover_text("Fill every empty clip from the Organizer folder: unused assets first");
        if autofill.clicked() {
            nav.run("storyboard.autofill", json!({}));
        }
        let clear =
            ui.add_enabled(filled > 0, egui::Button::new("Clear")).on_hover_text("Remove every shot (Ctrl+Z undoes)");
        if clear.clicked() {
            nav.run("storyboard.clear", json!({}));
        }
    });
}

/// Clip spans along the timeline from `left`, in Cut order: `(id, x, width)`. Widths animate
/// toward their new lengths after a Re-cut (`ctx = None` gives the final layout).
pub(crate) fn clip_spans(board: &Storyboard, left: f32, ctx: Option<&egui::Context>) -> Vec<(ClipId, f32, f32)> {
    let mut x = left;
    board
        .clips
        .iter()
        .map(|c| {
            let target = (c.duration.as_secs_f64() as f32 * PX_PER_SEC).max(2.0);
            let w = match ctx {
                Some(ctx) => ctx.animate_value_with_time(egui::Id::new(("sb-clip-w", c.id.0)), target, RECUT_ANIM_SECS),
                None => target,
            };
            let span = (c.id, x, w);
            x += w;
            span
        })
        .collect()
}

fn timeline(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, index: &Index, at: Ticks) {
    let total = board.total().as_secs_f64() as f32;
    let track_len = board.track.as_ref().map(|t| t.duration.as_secs_f64() as f32).unwrap_or(0.0);
    let width = (total.max(track_len) * PX_PER_SEC + 80.0).max(ui.available_width());
    let selected = nav.session.selected_clip();
    let playing = nav.session.transport().playing;
    let mut select: Option<ClipId> = None;
    let mut drop: Option<(ClipId, String)> = None;
    let mut switch: Option<(Clip, assetcraft_storyboard::TextKind)> = None;
    let ctx = ui.ctx().clone();
    egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
        let (area, _) = ui.allocate_exact_size(vec2(width, RULER_H + MARKER_H + STRIP_H), Sense::hover());
        let left = area.left() + 8.0;
        let painter = ui.painter().clone();
        let weak = ui.visuals().weak_text_color();
        // Ruler: a tick per second, a label every 5 s.
        let secs = (width / PX_PER_SEC).ceil() as i32;
        for s in 0..=secs {
            let x = left + s as f32 * PX_PER_SEC;
            let long = s % 5 == 0;
            let top = area.top() + if long { 4.0 } else { 12.0 };
            painter.line_segment([pos2(x, top), pos2(x, area.top() + RULER_H)], Stroke::new(1.0, weak));
            if long {
                let label = format!("{}:{:02}", s / 60, s % 60);
                painter.text(pos2(x + 3.0, area.top()), Align2::LEFT_TOP, label, FontId::monospace(10.0), weak);
            }
        }
        if track_len > 0.0 {
            let x = left + track_len * PX_PER_SEC;
            painter.line_segment([pos2(x, area.top()), pos2(x, area.bottom())], Stroke::new(1.0, ACCENT));
        }
        // Scene markers and clips.
        let strip_top = area.top() + RULER_H + MARKER_H;
        let marker_y = area.top() + RULER_H + 2.0;
        let mut prev_scene = None;
        let spans = clip_spans(board, left, Some(&ctx));
        for ((id, x, w), (n, clip)) in spans.into_iter().zip(board.clips.iter().enumerate()) {
            if prev_scene != Some(clip.scene_id) && clip.text_kind().is_some() {
                let title = board.scene_title(clip.scene_id).unwrap_or_default();
                painter.text(pos2(x, marker_y), Align2::LEFT_TOP, "§", FontId::monospace(11.0), SCENE_RED);
                let clip_rect = Rect::from_min_size(pos2(x + 10.0, area.top() + RULER_H), vec2(240.0, MARKER_H));
                let label = painter.with_clip_rect(clip_rect);
                label.text(pos2(x + 10.0, marker_y), Align2::LEFT_TOP, title, FontId::proportional(11.0), weak);
            }
            prev_scene = Some(clip.scene_id);
            let rect = Rect::from_min_size(pos2(x, strip_top), vec2(w, STRIP_H)).shrink2(vec2(1.0, 0.0));
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let response = ui.interact(rect, strip_clip_id(id), Sense::click());
            paint_clip(nav, ui, rect, n + 1, clip, index);
            let accepts = clip.shot_type.accepts_assets();
            let hovering = accepts && response.dnd_hover_payload::<DragAsset>().is_some();
            if selected == Some(id) || hovering {
                ui.painter().rect_stroke(rect, 3, Stroke::new(2.0, SELECT), StrokeKind::Inside);
            }
            if accepts && let Some(asset) = response.dnd_release_payload::<DragAsset>() {
                drop = Some((id, asset.0.clone()));
            }
            let tip = format!("#{:02} · {}\n{}", n + 1, clip.shot_type.name(), clip.description);
            if response.on_hover_text(tip).clicked() {
                select = Some(id);
            }
            if let Some(to) = crate::text_view::kind_chip(
                ui,
                Rect::from_min_max(rect.min + vec2(0.0, 4.0), rect.max),
                clip,
                strip_clip_id(id).with("kind"),
            ) {
                switch = Some((clip.clone(), to));
            }
        }
        let x = left + at.as_secs_f64() as f32 * PX_PER_SEC;
        painter.line_segment([pos2(x, area.top()), pos2(x, area.bottom())], Stroke::new(2.0, SELECT));
        if playing {
            ui.scroll_to_rect(Rect::from_min_size(pos2(x - 40.0, area.top()), vec2(200.0, 10.0)), None);
        }
    });
    if let Some((clip, to)) = switch {
        crate::text_view::switch_kind(nav, &clip, to);
    } else if let Some((clip, asset)) = drop {
        nav.run("storyboard.place", json!({ "asset": asset, "clip": clip.0 }));
    } else if let Some(id) = select {
        nav.run("storyboard.select", json!({ "clip": id.0 }));
    }
}

fn paint_clip(nav: &mut Navigator, ui: &Ui, rect: Rect, number: usize, clip: &Clip, index: &Index) {
    let painter = ui.painter().with_clip_rect(rect);
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 4.0));
    let body = Rect::from_min_max(rect.min + vec2(0.0, 4.0), rect.max);
    let bar_color = match clip.shot_type {
        ShotType::Est => TILE_BORDER,
        ShotType::Ins => Color32::from_rgb(0x6b, 0x72, 0x80),
        t => t.fill_rgb().map(rgb).unwrap_or(TILE_BORDER),
    };
    let title = clip.shot_type == ShotType::Ttl;
    let look = clip.text_style.theme.look();
    painter.rect_filled(body, 0, if title { rgb(look.background) } else { Color32::from_gray(0x22) });
    painter.rect_filled(bar, 0, bar_color);
    let mut drew = false;
    if let Some(hero) = clip.asset_ids.first() {
        let thumb_rect = Rect::from_min_size(body.min, vec2(body.width(), body.height() - 34.0));
        if let Cell::Ready(texture, size) = thumb(nav, ui, Path::new(&hero.0), 180.0, index) {
            let target = cover(thumb_rect, size);
            painter.image(texture, thumb_rect, uv_for_cover(thumb_rect, target), Color32::WHITE);
            drew = true;
        }
    }
    let text = if title { rgb(look.text) } else { Color32::from_gray(0xdd) };
    if !drew && !title {
        let well = Rect::from_min_size(body.min + vec2(4.0, 4.0), vec2(body.width() - 8.0, body.height() - 42.0));
        if well.width() > 20.0 {
            painter.rect_stroke(well, 3, Stroke::new(1.0, Color32::from_gray(0x55)), StrokeKind::Inside);
            let code = clip.shot_type.code();
            painter.text(
                well.center(),
                Align2::CENTER_CENTER,
                code,
                FontId::proportional(14.0),
                Color32::from_gray(0x8a),
            );
        }
    }
    if title {
        let wrap = (body.width() - 10.0).max(10.0);
        let galley = painter.layout(clip.description.clone(), FontId::proportional(12.0), text, wrap);
        painter.galley(body.min + vec2(5.0, 22.0), galley, text);
    }
    if let Some(words) = clip.overlay.as_deref() {
        overlay_band(&painter, body, words);
    }
    let info = pos2(body.left() + 5.0, body.bottom() - 30.0);
    let label = format!("#{number:02} {}", clip.shot_type.code());
    painter.text(info, Align2::LEFT_TOP, label, FontId::monospace(10.0), text);
    let secs = format!("{:.1}s", clip.duration.as_secs_f64());
    painter.text(info + vec2(0.0, 14.0), Align2::LEFT_TOP, secs, FontId::monospace(10.0), Color32::from_gray(0x8a));
    let border = if title { rgb(0xe5243b) } else { Color32::from_gray(0x3a) };
    painter.rect_stroke(rect, 3, Stroke::new(1.0, border), StrokeKind::Inside);
}

/// An Overlay's text as a band across the bottom of a thumbnail.
pub(crate) fn overlay_band(painter: &egui::Painter, rect: Rect, words: &str) {
    let band = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 46.0), pos2(rect.right(), rect.bottom() - 34.0));
    if band.width() < 12.0 {
        return;
    }
    painter.rect_filled(band, 0, Color32::from_black_alpha(190));
    let clipped = painter.with_clip_rect(band.intersect(painter.clip_rect()));
    clipped.text(
        band.left_center() + vec2(4.0, 0.0),
        Align2::LEFT_CENTER,
        format!("T  {words}"),
        FontId::proportional(9.0),
        Color32::WHITE,
    );
}

// ---- Toast ---------------------------------------------------------------------------------

/// The Re-cut toast, centred just above the timeline, for [`TOAST_SECS`].
pub(crate) fn toast(nav: &mut Navigator, ctx: &egui::Context, timeline_h: f32) {
    let now = ctx.input(|i| i.time);
    let Some((text, shown)) = &mut nav.state.storyboard.toast else { return };
    let since = *shown.get_or_insert(now);
    if now - since > TOAST_SECS {
        nav.state.storyboard.toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    let text = text.clone();
    egui::Area::new(egui::Id::new("sb-toast"))
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -(timeline_h + 34.0)))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(Color32::from_rgb(0x25, 0x25, 0x25))
                .stroke(Stroke::new(1.0, ACCENT))
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.label(RichText::new(text).color(Color32::from_gray(0xf0)));
                });
        });
}
