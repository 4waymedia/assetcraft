//! Storyboard Strip view (design 2b) and Quick fill (design 4a), plus Watch your Cut playback.
//!
//! Stage 4 steps 4–5. Draws only:
//! - **Strip:** Content panel, Viewer, Inspector, full-width timeline. *Clicking* (or
//!   Shift+clicking) an asset places it in the target clip, as in Quick fill; *dragging* it onto
//!   a clip or the Viewer runs `storyboard.place {asset, clip}` (a title card takes it as its
//!   background). Shift+click in the timeline selects a run of clips; Delete / Backspace runs
//!   `storyboard.delete` (shots first, then clips). Clicking or dragging on the ruler moves the
//!   playhead (findings B1–B5, 2026-10-17).
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
//! - **INSET row (SR2b, D-SR1 always there):** under the strip, one slot per section with
//!   shots, spanning them; drop an Organizer file on it → `storyboard.inset {scene, asset}` (a
//!   video's own length sizes the section). **AUDIO row:** the track under the Cut — each piece
//!   of its edit (the chorus loop, SR3) with its waveform, section bands and splice marks.
//! - **Resize (SR1):** dragging a card's right edge sets that clip's length through
//!   `storyboard.resize` when the drag ends (one undo step); with a track the edge snaps to
//!   whole beats, Shift drags free in tenths of a second. The Inspector's Duration field runs
//!   the same command. The card follows the pointer while dragging ([`ResizeDrag`]).
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
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
/// The INSET and AUDIO rows under the strip (SR2b; Paul, 2026-11-06: "when the storyboard is
/// created, we need to have the audio track, and the inset track available").
const INSET_H: f32 = 36.0;
const AUDIO_H: f32 = 40.0;
/// The VOICE row under AUDIO (SR4b): the voiceover, or a drop target for one.
const VOICE_H: f32 = 24.0;
pub(crate) const ROWS_H: f32 = INSET_H + AUDIO_H + VOICE_H + 12.0;
const CONTROLS_H: f32 = 30.0;
const TIMELINE_H: f32 = RULER_H + MARKER_H + STRIP_H + ROWS_H + 24.0;
const NEXT_FILL: Color32 = Color32::from_rgb(0x1f, 0x33, 0x50);
/// Picture-to-music sync: drift beyond this snaps the picture to the music; less is slewed away
/// at this fraction per repaint (≈ 60 ms to close a 20 ms gap at 60 Hz, invisible).
const SYNC_SNAP_SECS: f64 = 0.08;
const SYNC_SLEW: f64 = 0.15;
/// Re-cut width animation and toast time (design 4a).
const RECUT_ANIM_SECS: f32 = 0.4;
/// The resize handle on a card's right edge (SR1): its grab width.
const HANDLE_W: f32 = 8.0;
const TOAST_SECS: f64 = 3.8;

/// A clip length being dragged (SR1): the clip and the length under the pointer, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ResizeDrag {
    pub(crate) clip: u32,
    pub(crate) secs: f64,
}

/// The length a drag of `secs` lands on: whole beats of the track when there is one (unless
/// `free`), else tenths of a second; never under the engine's minimum.
pub(crate) fn snap_length(board: &Storyboard, secs: f64, free: bool) -> f64 {
    let min = assetcraft_engine::MIN_CLIP_SECS;
    match board.track.as_ref().filter(|_| !free) {
        Some(t) => {
            let beat = t.bpm.beat().as_secs_f64();
            let beats = (secs / beat).round().max(1.0);
            (beats * beat).max(min)
        }
        None => ((secs * 10.0).round() / 10.0).max(min),
    }
}

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
    /// The Start storyboard screen (design 3b / 3c).
    pub(crate) start: crate::start_view::StartUi,
    /// The open folder's modification times, built once per session generation (finding B1:
    /// rebuilding it every frame made dragging sluggish in big folders).
    pub(crate) index: Option<(u64, Arc<Index>)>,
    /// Scrubbing the ruler started while the Cut was playing: play on from where it stops.
    pub(crate) scrub_resume: bool,
    /// An Inspector amount slider mid-drag (SB-3D1), committed as one edit when it ends.
    pub(crate) motion_drag: Option<crate::motion_view::MotionDrag>,
    /// A clip's length mid-drag (SR1: a card's edge or the Inspector's Duration), committed as
    /// one `storyboard.resize` when the drag ends.
    pub(crate) resize_drag: Option<ResizeDrag>,
    /// The Viewer's 3D model while a clip with motion shows (SB-3D1 step 3).
    pub(crate) motion_player: Option<crate::motion_view::MotionPlayer>,
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
        // The music is the master clock, but it advances in device-buffer steps (10–40 ms), so
        // the picture runs on the window's smooth clock and only *follows* the music: a small
        // drift is slewed away over a few frames, a large one (a stall, a device change) is
        // snapped. Reading the audio clock straight made frames repeat and skip (Paul, 2026-11-05).
        let drift = clock.as_secs_f64() - at.as_secs_f64();
        let correction = if drift.abs() > SYNC_SNAP_SECS { drift } else { drift * SYNC_SLEW };
        if correction != 0.0 {
            nav.state.storyboard.play_started = Some(started - correction);
            let elapsed = Ticks::from_secs_f64((now - (started - correction)).max(0.0)).unwrap_or(Ticks::ZERO);
            at = transport.position.checked_add(elapsed).unwrap_or(total);
        }
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

pub(crate) type Index = HashMap<PathBuf, Option<SystemTime>>;

/// Modification times of the open folder's files, so storyboard thumbnails share the
/// Organizer's cache keys. Cached until the session's generation moves (any command, including
/// the folder watcher's `nav.changed`).
fn modified_index(nav: &mut Navigator) -> Arc<Index> {
    let generation = nav.session.generation();
    if let Some((g, index)) = &nav.state.storyboard.index
        && *g == generation
    {
        return Arc::clone(index);
    }
    let index: Arc<Index> = Arc::new(
        nav.session
            .items()
            .iter()
            .filter(|e| e.kind == EntryKind::File)
            .map(|e| (e.path.clone(), e.modified))
            .collect(),
    );
    nav.state.storyboard.index = Some((generation, Arc::clone(&index)));
    index
}

