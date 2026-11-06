//! File › New from Template ▸ and File › Save as Template… (ST3): the menu entries come from
//! `storyboard.templates` (the bundled three, then the user's own); Save as Template… asks for a
//! name in a small window and runs `storyboard.saveTemplate`, offering Replace when a template
//! of that name exists. The engine does the rest (`template_cmd.rs`).

use crate::Navigator;
use crate::menu::{MenuAction, MenuEntry};
use assetcraft_engine::Session;
use assetcraft_storyboard::BUNDLED;
use serde_json::json;

/// The entries under File › New from Template ▸.
pub(crate) fn entries(s: &Session) -> Vec<MenuEntry> {
    let item = |label: String, which: String| MenuEntry::Item {
        label,
        shortcut: None,
        enabled: true,
        checked: None,
        tooltip: None,
        action: MenuAction::Command("storyboard.new", json!({ "template": which })),
    };
    let mut out: Vec<MenuEntry> = BUNDLED
        .iter()
        .map(|t| match item(t.name.to_owned(), t.id.to_owned()) {
            MenuEntry::Item { label, shortcut, enabled, checked, action, .. } => {
                MenuEntry::Item { label, shortcut, enabled, checked, tooltip: Some(t.description.to_owned()), action }
            }
            other => other,
        })
        .collect();
    let user = s.user_templates();
    if !user.is_empty() {
        out.push(MenuEntry::Separator);
        for (name, path) in user {
            out.push(item(name, path.to_string_lossy().into_owned()));
        }
    }
    out
}

/// The Save as Template window's state while it is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SaveTemplateDialog {
    pub(crate) name: String,
    /// The engine refused because the name exists: offer Replace.
    exists: bool,
    error: Option<String>,
}

/// File › Save as Template…: open the window with the board's name filled in.
pub(crate) fn open(nav: &mut Navigator) {
    let Some(board) = nav.session.storyboard() else { return };
    nav.state.save_template = Some(SaveTemplateDialog { name: board.name.clone(), exists: false, error: None });
}

/// Run `storyboard.saveTemplate` for the window's name; `true` when it saved.
pub(crate) fn save(nav: &mut Navigator, overwrite: bool) -> bool {
    let Some(dialog) = nav.state.save_template.clone() else { return false };
    let params = json!({ "name": dialog.name.trim(), "overwrite": overwrite });
    match nav.session.execute("storyboard.saveTemplate", &params) {
        Ok(out) => {
            nav.inform(out["message"].as_str().unwrap_or("Saved template").to_owned());
            nav.state.save_template = None;
            true
        }
        Err(e) => {
            let text = e.to_string();
            if let Some(d) = nav.state.save_template.as_mut() {
                d.exists = text.contains("exists");
                d.error = Some(text);
            }
            false
        }
    }
}

/// The window, while it is open.
pub(crate) fn window(nav: &mut Navigator, ctx: &egui::Context) {
    let Some(mut dialog) = nav.state.save_template.clone() else { return };
    if nav.session.storyboard().is_none() {
        nav.state.save_template = None;
        return;
    }
    let mut action: Option<(bool, bool)> = None; // (save, overwrite)
    let mut open = true;
    egui::Window::new("Save as Template")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.label("A template keeps the scenes, shots, cards and pacing — not the pictures or the song.");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Name");
                let edit = ui.add(egui::TextEdit::singleline(&mut dialog.name).desired_width(f32::INFINITY));
                if edit.changed() {
                    dialog.exists = false;
                    dialog.error = None;
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    action = Some((true, false));
                }
            });
            if let Some(folder) = nav.session.templates_folder() {
                ui.weak(format!("Saved in {}", folder.display()));
            }
            if let Some(err) = &dialog.error {
                ui.colored_label(egui::Color32::from_rgb(0xe5, 0x6b, 0x6b), err);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can = !dialog.name.trim().is_empty();
                if dialog.exists {
                    if ui.add_enabled(can, egui::Button::new("Replace")).clicked() {
                        action = Some((true, true));
                    }
                } else if ui.add_enabled(can, egui::Button::new("Save")).clicked() {
                    action = Some((true, false));
                }
                if ui.button("Cancel").clicked() {
                    action = Some((false, false));
                }
            });
        });
    if !open || action == Some((false, false)) {
        nav.state.save_template = None;
        return;
    }
    nav.state.save_template = Some(dialog);
    if let Some((true, overwrite)) = action {
        save(nav, overwrite);
    }
}
