//! Storyboard mode: the mode bar, the Shot map (design 1a) and the clip Inspector.
//!
//! Stage 4 step 3, first slice. Draws only: every click is a `storyboard.*` or `app.mode`
//! command. Colours and sizes come from the design handoff
//! (`docs/design/storyboard-mode-handoff.md`): tiles 112 × 76 px with shot-type fills, gaps
//! 22 / 18, red § on title cards, ◆ on downbeat-synced clips, a 2 px `#4da3ff` selection ring,
//! and a 300 px Inspector.

use crate::Navigator;
use crate::thumbnails::Cell;
use assetcraft_engine::Mode;
use assetcraft_storyboard::{Clip, ClipId, ShotType, Storyboard, Ticks};
use assetcraft_thumbs::ThumbKey;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use serde_json::json;
use std::path::Path;

/// Tile size and gaps (design 1a).
const TILE: Vec2 = vec2(112.0, 76.0);
const CAPTION_H: f32 = 18.0;
const GAP_X: f32 = 22.0;
const GAP_Y: f32 = 18.0;
/// Inspector width.
const INSPECTOR_W: f32 = 300.0;

const ACCENT: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xd6);
const SELECT: Color32 = Color32::from_rgb(0x4d, 0xa3, 0xff);
const TILE_BORDER: Color32 = Color32::from_rgb(0x4a, 0x51, 0x60);
const INSERT_BORDER: Color32 = Color32::from_rgb(0x6b, 0x72, 0x80);
const SCENE_RED: Color32 = Color32::from_rgb(0xff, 0x4d, 0x5e);
const AI_TEXT: Color32 = Color32::from_rgb(0x6f, 0xa3, 0xdc);
const CU_TEXT: Color32 = Color32::from_rgb(0x14, 0x17, 0x1d);

/// Stand-in tracks for trying Re-cut until real tracks arrive (plan step 8):
/// `(label, bpm, seconds)`.
const DEMO_TRACKS: &[(&str, f64, f64)] = &[
    ("Sample track · 92 BPM · 1:00", 92.0, 60.0),
    ("Faster cut · 128 BPM · 0:30", 128.0, 30.0),
    ("Slow burn · 70 BPM · 1:35", 70.0, 95.0),
];

fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The mode bar, shown in both modes: Organizer | Storyboard, then the storyboard's own controls.
pub(crate) fn mode_bar(nav: &mut Navigator, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mode = nav.session.mode();
        for (m, label) in [(Mode::Organizer, "Organizer"), (Mode::Storyboard, "Storyboard")] {
            let on = mode == m;
            let text = if on { RichText::new(label).color(Color32::WHITE) } else { RichText::new(label) };
            let button = egui::Button::new(text).fill(if on { ACCENT } else { ui.visuals().widgets.inactive.bg_fill });
            if ui.add(button).clicked() && !on {
                nav.run("app.mode", json!({ "mode": m.as_str() }));
            }
        }
        ui.spacing_mut().item_spacing.x = 12.0;
        if mode != Mode::Storyboard {
            return;
        }
        ui.add_space(12.0);
        let Some(board) = nav.session.storyboard() else {
            ui.weak("No storyboard open");
            return;
        };
        let name = board.name.clone();
        let format = match (&board.format, &board.preset) {
            (Some(f), Some(p)) => format!("{f} › {p}"),
            (Some(f), None) => f.clone(),
            _ => "No preset".into(),
        };
        let track = board.track.as_ref().map(|t| (t.bpm.get(), t.duration));
        let total = board.total();
        ui.label(RichText::new(name).strong());
        ui.weak(format);
        match track {
            Some((bpm, len)) => {
                let current = format!("♪ {} · {} BPM · {}", track_name(board), fmt_bpm(bpm), short_time(len));
                egui::ComboBox::from_id_salt("sb-track").selected_text(current).width(240.0).show_ui(ui, |ui| {
                    for (label, b, secs) in DEMO_TRACKS {
                        if ui.selectable_label(false, *label).clicked() {
                            nav.run("storyboard.recut", json!({ "bpm": b, "seconds": secs }));
                        }
                    }
                });
            }
            None => {
                ui.weak("♪ + Add track (step 8)");
            }
        }
        let (can_undo, can_redo) = (nav.session.can_undo_storyboard(), nav.session.can_redo_storyboard());
        if ui.add_enabled(can_undo, egui::Button::new("Undo")).on_hover_text("Ctrl+Z").clicked() {
            nav.run("storyboard.undo", json!({}));
        }
        if ui.add_enabled(can_redo, egui::Button::new("Redo")).on_hover_text("Ctrl+Y").clicked() {
            nav.run("storyboard.redo", json!({}));
        }
        ui.label(RichText::new(format!("0:00.0 / {}", total.timecode())).font(FontId::monospace(12.0)));
    });
}