fn thumb(nav: &mut Navigator, ui: &Ui, path: &Path, side_pt: f32, index: &Index) -> Cell {
    let ctx = ui.ctx().clone();
    let side = (side_pt * ctx.pixels_per_point()).ceil() as u32;
    let modified = index.get(path).copied().flatten();
    nav.thumbs.get(&ctx, ThumbKey::new(path, side).with_modified(modified), 1)
}

/// The Content panel's source folder, as a menu: the Organizer's open folder, switchable here
/// to a recent folder, a My Folders pin or a known folder without leaving Storyboard mode (Paul,
/// 2026-11-04: going back to the Organizer to change it "is somewhat odd behavior"), or
/// "Browse in the Organizer…" for anything else.
fn source_folder(nav: &mut Navigator, ui: &mut Ui) {
    let folder = nav.session.location().label();
    let mut open: Option<PathBuf> = None;
    let mut browse = false;
    ui.horizontal(|ui| {
        ui.weak("From:");
        ui.menu_button(format!("{folder} ▾"), |ui| {
            ui.set_min_width(220.0);
            let here = nav.session.location().path().map(Path::to_path_buf);
            let mut group = |ui: &mut Ui, title: &str, paths: &[PathBuf]| {
                if paths.is_empty() {
                    return;
                }
                ui.label(RichText::new(title).size(10.0).weak());
                for p in paths {
                    let current = here.as_deref() == Some(p.as_path());
                    if ui
                        .selectable_label(current, assetcraft_fs::display_name(p))
                        .on_hover_text(p.display().to_string())
                        .clicked()
                    {
                        open = Some(p.clone());
                        ui.close();
                    }
                }
                ui.separator();
            };
            let recent: Vec<PathBuf> = nav.session.recent().to_vec();
            group(ui, "RECENT", &recent);
            let mine: Vec<PathBuf> = nav.session.favorites().to_vec();
            group(ui, "MY FOLDERS", &mine);
            let known: Vec<PathBuf> = nav.session.known_folders().iter().map(|(_, p)| p.clone()).collect();
            group(ui, "FOLDERS", &known);
            if ui.button("Browse in the Organizer…").clicked() {
                browse = true;
                ui.close();
            }
        })
        .response
        .on_hover_text("The folder the Content panel shows; pick another without leaving the storyboard");
    });
    if let Some(p) = open {
        nav.run("nav.open", json!({ "path": p.to_string_lossy() }));
    }
    if browse {
        nav.run("app.mode", json!({ "mode": "organizer" }));
    }
}

/// Strip or Quick fill, by the session's view.
pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    let Some(board) = nav.session.storyboard().cloned() else { return };
    let fill = nav.session.board_view() == BoardView::Fill;
    let index = modified_index(nav);
    let index = index.as_ref();
    let at = playhead(nav, ui.ctx());
    let height = if fill { TIMELINE_H + CONTROLS_H } else { TIMELINE_H };
    egui::Panel::bottom("sb-timeline").exact_size(height).resizable(false).show(ui, |ui| {
        if fill {
            fill_controls(nav, ui, &board);
        }
        timeline(nav, ui, &board, index, at);
    });
    let width = if fill { FILL_CONTENT_W } else { CONTENT_W };
    egui::Panel::left("sb-content").exact_size(width).resizable(false).show(ui, |ui| {
        content_panel(nav, ui, &board, index, fill);
    });
    if !fill {
        egui::Panel::right("sb-inspector").exact_size(INSPECTOR_W).resizable(false).show(ui, |ui| {
            inspector(nav, ui);
        });
    }
    egui::CentralPanel::default().show(ui, |ui| viewer(nav, ui, &board, index, at, fill));
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
    source_folder(nav, ui);
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
    let rows: Vec<&[PathBuf]> = files.chunks(CONTENT_COLS).collect();
    // Only the rows on screen are laid out and asked for thumbnails (finding B1).
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, cell + gap, rows.len(), |ui, range| {
        for row in &rows[range] {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for path in row.iter() {
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

/// One asset cell, in both views: click (or Shift+click) to place it in the target clip
/// (returns `true`), or drag it onto a clip or the Viewer. The drag carries a [`DragAsset`]
/// and a small thumbnail follows the pointer.
fn asset_cell(
    nav: &mut Navigator,
    ui: &mut Ui,
    path: &Path,
    side: f32,
    used: Option<usize>,
    index: &Index,
    fill: bool,
) -> bool {
    let name = assetcraft_fs::display_name(path);
    let cell = thumb(nav, ui, path, side, index);
    let draw = |ui: &mut Ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
        // A stable id (the path) so tests and future keyboard focus can find the cell.
        let response = ui.interact(rect, asset_cell_id(path), Sense::click_and_drag());
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
    let response = draw(ui);
    if response.dragged() {
        response.dnd_set_drag_payload(DragAsset(path.to_string_lossy().into_owned()));
        drag_ghost(ui.ctx(), &cell);
    }
    let tip = if fill {
        format!("{name}\nClick to place in the target clip")
    } else {
        format!("{name}\nClick to place in the next clip, or drag onto a clip or the Viewer")
    };
    response.on_hover_text(tip).clicked()
}

/// The dragged asset's thumbnail beside the pointer (one image, on the tooltip layer).
fn drag_ghost(ctx: &egui::Context, cell: &Cell) {
    let Some(pos) = ctx.pointer_interact_pos() else { return };
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("sb-drag-ghost")));
    let rect = Rect::from_min_size(pos + vec2(12.0, 12.0), vec2(72.0, 72.0));
    painter.rect_filled(rect, 4, Color32::from_gray(0x15));
    if let Cell::Ready(texture, size) = cell {
        let target = cover(rect, *size);
        painter.image(*texture, rect, uv_for_cover(rect, target), Color32::from_white_alpha(220));
    }
    painter.rect_stroke(rect, 4, Stroke::new(2.0, SELECT), StrokeKind::Outside);
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
    // Keep the motion cache and any render job moving, the clip about to show first.
    let on_screen = if playing || paused_mid {
        board.clip_at(at).cloned()
    } else if fill {
        None
    } else {
        nav.session
            .selected_clip()
            .and_then(|id| board.clip(id))
            .or_else(|| board.clip_at(at.min(board.total())))
            .or_else(|| board.clips.first())
            .cloned()
    };
    crate::motion_view::tick(nav, ui, frame, board, at, on_screen.as_ref());
    if let Some((done, total)) = crate::motion_view::job_progress(nav) {
        render_progress(ui, frame, done, total);
    }
    crate::export_view::progress(nav, ui, frame);
    if playing || paused_mid {
        if let Some(clip) = board.clip_at(at).cloned() {
            let start = board.index_of(clip.id).and_then(|i| board.starts().get(i).copied()).unwrap_or(Ticks::ZERO);
            let elapsed = at.as_secs_f64() - start.as_secs_f64();
            show_section(nav, ui, frame, board, &clip, index, elapsed);
        }
        return;
    }
    if fill {
        fill_brief(nav, ui, frame, board);
        return;
    }
    let selected = nav.session.selected_clip().and_then(|id| board.clip(id)).cloned();
    match &selected {
        // Nothing selected: the frame under the playhead (the first clip of a new board).
        None => match board.clip_at(at.min(board.total())).or_else(|| board.clips.first()).cloned() {
            Some(clip) => show_section(nav, ui, frame, board, &clip, index, f64::INFINITY),
            None => {
                ui.painter().text(
                    frame.center(),
                    Align2::CENTER_CENTER,
                    "Select a clip",
                    FontId::proportional(14.0),
                    Color32::GRAY,
                );
            }
        },
        Some(clip) => show_section(nav, ui, frame, board, clip, index, f64::INFINITY),
    }
    // Dropping on the Viewer places into the selected clip (a title card's background).
    if let Some(clip) = selected.filter(|c| c.shot_type.accepts_drop()) {
        if response.dnd_hover_payload::<DragAsset>().is_some() {
            ui.painter().rect_stroke(frame, 0, Stroke::new(2.0, SELECT), StrokeKind::Outside);
        }
        if let Some(asset) = response.dnd_release_payload::<DragAsset>() {
            nav.run("storyboard.place", json!({ "asset": asset.0, "clip": clip.id.0 }));
        }
    }
}

/// The render job's bar along the Viewer's top edge: "Rendering 34 / 240 · Return or Esc stops".
fn render_progress(ui: &Ui, frame: Rect, done: u32, total: u32) {
    let painter = ui.painter();
    let bar = Rect::from_min_size(frame.left_top(), vec2(frame.width(), 3.0));
    painter.rect_filled(bar, 0, Color32::from_gray(0x3a));
    let part = if total == 0 { 1.0 } else { (done as f32 / total as f32).clamp(0.0, 1.0) };
    painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * part, 3.0)), 0, ACCENT);
    let text = format!("Rendering {done} / {total} · Return or Esc stops");
    let galley = painter.layout_no_wrap(text, FontId::monospace(10.0), Color32::from_gray(0xe0));
    let band =
        Rect::from_min_size(frame.right_top() + vec2(-galley.size().x - 20.0, 8.0), galley.size() + vec2(12.0, 4.0));
    painter.rect_filled(band, 2, Color32::from_black_alpha(150));
    painter.galley(band.min + vec2(6.0, 2.0), galley, Color32::from_gray(0xe0));
}

