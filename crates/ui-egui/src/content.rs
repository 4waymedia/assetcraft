//! Content panel: view toolbar, a virtualized thumbnail grid, and a virtualized details list.
//!
//! Both views lay out only the rows inside the scroll viewport (manual row math inside
//! `ScrollArea::show_viewport`), so a folder of tens of thousands of files costs the same per
//! frame as one screenful. Thumbnails are requested only for the cells actually drawn.

use crate::Navigator;
use crate::thumbnails::{Badge, Cell, PRIORITY_GRID, bucket};
use crate::widgets::{badge, fit_at_most, format_size, format_time, paint_folder, placeholder};
use assetcraft_engine::{FileInfo, Location, SortKey, ViewMode};
use assetcraft_fs::{Entry, EntryKind};
use assetcraft_thumbs::ThumbKey;
use egui::{Align2, Color32, FontId, Rect, Response, ScrollArea, Sense, Stroke, StrokeKind, TextStyle, Ui, pos2, vec2};
use serde_json::json;

/// Details-list row height.
const ROW_HEIGHT: f32 = 22.0;
/// Space around a grid thumbnail, and below it for the name.
const CELL_PAD: f32 = 8.0;
const LABEL_HEIGHT: f32 = 18.0;

pub(crate) fn sort_name(key: SortKey) -> &'static str {
    match key {
        SortKey::Name => "name",
        SortKey::Date => "date",
        SortKey::Size => "size",
        SortKey::Type => "type",
    }
}

pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    toolbar(nav, ui);
    ui.separator();
    if nav.session.items().is_empty() {
        nav.state.grid_columns = 1;
        ui.centered_and_justified(|ui| {
            ui.weak(if matches!(nav.session.location(), Location::Computer) {
                "No drives found"
            } else {
                "This folder is empty"
            });
        });
        return;
    }
    match nav.session.view().mode {
        ViewMode::Grid => grid(nav, ui),
        ViewMode::List => list(nav, ui),
    }
}

fn toolbar(nav: &mut Navigator, ui: &mut Ui) {
    let view = nav.session.view().clone();
    ui.horizontal(|ui| {
        if ui.selectable_label(view.mode == ViewMode::Grid, "Grid").clicked() {
            nav.run("view.mode", json!({ "mode": "grid" }));
        }
        if ui.selectable_label(view.mode == ViewMode::List, "List").clicked() {
            nav.run("view.mode", json!({ "mode": "list" }));
        }
        ui.separator();
        ui.label("Sort");
        egui::ComboBox::from_id_salt("content-sort").selected_text(label(view.sort)).show_ui(ui, |ui| {
            for key in [SortKey::Name, SortKey::Date, SortKey::Size, SortKey::Type] {
                if ui.selectable_label(view.sort == key, label(key)).clicked() {
                    nav.run("view.sort", json!({ "by": sort_name(key) }));
                }
            }
        });
        let direction = if view.ascending { "Ascending" } else { "Descending" };
        if ui.button(direction).on_hover_text("Reverse the sort order").clicked() {
            nav.run("view.sort", json!({ "by": sort_name(view.sort), "ascending": !view.ascending }));
        }
        let mut hidden = view.show_hidden;
        if ui.checkbox(&mut hidden, "Hidden").on_hover_text("Show hidden files (Ctrl+H)").changed() {
            nav.run("view.hidden", json!({ "show": hidden }));
        }
        if view.mode == ViewMode::Grid {
            ui.separator();
            let (lo, hi) = assetcraft_engine::prefs::THUMB_SIZE_RANGE;
            let mut px = view.thumb_size;
            let slider = egui::Slider::new(&mut px, lo..=hi).show_value(false).text("Size");
            if ui.add(slider).changed() {
                nav.run("view.thumbSize", json!({ "px": px }));
            }
        }
    });
}

fn label(key: SortKey) -> &'static str {
    match key {
        SortKey::Name => "Name",
        SortKey::Date => "Date modified",
        SortKey::Size => "Size",
        SortKey::Type => "Type",
    }
}

