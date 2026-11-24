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
//!   `storyboard.resize` when the drag ends (one undo step). With snapping on (S) the clip's end
//!   snaps to the song's beats, bars, sections, the playhead and earlier cuts within 9 pt
//!   ([`snap_resize`], cut-editing.md C5), with a labelled line; snapping off or Shift drags free
//!   in tenths of a second. The Inspector's Duration field runs the same command. The card
//!   follows the pointer while dragging ([`ResizeDrag`]).
//! - **Zoom (Z1):** `storyboard.zoom` (0.25×–16×; Ctrl+wheel around the pointer, − / + / Fit in
//!   the bar). The waveform is measured for the visible window (`track.peaks {from, to}`).
//! - **Fine scrub (Z2):** Shift-drag on the ruler at a quarter speed; ← / → a frame, Shift+← / →
//!   a beat; frame timecode from 2×.
//! - **Volume line (V3, replacing V1's Song slider):** a line over the AUDIO row. With no points,
//!   dragging it sets `Track.level_db` (`storyboard.trackLevel`); Shift+click adds a point, a
//!   point drags, a double-click removes it (`storyboard.volume`); one undo step each; playback
//!   and the export both follow it. The dip under title cards (B-R3) stays on top of it.
//!   The bar's 🔊 is the listening volume (V2, `storyboard.listenVolume`): how loud the Cut plays
//!   here, at once and saved; the export doesn't use it.
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
    /// Where the clip's end snapped (seconds in the Cut), for the snap line (C5).
    pub(crate) snapped: Option<f64>,
}

/// A cut being dragged (C1b): the clip on its left, where the cut was, and where it would go
/// (seconds in the Cut), with whether that place is a snap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CutDrag {
    pub(crate) left: u32,
    pub(crate) from: f64,
    pub(crate) to: f64,
    pub(crate) snapped: bool,
}

/// Width of a cut's grab handle in the marker row.
const CUT_HANDLE_W: f32 = 10.0;

/// Where a dragged cut lands (cut-editing.md C1b + C5): `want` seconds, snapped (unless
/// `snapping` is off) to the nearest cut, playhead, section, bar or beat within reach — not to
/// itself — and kept 0.5 s inside both neighbours, as `storyboard.roll` would limit it.
pub(crate) fn snap_cut(
    board: &Storyboard,
    left: usize,
    want: f64,
    playhead: Ticks,
    snapping: bool,
    px: f32,
) -> (f64, Option<assetcraft_storyboard::Snapped>) {
    let starts = board.starts();
    let min = assetcraft_engine::MIN_CLIP_SECS;
    let (Some(a), Some(right)) = (starts.get(left), board.clips.get(left + 1)) else { return (want, None) };
    let cut = starts.get(left + 1).copied().unwrap_or(Ticks::ZERO);
    let lo = a.as_secs_f64() + min;
    let hi = cut.as_secs_f64() + right.duration.as_secs_f64() - min;
    if hi < lo {
        return (cut.as_secs_f64(), None);
    }
    let free = free_step(want, px).clamp(lo, hi);
    if !snapping {
        return (free, None);
    }
    let o =
        assetcraft_storyboard::SnapOptions { px_per_sec: px, playhead: Some(playhead), exclude: &[cut], music: true };
    match Ticks::from_secs_f64(want.max(0.0)).and_then(|t| assetcraft_storyboard::snap(board, t, &o)) {
        Some(s) if (lo..=hi).contains(&s.at.as_secs_f64()) => (s.at.as_secs_f64(), Some(s)),
        _ => (free, None),
    }
}

/// How much slower a Shift-drag on the ruler scrubs (Z2).
pub(crate) const FINE_SCRUB: f64 = 4.0;

/// Frames a second for stepping and the frame timecode (Z2).
pub(crate) const FPS: f64 = 24.0;

/// `m:ss:ff` at [`FPS`] (Z2): the transport's time when zoomed in to frames.
pub(crate) fn frame_timecode(secs: f64) -> String {
    let f = (secs.max(0.0) * FPS).round() as u64;
    let fps = FPS as u64;
    format!("{}:{:02}:{:02}", f / fps / 60, (f / fps) % 60, f % fps)
}

/// ← / → move the playhead one frame, Shift+← / → to the song's previous or next beat (Z2),
/// within the Cut.
pub(crate) fn step_playhead(nav: &mut Navigator, ctx: &egui::Context, forward: bool, by_beat: bool) {
    let at = playhead(nav, ctx);
    let Some(board) = nav.session.storyboard() else { return };
    let total = board.total().as_secs_f64();
    let to = if by_beat {
        match assetcraft_storyboard::step_beat(board, at, forward) {
            Some(t) => t.as_secs_f64(),
            None => return,
        }
    } else {
        let frame = (at.as_secs_f64() * FPS).round() + if forward { 1.0 } else { -1.0 };
        (frame / FPS).clamp(0.0, total)
    };
    nav.run("storyboard.seek", json!({ "seconds": to }));
}

/// The Strip's scale at the session's zoom (Z1): points per second.
pub(crate) fn px_per_sec(nav: &Navigator) -> f32 {
    PX_PER_SEC * nav.session.strip_zoom()
}

/// A free (unsnapped) drag's step at a scale (Z1/Z2): tenths of a second below 2× zoom, single
/// frames (24 a second) from 2× up, so a zoomed-in drag can land on any frame.
pub(crate) fn free_step(secs: f64, px: f32) -> f64 {
    let per = if px >= 2.0 * PX_PER_SEC { 24.0 } else { 10.0 };
    (secs * per).round() / per
}

/// The Viewer's video textures (P1): per file, the frame number shown and its texture.
#[derive(Default)]
pub(crate) struct VideoTextures(pub(crate) std::collections::HashMap<std::path::PathBuf, (u64, egui::TextureHandle)>);

impl std::fmt::Debug for VideoTextures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VideoTextures({} files)", self.0.len())
    }
}

/// The Viewer's title frames (L3): per clip, the frame shown (by identity) and its texture.
#[derive(Default)]
pub(crate) struct TitleTextures(pub(crate) HashMap<u32, (usize, egui::TextureHandle)>);

impl std::fmt::Debug for TitleTextures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TitleTextures({} clips)", self.0.len())
    }
}

/// The snap line's colour: apart from the selection blue and the playhead.
const SNAP_LINE: Color32 = Color32::from_rgb(0xff, 0xc8, 0x3d);

