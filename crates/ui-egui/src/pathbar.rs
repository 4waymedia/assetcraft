//! Path bar: back / forward / up / refresh, a clickable breadcrumb, and an editable path field.

use crate::Navigator;
use crate::widgets::{Icon, icon_button};
use assetcraft_engine::Location;
use egui::{Id, Key, TextEdit, Ui};
use serde_json::json;

/// The path field's widget id (keyboard shortcuts are off while it has focus).
pub(crate) fn edit_id() -> Id {
    Id::new("assetcraft-path-edit")
}

pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if icon_button(ui, Icon::Back, nav.session.can_back(), "Back (Alt+Left)").clicked() {
            nav.run("nav.back", json!({}));
        }
        if icon_button(ui, Icon::Forward, nav.session.can_forward(), "Forward (Alt+Right)").clicked() {
            nav.run("nav.forward", json!({}));
        }
        if icon_button(ui, Icon::Up, nav.session.can_up(), "Up (Alt+Up, Backspace)").clicked() {
            nav.run("nav.up", json!({}));
        }
        if icon_button(ui, Icon::Refresh, true, "Refresh (F5)").clicked() {
            nav.run("nav.refresh", json!({}));
        }
        ui.add_space(8.0);
        breadcrumb(nav, ui);
    });
    ui.add_space(2.0);
    path_field(nav, ui);
    ui.add_space(2.0);
}

fn breadcrumb(nav: &mut Navigator, ui: &mut Ui) {
    ui.spacing_mut().item_spacing.x = 0.0;
    if ui.selectable_label(matches!(nav.session.location(), Location::Computer), "Computer").clicked() {
        nav.run("nav.computer", json!({}));
    }
    let crumbs = nav.session.breadcrumb();
    let last = crumbs.len().saturating_sub(1);
    for (i, crumb) in crumbs.iter().enumerate() {
        ui.weak(" › ");
        let label = assetcraft_fs::display_name(crumb);
        let response = ui.selectable_label(i == last, label).on_hover_text(crumb.display().to_string());
        if response.clicked() && i != last {
            nav.run("nav.open", json!({ "path": crumb.to_string_lossy() }));
        }
    }
}

/// The editable path. Shows the current location until focused; Enter opens, Esc reverts.
fn path_field(nav: &mut Navigator, ui: &mut Ui) {
    let focused = ui.memory(|m| m.has_focus(edit_id()));
    if !focused {
        nav.state.path_text = match nav.session.location() {
            Location::Computer => "Computer".into(),
            Location::Folder(path) => path.display().to_string(),
        };
    }
    let response = ui.add(
        TextEdit::singleline(&mut nav.state.path_text)
            .id(edit_id())
            .desired_width(f32::INFINITY)
            .hint_text("Type a folder path and press Enter"),
    );
    if response.lost_focus() {
        let (enter, escape) = ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        if enter && !escape {
            let text = nav.state.path_text.trim().to_string();
            if text.is_empty() || text.eq_ignore_ascii_case("computer") {
                nav.run("nav.computer", json!({}));
            } else {
                nav.run("nav.open", json!({ "path": text }));
            }
        }
    }
}
