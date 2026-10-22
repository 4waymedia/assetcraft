//! Tools → Batch Rename… (T1): a template line with token buttons, a start number, and a live
//! preview from `file.renamePreview`. Nothing is renamed yet; the Rename button arrives with T2
//! (`file.rename`, undo, sidecars).
//!
//! The preview runs only when the template, the start number or the session (selection, folder)
//! changed, never every frame.

use crate::Navigator;
use assetcraft_engine::rename::TOKENS;
use serde_json::{Value, json};

/// Built-in presets: label, template.
pub(crate) const PRESETS: &[(&str, &str)] = &[
    ("Keep names", "{name}"),
    ("Folder + number", "{folder}_{seq:3}"),
    ("Name + number", "{name}_{seq:3}"),
    ("Number only", "{seq:4}"),
];

/// Rows shown in the preview list before "… and N more".
const MAX_ROWS: usize = 500;

/// The dialog's state while it is open.
#[derive(Debug, Clone)]
pub(crate) struct RenameDialog {
    pub(crate) template: String,
    pub(crate) start: u32,
    /// What the last preview was computed from: template, start, session generation after it ran.
    key: Option<(String, u32, u64)>,
    /// The last `file.renamePreview` result, or why there is none.
    pub(crate) preview: Result<Value, String>,
}

impl Default for RenameDialog {
    fn default() -> Self {
        let template = PRESETS.get(1).map_or("{name}", |(_, t)| *t).to_string();
        Self { template, start: 1, key: None, preview: Err(String::new()) }
    }
}

impl RenameDialog {
    /// Re-run the preview if anything it depends on changed.
    pub(crate) fn refresh(&mut self, nav: &mut Navigator) {
        let generation = nav.session.generation();
        let fresh =
            self.key.as_ref().is_some_and(|(t, s, g)| *t == self.template && *s == self.start && *g == generation);
        if fresh {
            return;
        }
        // Straight to the session: a preview error ("nothing selected") belongs in the dialog,
        // not in the status bar.
        let params = json!({ "template": self.template, "start": self.start });
        self.preview = nav.session.execute("file.renamePreview", &params).map_err(|e| e.to_string());
        self.key = Some((self.template.clone(), self.start, nav.session.generation()));
    }
}

/// Open the dialog (Tools → Batch Rename…).
pub(crate) fn open(nav: &mut Navigator) {
    if nav.state.rename.is_none() {
        nav.state.rename = Some(RenameDialog::default());
    }
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn count(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// Draw the dialog when it is open.
pub(crate) fn window(nav: &mut Navigator, ctx: &egui::Context) {
    let Some(mut dialog) = nav.state.rename.take() else { return };
    dialog.refresh(nav);
    let mut open = true;
    let mut close = false;
    let files = dialog.preview.as_ref().map(|v| count(v, "count")).unwrap_or(0);
    let title = "Batch Rename";
    egui::Window::new(title)
        .id(egui::Id::new("assetcraft-batch-rename"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(if files == 1 { "1 file selected".to_string() } else { format!("{files} files selected") });
            ui.add_space(4.0);
            egui::Grid::new("rename-form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Preset");
                let current = PRESETS.iter().find(|(_, t)| *t == dialog.template).map_or("Custom", |(l, _)| *l);
                egui::ComboBox::from_id_salt("rename-preset").selected_text(current).show_ui(ui, |ui| {
                    for (label, template) in PRESETS {
                        if ui.selectable_label(dialog.template == *template, *label).clicked() {
                            dialog.template = (*template).to_string();
                        }
                    }
                });
                ui.end_row();

                ui.label("Template");
                ui.add(egui::TextEdit::singleline(&mut dialog.template).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
                ui.end_row();

                ui.label("");
                ui.horizontal_wrapped(|ui| {
                    for token in TOKENS {
                        if ui.small_button(token.tag).on_hover_text(token.meaning).clicked() {
                            dialog.template.push_str(token.tag);
                        }
                    }
                });
                ui.end_row();

                ui.label("Start at");
                ui.add(egui::DragValue::new(&mut dialog.start).range(0..=1_000_000_000));
                ui.end_row();
            });
            ui.separator();
            match &dialog.preview {
                Err(e) if e.is_empty() => {}
                Err(e) => {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
                Ok(v) => preview(ui, v),
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    close = true;
                }
                ui.add_enabled(false, egui::Button::new("Rename"))
                    .on_disabled_hover_text("Renaming comes in the next step (T2): undo, and .xmp sidecars move with their files. This is a preview; nothing changes yet.");
            });
        });
    if open && !close {
        nav.state.rename = Some(dialog);
    }
}

fn preview(ui: &mut egui::Ui, v: &Value) {
    let unknown: Vec<String> = v
        .get("unknownTokens")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    if !unknown.is_empty() {
        let warn = ui.visuals().warn_fg_color;
        ui.colored_label(warn, format!("⚠ Unknown token {} stays as typed", unknown.join(", ")));
    }
    let plans = v.get("plans").and_then(Value::as_array).cloned().unwrap_or_default();
    egui::ScrollArea::vertical().max_height(320.0).auto_shrink([false, true]).show(ui, |ui| {
        egui::Grid::new("rename-preview").num_columns(3).striped(true).spacing([12.0, 2.0]).show(ui, |ui| {
            for plan in plans.iter().take(MAX_ROWS) {
                let changes = plan.get("changes").and_then(Value::as_bool).unwrap_or(false);
                let suffixed = plan.get("suffixed").and_then(Value::as_bool).unwrap_or(false);
                ui.label(egui::RichText::new(text(plan, "name")).monospace());
                ui.label(if changes { "→" } else { "=" });
                let new = egui::RichText::new(text(plan, "newName")).monospace();
                if suffixed {
                    ui.label(new.color(ui.visuals().warn_fg_color))
                        .on_hover_text("That name was taken, so a -N suffix was added");
                } else if changes {
                    ui.label(new.strong());
                } else {
                    ui.label(new.weak());
                }
                ui.end_row();
            }
        });
        if plans.len() > MAX_ROWS {
            ui.weak(format!("… and {} more", plans.len() - MAX_ROWS));
        }
    });
    let changed = count(v, "changed");
    let suffixed = count(v, "suffixed");
    let unchanged = count(v, "count").saturating_sub(changed);
    let skipped = v.get("skipped").and_then(Value::as_array).map_or(0, Vec::len);
    let mut summary = format!("{changed} renamed · {suffixed} got a -N suffix · {unchanged} unchanged");
    if skipped > 0 {
        summary.push_str(&format!(" · {skipped} folders skipped"));
    }
    ui.label(summary);
}