/// Where a resize drag of the clip at `index` to `want` seconds lands (cut-editing.md C5): with
/// snapping on and Shift not held, the clip's **end** goes to the nearest cut, playhead, song
/// section, bar or beat of the track within reach (`assetcraft_storyboard::snap`, the song's
/// detected beats when the board has them); otherwise, or with nothing in reach, tenths of a
/// second. Never under the engine's minimum. The cuts after the clip move with it, so they are
/// not candidates.
pub(crate) fn snap_resize(
    board: &Storyboard,
    index: usize,
    want: f64,
    playhead: Ticks,
    snapping: bool,
    px: f32,
) -> (f64, Option<assetcraft_storyboard::Snapped>) {
    let min = assetcraft_engine::MIN_CLIP_SECS;
    let free = free_step(want, px).max(min);
    let starts = board.starts();
    let Some(start) = starts.get(index).copied().filter(|_| snapping) else { return (free, None) };
    let mut moving: Vec<Ticks> = starts.get(index + 1..).unwrap_or_default().to_vec();
    moving.push(board.total());
    let o =
        assetcraft_storyboard::SnapOptions { px_per_sec: px, playhead: Some(playhead), exclude: &moving, music: true };
    let end = start.as_secs_f64() + want.max(0.0);
    let Some(snapped) = Ticks::from_secs_f64(end).and_then(|t| assetcraft_storyboard::snap(board, t, &o)) else {
        return (free, None);
    };
    let len = snapped.at.as_secs_f64() - start.as_secs_f64();
    if len < min {
        return (free, None);
    }
    (len, Some(snapped))
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
    /// egui time the theme packs were last checked for changes (TP4c-2).
    pub(crate) themes_checked: Option<f64>,
    /// The Start storyboard screen (design 3b / 3c).
    pub(crate) start: crate::start_view::StartUi,
    /// The open folder's modification times, built once per session generation (finding B1:
    /// rebuilding it every frame made dragging sluggish in big folders).
    pub(crate) index: Option<(u64, Arc<Index>)>,
    /// Scrubbing the ruler started while the Cut was playing: play on from where it stops.
    pub(crate) scrub_resume: bool,
    /// An Inspector amount slider mid-drag (SB-3D1), committed as one edit when it ends.
    pub(crate) motion_drag: Option<crate::motion_view::MotionDrag>,
    /// A title's area while one of its sliders is held (L5): the clip and the box shown live in
    /// the Viewer; committed as one undo step on release.
    pub(crate) area_drag: Option<(u32, assetcraft_storyboard::TextBox)>,
    /// A clip's length mid-drag (SR1: a card's edge or the Inspector's Duration), committed as
    /// one `storyboard.resize` when the drag ends.
    pub(crate) resize_drag: Option<ResizeDrag>,
    /// The snap line's label while a resize drag is snapped ("bar 17 · beat 3").
    pub(crate) snap_label: Option<String>,
    /// A cut mid-drag on the marker row (C1b), committed as one `storyboard.roll` on release.
    pub(crate) cut_drag: Option<CutDrag>,
    /// Last frame's scale, scroll offset and visible width (Z1): a zoom keeps the time under its
    /// anchor in place.
    pub(crate) view: Option<(f32, f32, f32)>,
    /// Where a Ctrl+wheel zoom is anchored, in points from the Strip's left edge (Z1).
    pub(crate) zoom_anchor: Option<f32>,
    /// Where a ruler drag is anchored for Shift's quarter-speed scrub (Z2): the time it last
    /// moved at full speed.
    pub(crate) fine_scrub: Option<f64>,
    /// A volume-line drag in progress (V3).
    pub(crate) vol_drag: Option<VolDrag>,
    /// The Inspector's In field mid-drag (C6): `(clip, seconds into its video)`, shown live and
    /// committed as one `storyboard.slip` when it ends.
    pub(crate) slip_drag: Option<(u32, f64)>,
    /// A section fade being dragged (V4): `(section index, the fade-out?, seconds)`, committed as
    /// one `storyboard.fade` on release.
    pub(crate) fade_drag: Option<(usize, bool, f64)>,
    /// An Alt-drag slip on a card (C6b): the clip and its in-point when the drag started.
    pub(crate) slip_card: Option<(u32, f64)>,
    /// Videos' lengths in seconds, read once from their headers (C6: the In field's limit).
    pub(crate) video_len: HashMap<String, Option<f64>>,
    /// The Viewer's video frames (P1): the frame number shown per file, and its texture.
    pub(crate) video_tex: VideoTextures,
    /// The Viewer's title frames (L3).
    pub(crate) title_tex: TitleTextures,
    /// Video cards' pictures (C6): per card, the in-point frame shown (in 1/24 s) and its texture.
    pub(crate) card_tex: VideoTextures,
    /// The last frame pass on which the Viewer waited for a video frame: cards leave the decoder
    /// to it then.
    pub(crate) viewer_waited: Option<u64>,
    /// The zoomed waveform's last window per music piece: `(key, peaks)` (Z1).
    pub(crate) wave_windows: std::collections::HashMap<usize, (String, Vec<(f32, f32)>)>,
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
    // Frames when zoomed in (Z2), tenths otherwise.
    let time = if px_per_sec(nav) >= 2.0 * PX_PER_SEC {
        format!("{} / {}", frame_timecode(at.as_secs_f64()), frame_timecode(total.as_secs_f64()))
    } else {
        format!("{} / {}", at.timecode(), total.timecode())
    };
    ui.label(RichText::new(time).font(FontId::monospace(12.0)));
    // The song's level in the mix (V1): one `storyboard.trackLevel` when the slider is let go.
    // The song's volume lives on the AUDIO row as its volume line (V3; Paul, 2026-11-15: "move the
    // volume control onto the audio track"); the bar keeps the listening volume.
    // The listening volume (V2): how loud it plays here, at once and saved; not exported.
    ui.separator();
    let mut v = nav.session.listen_volume();
    ui.label("🔊");
    let r = ui
        .add_sized(vec2(90.0, 16.0), egui::Slider::new(&mut v, 0.0..=1.0).show_value(false))
        .on_hover_text("How loud the Cut plays while you edit (saved; the exported video uses the Song level)");
    if r.changed() {
        nav.run("storyboard.listenVolume", json!({ "volume": v }));
    }
    ui.label(RichText::new(format!("{:>3.0}%", v * 100.0)).monospace().small());
    // The Strip's zoom (Z1): − / + by halves and doubles, Fit for the whole Cut; Ctrl+wheel
    // over the Strip zooms around the pointer.
    if nav.session.board_view() == assetcraft_engine::BoardView::Strip {
        ui.separator();
        let zoom = nav.session.strip_zoom();
        let (lo, hi) = assetcraft_engine::STRIP_ZOOM_RANGE;
        if ui
            .add_enabled(zoom > lo, egui::Button::new("−"))
            .on_hover_text("Zoom out (Ctrl+wheel over the Strip)")
            .clicked()
        {
            nav.run("storyboard.zoom", json!({ "by": 0.5 }));
        }
        ui.label(RichText::new(format!("{:>4.0}%", zoom * 100.0)).monospace().small());
        if ui
            .add_enabled(zoom < hi, egui::Button::new("+"))
            .on_hover_text("Zoom in to see the waveform and single frames")
            .clicked()
        {
            nav.run("storyboard.zoom", json!({ "by": 2.0 }));
        }
        if ui.button("Fit").on_hover_text("The whole Cut in view").clicked() {
            let w = nav.state.storyboard.view.map_or(1200.0, |(_, _, w)| w);
            let secs = total.as_secs_f64().max(1.0) as f32;
            nav.run("storyboard.zoom", json!({ "zoom": fit_zoom(w, secs) }));
        }
    }
}

