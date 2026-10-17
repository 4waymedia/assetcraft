//! Storyboard mode: the mode bar, the Shot map (design 1a) and the clip Inspector.
//!
//! Stage 4 step 3, first slice. Draws only: every click is a `storyboard.*` or `app.mode`
//! command. Colours and sizes come from the design handoff
//! (`docs/design/storyboard-mode-handoff.md`): tiles 112 × 76 px with shot-type fills, gaps
//! 22 / 18, red § on title cards, ◆ on downbeat-synced clips, a 2 px `#4da3ff` selection ring,
//! and a 300 px Inspector.

use crate::Navigator;
use crate::thumbnails::Cell;
use assetcraft_engine::{BoardView, Mode};
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
pub(crate) const INSPECTOR_W: f32 = 300.0;

pub(crate) const ACCENT: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xd6);
pub(crate) const SELECT: Color32 = Color32::from_rgb(0x4d, 0xa3, 0xff);
pub(crate) const TILE_BORDER: Color32 = Color32::from_rgb(0x4a, 0x51, 0x60);
pub(crate) const INSERT_BORDER: Color32 = Color32::from_rgb(0x6b, 0x72, 0x80);
pub(crate) const SCENE_RED: Color32 = Color32::from_rgb(0xff, 0x4d, 0x5e);
const AI_TEXT: Color32 = Color32::from_rgb(0x6f, 0xa3, 0xdc);
pub(crate) const CU_TEXT: Color32 = Color32::from_rgb(0x14, 0x17, 0x1d);

/// Stand-in tracks for trying Re-cut until real tracks arrive (plan step 8):
/// `(label, bpm, seconds)`.
const DEMO_TRACKS: &[(&str, f64, f64)] = &[
    ("Sample track · 92 BPM · 1:00", 92.0, 60.0),
    ("Faster cut · 128 BPM · 0:30", 128.0, 30.0),
    ("Slow burn · 70 BPM · 1:35", 70.0, 95.0),
];

pub(crate) fn rgb(hex: u32) -> Color32 {
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
        let starting = nav.session.storyboard().is_none() || nav.state.storyboard.start.open;
        let new = egui::Button::new("+ New").selected(starting);
        if ui.add(new).on_hover_text("Start a storyboard: style, preset, music").clicked() && !starting {
            nav.state.storyboard.start.open = true;
            nav.state.storyboard.start.library_refresh();
        }
        if starting {
            return;
        }
        let Some(board) = nav.session.storyboard() else { return };
        let name = board.name.clone();
        let format = match (&board.format, &board.preset) {
            (Some(f), Some(p)) => format!("{f} › {p}"),
            (Some(f), None) => f.clone(),
            _ => "No preset".into(),
        };
        let track = board.track.as_ref().map(|t| (t.bpm.get(), t.duration));
        let total = board.total();
        let tname = track_name(board);
        ui.label(RichText::new(name).strong());
        ui.weak(format);
        // View switcher, as a segmented control like Organizer | Storyboard.
        ui.spacing_mut().item_spacing.x = 0.0;
        let view = nav.session.board_view();
        let views = [
            (BoardView::Strip, "Strip", "Viewer, assets and the timeline"),
            (BoardView::Map, "Shot map", "Every clip as a tile"),
            (BoardView::Fill, "Quick fill", "Click assets into empty clips in order"),
        ];
        for (v, label, tip) in views {
            let on = view == v;
            let text = if on { RichText::new(label).color(Color32::WHITE) } else { RichText::new(label) };
            let fill = if on { ACCENT } else { ui.visuals().widgets.inactive.bg_fill };
            if ui.add(egui::Button::new(text).fill(fill)).on_hover_text(tip).clicked() && !on {
                nav.run("storyboard.view", json!({ "view": v.as_str() }));
            }
        }
        ui.spacing_mut().item_spacing.x = 12.0;
        let generated = nav.session.generated_from().is_some();
        match track {
            Some((bpm, len)) if generated => {
                ui.label(format!("♪ {tname} · {} BPM · {}", fmt_bpm(bpm), short_time(len)));
                regenerate_controls(nav, ui);
            }
            Some((bpm, len)) => {
                let current = format!("♪ {tname} · {} BPM · {}", fmt_bpm(bpm), short_time(len));
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
        let _ = total;
        crate::strip_view::transport_controls(nav, ui);
    });
}

