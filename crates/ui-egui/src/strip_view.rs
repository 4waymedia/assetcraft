//! Storyboard Strip view (design 2b): Content panel, Viewer, Inspector, full-width timeline.
//!
//! Stage 4 step 4. Draws only. Dragging an asset from the Content panel onto a clip (in the
//! timeline) or onto the Viewer runs `storyboard.place {asset, clip}`; clicking a clip runs
//! `storyboard.select`. The Content panel lists the files of the folder open in the Organizer
//! (from the session's listing: the UI never reads the disk), with the same thumbnails.
//!
//! Sizes from the handoff: Content panel 340 px with a 4-column grid, Inspector 300 px, timeline
//! at 44 px per second with a 150 px clip strip, 2 px `#4da3ff` playhead and drop highlight.

use crate::Navigator;
use crate::storyboard_view::{
    ACCENT, INSPECTOR_W, SCENE_RED, SELECT, TILE_BORDER, contain, cover, inspector, rgb, uv_for_cover,
};
use crate::thumbnails::Cell;
use assetcraft_fs::EntryKind;
use assetcraft_storyboard::{Clip, ClipId, ShotType, Storyboard};
use assetcraft_thumbs::ThumbKey;
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Content panel width and grid columns.
const CONTENT_W: f32 = 340.0;
const CONTENT_COLS: usize = 4;
/// Timeline scale and strip height.
pub(crate) const PX_PER_SEC: f32 = 44.0;
const STRIP_H: f32 = 150.0;
const RULER_H: f32 = 20.0;
const MARKER_H: f32 = 18.0;
const TIMELINE_H: f32 = RULER_H + MARKER_H + STRIP_H + 24.0;
const TITLE_FILL: Color32 = Color32::from_rgb(0x4a, 0x18, 0x20);

/// What an asset drag carries: the file path, as the Organizer lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DragAsset(pub String);

/// Modification times of the open folder's files, so storyboard thumbnails share the
/// Organizer's cache keys.
fn modified_index(nav: &Navigator) -> HashMap<PathBuf, Option<SystemTime>> {
    nav.session.items().iter().filter(|e| e.kind == EntryKind::File).map(|e| (e.path.clone(), e.modified)).collect()
}

fn thumb(
    nav: &mut Navigator,
    ui: &Ui,
    path: &Path,
    side_pt: f32,
    index: &HashMap<PathBuf, Option<SystemTime>>,
) -> Cell {
    let ctx = ui.ctx().clone();
    let side = (side_pt * ctx.pixels_per_point()).ceil() as u32;
    let modified = index.get(path).copied().flatten();
    nav.thumbs.get(&ctx, ThumbKey::new(path, side).with_modified(modified), 1)
}

pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    let Some(board) = nav.session.storyboard().cloned() else { return };
    let index = modified_index(nav);
    egui::Panel::bottom("sb-timeline").exact_size(TIMELINE_H).resizable(false).show(ui, |ui| {
        timeline(nav, ui, &board, &index);
    });
    egui::Panel::left("sb-content").exact_size(CONTENT_W).resizable(false).show(ui, |ui| {
        content_panel(nav, ui, &board, &index);
    });
    egui::Panel::right("sb-inspector").exact_size(INSPECTOR_W).resizable(false).show(ui, |ui| {
        inspector(nav, ui);
    });
    egui::CentralPanel::default().show(ui, |ui| viewer(nav, ui, &board, &index));
}

// ---- Content panel -------------------------------------------------------------------------

fn content_panel(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, index: &HashMap<PathBuf, Option<SystemTime>>) {
    ui.horizontal(|ui| {
        let _ = ui.selectable_label(true, "Content");
        ui.add_enabled(false, egui::Button::selectable(false, "Effects")).on_disabled_hover_text("Later (Stage 5)");
        ui.add_enabled(false, egui::Button::selectable(false, "Titles")).on_disabled_hover_text("Later (step 6)");
    });
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
    let gap = 6.0;
    let cell = ((ui.available_width() - gap * (CONTENT_COLS as f32 - 1.0)) / CONTENT_COLS as f32).floor().max(32.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for row in files.chunks(CONTENT_COLS) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for path in row {
                    asset_cell(nav, ui, path, cell, usage.get(path.to_string_lossy().as_ref()).copied(), index);
                }
            });
            ui.add_space(gap);
        }
    });
}

