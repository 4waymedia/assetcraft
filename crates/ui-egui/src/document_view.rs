//! Saving the storyboard from the window (SF1): File › Save Storyboard (Ctrl+S) and Save
//! Storyboard As…, the native Save picker, and the "Save changes?" prompt before the window
//! closes with unsaved work. The engine does the writing (`storyboard.save`, `storyboard.saveAs`);
//! this file only picks a path and asks.
//!
//! New Storyboard does not ask: replacing the open board is one undo step (`storyboard.new`
//! checkpoints), so nothing is lost until the window closes (D-SF5).

use crate::Navigator;
#[cfg(all(not(test), not(target_arch = "wasm32")))]
use assetcraft_engine::document_cmd::SUFFIX;
use serde_json::json;

/// What to do once the prompt is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pending {
    /// File › Exit, the title bar's ✕, or the OS close.
    Exit,
}

/// Save to the document's own file, or ask for one (Save As) when it has none.
pub(crate) fn save(nav: &mut Navigator) -> bool {
    if nav.session.storyboard().is_none() {
        return false;
    }
    if nav.session.document().is_some() { nav.run("storyboard.save", json!({})).is_some() } else { save_as(nav) }
}

/// The native Save picker, starting in the document's folder or `<working>/Storyboards`, then
/// `storyboard.save {path, overwrite: true}` (the picker has already asked about replacing).
pub(crate) fn save_as(nav: &mut Navigator) -> bool {
    if nav.session.storyboard().is_none() {
        return false;
    }
    let Some(path) = pick_save_path(nav) else { return false };
    nav.run("storyboard.save", json!({ "path": path.to_string_lossy(), "overwrite": true })).is_some()
}

#[cfg(test)]
fn pick_save_path(nav: &mut Navigator) -> Option<std::path::PathBuf> {
    // Tests never open a native dialog: they set the answer beforehand.
    nav.state.test_save_path.clone()
}

#[cfg(all(not(test), not(target_arch = "wasm32")))]
fn pick_save_path(nav: &mut Navigator) -> Option<std::path::PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Save storyboard")
        .add_filter("Storyboard", &["json"])
        .set_file_name(nav.session.suggested_document_name());
    let start = match nav.session.document().and_then(|d| d.parent().map(std::path::Path::to_path_buf)) {
        Some(own) => Some(own),
        None => nav.session.prepare_storyboards_folder(),
    };
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let mut path = dialog.save_file()?;
    // The picker may hand back `Name` or `Name.json`; the document is `Name.storyboard.json`.
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !name.to_ascii_lowercase().ends_with(SUFFIX) {
        let stem = name.strip_suffix(".json").unwrap_or(&name).trim_end_matches('.');
        let stem = if stem.is_empty() { "Storyboard" } else { stem };
        path.set_file_name(format!("{stem}{SUFFIX}"));
    }
    Some(path)
}

#[cfg(all(not(test), target_arch = "wasm32"))]
fn pick_save_path(nav: &mut Navigator) -> Option<std::path::PathBuf> {
    nav.notify("Saving to a file is not available on the web yet".to_owned());
    None
}

/// Close the window, asking first when the storyboard has unsaved changes.
pub(crate) fn request_exit(nav: &mut Navigator, ctx: &egui::Context) {
    if nav.session.storyboard_dirty() && !nav.state.exit_requested {
        nav.state.unsaved = Some(Pending::Exit);
        return;
    }
    nav.state.exit_requested = true;
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
}

/// The OS or the title bar asked the window to close: hold it while there is unsaved work.
pub(crate) fn intercept_close(nav: &mut Navigator, ctx: &egui::Context) {
    if !ctx.input(|i| i.viewport().close_requested()) || nav.state.exit_requested {
        return;
    }
    if nav.session.storyboard_dirty() {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        nav.state.unsaved = Some(Pending::Exit);
    }
}

/// The "Save changes?" prompt, while one is pending.
pub(crate) fn window(nav: &mut Navigator, ctx: &egui::Context) {
    let Some(pending) = nav.state.unsaved else { return };
    if !nav.session.storyboard_dirty() {
        nav.state.unsaved = None;
        proceed(nav, ctx, pending);
        return;
    }
    let name = nav.session.storyboard().map(|b| b.name.clone()).unwrap_or_default();
    let mut answer: Option<Answer> = None;
    egui::Window::new("Save changes?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.label(format!("Save changes to \u{201c}{name}\u{201d} before closing?"));
            ui.weak("Your changes will be lost if you don't save them.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(Answer::Save);
                }
                if ui.button("Don't Save").clicked() {
                    answer = Some(Answer::Discard);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(Answer::Cancel);
                }
            });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        answer = Some(Answer::Cancel);
    }
    match answer {
        None => {}
        Some(Answer::Cancel) => nav.state.unsaved = None,
        Some(Answer::Discard) => {
            nav.state.unsaved = None;
            proceed(nav, ctx, pending);
        }
        // A cancelled picker or a failed write keeps the prompt (the message says why).
        Some(Answer::Save) if save(nav) && !nav.session.storyboard_dirty() => {
            nav.state.unsaved = None;
            proceed(nav, ctx, pending);
        }
        Some(Answer::Save) => {}
    }
}

#[derive(Debug, Clone, Copy)]
enum Answer {
    Save,
    Discard,
    Cancel,
}

fn proceed(nav: &mut Navigator, ctx: &egui::Context, pending: Pending) {
    match pending {
        Pending::Exit => {
            nav.state.exit_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