/// The scroll offset that brings `row` fully into view, if a keyboard move asked for it.
fn reveal_offset(nav: &mut Navigator, row_height: f32, viewport: f32, columns: usize) -> Option<f32> {
    if !std::mem::take(&mut nav.state.reveal_focus) {
        return None;
    }
    let focus = nav.session.selection().focus()?;
    let index = nav.session.items().iter().position(|e| e.path == focus)?;
    let top = (index / columns.max(1)) as f32 * row_height;
    let current = match &nav.state.scroll {
        Some((loc, y)) if loc == nav.session.location() => *y,
        _ => 0.0,
    };
    if top < current {
        Some(top)
    } else if top + row_height > current + viewport {
        Some((top + row_height - viewport).max(0.0))
    } else {
        None
    }
}

fn grid(nav: &mut Navigator, ui: &mut Ui) {
    let size = nav.session.view().thumb_size as f32;
    let cell = vec2(size + CELL_PAD * 2.0, size + CELL_PAD * 2.0 + LABEL_HEIGHT);
    let width = ui.available_width();
    let columns = ((width / cell.x).floor() as usize).max(1);
    nav.state.grid_columns = columns;
    let count = nav.session.items().len();
    let rows = count.div_ceil(columns);
    let mut area = ScrollArea::vertical().id_salt(("grid", nav.session.location().clone())).auto_shrink(false);
    if let Some(y) = reveal_offset(nav, cell.y, ui.available_height(), columns) {
        area = area.vertical_scroll_offset(y);
    }
    let side = bucket(size * ui.ctx().pixels_per_point());
    let output = area.show_viewport(ui, |ui, viewport| {
        ui.set_height(rows as f32 * cell.y);
        ui.set_width(width - 4.0);
        let origin = ui.max_rect().min;
        let background = ui.interact(ui.max_rect(), ui.id().with("grid-bg"), Sense::click());
        let first_row = (viewport.min.y / cell.y).floor().max(0.0) as usize;
        let last_row = ((viewport.max.y / cell.y).ceil() as usize).min(rows);
        let visible: Vec<(usize, Entry)> = nav
            .session
            .items()
            .iter()
            .enumerate()
            .skip(first_row * columns)
            .take(last_row.saturating_sub(first_row) * columns)
            .map(|(i, e)| (i, e.clone()))
            .collect();
        let mut hit = false;
        for (index, entry) in &visible {
            let (row, col) = (index / columns, index % columns);
            let rect = Rect::from_min_size(origin + vec2(col as f32 * cell.x, row as f32 * cell.y), cell);
            hit |= grid_cell(nav, ui, entry, rect, size, side);
        }
        if background.clicked() && !hit {
            nav.run("select.none", json!({}));
        }
    });
    nav.state.scroll = Some((nav.session.location().clone(), output.state.offset.y));
}

/// One grid cell. Returns `true` if it was clicked.
fn grid_cell(nav: &mut Navigator, ui: &mut Ui, entry: &Entry, rect: Rect, size: f32, side: u32) -> bool {
    let response = ui.interact(rect, ui.id().with(("cell", &entry.path)), Sense::click());
    let selected = nav.session.selection().contains(&entry.path);
    let focused = nav.session.selection().focus() == Some(entry.path.as_path());
    let visuals = ui.visuals().clone();
    if selected {
        ui.painter().rect_filled(rect.shrink(2.0), 6.0, visuals.selection.bg_fill.gamma_multiply(0.6));
    } else if response.hovered() {
        ui.painter().rect_filled(rect.shrink(2.0), 6.0, visuals.widgets.hovered.weak_bg_fill);
    }
    if focused {
        ui.painter().rect_stroke(rect.shrink(2.0), 6.0, visuals.selection.stroke, StrokeKind::Inside);
    }
    let thumb_rect = Rect::from_min_size(rect.min + vec2(CELL_PAD, CELL_PAD), vec2(size, size));
    let drawn = draw_thumb(nav, ui, entry, thumb_rect, side, PRIORITY_GRID);
    nav.state.drawn.push((entry.path.clone(), drawn));
    let name_rect =
        Rect::from_min_size(pos2(rect.min.x + 4.0, thumb_rect.max.y + 2.0), vec2(rect.width() - 8.0, LABEL_HEIGHT));
    let galley = egui::WidgetText::from(entry.name.as_str()).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        name_rect.width(),
        TextStyle::Small,
    );
    let pos = pos2(name_rect.center().x - galley.size().x / 2.0, name_rect.min.y);
    ui.painter().with_clip_rect(name_rect).galley(pos, galley, visuals.text_color());
    let response = response.on_hover_text(&entry.name);
    item_interaction(nav, ui, &response, entry)
}