/// The zoom that shows `secs` of Cut in a Strip `width` points wide, with its margins (Z1).
pub(crate) fn fit_zoom(width: f32, secs: f32) -> f32 {
    let (lo, hi) = assetcraft_engine::STRIP_ZOOM_RANGE;
    ((width - 88.0).max(40.0) / (secs.max(0.1) * PX_PER_SEC)).clamp(lo, hi)
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

/// The Viewer's frame, for tests and drops: its rect is the board's frame as drawn.
pub(crate) fn viewer_id() -> egui::Id {
    egui::Id::new("sb-viewer-frame")
}

fn viewer(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, index: &Index, at: Ticks, fill: bool) {
    let avail = ui.available_rect_before_wrap().shrink(8.0);
    // The board's frame (F2): 16:9 until it says otherwise, 9:16 for a vertical post.
    let ratio = (board.frame_spec().ratio() as f32).clamp(0.1, 10.0);
    let w = avail.width().min(avail.height() * ratio).max(16.0);
    let frame = Rect::from_center_size(avail.center(), vec2(w, w / ratio));
    viewer_in(nav, ui, board, index, at, fill, frame);
    // On top of whatever the frame shows (F6).
    if nav.session.safe_zones_on() {
        safe_zones(ui, &board.frame_spec(), frame);
    }
}

/// The Viewer's safe-zone guides (frame-settings.md F6), drawn over the picture and never
/// exported: on a vertical frame the margins where Reels, TikTok and Shorts put their username,
/// caption and button column are shaded and the safe area outlined; on any other frame the
/// title-safe 90 % box is outlined.
fn safe_zones(ui: &Ui, spec: &assetcraft_storyboard::FrameSpec, frame: Rect) {
    let painter = ui.painter().with_clip_rect(frame);
    let [x, y, w, h] = spec.safe_area();
    let safe = Rect::from_min_size(frame.min + vec2(x, y) * frame.size(), vec2(w, h) * frame.size());
    let line = Stroke::new(1.0, Color32::from_white_alpha(110));
    let label = |at: egui::Pos2, align: Align2, text: &str| {
        painter.text(at, align, text, FontId::monospace(9.0), Color32::from_white_alpha(150));
    };
    if spec.is_vertical() {
        let shade = Color32::from_black_alpha(70);
        // Above, below, then the sides between them.
        painter.rect_filled(Rect::from_min_max(frame.min, pos2(frame.right(), safe.top())), 0, shade);
        painter.rect_filled(Rect::from_min_max(pos2(frame.left(), safe.bottom()), frame.max), 0, shade);
        painter.rect_filled(
            Rect::from_min_max(pos2(frame.left(), safe.top()), pos2(safe.left(), safe.bottom())),
            0,
            shade,
        );
        painter.rect_filled(
            Rect::from_min_max(pos2(safe.right(), safe.top()), pos2(frame.right(), safe.bottom())),
            0,
            shade,
        );
        painter.rect_stroke(safe, 0, line, StrokeKind::Inside);
        label(pos2(frame.center().x, safe.top() - 3.0), Align2::CENTER_BOTTOM, "USERNAME · TOP BAR");
        label(pos2(frame.center().x, safe.bottom() + 3.0), Align2::CENTER_TOP, "CAPTION · AUDIO");
        let buttons = pos2((safe.right() + frame.right()) / 2.0, safe.center().y);
        painter.text(
            buttons,
            Align2::CENTER_CENTER,
            "LIKE\nCOMMENT\nSHARE",
            FontId::monospace(8.0),
            Color32::from_white_alpha(140),
        );
    } else {
        let dashed = egui::Shape::dashed_line(
            &[safe.left_top(), safe.right_top(), safe.right_bottom(), safe.left_bottom(), safe.left_top()],
            line,
            4.0,
            4.0,
        );
        painter.extend(dashed);
        label(safe.left_top() + vec2(4.0, 3.0), Align2::LEFT_TOP, "TITLE SAFE 90%");
    }
}

/// The Viewer's contents in `frame`.
fn viewer_in(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, index: &Index, at: Ticks, fill: bool, frame: Rect) {
    let response = ui.interact(frame, viewer_id(), Sense::hover());
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
    // Slipping a clip (C6): the Viewer shows its new first frame, wherever the playhead is.
    if !playing && let Some(clip) = nav.state.storyboard.slip_drag.and_then(|(id, _)| board.clip(ClipId(id))).cloned() {
        nav.session.video_stop_ahead();
        show_section(nav, ui, frame, board, &clip, index, f64::INFINITY);
        return;
    }
    if playing || paused_mid {
        if let Some(clip) = board.clip_at(at).cloned() {
            let start = board.index_of(clip.id).and_then(|i| board.starts().get(i).copied()).unwrap_or(Ticks::ZERO);
            let elapsed = at.as_secs_f64() - start.as_secs_f64();
            if playing {
                read_ahead(nav, ui, frame, board, &clip, elapsed);
            } else {
                nav.session.video_stop_ahead();
            }
            show_section(nav, ui, frame, board, &clip, index, elapsed);
        }
        return;
    }
    nav.session.video_stop_ahead();
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
    // A split stage (F5, L1): each region shows its own shot, cropped into it as the export does.
    if let Some(layout) = &board.layout
        && let Some(start) = board.index_of(clip.id).and_then(|i| board.starts().get(i).copied())
        && let Some(shots) = board.split_shots(Ticks(start.0.saturating_add(
            Ticks::from_secs_f64(if elapsed.is_finite() { elapsed.max(0.0) } else { 0.0 }).unwrap_or(Ticks::ZERO).0,
        )))
    {
        ui.painter().rect_filled(frame, 0, Color32::BLACK);
        let spec = board.frame_spec();
        for (rect, shot) in layout.regions().into_iter().zip(shots) {
            let region = Rect::from_min_size(
                frame.min + vec2(rect[0], rect[1]) * frame.size(),
                vec2(rect[2], rect[3]) * frame.size(),
            );
            let Some((i, at)) = shot else { continue };
            let Some(c) = board.clips.get(i) else { continue };
            // The board's crop at the region's own shape.
            let fit = assetcraft_storyboard::FrameSpec {
                width: (region.width().round() as u32).max(2),
                height: (region.height().round() as u32).max(2),
                ..spec.clone()
            };
            let c = c.clone();
            ui.scope_builder(egui::UiBuilder::new().max_rect(region), |ui| {
                ui.set_clip_rect(region.intersect(ui.clip_rect()));
                show_clip(nav, ui, region, &c, index, at, Some(&fit));
            });
        }
        return;
    }
    // A framed post (F5, L2): the bands, and the shot in the picture region between them.
    if let Some(layout @ assetcraft_storyboard::Layout::Framed { headline, caption, theme, .. }) = &board.layout
        && clip.shot_type != ShotType::Ttl
        && let Some([top, picture, bottom]) = layout.framed()
    {
        let at = |r: [f32; 4]| {
            Rect::from_min_size(frame.min + vec2(r[0], r[1]) * frame.size(), vec2(r[2], r[3]) * frame.size())
        };
        let (top, picture, bottom) = (at(top), at(picture), at(bottom));
        let look = theme.look();
        let painter = ui.painter().clone();
        painter.rect_filled(frame, 0, Color32::BLACK);
        for band in [top, bottom] {
            painter.rect_filled(band, 0, rgb(look.background));
        }
        let spec = board.frame_spec();
        let fit = assetcraft_storyboard::FrameSpec {
            width: (picture.width().round() as u32).max(2),
            height: (picture.height().round() as u32).max(2),
            ..spec
        };
        ui.scope_builder(egui::UiBuilder::new().max_rect(picture), |ui| {
            ui.set_clip_rect(picture.intersect(ui.clip_rect()));
            show_clip(nav, ui, picture, clip, index, elapsed, Some(&fit));
        });
        for (band, words, scale) in [(top, headline, 0.42), (bottom, caption, 0.3)] {
            let words = if look.uppercase { words.trim().to_uppercase() } else { words.trim().to_owned() };
            if words.is_empty() || band.height() < 4.0 {
                continue;
            }
            let size = (band.height() * scale).min(frame.height() * 0.06).max(6.0);
            let galley = painter.layout(words, egui::FontId::proportional(size), rgb(look.text), band.width() * 0.9);
            let pos = band.center() - galley.size() / 2.0;
            painter.with_clip_rect(band).galley(pos, galley, rgb(look.text));
        }
        return;
    }
    let inset = board
        .scenes
        .iter()
        .find(|s| s.id == clip.scene_id)
        .and_then(|s| s.inset.as_ref())
        .filter(|i| i.asset.is_some() && clip.shot_type != ShotType::Ttl);
    let Some(inset) = inset else {
        show_clip(nav, ui, frame, clip, index, elapsed, board.frame.as_ref());
        // An empty slot with a note: a dashed box where the inset will go, with the note — a
        // planning aid in the Viewer only, never in the export.
        if let Some(slot) = board
            .scenes
            .iter()
            .find(|s| s.id == clip.scene_id)
            .and_then(|s| s.inset.as_ref())
            .filter(|i| i.asset.is_none() && i.note.is_some() && clip.shot_type != ShotType::Ttl)
        {
            let r = inset_corner(frame, slot.corner, slot.size);
            let p = ui.painter();
            let dash = Color32::from_gray(0xb0);
            for (a, b) in [
                (r.left_top(), r.right_top()),
                (r.right_top(), r.right_bottom()),
                (r.right_bottom(), r.left_bottom()),
                (r.left_bottom(), r.left_top()),
            ] {
                p.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, dash), 6.0, 4.0));
            }
            p.rect_filled(r, 0, Color32::from_black_alpha(90));
            p.with_clip_rect(r).text(
                r.center(),
                Align2::CENTER_CENTER,
                slot.note.as_deref().unwrap_or(""),
                FontId::proportional((r.height() * 0.12).clamp(9.0, 14.0)),
                Color32::from_gray(0xe0),
            );
        }
        return;
    };
    let corner = inset_corner(frame, inset.corner, inset.size);
    if inset.swap {
        paint_inset(nav, ui, frame, inset, index);
        ui.painter().rect_filled(corner.expand(2.0), 0, Color32::from_gray(0x10));
        ui.painter().rect_filled(corner, 0, Color32::BLACK);
        show_clip(nav, ui, corner, clip, index, elapsed, None);
    } else {
        show_clip(nav, ui, frame, clip, index, elapsed, board.frame.as_ref());
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
/// hero frame (or "No shot assigned") with any Overlay text on top. `place` is the board's
/// frame, whose crop fits the picture (F2); without one (an older board, or the inset's
/// corner) the whole picture is fitted in, as before.
fn show_clip(
    nav: &mut Navigator,
    ui: &mut Ui,
    frame: Rect,
    clip: &Clip,
    index: &Index,
    elapsed: f64,
    place: Option<&assetcraft_storyboard::FrameSpec>,
) {
    if clip.shot_type == ShotType::Ttl {
        if let Some(bg) = clip.background()
            && let Cell::Ready(texture, size) = thumb(nav, ui, Path::new(&bg.0), 512.0, index)
        {
            let target = cover(frame, size);
            ui.painter().image(texture, frame, uv_for_cover(frame, target), Color32::WHITE);
        }
        show_title(nav, ui, frame, clip, elapsed);
        return;
    }
    match clip.asset_ids.first() {
        // A video plays its own frames (P1) from the clip's in-point (C6): the frame `elapsed`
        // seconds after it, the in-point itself at rest.
        Some(hero)
            if assetcraft_engine::is_video(Path::new(&hero.0))
                && show_video(nav, ui, frame, Path::new(&hero.0), video_secs(in_point(nav, clip), elapsed), place) => {}
        Some(hero) => {
            let still = match thumb(nav, ui, Path::new(&hero.0), 512.0, index) {
                Cell::Ready(texture, size) => Some((texture, size)),
                _ => None,
            };
            // A clip with motion draws through its camera (SB-3D1 step 3); the rest hold still.
            if !crate::motion_view::show(nav, ui, frame, clip, still, elapsed)
                && let Some((texture, size)) = still
            {
                paint_placed(ui, frame, texture, size, place);
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
    show_title(nav, ui, frame, clip, elapsed);
}

/// A text clip's title as the export draws it (L3): the compositor's title, alone, from the
/// background frame service, laid over the frame (a card's own picture is already under it; a
/// card without one is drawn whole). While a frame renders, the clip's last one stays up, so
/// typing or restyling never blinks; a clip without text draws nothing.
fn show_title(nav: &mut Navigator, ui: &mut Ui, frame: Rect, clip: &Clip, elapsed: f64) {
    show_title_as(nav, ui, frame, clip, elapsed, clip.id.0);
}

/// The texture slot of a clip's title in the Inspector's preview (TF-4): its own, so the
/// preview's smaller frames don't take turns with the Viewer's in one texture.
pub(crate) fn preview_slot(clip: &Clip) -> u32 {
    clip.id.0 | 0x8000_0000
}

/// [`show_title`] into texture slot `key` (the clip's id in the Viewer, [`preview_slot`] in the
/// Inspector).
pub(crate) fn show_title_as(nav: &mut Navigator, ui: &mut Ui, frame: Rect, clip: &Clip, elapsed: f64, key: u32) {
    let width = (frame.width() * ui.ctx().pixels_per_point()).clamp(64.0, 1920.0) as u32;
    // An area slider held (L5): the box as it is under the pointer, before it's committed.
    let dragged = nav.state.storyboard.area_drag.filter(|(id, _)| *id == clip.id.0).map(|(_, area)| {
        let mut c = clip.clone();
        c.text_style.area = Some(area);
        c
    });
    let clip = dragged.as_ref().unwrap_or(clip);
    let Some(lookup) = nav.session.title_frame(clip, elapsed, width) else { return };
    let shown = match lookup {
        assetcraft_engine::DesignLookup::Ready(f) => Some(f),
        assetcraft_engine::DesignLookup::Pending(near) => {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(15));
            near
        }
        assetcraft_engine::DesignLookup::Failed(_) => None,
    };
    let full = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let Some(f) = shown else {
        if let Some((_, tex)) = nav.state.storyboard.title_tex.0.get(&key) {
            ui.painter().image(tex.id(), frame, full, Color32::WHITE);
        }
        return;
    };
    let size = [f.width as usize, f.height as usize];
    let ident = std::sync::Arc::as_ptr(&f) as usize;
    let texture = match nav.state.storyboard.title_tex.0.get_mut(&key) {
        Some((shown, tex)) if *shown == ident && tex.size() == size => tex.clone(),
        Some((shown, tex)) => {
            tex.set(egui::ColorImage::from_rgba_unmultiplied(size, &f.rgba), egui::TextureOptions::LINEAR);
            *shown = ident;
            tex.clone()
        }
        None => {
            if nav.state.storyboard.title_tex.0.len() >= 8 {
                nav.state.storyboard.title_tex.0.clear();
            }
            let tex = ui.ctx().load_texture(
                format!("sb-title-{key}"),
                egui::ColorImage::from_rgba_unmultiplied(size, &f.rgba),
                egui::TextureOptions::LINEAR,
            );
            nav.state.storyboard.title_tex.0.insert(key, (ident, tex.clone()));
            tex
        }
    };
    ui.painter().image(texture.id(), frame, full, Color32::WHITE);
}

/// How far ahead of the playhead playback decodes video (P2).
const READ_AHEAD_SECS: f64 = 3.0;

/// While playing (P2): read ahead in the video under the playhead, and in its last second switch
/// to the next clip's opening so the first frame after the cut is ready.
fn read_ahead(nav: &Navigator, ui: &Ui, frame: Rect, board: &Storyboard, clip: &Clip, elapsed: f64) {
    let side = video_side(ui, frame);
    let video = |c: &Clip| {
        c.asset_ids.first().map(|a| Path::new(a.0.as_str()).to_path_buf()).filter(|p| assetcraft_engine::is_video(p))
    };
    let left = clip.duration.as_secs_f64() - elapsed;
    let next = board.index_of(clip.id).and_then(|i| board.clips.get(i + 1));
    match (video(clip), next.and_then(|n| video(n).map(|p| (p, n.source_in.as_secs_f64())))) {
        (_, Some((next, from))) if left <= 1.0 => nav.session.video_ahead(&next, from, 1.0, side),
        (Some(this), _) => nav.session.video_ahead(
            &this,
            video_secs(clip.source_in.as_secs_f64(), elapsed),
            READ_AHEAD_SECS.min(left.max(0.0) + 0.1),
            side,
        ),
        _ => nav.session.video_stop_ahead(),
    }
}

/// Where in its video a clip with in-point `from` is, `elapsed` seconds into the clip
/// (`f64::INFINITY`: at rest, the in-point).
fn video_secs(from: f64, elapsed: f64) -> f64 {
    from + if elapsed.is_finite() { elapsed.max(0.0) } else { 0.0 }
}

/// Where `clip` starts in its video (C6): the In field's value while it is dragged, else the
/// clip's own.
fn in_point(nav: &Navigator, clip: &Clip) -> f64 {
    nav.state
        .storyboard
        .slip_drag
        .filter(|(id, _)| *id == clip.id.0)
        .map_or_else(|| clip.source_in.as_secs_f64(), |(_, secs)| secs)
}

/// The size video frames are decoded at for the Viewer `frame` (its longest side in pixels).
fn video_side(ui: &Ui, frame: Rect) -> u32 {
    (frame.width().max(frame.height()) * ui.ctx().pixels_per_point()).clamp(64.0, 1920.0) as u32
}

/// The video at `path`, `secs` seconds in, into
/// `frame` (P1): the decoded frame if ready, else the nearest one decoded so far while the
/// worker catches up. `false` when there is nothing to show yet (the caller shows the poster) or
/// the file can't be played.
fn show_video(
    nav: &mut Navigator,
    ui: &mut Ui,
    frame: Rect,
    path: &Path,
    secs: f64,
    place: Option<&assetcraft_storyboard::FrameSpec>,
) -> bool {
    let side = video_side(ui, frame);
    let shown = match nav.session.video_frame(path, secs, side) {
        assetcraft_engine::VideoLookup::Ready(f) => Some(f),
        assetcraft_engine::VideoLookup::Pending(near) => {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(15));
            nav.state.storyboard.viewer_waited = Some(ui.ctx().cumulative_pass_nr());
            near
        }
        assetcraft_engine::VideoLookup::Failed(_) => None,
    };
    let Some(f) = shown else { return false };
    let size = [f.width as usize, f.height as usize];
    let key = path.to_path_buf();
    let texture = match nav.state.storyboard.video_tex.0.get_mut(&key) {
        Some((index, tex)) if *index == f.index && tex.size() == size => tex.clone(),
        Some((index, tex)) => {
            tex.set(egui::ColorImage::from_rgba_unmultiplied(size, &f.rgba), egui::TextureOptions::LINEAR);
            *index = f.index;
            tex.clone()
        }
        None => {
            if nav.state.storyboard.video_tex.0.len() >= 8 {
                nav.state.storyboard.video_tex.0.clear();
            }
            let tex = ui.ctx().load_texture(
                format!("sb-video-{}", key.display()),
                egui::ColorImage::from_rgba_unmultiplied(size, &f.rgba),
                egui::TextureOptions::LINEAR,
            );
            nav.state.storyboard.video_tex.0.insert(key, (f.index, tex.clone()));
            tex
        }
    };
    paint_placed(ui, frame, texture.id(), vec2(f.width as f32, f.height as f32), place);
    true
}

/// A picture of `size` into `frame` (F2): by the board's crop (`FrameSpec::place`, the export's
/// rule too), cropped to the frame; without a board frame, the whole picture fitted in.
fn paint_placed(
    ui: &Ui,
    frame: Rect,
    texture: egui::TextureId,
    size: egui::Vec2,
    place: Option<&assetcraft_storyboard::FrameSpec>,
) {
    let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let Some(spec) = place else {
        ui.painter().image(texture, contain(frame, size), full, Color32::WHITE);
        return;
    };
    let [x, y, w, h] = spec.place(size.x, size.y);
    let target = Rect::from_min_size(frame.min + vec2(x, y) * frame.size(), vec2(w, h) * frame.size());
    let shown = target.intersect(frame);
    if shown.is_positive() {
        ui.painter().image(texture, shown, uv_for_cover(shown, target), Color32::WHITE);
    }
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
    roll: Option<CutDrag>,
    px: f32,
) -> Vec<(ClipId, f32, f32)> {
    let mut x = left;
    // A cut being rolled: the clip before it gains what the clip after it gives up.
    let rolled = roll.and_then(|d| {
        let i = board.index_of(ClipId(d.left))?;
        let after = board.clips.get(i + 1)?;
        let by = d.to - d.from;
        Some([(d.left, board.clips[i].duration.as_secs_f64() + by), (after.id.0, after.duration.as_secs_f64() - by)])
    });
    board
        .clips
        .iter()
        .map(|c| {
            let dragged = drag
                .filter(|d| d.clip == c.id.0)
                .map(|d| d.secs)
                .or_else(|| rolled.and_then(|r| r.iter().find(|(id, _)| *id == c.id.0).map(|(_, s)| *s)));
            let secs = dragged.unwrap_or_else(|| c.duration.as_secs_f64());
            // Lengths animate in seconds, so a zoom changes every card at once with the ruler
            // and only a Re-cut's new lengths ease in (Z1).
            let anim = if dragged.is_some() { 0.0 } else { RECUT_ANIM_SECS };
            let secs = match ctx {
                Some(ctx) => ctx.animate_value_with_time(egui::Id::new(("sb-clip-w", c.id.0)), secs as f32, anim),
                None => secs as f32,
            };
            let w = (secs * px).max(2.0);
            let span = (c.id, x, w);
            x += w;
            span
        })
        .collect()
}

fn timeline(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, index: &Index, at: Ticks) {
    // Ctrl+wheel over the Strip zooms around the pointer (Z1).
    let frame = ui.max_rect();
    let zoom_by = ui.input(|i| i.zoom_delta());
    if zoom_by != 1.0
        && let Some(p) = ui.input(|i| i.pointer.hover_pos()).filter(|p| frame.contains(*p))
    {
        nav.state.storyboard.zoom_anchor = Some(p.x - frame.left());
        nav.run("storyboard.zoom", json!({ "by": zoom_by }));
    }
    let px = px_per_sec(nav);
    // A new scale (wheel, − / + / Fit, or `storyboard.zoom` from anywhere): scroll so the time
    // under the anchor (the pointer, else the middle) stays where it was.
    let anchor = nav.state.storyboard.zoom_anchor.take();
    let rescroll = match nav.state.storyboard.view {
        Some((old, offset, w)) if (old - px).abs() > f32::EPSILON => {
            let at_x = anchor.unwrap_or(w / 2.0);
            let t = (offset + at_x - 8.0) / old;
            Some((t * px + 8.0 - at_x).max(0.0))
        }
        _ => None,
    };
    let total = board.total().as_secs_f64() as f32;
    let track_len = board.track.as_ref().map(|t| t.duration.as_secs_f64() as f32).unwrap_or(0.0);
    let width = (total.max(track_len) * px + 80.0).max(ui.available_width());
    let chosen: HashSet<ClipId> = nav.session.selected_clips().into_iter().collect();
    let playing = nav.session.transport().playing;
    let shift = ui.input(|i| i.modifiers.shift);
    let mut select: Option<ClipId> = None;
    let mut scrub: Option<(f64, crate::timebar::Scrub)> = None;
    let mut drop: Option<(ClipId, String)> = None;
    let mut inset_cmds: Vec<(&'static str, serde_json::Value)> = Vec::new();
    let mut switch: Option<(Clip, assetcraft_storyboard::TextKind)> = None;
    let mut resize: Option<ResizeDrag> = nav.state.storyboard.resize_drag;
    let snapping = nav.session.snapping_on();
    let mut snap_label: Option<String> = nav.state.storyboard.snap_label.clone();
    let mut resized: Option<ResizeDrag> = None;
    let mut cut_drag: Option<CutDrag> = nav.state.storyboard.cut_drag;
    let mut rolled: Option<CutDrag> = None;
    let alt = ui.input(|i| i.modifiers.alt);
    let mut slip_card = nav.state.storyboard.slip_card;
    let mut slipped: Option<(u32, f64)> = None;
    let ctx = ui.ctx().clone();
    // A solid, always-visible scroll bar: clicking its track jumps there (finding B3).
    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    let mut area = egui::ScrollArea::horizontal()
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);
    if let Some(x) = rescroll {
        area = area.horizontal_scroll_offset(x);
    }
    let shown = area.show(ui, |ui| {
            let (area, _) = ui.allocate_exact_size(vec2(width, RULER_H + MARKER_H + STRIP_H + ROWS_H), Sense::hover());
            let left = area.left() + 8.0;
            let to_secs = |x: f32| (((x - left) / px).max(0.0) as f64).min(total.max(0.0) as f64);
            // Ruler and scene-marker row: click or drag to move the playhead (finding B3).
            let ruler = Rect::from_min_size(area.min, vec2(width, RULER_H + MARKER_H));
            let tip = "Click or drag to move the playhead";
            if let Some((x, how)) = crate::timebar::scrub_area(ui, ruler, ui.id().with("sb-ruler"), tip) {
                // A drag starts where the button went down, not where egui saw it move (Z2), so a
                // Shift-drag is fine from its first point.
                let x = match how {
                    crate::timebar::Scrub::Start => ui.input(|i| i.pointer.press_origin()).map_or(x, |p| p.x),
                    _ => x,
                };
                scrub = Some((to_secs(x), how));
            }
            let painter = ui.painter().clone();
            let weak = ui.visuals().weak_text_color();
            let ruler_line = Rect::from_min_size(area.min, vec2(width, RULER_H));
            let len = f64::from((width - 8.0) / px);
            crate::timebar::paint_ruler(&painter, ruler_line, px, len, |t| left + t as f32 * px, weak);
            if track_len > 0.0 {
                let x = left + track_len * px;
                painter.line_segment([pos2(x, area.top()), pos2(x, area.bottom())], Stroke::new(1.0, ACCENT));
            }
            // Scene markers and clips.
            let strip_top = area.top() + RULER_H + MARKER_H;
            let marker_y = area.top() + RULER_H + 2.0;
            let mut prev_scene = None;
            let spans = clip_spans(board, left, Some(&ctx), resize, cut_drag, px);
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
                paint_clip(
                    nav,
                    ui,
                    rect,
                    n + 1,
                    clip,
                    index,
                    matches!(board.layout, Some(assetcraft_storyboard::Layout::Split { .. })),
                );
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
                // The right edge (SR1): registered after the card so it wins the pointer there.
                // Between two clips it moves the cut — only the clips either side change, as
                // FilmCraft's roll (Paul, 2026-11-16: "It should only adjust the two clips at
                // that edge"); the last clip's edge sets its length. The cards follow the
                // pointer; the command runs when the drag ends.
                let handle = Rect::from_min_max(pos2(rect.right() - HANDLE_W, rect.top() + 4.0), rect.max);
                let grip = ui.interact(handle, strip_clip_id(id).with("resize"), Sense::drag());
                let last = n + 1 == board.clips.len();
                let grip = grip.on_hover_cursor(egui::CursorIcon::ResizeHorizontal).on_hover_text(match (last, snapping) {
                    (false, true) => "Drag to move this cut: this clip and the next trade length, nothing else moves (snaps to the song's beats, sections and cuts; Shift for a free drag; S turns snapping off)",
                    (false, false) => "Drag to move this cut: this clip and the next trade length, nothing else moves (snapping is off: S turns it on)",
                    (true, true) => "Drag to set the last clip's length (its end snaps to the song's beats, sections and cuts; Shift for a free drag; S turns snapping off)",
                    (true, false) => "Drag to set the last clip's length (snapping is off: S turns it on)",
                });
                if last {
                    if grip.drag_started() {
                        resize = Some(ResizeDrag { clip: id.0, secs: clip.duration.as_secs_f64(), snapped: None });
                    }
                    if grip.dragged()
                        && let Some(pointer) = grip.interact_pointer_pos()
                    {
                        let want = f64::from((pointer.x - x) / px);
                        let (secs, snapped) = snap_resize(board, n, want, at, snapping && !shift, px);
                        snap_label = snapped.as_ref().map(|s| s.label.clone());
                        resize = Some(ResizeDrag { clip: id.0, secs, snapped: snapped.map(|s| s.at.as_secs_f64()) });
                    }
                    if grip.drag_stopped() {
                        resized = resize.take().filter(|d| d.clip == id.0);
                    }
                } else {
                    let from = board.starts().get(n + 1).map_or(0.0, |s| s.as_secs_f64());
                    if grip.drag_started() {
                        cut_drag = Some(CutDrag { left: id.0, from, to: from, snapped: false });
                    }
                    if grip.dragged()
                        && let Some(pointer) = grip.interact_pointer_pos()
                    {
                        let want = f64::from((pointer.x - left) / px);
                        let (to, snapped) = snap_cut(board, n, want, at, snapping && !shift, px);
                        snap_label = snapped.as_ref().map(|s| s.label.clone());
                        cut_drag = Some(CutDrag { left: id.0, from, to, snapped: snapped.is_some() });
                    }
                    if grip.drag_stopped() {
                        rolled = cut_drag.take().filter(|d| d.left == id.0);
                    }
                }
                // Alt-drag inside a video card slips it (C6b): the video moves under the clip (it
                // follows the pointer, so dragging right shows earlier footage), the Viewer shows
                // the new first frame, and one `storyboard.slip` runs on release. Shift: ¼ speed.
                let slipping = slip_card.is_some_and(|(c, _)| c == id.0);
                if (alt || slipping)
                    && !playing
                    && resize.is_none()
                    && let Some(len) = clip_video_len(nav, clip)
                {
                    let body = Rect::from_min_max(rect.min + vec2(0.0, 4.0), pos2(rect.right() - HANDLE_W, rect.bottom()));
                    let r = ui.interact(body, strip_clip_id(id).with("slip"), Sense::drag());
                    let r = r.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text(
                        "Alt-drag to slip: change which part of the video plays, without moving the clip (Shift for fine)",
                    );
                    if r.drag_started() {
                        slip_card = Some((id.0, clip.source_in.as_secs_f64()));
                    }
                    if r.dragged()
                        && let Some((_, from)) = slip_card.filter(|(c, _)| *c == id.0)
                        && let (Some(p), Some(o)) = (r.interact_pointer_pos(), ui.input(|i| i.pointer.press_origin()))
                    {
                        let max = (len - clip.duration.as_secs_f64()).max(0.0);
                        let speed = if shift { 0.25 } else { 1.0 };
                        let secs = from - f64::from((p.x - o.x) / px) * speed;
                        nav.state.storyboard.slip_drag = Some((id.0, ((secs * FPS).round() / FPS).clamp(0.0, max)));
                    }
                    if r.drag_stopped() {
                        slipped = nav.state.storyboard.slip_drag.take().filter(|(c, _)| *c == id.0);
                        slip_card = None;
                    }
                }
                if grip.hovered() || grip.dragged() || resize.is_some_and(|d| d.clip == id.0) || cut_drag.is_some_and(|d| d.left == id.0) {
                    let edge = Rect::from_min_max(pos2(rect.right() - 3.0, rect.top() + 4.0), rect.max);
                    ui.painter().rect_filled(edge, 0, SELECT);
                }
                // The cut after this clip (C1b): a handle in the marker row; dragging it moves the
                // cut, the clips either side trading length (FilmCraft's roll), nothing after it.
                if n + 1 < board.clips.len() && resize.is_none() {
                    let cx = x + w;
                    let handle = Rect::from_center_size(
                        pos2(cx, area.top() + RULER_H + MARKER_H / 2.0),
                        vec2(CUT_HANDLE_W, MARKER_H),
                    );
                    let r = ui.interact(handle, strip_clip_id(id).with("cut"), Sense::drag());
                    let r = r.on_hover_cursor(egui::CursorIcon::ResizeColumn).on_hover_text(if snapping {
                        "Drag to move this cut: the clips either side trade length, nothing after it moves (snaps to the song's beats; Shift for a free drag)"
                    } else {
                        "Drag to move this cut: the clips either side trade length (snapping is off: S turns it on)"
                    });
                    let from = f64::from((cx - left) / px);
                    if r.drag_started() {
                        cut_drag = Some(CutDrag { left: id.0, from, to: from, snapped: false });
                    }
                    if r.dragged()
                        && let Some(pointer) = r.interact_pointer_pos()
                    {
                        let want = f64::from((pointer.x - left) / px);
                        let (to, snapped) = snap_cut(board, n, want, at, snapping && !shift, px);
                        snap_label = snapped.as_ref().map(|s| s.label.clone());
                        cut_drag = Some(CutDrag { left: id.0, from, to, snapped: snapped.is_some() });
                    }
                    if r.drag_stopped() {
                        rolled = cut_drag.take().filter(|d| d.left == id.0);
                    }
                    let active = r.hovered() || cut_drag.is_some_and(|d| d.left == id.0);
                    let tip = handle.center_bottom();
                    let tri = vec![tip, tip + vec2(-4.0, -7.0), tip + vec2(4.0, -7.0)];
                    let fill = if active { SELECT } else { weak };
                    painter.add(egui::Shape::convex_polygon(tri, fill, Stroke::NONE));
                }
            }
            // A cut being dragged: where it would go, across the strip, with how far it moves.
            if let Some(d) = cut_drag {
                let x = left + d.to as f32 * px;
                let color = if d.snapped { SNAP_LINE } else { SELECT };
                painter.vline(x, area.top()..=strip_top + STRIP_H, Stroke::new(2.0, color));
                let moved = d.to - d.from;
                let text = match (&snap_label, d.snapped) {
                    (Some(l), true) => format!("{l} · {moved:+.2} s"),
                    _ => format!("{moved:+.2} s"),
                };
                painter.text(
                    pos2(x + 4.0, area.top() + RULER_H + 2.0),
                    Align2::LEFT_TOP,
                    text,
                    FontId::proportional(10.0),
                    color,
                );
            }
            // The snap line (C5): where the dragged end snapped, labelled in the marker row.
            if let Some(secs) = resize.and_then(|d| d.snapped) {
                let x = left + secs as f32 * px;
                painter.vline(x, area.top()..=area.bottom(), Stroke::new(1.5, SNAP_LINE));
                if let Some(label) = &snap_label {
                    painter.text(
                        pos2(x + 4.0, area.top() + RULER_H + 2.0),
                        egui::Align2::LEFT_TOP,
                        label,
                        egui::FontId::proportional(10.0),
                        SNAP_LINE,
                    );
                }
            }
            let x = left + at.as_secs_f64() as f32 * px;
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
    nav.state.storyboard.view = Some((px, shown.state.offset.x, shown.inner_rect.width()));
    nav.state.storyboard.resize_drag = resize;
    nav.state.storyboard.cut_drag = cut_drag;
    // A slip whose release never reached its card (playback started, the card scrolled away):
    // drop it once no button is down, so the card takes clicks again.
    if slipped.is_none() && slip_card.is_some() && !ui.input(|i| i.pointer.any_down()) {
        slip_card = None;
        nav.state.storyboard.slip_drag = None;
    }
    nav.state.storyboard.slip_card = slip_card;
    if let Some((clip, to)) = slipped {
        commit_slip(nav, clip, to);
    }
    nav.state.storyboard.snap_label =
        snap_label.filter(|_| resize.is_some_and(|d| d.snapped.is_some()) || cut_drag.is_some_and(|d| d.snapped));
    if let Some((mut secs, how)) = scrub {
        // Shift-drag on the ruler scrubs at a quarter of the pointer's speed, from where the drag
        // began (Z2: "slowly scrub the video for positioning").
        use crate::timebar::Scrub;
        let origin = nav.state.storyboard.fine_scrub;
        if shift
            && matches!(how, Scrub::Move | Scrub::Stop)
            && let Some(origin) = origin
        {
            secs = origin + (secs - origin) / FINE_SCRUB;
        } else if matches!(how, Scrub::Start | Scrub::Move) {
            // Full speed: the anchor follows, so Shift slows down from wherever it is pressed.
            nav.state.storyboard.fine_scrub = Some(secs);
        }
        if matches!(how, Scrub::Stop | Scrub::Click) {
            nav.state.storyboard.fine_scrub = None;
        }
        scrub_to(nav, secs, how, playing);
    }
    if let Some(d) = resized {
        commit_resize(nav, d);
    }
    if let Some(d) = rolled {
        commit_roll(nav, d);
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
            // The note first: a long file name would otherwise hide it.
            (Some(_), Some(n)) => format!("{n} — {line2}"),
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
            if let Some(note) = crate::inset_view::note_field(ui, "menu", scene_id, inset.note.as_deref()) {
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
        egui::Popup::menu(&response)
            .id(egui::Id::new(("sb-inset-menu", scene_id)))
            // A click inside (the note field, a slider) must not close it.
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| options(ui, &mut out));
        egui::Popup::context_menu(&response)
            .id(egui::Id::new(("sb-inset-ctx", scene_id)))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
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
    let px = px_per_sec(nav);
    let mut out: Vec<(&'static str, serde_json::Value)> = Vec::new();
    let painter = ui.painter().clone();
    let w = (board.total().as_secs_f64() as f32 * px).max(120.0);
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
    egui::Popup::menu(&response)
        .id(egui::Id::new("sb-voiceover-menu"))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
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
    let px = px_per_sec(nav);
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
    let x_of = |t: f64| left + t as f32 * px;
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
    for (i_piece, &(a, b, off)) in pieces.iter().enumerate() {
        let (x0, x1) = (x_of(off), x_of(off + (b - a)));
        // The visible part of this piece, a column per point (Z1): measured from the decoded
        // audio for just that window when the overview is too coarse for the zoom, the overview
        // otherwise or while the window is being read.
        let (vx0, vx1) = (x0.max(visible.left()).floor(), x1.min(visible.right()).ceil());
        if vx1 <= vx0 || samples.is_empty() || wave_secs <= 0.0 {
            // Nothing to draw here.
        } else {
            let n = samples.len();
            let coarse = wave_secs / n as f64 > 1.0 / f64::from(px);
            let cols = ((vx1 - vx0) as usize).clamp(16, 4096);
            let (from, to) = (a + f64::from((vx0 - x0) / px), a + f64::from((vx1 - x0) / px));
            let fine =
                if coarse && from < wave_secs { window_peaks(nav, &track.id.0, i_piece, from, to, cols) } else { None };
            let mut x = vx0;
            while x < vx1 {
                let (lo, hi) = match &fine {
                    Some(w) => w
                        .get(((x - vx0) as usize * w.len()) / ((vx1 - vx0) as usize).max(1))
                        .copied()
                        .unwrap_or_default(),
                    None => {
                        let (t0, t1) = (a + f64::from((x - x0) / px), a + f64::from((x + 1.0 - x0) / px));
                        if t0 >= wave_secs {
                            (0.0, 0.0)
                        } else {
                            let i0 = ((t0 / wave_secs * n as f64) as usize).min(n - 1);
                            let i1 = ((t1 / wave_secs * n as f64).ceil() as usize).clamp(i0 + 1, n);
                            let (lo, hi) = samples[i0..i1]
                                .iter()
                                .fold((0.0f64, 0.0f64), |(lo, hi), (p, q)| (lo.min(*p), hi.max(*q)));
                            (lo as f32, hi as f32)
                        }
                    }
                };
                painter.line_segment(
                    [pos2(x + 0.5, mid - hi * half), pos2(x + 0.5, mid - lo * half + 1.0)],
                    Stroke::new(1.0, wave),
                );
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
    volume_line(nav, ui, board, rect, left, px);
    section_fades(nav, ui, board, rect, left, px);
}

/// How close (points) the pointer must be to a fade handle (V4).
const FADE_GRAB: f32 = 6.0;

/// The fade in and out of each song section on the AUDIO row (V4; Paul, 2026-11-15: "allow audio
/// fade in/out for each section"): a handle at each top corner of a section's band; drag it in to
/// fade the music up from silence at the start, or down to silence by the end, the ramp drawn
/// over the band. One `storyboard.fade` on release; a double-click removes the fade. Registered
/// after the volume line, so the corners are the fades'.
fn section_fades(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, rect: Rect, left: f32, px: f32) {
    let Some(track) = &board.track else { return };
    let painter = ui.painter().clone();
    let mut drag = nav.state.storyboard.fade_drag;
    let mut commit: Option<(usize, bool, f64)> = None;
    let ramp = Color32::from_gray(0xd8);
    let shade = Color32::from_black_alpha(110);
    for (i, sec) in track.song_sections.iter().enumerate() {
        let (a, b) = (sec.start.as_secs_f64(), sec.end.as_secs_f64());
        let (x0, x1) = (left + a as f32 * px, left + b as f32 * px);
        let band = Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom()));
        if band.width() < 4.0 * FADE_GRAB || !ui.is_rect_visible(band) {
            continue;
        }
        let shown = |out: bool, saved: Ticks| {
            drag.filter(|(s, o, _)| *s == i && *o == out).map_or_else(|| saved.as_secs_f64(), |(_, _, v)| v)
        };
        let (fi, fo) = (shown(false, sec.fade_in), shown(true, sec.fade_out));
        let len = b - a;
        // The ramps: silence shaded above a line from the band's foot to its top.
        let (top, foot) = (rect.top() + 1.0, rect.bottom() - 1.0);
        let xi = x0 + fi as f32 * px;
        let xo = x1 - fo as f32 * px;
        if fi > 0.0 {
            painter.add(egui::Shape::convex_polygon(
                vec![pos2(x0, top), pos2(xi, top), pos2(x0, foot)],
                shade,
                Stroke::NONE,
            ));
            painter.line_segment([pos2(x0, foot), pos2(xi, top)], Stroke::new(1.5, ramp));
        }
        if fo > 0.0 {
            painter.add(egui::Shape::convex_polygon(
                vec![pos2(xo, top), pos2(x1, top), pos2(x1, foot)],
                shade,
                Stroke::NONE,
            ));
            painter.line_segment([pos2(xo, top), pos2(x1, foot)], Stroke::new(1.5, ramp));
        }
        for out in [false, true] {
            let x = if out { xo } else { xi };
            // Kept just inside the band so both corners can be grabbed at zero.
            let x = if out { x.min(x1 - FADE_GRAB) } else { x.max(x0 + FADE_GRAB) };
            let at = pos2(x, top + FADE_GRAB);
            let id = egui::Id::new(("sb-fade", i, out));
            let r = ui.interact(
                Rect::from_center_size(at, vec2(2.0 * FADE_GRAB, 2.0 * FADE_GRAB)),
                id,
                Sense::click_and_drag(),
            );
            let (name, side) = (sec.name.to_lowercase(), if out { "out by the end of" } else { "in at the start of" });
            let r = r
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                .on_hover_text(format!("Drag to fade the music {side} the {name} (double-click removes the fade)"));
            let active = r.hovered() || r.dragged() || (if out { fo } else { fi }) > 0.0;
            painter.rect_filled(
                Rect::from_center_size(at, vec2(7.0, 7.0)),
                1,
                if r.hovered() || r.dragged() {
                    SELECT
                } else if active {
                    ramp
                } else {
                    Color32::from_gray(0x70)
                },
            );
            if r.dragged()
                && let Some(p) = r.interact_pointer_pos()
            {
                let other = if out { fi } else { fo };
                let want = if out { f64::from((x1 - p.x) / px) } else { f64::from((p.x - x0) / px) };
                // To twentieths of a second, held to what the section has left.
                let v = ((want * 20.0).round() / 20.0).clamp(0.0, (len - other).max(0.0));
                drag = Some((i, out, v));
                let label = format!("fade {} {v:.2} s", if out { "out" } else { "in" });
                painter.text(
                    pos2(x, rect.top() - 2.0),
                    if out { Align2::RIGHT_BOTTOM } else { Align2::LEFT_BOTTOM },
                    label,
                    FontId::monospace(9.0),
                    SELECT,
                );
            }
            if r.drag_stopped() {
                commit = drag.take().filter(|(s, o, _)| *s == i && *o == out);
            }
            if r.double_clicked() {
                commit = Some((i, out, 0.0));
                drag = None;
            }
        }
    }
    nav.state.storyboard.fade_drag = drag;
    if let Some((section, out, v)) = commit {
        let saved = track.song_sections.get(section).map(|s| if out { s.fade_out } else { s.fade_in });
        if saved.is_some_and(|t| (t.as_secs_f64() - v).abs() >= 1e-3) {
            nav.run("storyboard.fade", json!({ "section": section, if out { "out" } else { "in" }: v }));
        }
    }
}

/// The top of the volume line's scale in dB (V3).
const VOL_TOP_DB: f32 = 6.0;

/// Where a level sits in the AUDIO row (V3): on the square root of its gain against the top, so
/// the useful range (−20 to +6 dB) takes most of the height; the floor is silence.
pub(crate) fn vol_y(db: f32, rect: Rect) -> f32 {
    let (lo, _) = assetcraft_storyboard::VOLUME_DB_RANGE;
    let norm = if db <= lo { 0.0 } else { (10f32.powf((db - VOL_TOP_DB) / 20.0)).sqrt().clamp(0.0, 1.0) };
    rect.bottom() - 3.0 - norm * (rect.height() - 6.0)
}

/// The level at height `y` in the AUDIO row: [`vol_y`]'s inverse, within the range.
pub(crate) fn vol_db(y: f32, rect: Rect) -> f32 {
    let (lo, hi) = assetcraft_storyboard::VOLUME_DB_RANGE;
    let norm = ((rect.bottom() - 3.0 - y) / (rect.height() - 6.0)).clamp(0.0, 1.0);
    if norm <= 0.0 { lo } else { (VOL_TOP_DB + 40.0 * norm.log10()).clamp(lo, hi) }
}

/// The volume line's level at `t` seconds: `level` with no points, else straight lines in dB
/// between them, held past the ends (as `assetcraft_audio::Envelope` plays it).
pub(crate) fn line_db(points: &[(f64, f32)], level: f32, t: f64) -> f32 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return level };
    if t <= first.0 {
        return first.1;
    }
    if t >= last.0 {
        return last.1;
    }
    let i = points.partition_point(|q| q.0 <= t);
    let (a, b) = (points[i - 1], points[i]);
    if b.0 > a.0 { a.1 + (b.1 - a.1) * ((t - a.0) / (b.0 - a.0)) as f32 } else { b.1 }
}

/// The volume line's widget (there is one AUDIO row).
pub(crate) fn volume_line_id() -> egui::Id {
    egui::Id::new("sb-volume-line")
}

/// A volume-line drag in progress (V3): the line as it is being edited, and what is held — a
/// point by index, or (`None`) the whole line while it has no points.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VolDrag {
    pub(crate) points: Vec<(f64, f32)>,
    pub(crate) level: f32,
    pub(crate) held: Option<usize>,
}

/// The song's volume line over the AUDIO row (V3; Paul, 2026-11-15: "a horizontal line on the
/// track where you can shift+click to add a point and then click the point and move up/down"):
/// with no points, dragging the line sets the song level (`storyboard.trackLevel`); Shift+click
/// adds a point on the line, a point drags up / down (level) and sideways (between its
/// neighbours), a double-click removes it (`storyboard.volume`). One undo step per change.
fn volume_line(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, rect: Rect, left: f32, px: f32) {
    let Some(track) = &board.track else { return };
    let saved: Vec<(f64, f32)> = track.volume.iter().map(|v| (v.at.as_secs_f64(), v.db)).collect();
    let level = f32::from(track.level_db);
    let mut drag = nav.state.storyboard.vol_drag.clone();
    let (points, line_level) = match &drag {
        Some(d) => (d.points.clone(), d.level),
        None => (saved.clone(), level),
    };
    let x_of = |t: f64| left + t as f32 * px;
    let t_of = |x: f32| f64::from((x - left) / px).max(0.0);
    let resp = ui.interact(rect, volume_line_id(), Sense::click_and_drag());
    let pointer = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));
    let near_point = |p: egui::Pos2| {
        points
            .iter()
            .enumerate()
            .map(|(i, (t, db))| (i, pos2(x_of(*t), vol_y(*db, rect)).distance(p)))
            .filter(|(_, d)| *d <= 7.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let on_line = |p: egui::Pos2| (vol_y(line_db(&points, line_level, t_of(p.x)), rect) - p.y).abs() <= 6.0;
    let hover_point = pointer.filter(|p| rect.contains(*p)).and_then(near_point);
    let hover_line = pointer.is_some_and(|p| rect.contains(p) && on_line(p));
    let shift = ui.input(|i| i.modifiers.shift);
    // Start: a point, or the whole line while there are none.
    if resp.drag_started()
        && let Some(p) = ui.input(|i| i.pointer.press_origin())
    {
        drag = match near_point(p) {
            Some(i) => Some(VolDrag { points: points.clone(), level: line_level, held: Some(i) }),
            None if points.is_empty() && on_line(p) => {
                Some(VolDrag { points: Vec::new(), level: line_level, held: None })
            }
            None => None,
        };
    }
    if resp.dragged()
        && let (Some(d), Some(p)) = (drag.as_mut(), pointer)
    {
        match d.held {
            Some(i) => {
                let lo = if i == 0 { 0.0 } else { d.points[i - 1].0 + 0.01 };
                let hi = d.points.get(i + 1).map_or(f64::MAX, |q| q.0 - 0.01);
                if let Some(q) = d.points.get_mut(i) {
                    *q = (t_of(p.x).clamp(lo, hi.max(lo)), (vol_db(p.y, rect) * 2.0).round() / 2.0);
                }
            }
            None => {
                let (lo, hi) = assetcraft_storyboard::TRACK_DB_RANGE;
                d.level = vol_db(p.y, rect).round().clamp(f32::from(lo), f32::from(hi));
            }
        }
    }
    if resp.drag_stopped()
        && let Some(d) = drag.take()
    {
        match d.held {
            Some(_) if d.points != saved => {
                let pts: Vec<[f64; 2]> = d.points.iter().map(|(t, db)| [*t, f64::from(*db)]).collect();
                nav.run("storyboard.volume", json!({ "points": pts }));
            }
            None if (d.level - level).abs() > 0.01 => {
                nav.run("storyboard.trackLevel", json!({ "db": d.level as i64 }));
            }
            _ => {}
        }
    }
    // Shift+click on the line adds a point there; a double-click on a point removes it.
    if resp.double_clicked()
        && let Some(i) = ui.input(|i| i.pointer.interact_pos()).and_then(near_point)
    {
        let pts: Vec<[f64; 2]> =
            saved.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, (t, db))| [*t, f64::from(*db)]).collect();
        nav.run("storyboard.volume", json!({ "points": pts }));
    } else if resp.clicked()
        && shift
        && let Some(p) = ui.input(|i| i.pointer.interact_pos()).filter(|p| on_line(*p) && near_point(*p).is_none())
    {
        let t = t_of(p.x);
        let mut pts: Vec<[f64; 2]> = saved.iter().map(|(t, db)| [*t, f64::from(*db)]).collect();
        pts.push([t, f64::from((line_db(&saved, level, t) * 2.0).round() / 2.0)]);
        nav.run("storyboard.volume", json!({ "points": pts }));
    }
    nav.state.storyboard.vol_drag = drag.clone();
    // Hover help and cursor.
    if hover_point.is_some() || drag.as_ref().is_some_and(|d| d.held.is_some()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    } else if hover_line && points.is_empty() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if drag.is_none() && (hover_line || hover_point.is_some()) {
        resp.on_hover_text(if points.is_empty() {
            "The song's volume: drag the line up or down · Shift+click adds a point"
        } else {
            "Volume line: drag a point · Shift+click the line adds one · double-click a point removes it"
        });
    }
    // Draw: the line across the visible stretch, then its points.
    let painter = ui.painter().with_clip_rect(rect);
    let line = Color32::from_rgb(0xf2, 0xc1, 0x4e);
    let visible = ui.clip_rect().intersect(rect);
    let mut xs: Vec<egui::Pos2> = Vec::new();
    let mut x = visible.left();
    while x <= visible.right() + 3.0 {
        xs.push(pos2(x, vol_y(line_db(&points, line_level, t_of(x)), rect)));
        x += 3.0;
    }
    painter.add(egui::Shape::line(xs, Stroke::new(1.5, line)));
    let held = drag.as_ref().and_then(|d| d.held);
    for (i, (t, db)) in points.iter().enumerate() {
        let c = pos2(x_of(*t), vol_y(*db, rect));
        let hot = held == Some(i) || hover_point == Some(i);
        let r = if hot { 4.5 } else { 3.5 };
        painter.rect_filled(
            Rect::from_center_size(c, vec2(r * 2.0, r * 2.0)),
            1,
            if hot { Color32::WHITE } else { line },
        );
        if hot {
            let (lo, _) = assetcraft_storyboard::VOLUME_DB_RANGE;
            let text = if *db <= lo { "silent".to_owned() } else { format!("{db:+.1} dB") };
            painter.text(c + vec2(7.0, -4.0), Align2::LEFT_BOTTOM, text, FontId::monospace(9.0), Color32::WHITE);
        }
    }
    if let Some(d) = drag.as_ref().filter(|d| d.held.is_none())
        && let Some(p) = pointer
    {
        painter.text(
            p + vec2(10.0, -6.0),
            Align2::LEFT_BOTTOM,
            format!("Song {:+.0} dB", d.level),
            FontId::monospace(9.0),
            Color32::WHITE,
        );
    }
}

/// The song's peaks for `from..to` seconds of the file at `cols` columns (Z1), from
/// `track.peaks {from, to}`: kept per music piece, so a still Strip reads nothing new each frame.
/// `None` while the song is still decoding.
fn window_peaks(
    nav: &mut Navigator,
    path: &str,
    piece: usize,
    from: f64,
    to: f64,
    cols: usize,
) -> Option<Vec<(f32, f32)>> {
    let key = format!("{path}|{from:.4}|{to:.4}|{cols}");
    if let Some((k, v)) = nav.state.storyboard.wave_windows.get(&piece)
        && *k == key
    {
        return Some(v.clone());
    }
    let r =
        nav.session.execute("track.peaks", &json!({ "path": path, "from": from, "to": to, "buckets": cols })).ok()?;
    let peaks: Vec<(f32, f32)> = r["peaks"]
        .as_array()?
        .iter()
        .map(|p| (p[0].as_f64().unwrap_or(0.0) as f32, p[1].as_f64().unwrap_or(0.0) as f32))
        .collect();
    if nav.state.storyboard.wave_windows.len() > 32 {
        nav.state.storyboard.wave_windows.clear();
    }
    nav.state.storyboard.wave_windows.insert(piece, (key, peaks.clone()));
    Some(peaks)
}

/// A cut drag released (C1b): one `storyboard.roll {left, to}` — one undo step — unless it ends
/// where it started.
pub(crate) fn commit_roll(nav: &mut Navigator, d: CutDrag) {
    if (d.to - d.from).abs() < 1e-3 {
        return;
    }
    nav.run("storyboard.roll", json!({ "left": d.left, "to": d.to }));
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
    let length = clip.duration.as_secs_f64();
    // A clip with one after it trades length with that one (C1c): the cut moves, nothing after
    // it does, so the timed cuts stay on the music. The last clip's length is its own.
    let index = board.index_of(clip.id);
    let next = index.and_then(|i| board.clips.get(i + 1));
    let cut = index.and_then(|i| board.starts().get(i + 1).copied()).map_or(0.0, |t| t.as_secs_f64());
    let dragging = match next {
        Some(_) => nav.state.storyboard.cut_drag.filter(|d| d.left == id).map(|d| length + d.to - d.from),
        None => nav.state.storyboard.resize_drag.filter(|d| d.clip == id).map(|d| d.secs),
    };
    let before = dragging.unwrap_or(length);
    let mut value = before;
    let max = match next {
        Some(n) => {
            (length + n.duration.as_secs_f64() - assetcraft_engine::MIN_CLIP_SECS).max(assetcraft_engine::MIN_CLIP_SECS)
        }
        None => assetcraft_storyboard::Limits::default().max_duration.as_secs_f64(),
    };
    let r = ui
        .horizontal(|ui| {
            let r = ui.add(
                egui::DragValue::new(&mut value)
                    .range(assetcraft_engine::MIN_CLIP_SECS..=max)
                    .speed(0.1)
                    .fixed_decimals(1)
                    .suffix(" s"),
            );
            let r = r.on_hover_text(if next.is_some() {
                "Drag or type the clip's length: the next clip gives or takes the difference, so nothing after it moves off the music (Ctrl+Z undoes)"
            } else {
                "Drag or type the last clip's length (Ctrl+Z undoes)"
            });
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
    let roll = |secs: f64| CutDrag { left: id, from: cut, to: cut + secs - length, snapped: false };
    if moved {
        match next {
            Some(_) => nav.state.storyboard.cut_drag = Some(roll(value)),
            None => nav.state.storyboard.resize_drag = Some(ResizeDrag { clip: id, secs: value, snapped: None }),
        }
    }
    if r.drag_stopped() || (moved && !r.dragged()) {
        let secs = if moved { value } else { before };
        match next {
            Some(_) => {
                nav.state.storyboard.cut_drag = None;
                commit_roll(nav, roll(secs));
            }
            None => commit_resize(nav, ResizeDrag { clip: id, secs, snapped: None }),
        }
    }
    ui.add_space(4.0);
}

/// How long a clip's video runs, in seconds, if its hero is a video (title cards have none): read
/// once per file from its header and kept (C6).
fn clip_video_len(nav: &mut Navigator, clip: &Clip) -> Option<f64> {
    let hero = clip
        .asset_ids
        .first()
        .filter(|a| clip.shot_type != ShotType::Ttl && assetcraft_engine::is_video(Path::new(&a.0)))?;
    *nav.state
        .storyboard
        .video_len
        .entry(hero.0.clone())
        .or_insert_with(|| assetcraft_engine::video_seconds(Path::new(&hero.0)).ok())
}

/// The Inspector's In row (C6) for a video clip: where in its video the clip starts, a drag value
/// in seconds that the Viewer follows live, committed as one `storyboard.slip` (one undo step)
/// when the drag ends or a typed value lands. Limited to the video's start and the last in-point
/// that still fills the clip. Nothing for other clips.
pub(crate) fn in_field(nav: &mut Navigator, ui: &mut Ui, clip: &Clip) {
    let Some(len) = clip_video_len(nav, clip) else { return };
    ui.label(RichText::new("In").size(10.0).color(ui.visuals().weak_text_color()));
    let id = clip.id.0;
    let before = in_point(nav, clip);
    let max = (len - clip.duration.as_secs_f64()).max(0.0);
    let mut value = before;
    let r = ui
        .horizontal(|ui| {
            let r = ui.add_enabled(
                max > 0.0,
                egui::DragValue::new(&mut value).range(0.0..=max).speed(1.0 / FPS).fixed_decimals(2).suffix(" s"),
            );
            let r = if max > 0.0 {
                r.on_hover_text("Where in its video the clip starts: drag or type (Ctrl+Z undoes)")
            } else {
                r.on_disabled_hover_text("The video is no longer than the clip, so it can only start at its start")
            };
            ui.weak(format!("{} · of {len:.1} s", frame_timecode(value)));
            r
        })
        .inner;
    // As the Duration field: the widget's first-frame rounding isn't a move.
    let moved = r.changed() && (value - before).abs() >= 0.005;
    if moved {
        nav.state.storyboard.slip_drag = Some((id, value));
    }
    if r.drag_stopped() || (moved && !r.dragged()) {
        nav.state.storyboard.slip_drag = None;
        let to = if moved { value } else { before };
        commit_slip(nav, id, to);
    }
    ui.add_space(4.0);
}

/// End a slip (C6: the In field or an Alt-drag on a card): one `storyboard.slip {clip, to}` when
/// the in-point moved, nothing otherwise.
pub(crate) fn commit_slip(nav: &mut Navigator, clip: u32, to: f64) {
    let same = nav
        .session
        .storyboard()
        .and_then(|b| b.clip(ClipId(clip)))
        .is_none_or(|c| (c.source_in.as_secs_f64() - to).abs() < 0.005);
    if !same {
        nav.run("storyboard.slip", json!({ "clip": clip, "to": to }));
    }
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

fn paint_clip(nav: &mut Navigator, ui: &Ui, rect: Rect, number: usize, clip: &Clip, index: &Index, split: bool) {
    let painter = ui.painter().with_clip_rect(rect);
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 4.0));
    let body = Rect::from_min_max(rect.min + vec2(0.0, 4.0), rect.max);
    let bar_color = match clip.shot_type {
        ShotType::Est => TILE_BORDER,
        ShotType::Ins => Color32::from_rgb(0x6b, 0x72, 0x80),
        t => t.fill_rgb().map(rgb).unwrap_or(TILE_BORDER),
    };
    let title = clip.shot_type == ShotType::Ttl;
    let look = clip.text_style.look_with(nav.session.user_looks());
    painter.rect_filled(body, 0, if title { rgb(look.background) } else { Color32::from_gray(0x22) });
    painter.rect_filled(bar, 0, bar_color);
    let mut drew = false;
    if let Some(hero) = clip.asset_ids.first() {
        let thumb_rect = Rect::from_min_size(body.min, vec2(body.width(), body.height() - 34.0));
        let path = Path::new(&hero.0);
        // A video shows its frame at the clip's in-point (C6), live while it is slipped.
        let picture = if assetcraft_engine::is_video(path) && !title {
            card_frame(nav, ui, path, clip.id, in_point(nav, clip))
        } else if let Cell::Ready(texture, size) = thumb(nav, ui, path, 180.0, index) {
            Some((texture, size))
        } else {
            None
        };
        if let Some((texture, size)) = picture {
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
    // A video's in-point (C6), when it isn't the start or while it is being slipped.
    let slipping = nav.state.storyboard.slip_drag.is_some_and(|(c, _)| c == clip.id.0);
    let from = in_point(nav, clip);
    if (slipping || from > 0.0) && clip.asset_ids.first().is_some_and(|a| assetcraft_engine::is_video(Path::new(&a.0)))
    {
        painter.text(
            pos2(body.right() - 5.0, info.y + 14.0),
            Align2::RIGHT_TOP,
            format!("in {}", frame_timecode(from)),
            FontId::monospace(10.0),
            if slipping { SELECT } else { Color32::from_gray(0x8a) },
        );
    }
    // On a split stage, the clip's region (F5, L1).
    if split && !title {
        let tag = Rect::from_min_size(pos2(body.right() - 20.0, body.top() + 4.0), vec2(16.0, 14.0));
        painter.rect_filled(tag, 2, Color32::from_black_alpha(170));
        painter.text(tag.center(), Align2::CENTER_CENTER, clip.region().letter(), FontId::monospace(10.0), SELECT);
    }
    let border = if title { rgb(0xe5243b) } else { Color32::from_gray(0x3a) };
    painter.rect_stroke(rect, 3, Stroke::new(1.0, border), StrokeKind::Inside);
}

/// A video card's picture (C6): the frame `secs` into `path`, from the Viewer's video service at
/// card size (a frame the Viewer already decoded serves too). Kept per card once shown, so a
/// card asks again only when its in-point moves. While playing, or while the Viewer waits for a
/// frame, a card keeps what it has and leaves the decoder to the Viewer.
fn card_frame(
    nav: &mut Navigator,
    ui: &Ui,
    path: &Path,
    clip: ClipId,
    secs: f64,
) -> Option<(egui::TextureId, egui::Vec2)> {
    let at = (secs.max(0.0) * FPS).round() as u64;
    // Per card: the two halves of a split share a file but not an in-point.
    let key = card_key(path, clip);
    let have = nav.state.storyboard.card_tex.0.get(&key).map(|(k, tex)| (*k, tex.id(), tex.size_vec2()));
    if let Some((k, id, size)) = have
        && k == at
    {
        return Some((id, size));
    }
    let pass = ui.ctx().cumulative_pass_nr();
    let viewer_busy = nav.state.storyboard.viewer_waited.is_some_and(|p| p + 1 >= pass);
    if nav.session.transport().playing || viewer_busy {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        return have.map(|(_, id, size)| (id, size));
    }
    let side = (CARD_PICTURE_PT * ui.ctx().pixels_per_point()).ceil() as u32;
    let (f, exact) = match nav.session.video_frame(path, secs.max(0.0), side) {
        assetcraft_engine::VideoLookup::Ready(f) => (f, true),
        assetcraft_engine::VideoLookup::Pending(near) => {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(15));
            match near {
                Some(f) => (f, false),
                None => return have.map(|(_, id, size)| (id, size)),
            }
        }
        assetcraft_engine::VideoLookup::Failed(_) => return None,
    };
    let size = [f.width as usize, f.height as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, &f.rgba);
    // A stand-in frame is shown but not kept as the answer, so the card asks again.
    let k = if exact { at } else { u64::MAX };
    let tex = match nav.state.storyboard.card_tex.0.get_mut(&key) {
        Some((old, tex)) => {
            tex.set(image, egui::TextureOptions::LINEAR);
            *old = k;
            tex.clone()
        }
        None => {
            if nav.state.storyboard.card_tex.0.len() >= MAX_CARD_PICTURES {
                nav.state.storyboard.card_tex.0.clear();
            }
            let tex = ui.ctx().load_texture(format!("sb-card-{}", key.display()), image, egui::TextureOptions::LINEAR);
            nav.state.storyboard.card_tex.0.insert(key, (k, tex.clone()));
            tex
        }
    };
    Some((tex.id(), tex.size_vec2()))
}

/// Where a card's picture is kept in `card_tex`: its file and clip.
pub(crate) fn card_key(path: &Path, clip: ClipId) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}#{}", path.display(), clip.0))
}

/// How big a video card's picture is decoded, in points (its longest side).
const CARD_PICTURE_PT: f32 = 240.0;

/// How many video cards keep a picture before they are all dropped and asked again.
const MAX_CARD_PICTURES: usize = 128;

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
