//! Preview panel, metadata panel, and the full-window preview (Space).

use crate::content::draw_thumb;
use crate::thumbnails::{Badge, PRIORITY_OVERLAY, PRIORITY_PREVIEW, preview_side};
use crate::widgets::{format_size, format_time};
use crate::{Navigator, PENDING};
use assetcraft_engine::FileInfo;
use assetcraft_fs::{Entry, EntryKind};
use egui::{Align2, Color32, FontId, Id, Order, Rect, RichText, Sense, Ui, vec2};
use serde_json::json;

/// The focused item, else the first selected one.
fn current(nav: &Navigator) -> Option<Entry> {
    let path = nav
        .session
        .selection()
        .focus()
        .map(|p| p.to_path_buf())
        .or_else(|| nav.session.selected_paths().into_iter().next())?;
    nav.session.items().iter().find(|e| e.path == path).cloned()
}

pub(crate) fn preview(nav: &mut Navigator, ui: &mut Ui) {
    let Some(entry) = current(nav) else {
        nav.state.model = None;
        ui.centered_and_justified(|ui| ui.weak("Select an item to preview it"));
        return;
    };
    let rect = ui.available_rect_before_wrap();
    if crate::model_preview::wants_orbit(&entry) {
        if nav.session.preview_open() {
            // The full-window overlay drives the same model view at its own size; two drawers
            // would re-request frames at two sizes every frame.
            ui.centered_and_justified(|ui| ui.weak("Shown in the full-window preview"));
            return;
        }
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        crate::model_preview::draw(nav, ui, &entry, rect.shrink(4.0), &response);
        return;
    }
    nav.state.model = None;
    let side = preview_side(rect.width().max(rect.height()) * ui.ctx().pixels_per_point());
    let response = ui.allocate_rect(rect, Sense::click());
    let drawn = draw_thumb(nav, ui, &entry, rect.shrink(4.0), side, PRIORITY_PREVIEW);
    if response.double_clicked() && drawn == Badge::Image {
        nav.run("view.preview", json!({ "open": true }));
    }
}

pub(crate) fn metadata(nav: &mut Navigator, ui: &mut Ui) {
    nav.state.metadata_rows.clear();
    let selected = nav.session.selection().len();
    let Some(entry) = current(nav) else {
        ui.weak("Nothing selected");
        return;
    };
    egui::ScrollArea::vertical().id_salt("metadata-panel").auto_shrink(false).show(ui, |ui| {
        let mut rows: Vec<(&str, String)> = vec![("Name", entry.name.clone()), ("Path", entry.path.display().to_string())];
        let mut notes = Vec::new();
        if entry.kind == EntryKind::File {
            match nav.file_info(&entry.path) {
                FileInfo::Pending => {
                    for key in ["Format", "Dimensions"] {
                        rows.push((key, PENDING.into()));
                    }
                    rows.push(("Size", format_size(entry.size)));
                    rows.push(("Created", format_time(entry.created)));
                    rows.push(("Modified", format_time(entry.modified)));
                }
                FileInfo::Ready(probe) => {
                    let mut format = probe.format.label().to_string();
                    if let Some(claimed) = probe.claimed.filter(|c| *c != probe.format) {
                        format = format!("{format} (extension says {})", claimed.label());
                    }
                    rows.push(("Format", format));
                    rows.push((
                        "Dimensions",
                        probe.dimensions.map(|d| format!("{} × {} px", d.width, d.height)).unwrap_or_else(|| "—".into()),
                    ));
                    rows.push(("Size", format_size(probe.size)));
                    rows.push(("Created", format_time(probe.created)));
                    rows.push(("Modified", format_time(probe.modified)));
                    rows.push(("Status", format!("{:?}", probe.status)));
                    if probe.is_unreadable() {
                        notes.push("Unreadable: the contents are not the format its extension claims, or the header is damaged.");
                    }
                    if probe.extension_mismatch() {
                        notes.push("Extension mismatch: the contents are a different format than the extension says.");
                    }
                }
                FileInfo::Failed(error) => {
                    let error = error.to_string();
                    rows.push(("Size", format_size(entry.size)));
                    rows.push(("Created", format_time(entry.created)));
                    rows.push(("Modified", format_time(entry.modified)));
                    rows.push(("Status", error));
                }
            }
        } else {
            rows.push(("Kind", if entry.kind == EntryKind::Dir { "Folder".into() } else { format!("{:?}", entry.kind) }));
            rows.push(("Created", format_time(entry.created)));
            rows.push(("Modified", format_time(entry.modified)));
        }
        if selected > 1 {
            rows.push(("Selected", format!("{selected} items")));
        }
        nav.state.metadata_rows = rows.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect();
        egui::Grid::new("metadata-grid").num_columns(2).spacing(vec2(10.0, 4.0)).striped(true).show(ui, |ui| {
            for (key, value) in rows {
                ui.label(RichText::new(key).weak());
                let text = if matches!(key, "Dimensions" | "Size" | "Created" | "Modified") {
                    RichText::new(value).font(crate::theme::numbers(11.5))
                } else {
                    RichText::new(value)
                };
                ui.add(egui::Label::new(text).wrap());
                ui.end_row();
            }
        });
        for note in notes {
            ui.add_space(6.0);
            ui.colored_label(ui.visuals().error_fg_color, note);
        }
    });
}

/// Full-window preview of the focused item. Click, Space or Esc closes it.
pub(crate) fn overlay(nav: &mut Navigator, ctx: &egui::Context) {
    let screen = ctx.content_rect();
    let entry = current(nav);
    egui::Area::new(Id::new("assetcraft-preview-overlay")).order(Order::Foreground).fixed_pos(screen.min).show(
        ctx,
        |ui| {
            let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(235));
            let caption = Rect::from_min_max(rect.left_bottom() - vec2(0.0, 36.0), rect.max);
            let image = Rect::from_min_max(rect.min + vec2(24.0, 24.0), caption.right_top() - vec2(24.0, 4.0));
            let mut orbit = false;
            if let Some(entry) = &entry {
                if crate::model_preview::wants_orbit(entry) {
                    // A model orbits here too (drag), so a click on it must not close the overlay.
                    orbit = true;
                    let drag = ui.allocate_rect(image, Sense::click_and_drag());
                    crate::model_preview::draw(nav, ui, entry, image, &drag);
                } else {
                    draw_thumb(nav, ui, entry, image, 1024, PRIORITY_OVERLAY);
                }
                let hint = if orbit { "drag: orbit    ·    scroll: zoom" } else { "arrows: next / previous" };
                ui.painter().text(
                    caption.center(),
                    Align2::CENTER_CENTER,
                    format!("{}    ·    {hint}    ·    Space or Esc: close", entry.name),
                    FontId::proportional(14.0),
                    Color32::from_gray(220),
                );
            }
            let on_model = orbit && response.interact_pointer_pos().is_some_and(|p| image.contains(p));
            if response.clicked() && !on_model {
                nav.run("view.preview", json!({ "open": false }));
            }
        },
    );
}