fn asset_cell(
    nav: &mut Navigator,
    ui: &mut Ui,
    path: &Path,
    side: f32,
    used: Option<usize>,
    index: &HashMap<PathBuf, Option<SystemTime>>,
) {
    let name = assetcraft_fs::display_name(path);
    let payload = DragAsset(path.to_string_lossy().into_owned());
    let id = egui::Id::new(("sb-asset", path));
    let cell = thumb(nav, ui, path, side, index);
    let inner = ui.dnd_drag_source(id, payload, |ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 4, Color32::from_gray(0x15));
        match cell {
            Cell::Ready(texture, size) => {
                let target = cover(rect, size);
                painter.image(texture, rect, uv_for_cover(rect, target), Color32::WHITE);
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
    });
    inner.response.on_hover_text(format!("{name}\nDrag onto a clip in the timeline, or onto the Viewer"));
}

// ---- Viewer --------------------------------------------------------------------------------

fn viewer(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, index: &HashMap<PathBuf, Option<SystemTime>>) {
    let avail = ui.available_rect_before_wrap().shrink(8.0);
    let w = avail.width().min(avail.height() * 16.0 / 9.0).max(16.0);
    let frame = Rect::from_center_size(avail.center(), vec2(w, w * 9.0 / 16.0));
    let response = ui.interact(frame, ui.id().with("sb-viewer"), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(frame, 0, Color32::BLACK);
    let selected = nav.session.selected_clip().and_then(|id| board.clip(id)).cloned();
    match &selected {
        None => {
            painter.text(
                frame.center(),
                Align2::CENTER_CENTER,
                "Select a clip",
                FontId::proportional(14.0),
                Color32::GRAY,
            );
        }
        Some(clip) if clip.shot_type == ShotType::Ttl => {
            painter.text(
                frame.center(),
                Align2::CENTER_CENTER,
                &clip.description,
                FontId::proportional((w / 22.0).clamp(14.0, 40.0)),
                Color32::WHITE,
            );
        }
        Some(clip) => match clip.asset_ids.first() {
            Some(hero) => {
                if let Cell::Ready(texture, size) = thumb(nav, ui, Path::new(&hero.0), 512.0, index) {
                    let target = contain(frame, size);
                    ui.painter().image(
                        texture,
                        target,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
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
        },
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

// ---- Timeline ------------------------------------------------------------------------------

/// Clip rects along the timeline, starting at `origin.x`, in Cut order.
pub(crate) fn clip_spans(board: &Storyboard, left: f32) -> Vec<(ClipId, f32, f32)> {
    let mut x = left;
    board
        .clips
        .iter()
        .map(|c| {
            let w = (c.duration.as_secs_f64() as f32 * PX_PER_SEC).max(2.0);
            let span = (c.id, x, w);
            x += w;
            span
        })
        .collect()
}

fn timeline(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, index: &HashMap<PathBuf, Option<SystemTime>>) {
    let total = board.total().as_secs_f64() as f32;
    let track_len = board.track.as_ref().map(|t| t.duration.as_secs_f64() as f32).unwrap_or(0.0);
    let width = (total.max(track_len) * PX_PER_SEC + 80.0).max(ui.available_width());
    let selected = nav.session.selected_clip();
    let mut select: Option<ClipId> = None;
    let mut drop: Option<(ClipId, String)> = None;
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
                painter.text(
                    pos2(x + 3.0, area.top()),
                    Align2::LEFT_TOP,
                    format!("{}:{:02}", s / 60, s % 60),
                    FontId::monospace(10.0),
                    weak,
                );
            }
        }
        if track_len > 0.0 {
            let x = left + track_len * PX_PER_SEC;
            painter.line_segment([pos2(x, area.top()), pos2(x, area.bottom())], Stroke::new(1.0, ACCENT));
        }
        // Scene markers and clips.
        let strip_top = area.top() + RULER_H + MARKER_H;
        let mut prev_scene = None;
        for ((id, x, w), (n, clip)) in clip_spans(board, left).into_iter().zip(board.clips.iter().enumerate()) {
            if prev_scene != Some(clip.scene_id) && clip.shot_type == ShotType::Ttl {
                let title = board.scene_title(clip.scene_id).unwrap_or_default();
                painter.text(
                    pos2(x, area.top() + RULER_H + 2.0),
                    Align2::LEFT_TOP,
                    "§",
                    FontId::monospace(11.0),
                    SCENE_RED,
                );
                let label = painter
                    .with_clip_rect(Rect::from_min_size(pos2(x + 10.0, area.top() + RULER_H), vec2(240.0, MARKER_H)));
                label.text(
                    pos2(x + 10.0, area.top() + RULER_H + 2.0),
                    Align2::LEFT_TOP,
                    title,
                    FontId::proportional(11.0),
                    weak,
                );
            }
            prev_scene = Some(clip.scene_id);
            let rect = Rect::from_min_size(pos2(x, strip_top), vec2(w, STRIP_H)).shrink2(vec2(1.0, 0.0));
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let response = ui.interact(rect, ui.id().with(("sb-strip", id.0)), Sense::click());
            paint_clip(nav, ui, rect, n + 1, clip, index);
            let accepts = clip.shot_type.accepts_assets();
            let hovering = accepts && response.dnd_hover_payload::<DragAsset>().is_some();
            if selected == Some(id) || hovering {
                ui.painter().rect_stroke(rect, 3, Stroke::new(2.0, SELECT), StrokeKind::Inside);
            }
            if accepts && let Some(asset) = response.dnd_release_payload::<DragAsset>() {
                drop = Some((id, asset.0.clone()));
            }
            if response
                .on_hover_text(format!("#{:02} · {}\n{}", n + 1, clip.shot_type.name(), clip.description))
                .clicked()
            {
                select = Some(id);
            }
        }
        // Playhead at 0 until Watch your Cut (step 5).
        painter.line_segment([pos2(left, area.top()), pos2(left, area.bottom())], Stroke::new(2.0, SELECT));
    });
    if let Some((clip, asset)) = drop {
        nav.run("storyboard.place", json!({ "asset": asset, "clip": clip.0 }));
    } else if let Some(id) = select {
        nav.run("storyboard.select", json!({ "clip": id.0 }));
    }
}

fn paint_clip(
    nav: &mut Navigator,
    ui: &Ui,
    rect: Rect,
    number: usize,
    clip: &Clip,
    index: &HashMap<PathBuf, Option<SystemTime>>,
) {
    let painter = ui.painter().with_clip_rect(rect);
    let (bar, body) = (
        Rect::from_min_size(rect.min, vec2(rect.width(), 4.0)),
        Rect::from_min_max(rect.min + vec2(0.0, 4.0), rect.max),
    );
    let bar_color = match clip.shot_type {
        ShotType::Est => TILE_BORDER,
        ShotType::Ins => Color32::from_rgb(0x6b, 0x72, 0x80),
        t => t.fill_rgb().map(rgb).unwrap_or(TILE_BORDER),
    };
    let title = clip.shot_type == ShotType::Ttl;
    painter.rect_filled(body, 0, if title { TITLE_FILL } else { Color32::from_gray(0x22) });
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
    let text = if title { Color32::WHITE } else { Color32::from_gray(0xdd) };
    if !drew && !title {
        let well = Rect::from_min_size(body.min + vec2(4.0, 4.0), vec2(body.width() - 8.0, body.height() - 42.0));
        if well.width() > 20.0 {
            painter.rect_stroke(well, 3, Stroke::new(1.0, Color32::from_gray(0x55)), StrokeKind::Inside);
            painter.text(
                well.center(),
                Align2::CENTER_CENTER,
                clip.shot_type.code(),
                FontId::proportional(14.0),
                Color32::from_gray(0x8a),
            );
        }
    }
    if title {
        let wrap = (body.width() - 10.0).max(10.0);
        let galley = painter.layout(clip.description.clone(), FontId::proportional(12.0), Color32::WHITE, wrap);
        painter.galley(body.min + vec2(5.0, 8.0), galley, Color32::WHITE);
    }
    let info = pos2(body.left() + 5.0, body.bottom() - 30.0);
    painter.text(
        info,
        Align2::LEFT_TOP,
        format!("#{number:02} {}", clip.shot_type.code()),
        FontId::monospace(10.0),
        text,
    );
    painter.text(
        info + vec2(0.0, 14.0),
        Align2::LEFT_TOP,
        format!("{:.1}s", clip.duration.as_secs_f64()),
        FontId::monospace(10.0),
        Color32::from_gray(0x8a),
    );
    if title {
        painter.rect_stroke(rect, 3, Stroke::new(1.0, rgb(0xe5243b)), StrokeKind::Inside);
    } else {
        painter.rect_stroke(rect, 3, Stroke::new(1.0, Color32::from_gray(0x3a)), StrokeKind::Inside);
    }
}