/// A clip with its section's inset (SR2c): the shot as [`show_clip`], and the inset in its
/// corner — a graphic as its thumbnail, a video as a poster box with its name (FilmCraft plays
/// the real video); with Swap the inset takes the frame and the shot sits in the corner. Title
/// cards never carry one. Mirrors `animatic::Compositor::section_frame` for the export.
fn show_section(
    nav: &mut Navigator,
    ui: &mut Ui,
    frame: Rect,
    board: &Storyboard,
    clip: &Clip,
    index: &Index,
    elapsed: f64,
) {
    let inset = board
        .scenes
        .iter()
        .find(|s| s.id == clip.scene_id)
        .and_then(|s| s.inset.as_ref())
        .filter(|i| i.asset.is_some() && clip.shot_type != ShotType::Ttl);
    let Some(inset) = inset else {
        show_clip(nav, ui, frame, clip, index, elapsed);
        return;
    };
    let corner = inset_corner(frame, inset.corner, inset.size);
    if inset.swap {
        paint_inset(nav, ui, frame, inset, index);
        ui.painter().rect_filled(corner.expand(2.0), 0, Color32::from_gray(0x10));
        ui.painter().rect_filled(corner, 0, Color32::BLACK);
        show_clip(nav, ui, corner, clip, index, elapsed);
    } else {
        show_clip(nav, ui, frame, clip, index, elapsed);
        ui.painter().rect_filled(corner.expand(2.0), 0, Color32::from_gray(0x10));
        paint_inset(nav, ui, corner, inset, index);
    }
}