/// Draw a thumbnail (or folder glyph, placeholder, type label, badge) inside `rect`.
pub(crate) fn draw_thumb(nav: &mut Navigator, ui: &Ui, entry: &Entry, rect: Rect, side: u32, priority: u8) -> Badge {
    if entry.kind == EntryKind::Dir {
        paint_folder(ui, rect);
        return Badge::Folder;
    }
    if entry.kind != EntryKind::File {
        type_label(ui, rect, "LINK");
        return Badge::TypeIcon;
    }
    let ctx = ui.ctx().clone();
    match nav.thumbs.get(&ctx, ThumbKey::new(&entry.path, side).with_modified(entry.modified), priority) {
        Cell::Ready(texture, tex_size) => {
            let ppp = ctx.pixels_per_point();
            // Small images may be enlarged a little, never into a blur of pixels.
            let target = fit_at_most(rect, tex_size / ppp, 4.0);
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            ui.painter().image(texture, target, uv, Color32::WHITE);
            Badge::Image
        }
        Cell::Loading => {
            placeholder(ui, rect);
            Badge::Loading
        }
        Cell::Failed(error) => {
            let kind = Badge::for_error(&error, &entry.path);
            match kind {
                Badge::Unreadable => {
                    placeholder(ui, rect);
                    badge(ui, rect, "unreadable", crate::theme::BADGE_ERROR, Color32::WHITE);
                }
                Badge::TooLarge => {
                    placeholder(ui, rect);
                    badge(ui, rect, "too large", ui.visuals().widgets.inactive.bg_fill, ui.visuals().text_color());
                }
                _ => type_label(ui, rect, &type_text(entry)),
            }
            kind
        }
    }
}

fn type_label(ui: &Ui, rect: Rect, text: &str) {
    placeholder(ui, rect);
    let font = FontId::monospace((rect.width() / 6.0).clamp(10.0, 28.0));
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, ui.visuals().weak_text_color());
}

/// Extension in capitals, `FOLDER`, or `FILE`.
fn type_text(entry: &Entry) -> String {
    match entry.kind {
        EntryKind::Dir => "Folder".into(),
        _ => entry.extension().map(|e| e.to_uppercase()).unwrap_or_else(|| "File".into()),
    }
}

/// Click / Ctrl-click / Shift-click / double-click / context menu on an item.
/// Returns `true` if it was clicked (primary or secondary).
fn item_interaction(nav: &mut Navigator, ui: &Ui, response: &Response, entry: &Entry) -> bool {
    let path = entry.path.display().to_string();
    if response.clicked() {
        let m = ui.input(|i| i.modifiers);
        let mode = if m.command || m.ctrl {
            "toggle"
        } else if m.shift {
            "range"
        } else {
            "replace"
        };
        nav.run("select.click", json!({ "path": path, "mode": mode }));
    }
    if response.secondary_clicked() && !nav.session.selection().contains(&entry.path) {
        nav.run("select.click", json!({ "path": path }));
    }
    if response.double_clicked() {
        nav.run("file.open", json!({ "path": path }));
    }
    let is_dir = entry.kind == EntryKind::Dir;
    response.context_menu(|ui| {
        // Only while the menu is open: on Unix this stats the file, which must not happen
        // for every visible cell on every frame.
        let is_program = nav.session.is_program(&entry.path);
        if ui.button("Open").clicked() {
            nav.run("file.open", json!({ "path": path }));
        }
        if is_program && ui.button("Run…").clicked() {
            nav.run("file.run", json!({ "path": path }));
        }
        // Same entries as File → Open In ▸, for the selection (which includes this item).
        let entries = crate::menu::open_in_entries(nav);
        let mut chosen = None;
        ui.menu_button("Open In", |ui| crate::menu::draw(ui, &entries, &mut chosen, &mut Vec::new()));
        if let Some(action) = chosen {
            crate::menu::apply(nav, ui.ctx(), action);
        }
        if ui.button("Reveal in File Manager").clicked() {
            nav.run("file.reveal", json!({ "path": path }));
        }
        if is_dir && ui.button("Pin to My Folders").clicked() {
            nav.run("nav.favorite.add", json!({ "path": path }));
        }
    });
    response.clicked() || response.secondary_clicked()
}