/// Storyboard mode's main area: Inspector on the right, Shot map in the middle.
pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    if nav.session.storyboard().is_none() {
        empty_state(nav, ui);
        return;
    }
    egui::Panel::right("sb-inspector").exact_size(INSPECTOR_W).resizable(false).show(ui, |ui| {
        inspector(nav, ui);
    });
    egui::CentralPanel::default().show(ui, |ui| shot_map(nav, ui));
}

fn empty_state(nav: &mut Navigator, ui: &mut Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.3);
        ui.heading("No storyboard yet");
        ui.weak("Start empty, or open the 24-clip sample from the design.");
        ui.add_space(12.0);
        if ui.button("New storyboard").clicked() {
            nav.run("storyboard.new", json!({}));
        }
        if ui.button("New from sample").clicked() {
            nav.run("storyboard.new", json!({ "sample": true }));
        }
    });
}

/// Shot map, page-map style (design 1a).
fn shot_map(nav: &mut Navigator, ui: &mut Ui) {
    let Some(board) = nav.session.storyboard().cloned() else { return };
    let selected = nav.session.selected_clip();
    let next = assetcraft_storyboard::next_empty(&board);
    let mut clicked: Option<ClipId> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let width = ui.available_width();
        let per_row = (((width + GAP_X) / (TILE.x + GAP_X)).floor() as usize).max(1);
        let rows = board.clips.len().div_ceil(per_row);
        let cell_h = TILE.y + CAPTION_H;
        let height = rows as f32 * (cell_h + GAP_Y);
        let (area, _) = ui.allocate_exact_size(vec2(width, height.max(1.0)), Sense::hover());
        let mut prev_scene = None;
        for (i, clip) in board.clips.iter().enumerate() {
            let col = (i % per_row) as f32;
            let row = (i / per_row) as f32;
            let min = area.min + vec2(col * (TILE.x + GAP_X), row * (cell_h + GAP_Y));
            let tile = Rect::from_min_size(min, TILE);
            if !ui.is_rect_visible(tile.expand(CAPTION_H)) {
                prev_scene = Some(clip.scene_id);
                continue;
            }
            let id = ui.id().with(("sb-tile", clip.id.0));
            let response = ui.interact(tile, id, Sense::click());
            let scene_start = prev_scene != Some(clip.scene_id);
            prev_scene = Some(clip.scene_id);
            paint_tile(nav, ui, tile, clip, scene_start);
            if selected == Some(clip.id) {
                ui.painter().rect_stroke(tile.expand(3.0), 8, Stroke::new(2.0, SELECT), StrokeKind::Outside);
            } else if next == Some(clip.id) {
                ui.painter().rect_stroke(tile.expand(3.0), 8, Stroke::new(1.0, ACCENT), StrokeKind::Outside);
            }
            caption(ui, tile, i + 1, clip, scene_start);
            let hover = response.on_hover_text(hover_text(clip));
            if hover.clicked() {
                clicked = Some(clip.id);
            }
        }
    });
    if let Some(id) = clicked {
        nav.run("storyboard.select", json!({ "clip": id.0 }));
    }
}

fn paint_tile(nav: &mut Navigator, ui: &Ui, tile: Rect, clip: &Clip, _scene_start: bool) {
    let painter = ui.painter();
    let radius = CornerRadius::same(6);
    match clip.shot_type.fill_rgb() {
        Some(fill) => {
            painter.rect_filled(tile, radius, rgb(fill));
            painter.rect_stroke(tile, radius, Stroke::new(1.0, TILE_BORDER), StrokeKind::Inside);
        }
        None => dashed_rect(ui, tile, INSERT_BORDER),
    }
    // A filled clip shows its hero frame; an empty one its shot code.
    let hero = clip.asset_ids.first().map(|a| a.0.clone());
    let mut drew_image = false;
    if let Some(path) = hero {
        let ctx = ui.ctx().clone();
        let side = (TILE.x * ctx.pixels_per_point()).ceil() as u32;
        if let Cell::Ready(texture, size) = nav.thumbs.get(&ctx, ThumbKey::new(Path::new(&path), side), 1) {
            let inner = tile.shrink(1.0);
            let target = cover(inner, size);
            let uv = uv_for_cover(inner, target);
            ui.painter().image(texture, inner, uv, Color32::WHITE);
            drew_image = true;
        }
    }
    let painter = ui.painter();
    if !drew_image {
        let color = if clip.shot_type == ShotType::Cu { CU_TEXT } else { Color32::from_gray(0xe6) };
        painter.text(tile.center(), Align2::CENTER_CENTER, clip.shot_type.code(), FontId::proportional(18.0), color);
    } else {
        // Shot code as a small corner chip over the image.
        let chip = Rect::from_min_size(tile.right_bottom() - vec2(34.0, 16.0), vec2(30.0, 13.0));
        painter.rect_filled(chip, 3, Color32::from_black_alpha(170));
        painter.text(
            chip.center(),
            Align2::CENTER_CENTER,
            clip.shot_type.code(),
            FontId::monospace(9.0),
            Color32::WHITE,
        );
    }
    if clip.sync_to_downbeat {
        let color = if clip.shot_type == ShotType::Cu { CU_TEXT } else { Color32::WHITE };
        painter.text(tile.min + vec2(6.0, 4.0), Align2::LEFT_TOP, "◆", FontId::proportional(10.0), color);
    }
}