/// Where an inset of `size` sits in `corner` of `frame`: 16:9, `size.share()` of the width,
/// 2.5 % in from the edges (as the compositor).
pub(crate) fn inset_corner(
    frame: Rect,
    corner: assetcraft_storyboard::Corner,
    size: assetcraft_storyboard::InsetSize,
) -> Rect {
    use assetcraft_storyboard::Corner;
    let w = frame.width() * size.share();
    let h = (w * 9.0 / 16.0).min(frame.height() * 0.6);
    let m = frame.width() * 0.025;
    let x = match corner {
        Corner::TopLeft | Corner::BottomLeft => frame.left() + m,
        Corner::TopRight | Corner::BottomRight => frame.right() - m - w,
    };
    let y = match corner {
        Corner::TopLeft | Corner::TopRight => frame.top() + m,
        Corner::BottomLeft | Corner::BottomRight => frame.bottom() - m - h,
    };
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

/// The inset's picture into `target`: a graphic's thumbnail, contained; a video as a dark
/// poster box with "VIDEO" and the file's name.
fn paint_inset(nav: &mut Navigator, ui: &mut Ui, target: Rect, inset: &assetcraft_storyboard::Inset, index: &Index) {
    let Some(asset) = &inset.asset else { return };
    let path = Path::new(asset.0.as_str());
    let painter = ui.painter().clone();
    painter.rect_filled(target, 0, Color32::from_rgb(0x1e, 0x1e, 0x22));
    let graphic = inset.kind == assetcraft_storyboard::InsetKind::Graphic;
    if graphic && let Cell::Ready(texture, size) = thumb(nav, ui, path, 512.0, index) {
        let r = contain(target, size);
        painter.image(texture, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        return;
    }
    let name = assetcraft_fs::display_name(path);
    painter.with_clip_rect(target).text(
        target.center() - vec2(0.0, target.height() * 0.1),
        Align2::CENTER_CENTER,
        if graphic { "GRAPHIC" } else { "VIDEO" },
        FontId::proportional((target.height() * 0.16).max(8.0)),
        Color32::from_gray(0xd0),
    );
    painter.with_clip_rect(target).text(
        target.center() + vec2(0.0, target.height() * 0.12),
        Align2::CENTER_CENTER,
        name,
        FontId::proportional((target.height() * 0.11).max(7.0)),
        Color32::from_gray(0xa0),
    );
}

/// A clip as the Cut shows it, `elapsed` seconds in (`f64::INFINITY` = at rest): a Card, or the
/// hero frame (or "No shot assigned") with any Overlay text on top.
fn show_clip(nav: &mut Navigator, ui: &mut Ui, frame: Rect, clip: &Clip, index: &Index, elapsed: f64) {
    if clip.shot_type == ShotType::Ttl {
        if let Some(bg) = clip.background()
            && let Cell::Ready(texture, size) = thumb(nav, ui, Path::new(&bg.0), 512.0, index)
        {
            let target = cover(frame, size);
            ui.painter().image(texture, frame, uv_for_cover(frame, target), Color32::WHITE);
        }
        crate::text_view::paint_text(ui.painter(), frame, clip, elapsed);
        return;
    }
    match clip.asset_ids.first() {
        Some(hero) => {
            let still = match thumb(nav, ui, Path::new(&hero.0), 512.0, index) {
                Cell::Ready(texture, size) => Some((texture, size)),
                _ => None,
            };
            // A clip with motion draws through its camera (SB-3D1 step 3); the rest hold still.
            if !crate::motion_view::show(nav, ui, frame, clip, still, elapsed)
                && let Some((texture, size)) = still
            {
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
/// toward their new lengths after a Re-cut (`ctx = None` gives the final layout); a clip being
/// resized (`drag`) follows the pointer at once.
pub(crate) fn clip_spans(
    board: &Storyboard,
    left: f32,
    ctx: Option<&egui::Context>,
    drag: Option<ResizeDrag>,
) -> Vec<(ClipId, f32, f32)> {
    let mut x = left;
    board
        .clips
        .iter()
        .map(|c| {
            let dragged = drag.filter(|d| d.clip == c.id.0).map(|d| d.secs);
            let secs = dragged.unwrap_or_else(|| c.duration.as_secs_f64());
            let target = (secs as f32 * PX_PER_SEC).max(2.0);
            let anim = if dragged.is_some() { 0.0 } else { RECUT_ANIM_SECS };
            let w = match ctx {
                Some(ctx) => ctx.animate_value_with_time(egui::Id::new(("sb-clip-w", c.id.0)), target, anim),
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
    let chosen: HashSet<ClipId> = nav.session.selected_clips().into_iter().collect();
    let playing = nav.session.transport().playing;
    let shift = ui.input(|i| i.modifiers.shift);
    let mut select: Option<ClipId> = None;
    let mut scrub: Option<(f64, crate::timebar::Scrub)> = None;
    let mut drop: Option<(ClipId, String)> = None;
    let mut inset_cmds: Vec<(&'static str, serde_json::Value)> = Vec::new();
    let mut switch: Option<(Clip, assetcraft_storyboard::TextKind)> = None;
    let mut resize: Option<ResizeDrag> = nav.state.storyboard.resize_drag;
    let mut resized: Option<ResizeDrag> = None;
    let ctx = ui.ctx().clone();
    // A solid, always-visible scroll bar: clicking its track jumps there (finding B3).
    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    egui::ScrollArea::horizontal()
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            let (area, _) = ui.allocate_exact_size(vec2(width, RULER_H + MARKER_H + STRIP_H + ROWS_H), Sense::hover());
            let left = area.left() + 8.0;
            let to_secs = |x: f32| (((x - left) / PX_PER_SEC).max(0.0) as f64).min(total.max(0.0) as f64);
            // Ruler and scene-marker row: click or drag to move the playhead (finding B3).
            let ruler = Rect::from_min_size(area.min, vec2(width, RULER_H + MARKER_H));
            let tip = "Click or drag to move the playhead";
            if let Some((x, how)) = crate::timebar::scrub_area(ui, ruler, ui.id().with("sb-ruler"), tip) {
                scrub = Some((to_secs(x), how));
            }
            let painter = ui.painter().clone();
            let weak = ui.visuals().weak_text_color();
            let ruler_line = Rect::from_min_size(area.min, vec2(width, RULER_H));
            let len = f64::from((width - 8.0) / PX_PER_SEC);
            crate::timebar::paint_ruler(&painter, ruler_line, PX_PER_SEC, len, |t| left + t as f32 * PX_PER_SEC, weak);
            if track_len > 0.0 {
                let x = left + track_len * PX_PER_SEC;
                painter.line_segment([pos2(x, area.top()), pos2(x, area.bottom())], Stroke::new(1.0, ACCENT));
            }
            // Scene markers and clips.
            let strip_top = area.top() + RULER_H + MARKER_H;
            let marker_y = area.top() + RULER_H + 2.0;
            let mut prev_scene = None;
            let spans = clip_spans(board, left, Some(&ctx), resize);
            inset_cmds = inset_row(nav, ui, board, &spans, strip_top + STRIP_H + 4.0);
            audio_row(nav, ui, board, strip_top + STRIP_H + 4.0 + INSET_H + 4.0, left, width);
            inset_cmds.extend(voice_row(
                nav,
                ui,
                board,
                strip_top + STRIP_H + 4.0 + INSET_H + 4.0 + AUDIO_H + 4.0,
                left,
            ));
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
                let accepts = clip.shot_type.accepts_drop();
                let hovering = accepts && response.dnd_hover_payload::<DragAsset>().is_some();
                if chosen.contains(&id) || hovering {
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
                // The right edge resizes the clip (SR1): registered after the card so it wins the
                // pointer there; the card follows the pointer, the command runs when the drag ends.
                let handle = Rect::from_min_max(pos2(rect.right() - HANDLE_W, rect.top() + 4.0), rect.max);
                let grip = ui.interact(handle, strip_clip_id(id).with("resize"), Sense::drag());
                let grip =
                    grip.on_hover_cursor(egui::CursorIcon::ResizeHorizontal).on_hover_text(if board.track.is_some() {
                        "Drag to set the clip's length (snaps to beats; Shift for tenths of a second)"
                    } else {
                        "Drag to set the clip's length"
                    });
                if grip.drag_started() {
                    resize = Some(ResizeDrag { clip: id.0, secs: clip.duration.as_secs_f64() });
                }
                if grip.dragged()
                    && let Some(pointer) = grip.interact_pointer_pos()
                {
                    let want = f64::from((pointer.x - x) / PX_PER_SEC);
                    resize = Some(ResizeDrag { clip: id.0, secs: snap_length(board, want, shift) });
                }
                if grip.drag_stopped() {
                    resized = resize.take().filter(|d| d.clip == id.0);
                }
                if grip.hovered() || grip.dragged() || resize.is_some_and(|d| d.clip == id.0) {
                    let edge = Rect::from_min_max(pos2(rect.right() - 3.0, rect.top() + 4.0), rect.max);
                    ui.painter().rect_filled(edge, 0, SELECT);
                }
            }
            let x = left + at.as_secs_f64() as f32 * PX_PER_SEC;
            crate::timebar::paint_playhead(&painter, x, area.top(), area.bottom(), SELECT);
            // Grab the playhead anywhere along it (over the clips too) and drag to scrub.
            let grab = crate::timebar::playhead_handle(ui, x, area.top(), area.bottom(), ui.id().with("sb-playhead"));
            if let Some((x, how)) = grab {
                scrub = Some((to_secs(x), how));
            }
            if playing {
                ui.scroll_to_rect(Rect::from_min_size(pos2(x - 40.0, area.top()), vec2(200.0, 10.0)), None);
            }
        });
    nav.state.storyboard.resize_drag = resize;
    if let Some((secs, how)) = scrub {
        scrub_to(nav, secs, how, playing);
    }
    if let Some(d) = resized {
        commit_resize(nav, d);
    }
    for (cmd, params) in inset_cmds {
        nav.run(cmd, params);
    }
    if let Some((clip, to)) = switch {
        crate::text_view::switch_kind(nav, &clip, to);
    } else if let Some((clip, asset)) = drop {
        nav.run("storyboard.place", json!({ "asset": asset, "clip": clip.0 }));
    } else if let Some(id) = select {
        nav.run("storyboard.select", json!({ "clip": id.0, "extend": shift }));
    }
}

/// The INSET row (SR2b): a slot per section with shots, spanning them, named by the section;
/// a drop target for an Organizer file; a click (or right-click) opens the slot's options —
/// corner, size, Swap, Mix audio, the Organizer's selection, Clear — and selects the section
/// (Paul, 2026-11-07: "clicking on the inset should give options for display"). Returns the
/// commands to run.
fn inset_row(
    nav: &Navigator,
    ui: &mut Ui,
    board: &Storyboard,
    spans: &[(ClipId, f32, f32)],
    top: f32,
) -> Vec<(&'static str, serde_json::Value)> {
    use assetcraft_storyboard::{Corner, InsetKind, InsetSize};
    let painter = ui.painter().clone();
    let mut out: Vec<(&'static str, serde_json::Value)> = Vec::new();
    let focused = crate::storyboard_view::focused_file(nav);
    for scene in board.inset_scenes() {
        let shots: Vec<(ClipId, f32, f32)> = spans
            .iter()
            .zip(&board.clips)
            .filter(|(_, c)| c.scene_id == scene.id && c.shot_type != ShotType::Ttl)
            .map(|((id, x, w), _)| (*id, *x, *w))
            .collect();
        let (Some(&(first_id, x0, _)), Some(&(_, x1, w1))) = (shots.first(), shots.last()) else { continue };
        let rect = Rect::from_min_max(pos2(x0 + 1.0, top + 2.0), pos2(x1 + w1 - 1.0, top + INSET_H - 2.0));
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let inset = scene.inset.clone().unwrap_or_else(|| assetcraft_storyboard::Inset::slot(InsetKind::Video));
        let set = inset.asset.as_ref();
        let response = ui.interact(rect, egui::Id::new(("sb-inset", scene.id.0)), Sense::click());
        let hovering = response.dnd_hover_payload::<DragAsset>().is_some();
        let (fill, stroke) = match set {
            Some(_) => (Color32::from_rgb(0x2b, 0x3f, 0x5a), Color32::from_rgb(0x5b, 0x8f, 0xc9)),
            None => (Color32::from_gray(0x22), Color32::from_gray(0x4a)),
        };
        painter.rect_filled(rect, 2, fill);
        painter.rect_stroke(
            rect,
            2,
            Stroke::new(if hovering || response.hovered() { 2.0 } else { 1.0 }, if hovering { SELECT } else { stroke }),
            StrokeKind::Inside,
        );
        let title = board.scene_title(scene.id).unwrap_or_default();
        // Two lines: the section and the slot's state; the file's name and its placement.
        let (line1, line2) = match set {
            Some(a) => (
                format!(
                    "{title} · {} · {} {}{}",
                    if inset.kind == InsetKind::Video { "video" } else { "graphic" },
                    inset.corner.code(),
                    match inset.size {
                        InsetSize::Small => "S",
                        InsetSize::Medium => "M",
                        InsetSize::Large => "L",
                    },
                    if inset.swap { " · swapped" } else { "" }
                ),
                Path::new(&a.0).file_name().map_or(a.0.clone(), |f| f.to_string_lossy().into_owned()),
            ),
            None => (
                title.to_owned(),
                inset.note.clone().unwrap_or_else(|| "Drop b-roll, an interview, a host or a graphic here".to_owned()),
            ),
        };
        // A filled slot keeps its note beside the file's name.
        let line2 = match (set, &inset.note) {
            (Some(_), Some(n)) => format!("{line2} — {n}"),
            _ => line2,
        };
        let clip = painter.with_clip_rect(rect);
        if rect.width() > 24.0 {
            clip.text(
                pos2(rect.left() + 4.0, rect.top() + 4.0),
                Align2::LEFT_TOP,
                line1,
                FontId::proportional(9.0),
                Color32::from_gray(if set.is_some() { 0xd0 } else { 0x90 }),
            );
            clip.text(
                pos2(rect.left() + 4.0, rect.bottom() - 4.0),
                Align2::LEFT_BOTTOM,
                line2,
                FontId::proportional(9.0),
                Color32::from_gray(if set.is_some() { 0xa8 } else { 0x70 }),
            );
        }
        let tip = match set {
            Some(a) => format!("Inset on {title}: {}\nClick for options · drop another file to replace it", a.0),
            None => format!(
                "Inset slot for {title}: drop a video (its length sizes the section) or a graphic from the Organizer · click for options"
            ),
        };
        let response = response.on_hover_text(tip);
        if let Some(asset) = response.dnd_release_payload::<DragAsset>() {
            out.push(("storyboard.inset", json!({ "scene": scene.id.0, "asset": asset.0.clone() })));
        }
        if response.clicked() || response.secondary_clicked() {
            out.push(("storyboard.select", json!({ "clip": first_id.0 })));
        }
        let scene_id = scene.id.0;
        let options = |ui: &mut Ui, out: &mut Vec<(&'static str, serde_json::Value)>| {
            ui.set_min_width(220.0);
            ui.label(RichText::new(format!("INSET · {title}")).small().strong());
            match set {
                Some(a) => {
                    ui.label(RichText::new(assetcraft_fs::display_name(Path::new(&a.0))).small());
                }
                None => {
                    ui.weak("Empty slot");
                }
            }
            if let Some(note) = crate::inset_view::note_field(ui, scene_id, inset.note.as_deref()) {
                out.push(("storyboard.inset", json!({ "scene": scene_id, "note": note })));
            }
            let label = if set.is_some() { "Replace with Organizer selection" } else { "Use Organizer selection" };
            if ui.add_enabled(focused.is_some(), egui::Button::new(label)).clicked()
                && let Some(p) = &focused
            {
                out.push(("storyboard.inset", json!({ "scene": scene_id, "asset": p.to_string_lossy() })));
                ui.close();
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Kind");
                for (k, name) in [(InsetKind::Video, "Video"), (InsetKind::Graphic, "Graphic")] {
                    if ui.selectable_label(inset.kind == k, name).clicked() && inset.kind != k {
                        out.push((
                            "storyboard.inset",
                            json!({ "scene": scene_id, "kind": if k == InsetKind::Video { "video" } else { "graphic" } }),
                        ));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Corner");
                for c in Corner::ALL {
                    if ui.selectable_label(inset.corner == c, c.code()).clicked() && inset.corner != c {
                        out.push(("storyboard.inset", json!({ "scene": scene_id, "corner": c.code() })));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Size");
                for (sz, code) in [(InsetSize::Small, "S"), (InsetSize::Medium, "M"), (InsetSize::Large, "L")] {
                    if ui.selectable_label(inset.size == sz, code).clicked() && inset.size != sz {
                        out.push(("storyboard.inset", json!({ "scene": scene_id, "size": code })));
                    }
                }
            });
            let mut swap = inset.swap;
            if ui.checkbox(&mut swap, "Swap: inset fills the frame, the shot in the corner").changed() {
                out.push(("storyboard.inset", json!({ "scene": scene_id, "swap": swap })));
            }
            let mut mix = inset.mix_audio;
            if ui
                .add_enabled(inset.kind == InsetKind::Video, egui::Checkbox::new(&mut mix, "Mix the video's audio in"))
                .changed()
            {
                out.push(("storyboard.inset", json!({ "scene": scene_id, "mixAudio": mix })));
            }
            if inset.kind == InsetKind::Video && inset.mix_audio {
                let (m, v) = (assetcraft_storyboard::MUSIC_DB_RANGE, assetcraft_storyboard::VOICE_DB_RANGE);
                if let Some(db) = crate::inset_view::level_slider(ui, ("music", scene_id), "Music", inset.music_db, m) {
                    out.push(("storyboard.inset", json!({ "scene": scene_id, "musicDb": db })));
                }
                if let Some(db) = crate::inset_view::level_slider(ui, ("voice", scene_id), "Voice", inset.voice_db, v) {
                    out.push(("storyboard.inset", json!({ "scene": scene_id, "voiceDb": db })));
                }
            }
            if set.is_some() {
                ui.separator();
                if ui.button("Clear the slot").clicked() {
                    out.push(("storyboard.insetClear", json!({ "scene": scene_id })));
                    ui.close();
                }
            }
        };
        egui::Popup::menu(&response).id(egui::Id::new(("sb-inset-menu", scene_id))).show(|ui| options(ui, &mut out));
        egui::Popup::context_menu(&response)
            .id(egui::Id::new(("sb-inset-ctx", scene_id)))
            .show(|ui| options(ui, &mut out));
    }
    out
}

/// The VOICE row (SR4b): the voiceover as a bar from the top of the Cut (it plays from 0:00
/// and is cut at the end), named, with the song's level while it speaks; or "Drop a voiceover
/// here". Drop an Organizer file on it to set or replace it; click for its options (the
/// Organizer selection, Music / Voice, Remove). Returns the commands to run.
fn voice_row(
    nav: &Navigator,
    ui: &mut Ui,
    board: &Storyboard,
    top: f32,
    left: f32,
) -> Vec<(&'static str, serde_json::Value)> {
    let mut out: Vec<(&'static str, serde_json::Value)> = Vec::new();
    let painter = ui.painter().clone();
    let w = (board.total().as_secs_f64() as f32 * PX_PER_SEC).max(120.0);
    let rect = Rect::from_min_size(pos2(left, top), vec2(w, VOICE_H));
    let response = ui.interact(rect, egui::Id::new("sb-voiceover"), Sense::click());
    let hovering = response.dnd_hover_payload::<DragAsset>().is_some();
    let vo = board.voiceover.as_ref();
    let (fill, stroke) = match vo {
        Some(_) => (Color32::from_rgb(0x3a, 0x2f, 0x52), Color32::from_rgb(0x9a, 0x80, 0xd0)),
        None => (Color32::from_gray(0x1e), Color32::from_gray(0x44)),
    };
    painter.rect_filled(rect, 2, fill);
    painter.rect_stroke(
        rect,
        2,
        Stroke::new(if hovering || response.hovered() { 2.0 } else { 1.0 }, if hovering { SELECT } else { stroke }),
        StrokeKind::Inside,
    );
    let text = match vo {
        Some(v) => format!(
            "VOICEOVER · {} · the song at {} dB while it speaks",
            Path::new(&v.asset.0).file_name().map_or(v.asset.0.clone(), |f| f.to_string_lossy().into_owned()),
            v.music_db
        ),
        None => "VOICEOVER · drop a narration here (the song ducks while it speaks)".to_owned(),
    };
    let visible = rect.intersect(ui.clip_rect());
    if visible.width() > 40.0 {
        painter.with_clip_rect(rect).text(
            pos2(visible.left() + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(9.0),
            Color32::from_gray(if vo.is_some() { 0xd8 } else { 0x80 }),
        );
    }
    let response = response.on_hover_text(match vo {
        Some(v) => format!("{}\nClick for options · drop another file to replace it", v.asset.0),
        None => {
            "Drop an audio file (or a video with sound) from the Organizer: it plays from the top of the Cut".to_owned()
        }
    });
    if let Some(asset) = response.dnd_release_payload::<DragAsset>() {
        out.push(("storyboard.voiceover", json!({ "asset": asset.0.clone() })));
    }
    let focused = crate::storyboard_view::focused_file(nav);
    egui::Popup::menu(&response).id(egui::Id::new("sb-voiceover-menu")).show(|ui| {
        ui.set_min_width(220.0);
        ui.label(RichText::new("VOICEOVER").small().strong());
        let label = if vo.is_some() { "Replace with Organizer selection" } else { "Use Organizer selection" };
        if ui.add_enabled(focused.is_some(), egui::Button::new(label)).clicked()
            && let Some(p) = &focused
        {
            out.push(("storyboard.voiceover", json!({ "asset": p.to_string_lossy() })));
            ui.close();
        }
        if let Some(v) = vo {
            let (m, r) = (assetcraft_storyboard::MUSIC_DB_RANGE, assetcraft_storyboard::VOICE_DB_RANGE);
            if let Some(db) = crate::inset_view::level_slider(ui, ("vo-music", 0), "Music", v.music_db, m) {
                out.push(("storyboard.voiceover", json!({ "musicDb": db })));
            }
            if let Some(db) = crate::inset_view::level_slider(ui, ("vo-voice", 0), "Voice", v.voice_db, r) {
                out.push(("storyboard.voiceover", json!({ "voiceDb": db })));
            }
            ui.separator();
            if ui.button("Remove the voiceover").clicked() {
                out.push(("storyboard.voiceover", json!({ "asset": null })));
                ui.close();
            }
        }
    });
    out
}

/// The AUDIO row (SR2b): the track's pieces under the Cut in cut time — the whole song, or
/// the edit's segments (a chorus loop) butted together — with the waveform, the section bands
/// and a splice mark where the music jumps.
fn audio_row(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, top: f32, left: f32, width: f32) {
    let painter = ui.painter().clone();
    let rect = Rect::from_min_size(pos2(left, top), vec2(width - 16.0, AUDIO_H));
    painter.rect_filled(rect, 2, Color32::from_gray(0x1b));
    let Some(track) = &board.track else {
        painter.text(
            pos2(left + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            "No track · pick a song on the Start screen or in the Tracks view",
            FontId::proportional(9.0),
            Color32::from_gray(0x77),
        );
        return;
    };
    let x_of = |t: f64| left + t as f32 * PX_PER_SEC;
    // Section bands (output time).
    for sec in &track.song_sections {
        let r = Rect::from_min_max(
            pos2(x_of(sec.start.as_secs_f64()), rect.top()),
            pos2(x_of(sec.end.as_secs_f64()) - 1.0, rect.bottom()),
        );
        if r.width() <= 0.0 || !ui.is_rect_visible(r) {
            continue;
        }
        let (bg, fg) = crate::start_tracks::section_tint(&sec.name);
        painter.rect_filled(r, 1, bg);
        if r.width() > 40.0 {
            painter.with_clip_rect(r).text(
                r.left_top() + vec2(4.0, 2.0),
                Align2::LEFT_TOP,
                sec.name.to_uppercase(),
                FontId::proportional(9.0),
                fg,
            );
        }
    }
    // Pieces: source ranges in seconds at their cut offsets.
    let pieces: Vec<(f64, f64, f64)> = match &track.edit {
        Some(e) if e.sample_rate > 0 && !e.segments.is_empty() => {
            let sr = f64::from(e.sample_rate);
            let mut off = 0.0;
            e.segments
                .iter()
                .map(|[a, b]| {
                    let (a, b) = (*a as f64 / sr, *b as f64 / sr);
                    let p = (a, b, off);
                    off += (b - a).max(0.0);
                    p
                })
                .collect()
        }
        _ => vec![(0.0, track.duration.as_secs_f64(), 0.0)],
    };
    let peaks = crate::start_timeline::peaks(nav, &track.id.0);
    if peaks.is_none() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
    }
    let samples: Vec<(f64, f64)> = peaks
        .as_ref()
        .and_then(|p| p["peaks"].as_array())
        .map(|a| a.iter().map(|p| (p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0))).collect())
        .unwrap_or_default();
    let wave_secs = peaks.as_ref().and_then(|p| p["seconds"].as_f64()).filter(|s| *s > 0.0).unwrap_or(0.0);
    let loudest = samples.iter().fold(0.0f64, |m, (lo, hi)| m.max(-lo).max(*hi)).max(1e-3);
    let (mid, half) = (rect.center().y + 3.0, rect.height() * 0.38 / loudest as f32);
    let wave = Color32::from_rgb(0x5b, 0x8f, 0xc9);
    let splice = Color32::from_rgb(0xe0, 0xa0, 0x40);
    let visible = ui.clip_rect();
    for &(a, b, off) in &pieces {
        let (x0, x1) = (x_of(off), x_of(off + (b - a)));
        if !samples.is_empty() && wave_secs > 0.0 {
            let n = samples.len();
            let mut x = x0.max(visible.left()).floor();
            while x < x1.min(visible.right()) {
                let (t0, t1) = (a + f64::from((x - x0) / PX_PER_SEC), a + f64::from((x + 1.0 - x0) / PX_PER_SEC));
                if t0 < wave_secs {
                    let i0 = ((t0 / wave_secs * n as f64) as usize).min(n - 1);
                    let i1 = ((t1 / wave_secs * n as f64).ceil() as usize).clamp(i0 + 1, n);
                    let (lo, hi) =
                        samples[i0..i1].iter().fold((0.0f64, 0.0f64), |(lo, hi), (p, q)| (lo.min(*p), hi.max(*q)));
                    painter.line_segment(
                        [pos2(x + 0.5, mid - hi as f32 * half), pos2(x + 0.5, mid - lo as f32 * half + 1.0)],
                        Stroke::new(1.0, wave),
                    );
                }
                x += 1.0;
            }
        }
        if off > 0.0 {
            painter.line_segment([pos2(x0, rect.top()), pos2(x0, rect.bottom())], Stroke::new(1.0, splice));
            painter.text(
                pos2(x0 + 3.0, rect.bottom() - 1.0),
                Align2::LEFT_BOTTOM,
                format!("→ {}", crate::start_tracks::mmss(a)),
                FontId::monospace(8.0),
                splice,
            );
        }
    }
    if let Some(e) = &track.edit
        && e.sample_rate > 0
        && let Some([s, n]) = e.fade_out
    {
        let sr = f64::from(e.sample_rate);
        painter.line_segment(
            [pos2(x_of(s as f64 / sr), rect.top() + 2.0), pos2(x_of((s + n) as f64 / sr), rect.bottom() - 2.0)],
            Stroke::new(1.5, Color32::from_gray(0xcc)),
        );
    }
}

/// End a length drag (SR1): one `storyboard.resize` when the length moved, nothing otherwise.
pub(crate) fn commit_resize(nav: &mut Navigator, d: ResizeDrag) {
    nav.state.storyboard.resize_drag = None;
    let same = nav
        .session
        .storyboard()
        .and_then(|b| b.clip(ClipId(d.clip)))
        .is_some_and(|c| (c.duration.as_secs_f64() - d.secs).abs() < 0.005);
    if !same {
        nav.run("storyboard.resize", json!({ "clip": d.clip, "seconds": d.secs }));
    }
}

/// The Inspector's Duration row (SR1): a drag value in seconds (and the beats it makes on the
/// track), committed as one `storyboard.resize` when the drag ends or a typed value lands.
pub(crate) fn duration_field(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, clip: &Clip) {
    ui.label(RichText::new("Duration").size(10.0).color(ui.visuals().weak_text_color()));
    let id = clip.id.0;
    let dragging = nav.state.storyboard.resize_drag.filter(|d| d.clip == id);
    let before = dragging.map(|d| d.secs).unwrap_or_else(|| clip.duration.as_secs_f64());
    let mut value = before;
    let max = assetcraft_storyboard::Limits::default().max_duration.as_secs_f64();
    let r = ui
        .horizontal(|ui| {
            let r = ui.add(
                egui::DragValue::new(&mut value)
                    .range(assetcraft_engine::MIN_CLIP_SECS..=max)
                    .speed(0.1)
                    .fixed_decimals(1)
                    .suffix(" s"),
            );
            let r = r.on_hover_text("Drag or type the clip's length (Ctrl+Z undoes)");
            if let Some(t) = &board.track {
                let beats = t.bpm.ticks_to_beats(Ticks::from_secs_f64(value).unwrap_or(Ticks::ZERO));
                ui.weak(format!("· {beats} beat{}", if beats == 1 { "" } else { "s" }));
            }
            r
        })
        .inner;
    // The widget rounds to a tenth on its first frame and reports that as a change; only a move
    // of at least half a step is the user's (as the MOTION sliders).
    let moved = r.changed() && (value - before).abs() >= 0.05;
    if moved {
        nav.state.storyboard.resize_drag = Some(ResizeDrag { clip: id, secs: value });
    }
    if r.drag_stopped() || (moved && !r.dragged()) {
        let secs = if moved { value } else { before };
        commit_resize(nav, ResizeDrag { clip: id, secs });
    }
    ui.add_space(4.0);
}

/// Move the Cut's playhead to `secs`. A click while playing plays on from there; a drag scrubs
/// (paused), and if it started while playing, playback resumes where it stops.
pub(crate) fn scrub_to(nav: &mut Navigator, secs: f64, how: crate::timebar::Scrub, playing: bool) {
    use crate::timebar::Scrub;
    match how {
        Scrub::Click if playing => {
            nav.run("storyboard.play", json!({ "from": secs }));
        }
        Scrub::Start => {
            nav.state.storyboard.scrub_resume = playing;
            nav.run("storyboard.seek", json!({ "seconds": secs }));
        }
        Scrub::Stop if std::mem::take(&mut nav.state.storyboard.scrub_resume) => {
            nav.run("storyboard.play", json!({ "from": secs }));
        }
        _ => {
            nav.run("storyboard.seek", json!({ "seconds": secs }));
        }
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
    if title && drew {
        // A title over its background: darken the image so the words read.
        painter.rect_filled(
            Rect::from_min_size(body.min, vec2(body.width(), body.height() - 34.0)),
            0,
            Color32::from_black_alpha(110),
        );
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
    // The motion glyph (↻ turn, ⤢ zoom, ⇆ pan) sits before the duration (SB-3D1).
    let secs = match crate::motion_view::glyph(clip) {
        Some(g) => format!("{g} {:.1}s", clip.duration.as_secs_f64()),
        None => format!("{:.1}s", clip.duration.as_secs_f64()),
    };
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