/// Storyboard mode's main area: Inspector on the right, Shot map in the middle.
pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    if nav.session.storyboard().is_none() || nav.state.storyboard.start.open {
        crate::start_view::show(nav, ui);
        return;
    }
    if nav.session.board_view() != BoardView::Map {
        crate::strip_view::show(nav, ui);
        return;
    }
    egui::Panel::right("sb-inspector").exact_size(INSPECTOR_W).resizable(false).show(ui, |ui| {
        inspector(nav, ui);
    });
    egui::CentralPanel::default().show(ui, |ui| shot_map(nav, ui));
    crate::strip_view::toast(nav, ui.ctx(), 0.0);
}

/// Lengths offered when generating from a track: seconds (`None` = whole track) and label.
pub(crate) const LENGTH_CHOICES: [(Option<u32>, &str); 5] =
    [(None, "Full length"), (Some(30), "0:30"), (Some(60), "1:00"), (Some(90), "1:30"), (Some(120), "2:00")];

fn length_label(secs: Option<u32>) -> &'static str {
    LENGTH_CHOICES.iter().find(|(s, _)| *s == secs).map_or("Custom", |(_, l)| *l)
}

/// Pacing and Length pickers for a generated storyboard; changing either regenerates (undoable).
fn regenerate_controls(nav: &mut Navigator, ui: &mut Ui) {
    let Some(g) = nav.session.generated_from().cloned() else { return };
    let cue = g.cue_map.to_string_lossy().into_owned();
    let mut run: Option<serde_json::Value> = None;
    egui::ComboBox::from_id_salt("sb-pacing")
        .selected_text(format!("Pacing: {}", g.pacing.name()))
        .width(150.0)
        .show_ui(ui, |ui| {
            for p in assetcraft_storyboard::Pacing::ALL {
                if ui.selectable_label(p == g.pacing, p.name()).clicked() && p != g.pacing {
                    // A new style brings its own shape (as picking a preset does on the Start screen).
                    run = Some(json!({
                        "cueMap": cue, "pacing": p.id(), "seconds": g.seconds, "density": g.density, "shape": {}
                    }));
                }
            }
        });
    egui::ComboBox::from_id_salt("sb-length")
        .selected_text(format!("Length: {}", length_label(g.seconds)))
        .width(130.0)
        .show_ui(ui, |ui| {
            for secs in &g.lengths {
                if ui.selectable_label(*secs == g.seconds, length_label(*secs)).clicked() && *secs != g.seconds {
                    run =
                        Some(json!({ "cueMap": cue, "pacing": g.pacing.id(), "seconds": secs, "density": g.density }));
                }
            }
        });
    if let Some(params) = run {
        nav.run("storyboard.generate", params);
    }
}

/// Shot map, page-map style (design 1a).
fn shot_map(nav: &mut Navigator, ui: &mut Ui) {
    let Some(board) = nav.session.storyboard().cloned() else { return };
    let selected = nav.session.selected_clip();
    // While the Cut plays, the tile under the playhead gets the ring.
    let at = crate::strip_view::playhead(nav, ui.ctx());
    let playing_clip = nav.session.transport().playing.then(|| board.clip_at(at).map(|c| c.id)).flatten();
    let next = assetcraft_storyboard::next_empty(&board);
    let mut clicked: Option<ClipId> = None;
    let mut switch: Option<(Clip, assetcraft_storyboard::TextKind)> = None;
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
            if playing_clip == Some(clip.id) || (playing_clip.is_none() && selected == Some(clip.id)) {
                ui.painter().rect_stroke(tile.expand(3.0), 8, Stroke::new(2.0, SELECT), StrokeKind::Outside);
            } else if next == Some(clip.id) {
                ui.painter().rect_stroke(tile.expand(3.0), 8, Stroke::new(1.0, ACCENT), StrokeKind::Outside);
            }
            caption(ui, tile, i + 1, clip, scene_start);
            let hover = response.on_hover_text(hover_text(clip));
            if hover.clicked() {
                clicked = Some(clip.id);
            }
            if let Some(to) = crate::text_view::kind_chip(ui, tile, clip, id.with("kind")) {
                switch = Some((clip.clone(), to));
            }
        }
    });
    if let Some((clip, to)) = switch {
        crate::text_view::switch_kind(nav, &clip, to);
    } else if let Some(id) = clicked {
        nav.run("storyboard.select", json!({ "clip": id.0 }));
    }
}