/// `#n · § · duration` under the tile.
fn caption(ui: &Ui, tile: Rect, number: usize, clip: &Clip, scene_start: bool) {
    let painter = ui.painter();
    let y = tile.bottom() + 3.0;
    let mono = FontId::monospace(11.0);
    let weak = ui.visuals().weak_text_color();
    let num = format!("{number:02}");
    let galley = painter.layout_no_wrap(num, mono.clone(), ui.visuals().text_color());
    let mut x = tile.left();
    let w = galley.size().x;
    painter.galley(pos2(x, y), galley, Color32::PLACEHOLDER);
    x += w + 6.0;
    if scene_start && clip.shot_type == ShotType::Ttl {
        painter.text(pos2(x, y), Align2::LEFT_TOP, "§", mono.clone(), SCENE_RED);
    }
    let dur = format!("{:.1}s", clip.duration.as_secs_f64());
    painter.text(pos2(tile.right(), y), Align2::RIGHT_TOP, dur, mono, weak);
}

fn hover_text(clip: &Clip) -> String {
    let mut t = format!("#{} · {}", clip.id.0, clip.shot_type.name());
    if !clip.description.is_empty() {
        t.push('\n');
        t.push_str(&clip.description);
    }
    t
}

/// The Inspector for the selected clip (read-only in this slice; editing is the clip editor, step 6).
fn inspector(nav: &mut Navigator, ui: &mut Ui) {
    let Some(board) = nav.session.storyboard().cloned() else { return };
    let selected = nav.session.selected_clip().and_then(|id| board.clip(id).cloned());
    let focused = focused_file(nav);
    egui::ScrollArea::vertical().show(ui, |ui| {
        let (filled, fillable) = board.fill_progress();
        section(ui, "QUICK FILL");
        ui.horizontal(|ui| {
            let frac = if fillable == 0 { 1.0 } else { filled as f32 / fillable as f32 };
            ui.add(egui::ProgressBar::new(frac).desired_width(180.0).fill(ACCENT));
            ui.label(RichText::new(format!("{filled} / {fillable}")).font(FontId::monospace(11.0)));
        });
        match assetcraft_storyboard::next_empty(&board).and_then(|id| board.clip(id)) {
            Some(next) => {
                ui.label(
                    RichText::new(format!(
                        "NEXT · #{:02} · {} · {:.1}s",
                        board.index_of(next.id).map(|i| i + 1).unwrap_or(0),
                        next.shot_type.name(),
                        next.duration.as_secs_f64()
                    ))
                    .font(FontId::monospace(11.0)),
                );
                let can_place = focused.is_some();
                let hint = match &focused {
                    Some(p) => format!("Place {}", assetcraft_fs::display_name(p)),
                    None => {
                        "Select a file in the Organizer first (or use Edit → Place in Next Empty Clip there)".into()
                    }
                };
                if ui
                    .add_enabled(can_place, egui::Button::new("Place Organizer selection"))
                    .on_hover_text(hint)
                    .clicked()
                    && let Some(p) = &focused
                {
                    nav.run("storyboard.place", json!({ "asset": p.to_string_lossy() }));
                }
            }
            None => {
                ui.label(RichText::new(format!("All {fillable} clips filled")).color(SELECT));
            }
        }
        ui.add_space(10.0);
        let Some(clip) = selected else {
            ui.weak("Select a clip in the Shot map.");
            return;
        };
        let number = board.index_of(clip.id).map(|i| i + 1).unwrap_or(0);
        section(ui, "CLIP");
        ui.label(RichText::new(format!("#{number:02} · {}", clip.shot_type.name())).size(14.0).strong());
        if let Some(title) = board.scene_title(clip.scene_id) {
            ui.weak(format!("Scene: {title}"));
        }
        ui.add_space(6.0);
        field(ui, "Description", if clip.description.is_empty() { "—" } else { &clip.description });
        let beats = board.track.as_ref().map(|t| t.bpm.ticks_to_beats(clip.duration));
        let dur = match beats {
            Some(b) => format!("{:.1}s · {b} beats", clip.duration.as_secs_f64()),
            None => format!("{:.1}s", clip.duration.as_secs_f64()),
        };
        field(ui, "Duration", &dur);
        field(ui, "Transitions", &format!("{:?} in · {:?} out", clip.transition_in, clip.transition_out));
        if clip.sync_to_downbeat {
            ui.label(RichText::new("◆ Start locked to downbeat").color(AI_TEXT));
        }
        ui.add_space(6.0);
        section(ui, "SHOT");
        if !clip.shot_type.accepts_assets() {
            ui.weak("Title cards hold text only.");
        } else if let Some(hero) = clip.asset_ids.first() {
            hero_frame(nav, ui, &hero.0);
            ui.label(RichText::new(assetcraft_fs::display_name(Path::new(&hero.0))).small())
                .on_hover_text(hero.0.as_str());
            let can_replace = focused.is_some();
            if ui.add_enabled(can_replace, egui::Button::new("Replace with Organizer selection")).clicked()
                && let Some(p) = &focused
            {
                nav.run("storyboard.place", json!({ "asset": p.to_string_lossy(), "clip": clip.id.0 }));
            }
        } else {
            ui.weak("No shot assigned");
            let can_place = focused.is_some();
            if ui.add_enabled(can_place, egui::Button::new("Place Organizer selection here")).clicked()
                && let Some(p) = &focused
            {
                nav.run("storyboard.place", json!({ "asset": p.to_string_lossy(), "clip": clip.id.0 }));
            }
        }
    });
}