// ---- details list ------------------------------------------------------------------------

const COL_TYPE: f32 = 90.0;
const COL_SIZE: f32 = 80.0;
const COL_DIMS: f32 = 100.0;
const COL_DATE: f32 = 140.0;

/// Name, Type, Size, Dimensions, Modified, Created.
const COLUMNS: usize = 6;

fn columns(width: f32) -> [(f32, f32); COLUMNS] {
    let fixed = COL_TYPE + COL_SIZE + COL_DIMS + COL_DATE * 2.0;
    let name = (width - fixed).max(120.0);
    let mut x = 0.0;
    [name, COL_TYPE, COL_SIZE, COL_DIMS, COL_DATE, COL_DATE].map(|w| {
        let col = (x, w);
        x += w;
        col
    })
}

fn list(nav: &mut Navigator, ui: &mut Ui) {
    nav.state.grid_columns = 1;
    let width = ui.available_width();
    let cols = columns(width - 8.0);
    let view = nav.session.view().clone();
    // Header: click to sort (again to reverse).
    let (header, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    let headers = [
        ("Name", Some(SortKey::Name)),
        ("Type", Some(SortKey::Type)),
        ("Size", Some(SortKey::Size)),
        ("Dimensions", None),
        ("Modified", Some(SortKey::Date)),
        ("Created", None),
    ];
    for ((x, w), (title, key)) in cols.iter().zip(headers) {
        let rect = Rect::from_min_size(header.min + vec2(*x, 0.0), vec2(*w, ROW_HEIGHT));
        let response = ui.interact(
            rect,
            ui.id().with(("list-header", title)),
            if key.is_some() { Sense::click() } else { Sense::hover() },
        );
        let arrow = match key {
            Some(k) if k == view.sort => {
                if view.ascending {
                    " ^"
                } else {
                    " v"
                }
            }
            _ => "",
        };
        let color = if response.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
        ui.painter().with_clip_rect(rect).text(
            rect.left_center() + vec2(6.0, 0.0),
            Align2::LEFT_CENTER,
            format!("{title}{arrow}"),
            TextStyle::Button.resolve(ui.style()),
            color,
        );
        if let Some(k) = key
            && response.clicked()
        {
            let ascending = if k == view.sort { !view.ascending } else { true };
            nav.run("view.sort", json!({ "by": sort_name(k), "ascending": ascending }));
        }
    }
    ui.painter().hline(header.x_range(), header.bottom(), ui.visuals().widgets.noninteractive.bg_stroke);

    let count = nav.session.items().len();
    let mut area = ScrollArea::vertical().id_salt(("list", nav.session.location().clone())).auto_shrink(false);
    if let Some(y) = reveal_offset(nav, ROW_HEIGHT, ui.available_height(), 1) {
        area = area.vertical_scroll_offset(y);
    }
    let output = area.show_viewport(ui, |ui, viewport| {
        ui.set_height(count as f32 * ROW_HEIGHT);
        ui.set_width(width - 4.0);
        let origin = ui.max_rect().min;
        let background = ui.interact(ui.max_rect(), ui.id().with("list-bg"), Sense::click());
        let first = (viewport.min.y / ROW_HEIGHT).floor().max(0.0) as usize;
        let last = ((viewport.max.y / ROW_HEIGHT).ceil() as usize).min(count);
        let visible: Vec<(usize, Entry)> = nav
            .session
            .items()
            .iter()
            .enumerate()
            .skip(first)
            .take(last.saturating_sub(first))
            .map(|(i, e)| (i, e.clone()))
            .collect();
        let mut hit = false;
        for (index, entry) in &visible {
            let rect =
                Rect::from_min_size(origin + vec2(0.0, *index as f32 * ROW_HEIGHT), vec2(width - 4.0, ROW_HEIGHT));
            hit |= list_row(nav, ui, entry, rect, &cols, index % 2 == 1);
        }
        if background.clicked() && !hit {
            nav.run("select.none", json!({}));
        }
    });
    nav.state.scroll = Some((nav.session.location().clone(), output.state.offset.y));
}

fn list_row(
    nav: &mut Navigator,
    ui: &mut Ui,
    entry: &Entry,
    rect: Rect,
    cols: &[(f32, f32); COLUMNS],
    odd: bool,
) -> bool {
    let response = ui.interact(rect, ui.id().with(("row", &entry.path)), Sense::click());
    let visuals = ui.visuals().clone();
    let selected = nav.session.selection().contains(&entry.path);
    let focused = nav.session.selection().focus() == Some(entry.path.as_path());
    if selected {
        ui.painter().rect_filled(rect, 2.0, visuals.selection.bg_fill.gamma_multiply(0.6));
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 2.0, visuals.widgets.hovered.weak_bg_fill);
    } else if odd {
        ui.painter().rect_filled(rect, 0.0, visuals.faint_bg_color);
    }
    if focused {
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, visuals.selection.stroke.color), StrokeKind::Inside);
    }
    let (kind, dims, warn) = row_facts(nav, entry);
    let size = if entry.kind == EntryKind::Dir { String::new() } else { format_size(entry.size) };
    let cells = [entry.name.clone(), kind, size, dims, format_time(entry.modified), format_time(entry.created)];
    let font = TextStyle::Body.resolve(ui.style());
    for (i, ((x, w), text)) in cols.iter().zip(cells).enumerate() {
        let cell = Rect::from_min_size(rect.min + vec2(*x, 0.0), vec2(*w - 4.0, ROW_HEIGHT));
        let color = if i == 1 && warn { visuals.error_fg_color } else { visuals.text_color() };
        let mut start = cell.left_center() + vec2(6.0, 0.0);
        if i == 0 {
            let icon = Rect::from_center_size(start + vec2(7.0, 0.0), vec2(14.0, 14.0));
            if entry.kind == EntryKind::Dir {
                paint_folder(ui, icon);
            }
            start.x += 20.0;
        }
        // Sizes and dimensions in monospace figures so digits line up down the column.
        let face = if matches!(i, 2 | 3) { crate::theme::numbers(11.5) } else { font.clone() };
        ui.painter().with_clip_rect(cell).text(start, Align2::LEFT_CENTER, text, face, color);
    }
    nav.state
        .drawn
        .push((entry.path.clone(), if entry.kind == EntryKind::Dir { Badge::Folder } else { Badge::TypeIcon }));
    let response = response.on_hover_text(entry.path.display().to_string());
    item_interaction(nav, ui, &response, entry)
}

/// Type, dimensions and an "unreadable" flag for a visible list row. Probes run on worker
/// threads; until one arrives the row shows the extension's type and "…".
fn row_facts(nav: &mut Navigator, entry: &Entry) -> (String, String, bool) {
    if entry.kind != EntryKind::File {
        return (type_text(entry), String::new(), false);
    }
    match nav.file_info(&entry.path) {
        FileInfo::Pending => (type_text(entry), crate::PENDING.into(), false),
        FileInfo::Ready(probe) => {
            let dims = probe.dimensions.map(|d| format!("{} × {}", d.width, d.height)).unwrap_or_default();
            if probe.is_unreadable() {
                ("unreadable".into(), dims, true)
            } else if probe.format == assetcraft_probe::Format::Unknown {
                (type_text(entry), dims, false)
            } else {
                (probe.format.label().into(), dims, probe.extension_mismatch())
            }
        }
        FileInfo::Failed(_) => (type_text(entry), String::new(), true),
    }
}