fn paint_tile(nav: &mut Navigator, ui: &Ui, tile: Rect, clip: &Clip, _scene_start: bool) {
    let painter = ui.painter();
    let radius = CornerRadius::same(6);
    let card = clip.shot_type == ShotType::Ttl;
    let look = clip.text_style.theme.look();
    let fill = if card { Some(look.background) } else { clip.shot_type.fill_rgb() };
    match fill {
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
        let color = match clip.shot_type {
            ShotType::Cu => CU_TEXT,
            ShotType::Ttl => rgb(look.text),
            _ => Color32::from_gray(0xe6),
        };
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
    if let Some(words) = clip.overlay.as_deref() {
        let band = Rect::from_min_max(
            pos2(tile.left() + 1.0, tile.bottom() - 15.0),
            pos2(tile.right() - 1.0, tile.bottom() - 1.0),
        );
        painter.rect_filled(band, 0, Color32::from_black_alpha(190));
        let clipped = painter.with_clip_rect(band);
        clipped.text(
            band.left_center() + vec2(4.0, 0.0),
            Align2::LEFT_CENTER,
            format!("T  {words}"),
            FontId::proportional(9.0),
            Color32::WHITE,
        );
    }
    if clip.sync_to_downbeat {
        let color = match clip.shot_type {
            ShotType::Cu if !drew_image => CU_TEXT,
            ShotType::Ttl => rgb(look.text),
            _ => Color32::WHITE,
        };
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
    if scene_start && clip.text_kind().is_some() {
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
    if let Some(words) = clip.overlay.as_deref() {
        t.push_str(&format!("\nOverlay: {words}"));
    }
    t
}

/// The Inspector for the selected clip (read-only in this slice; editing is the clip editor, step 6).
pub(crate) fn inspector(nav: &mut Navigator, ui: &mut Ui) {
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
        if clip.text_kind().is_some() {
            crate::text_view::inspector_section(nav, ui, &clip);
            ui.add_space(6.0);
        }
        section(ui, if clip.shot_type.accepts_assets() { "SHOT" } else { "BACKGROUND" });
        if !clip.shot_type.accepts_assets() {
            // A title card: an optional background asset under the words (finding B5).
            match clip.background() {
                Some(bg) => {
                    hero_frame(nav, ui, &bg.0);
                    ui.label(RichText::new(assetcraft_fs::display_name(Path::new(&bg.0))).small())
                        .on_hover_text(bg.0.as_str());
                    if ui.button("Remove background").on_hover_text("Delete key; Ctrl+Z undoes").clicked() {
                        nav.run("storyboard.delete", json!({ "clips": [clip.id.0] }));
                    }
                }
                None => {
                    ui.weak("No background: drag an asset onto the card, or");
                }
            }
            let label = if clip.background().is_some() {
                "Replace with Organizer selection"
            } else {
                "Use Organizer selection"
            };
            if ui.add_enabled(focused.is_some(), egui::Button::new(label)).clicked()
                && let Some(p) = &focused
            {
                nav.run("storyboard.place", json!({ "asset": p.to_string_lossy(), "clip": clip.id.0 }));
            }
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

pub(crate) fn hero_frame(nav: &mut Navigator, ui: &mut Ui, path: &str) {
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

pub(crate) fn section(ui: &mut Ui, title: &str) {
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

pub(crate) fn dashed_rect(ui: &Ui, r: Rect, color: Color32) {
    let stroke = Stroke::new(1.0, color);
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for w in pts.windows(2) {
        ui.painter().extend(egui::Shape::dashed_line(w, stroke, 5.0, 4.0));
    }
}

/// Largest rect of `size`'s aspect inside `r`, centred.
pub(crate) fn contain(r: Rect, size: Vec2) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 {
        return r;
    }
    let s = (r.width() / size.x).min(r.height() / size.y);
    Rect::from_center_size(r.center(), size * s)
}

/// Rect of `size`'s aspect that covers `r`, centred.
pub(crate) fn cover(r: Rect, size: Vec2) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 {
        return r;
    }
    let s = (r.width() / size.x).max(r.height() / size.y);
    Rect::from_center_size(r.center(), size * s)
}

/// UV sub-rect so drawing into `r` crops `target` (a cover rect) to `r`.
pub(crate) fn uv_for_cover(r: Rect, target: Rect) -> Rect {
    let (w, h) = (target.width().max(1.0), target.height().max(1.0));
    Rect::from_min_max(
        pos2((r.left() - target.left()) / w, (r.top() - target.top()) / h),
        pos2((r.right() - target.left()) / w, (r.bottom() - target.top()) / h),
    )
}