/// The Organizer's focused item, when it is a file in the open folder's listing (no disk access:
/// the UI thread never stats files).
pub(crate) fn focused_file(nav: &Navigator) -> Option<std::path::PathBuf> {
    let focus = nav.session.selection().focus()?;
    nav.session
        .items()
        .iter()
        .find(|e| e.path == focus && e.kind == assetcraft_fs::EntryKind::File)
        .map(|e| e.path.clone())
}

fn hero_frame(nav: &mut Navigator, ui: &mut Ui, path: &str) {
    let w = ui.available_width().min(INSPECTOR_W - 16.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, w * 9.0 / 16.0), Sense::hover());
    ui.painter().rect_filled(rect, 4, Color32::BLACK);
    let ctx = ui.ctx().clone();
    let side = (w * ctx.pixels_per_point()).ceil() as u32;
    if let Cell::Ready(texture, size) = nav.thumbs.get(&ctx, ThumbKey::new(Path::new(path), side), 0) {
        let target = contain(rect, size);
        ui.painter().image(texture, target, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(10.0).color(ui.visuals().weak_text_color()));
}

fn field(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).size(10.0).color(ui.visuals().weak_text_color()));
    ui.label(value);
    ui.add_space(4.0);
}

/// The storyboard status bar: "24 clips · 1 selected · n assets assigned".
pub(crate) fn status(nav: &Navigator, ui: &mut Ui) {
    match nav.session.storyboard() {
        Some(board) => {
            ui.label(format!("{} clips", board.clips.len()));
            ui.separator();
            ui.label(format!("{} selected", usize::from(nav.session.selected_clip().is_some())));
            ui.separator();
            ui.label(format!("{} assets assigned", board.assets_assigned()));
        }
        None => {
            ui.label("No storyboard");
        }
    }
}

fn track_name(board: &Storyboard) -> String {
    board.track.as_ref().map(|t| t.name.clone()).unwrap_or_default()
}

fn fmt_bpm(bpm: f64) -> String {
    if (bpm - bpm.round()).abs() < 1e-9 { format!("{}", bpm.round() as i64) } else { format!("{bpm:.1}") }
}

fn short_time(t: Ticks) -> String {
    let secs = t.as_secs_f64().round() as u64;
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn dashed_rect(ui: &Ui, r: Rect, color: Color32) {
    let stroke = Stroke::new(1.0, color);
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for w in pts.windows(2) {
        ui.painter().extend(egui::Shape::dashed_line(w, stroke, 5.0, 4.0));
    }
}

/// Largest rect of `size`'s aspect inside `r`, centred.
fn contain(r: Rect, size: Vec2) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 {
        return r;
    }
    let s = (r.width() / size.x).min(r.height() / size.y);
    Rect::from_center_size(r.center(), size * s)
}

/// Rect of `size`'s aspect that covers `r`, centred.
fn cover(r: Rect, size: Vec2) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 {
        return r;
    }
    let s = (r.width() / size.x).max(r.height() / size.y);
    Rect::from_center_size(r.center(), size * s)
}

/// UV sub-rect so drawing into `r` crops `target` (a cover rect) to `r`.
fn uv_for_cover(r: Rect, target: Rect) -> Rect {
    let (w, h) = (target.width().max(1.0), target.height().max(1.0));
    Rect::from_min_max(
        pos2((r.left() - target.left()) / w, (r.top() - target.top()) / h),
        pos2((r.right() - target.left()) / w, (r.bottom() - target.top()) / h),
    )
}
