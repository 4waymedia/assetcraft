//! File › Export Video… (SF5-5): a small settings window (size, frame rate, quality, file
//! type, the estimated size from a dry run), the native Save picker, then
//! `storyboard.exportVideo`; while it runs, a progress bar along the Viewer's top with Esc to
//! cancel, and the engine's message when it ends. The engine does all the work
//! (`export_job.rs`); this file only asks and shows.

use crate::Navigator;
use assetcraft_engine::{ExportProgress, ExportStage};
use egui::{Color32, FontId, Rect, Ui, vec2};
use serde_json::{Value, json};

/// The sizes offered, in the engine's names.
const SIZES: [(&str, &str); 3] = [("720p", "1280 × 720"), ("1080p", "1920 × 1080"), ("4k", "3840 × 2160")];
/// The frame rates offered.
const RATES: [u32; 3] = [24, 25, 30];
/// Quality (quantizer) range offered; lower is better and bigger.
const QUALITY: (u8, u8) = (16, 36);
/// The engine's default (`assetcraft_video::DEFAULT_QP`).
const DEFAULT_QUALITY: u8 = 24;

/// The Export Video window's settings, while it is open.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExportDialog {
    size: usize,
    rate: usize,
    pub(crate) quality: u8,
    mov: bool,
    /// The last dry run for these settings (`None` until asked).
    pub(crate) plan: Option<Value>,
    planned_for: Option<(usize, usize, u8, bool)>,
    /// Tests export a tiny frame so the job finishes in a moment.
    #[cfg(test)]
    pub(crate) tiny: bool,
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self {
            size: 1,
            rate: 0,
            quality: DEFAULT_QUALITY,
            mov: false,
            plan: None,
            planned_for: None,
            #[cfg(test)]
            tiny: false,
        }
    }
}

impl ExportDialog {
    fn params(&self) -> Value {
        let mut p = json!({ "fps": RATES[self.rate], "quality": self.quality });
        #[cfg(test)]
        if self.tiny {
            p["size"] = json!({ "width": 160, "height": 96 });
            return p;
        }
        p["size"] = json!(SIZES[self.size].0);
        p
    }

    fn extension(&self) -> &'static str {
        if self.mov { "mov" } else { "mp4" }
    }
}

/// File › Export Video…: open the window (nothing happens until Export… is clicked).
pub(crate) fn open(nav: &mut Navigator) {
    if nav.session.enabled("storyboard.exportVideo").is_err() {
        return;
    }
    if nav.state.export.is_none() {
        nav.state.export = Some(ExportDialog::default());
    }
}

/// The export window, while it is open.
pub(crate) fn window(nav: &mut Navigator, ctx: &egui::Context) {
    let Some(mut dialog) = nav.state.export.clone() else { return };
    if nav.session.storyboard().is_none() {
        nav.state.export = None;
        return;
    }
    // A dry run for the settings as they stand (cheap: no frame is drawn).
    let key = (dialog.size, dialog.rate, dialog.quality, dialog.mov);
    if dialog.planned_for != Some(key) {
        let mut p = dialog.params();
        p["path"] = json!(plan_path(nav, dialog.extension()));
        p["dryRun"] = json!(true);
        dialog.plan = nav.session.execute("storyboard.exportVideo", &p).ok();
        dialog.planned_for = Some(key);
    }
    let mut action: Option<bool> = None;
    let mut open = true;
    egui::Window::new("Export Video")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            egui::Grid::new("export-settings").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Size");
                ui.horizontal(|ui| {
                    for (i, (name, dims)) in SIZES.iter().enumerate() {
                        ui.selectable_value(&mut dialog.size, i, *name).on_hover_text(*dims);
                    }
                });
                ui.end_row();
                ui.label("Frame rate");
                ui.horizontal(|ui| {
                    for (i, fps) in RATES.iter().enumerate() {
                        ui.selectable_value(&mut dialog.rate, i, format!("{fps} fps"));
                    }
                });
                ui.end_row();
                ui.label("Quality");
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut dialog.quality, QUALITY.0..=QUALITY.1).show_value(false));
                    ui.weak(quality_word(dialog.quality));
                });
                ui.end_row();
                ui.label("File");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut dialog.mov, false, "MP4").on_hover_text("H.264 + AAC; plays everywhere");
                    ui.selectable_value(&mut dialog.mov, true, "MOV")
                        .on_hover_text("The same picture and sound in a QuickTime file");
                });
                ui.end_row();
            });
            ui.add_space(6.0);
            match &dialog.plan {
                Some(plan) => {
                    let frames = plan["frames"].as_u64().unwrap_or(0);
                    let secs = plan["seconds"].as_f64().unwrap_or(0.0);
                    let bytes = plan["estimatedBytes"].as_u64().unwrap_or(0);
                    let audio = if plan["audio"] == true { "with the song" } else { "no song" };
                    ui.weak(format!("{frames} frames · {secs:.1} s · about {} · {audio}", mib(bytes)));
                }
                None => {
                    ui.weak("Nothing to export");
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(dialog.plan.is_some(), egui::Button::new("Export…")).clicked() {
                    action = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    action = Some(false);
                }
            });
        });
    if !open || action == Some(false) {
        nav.state.export = None;
        return;
    }
    nav.state.export = Some(dialog.clone());
    if action == Some(true) {
        start(nav, &dialog);
    }
}

/// The picker, then the command. Closes the window when the export starts.
pub(crate) fn start(nav: &mut Navigator, dialog: &ExportDialog) {
    let Some(path) = pick_path(nav, dialog.extension()) else { return };
    let mut p = dialog.params();
    p["path"] = json!(path.to_string_lossy());
    p["overwrite"] = json!(true);
    if nav.run("storyboard.exportVideo", p).is_some() {
        nav.state.export = None;
        nav.state.export_seen = Some(ExportStage::Rendering);
    }
}

/// A path for the dry run only (never written): the suggested file beside the document.
fn plan_path(nav: &Navigator, ext: &str) -> String {
    let dir = nav
        .session
        .document()
        .and_then(|d| d.parent().map(std::path::Path::to_path_buf))
        .or_else(|| nav.session.storyboards_folder())
        .unwrap_or_else(std::env::temp_dir);
    dir.join(suggested_name(nav, ext)).to_string_lossy().into_owned()
}

fn suggested_name(nav: &Navigator, ext: &str) -> String {
    let stem = nav.session.suggested_document_name();
    let stem = stem.strip_suffix(assetcraft_engine::document_cmd::SUFFIX).unwrap_or(&stem);
    format!("{stem}.{ext}")
}

#[cfg(test)]
fn pick_path(nav: &mut Navigator, _ext: &str) -> Option<std::path::PathBuf> {
    nav.state.test_save_path.clone()
}

#[cfg(all(not(test), not(target_arch = "wasm32")))]
fn pick_path(nav: &mut Navigator, ext: &str) -> Option<std::path::PathBuf> {
    let name = suggested_name(nav, ext);
    let mut dialog = rfd::FileDialog::new()
        .set_title("Export video")
        .add_filter(if ext == "mov" { "QuickTime movie" } else { "MP4 video" }, &[ext])
        .set_file_name(&name);
    let start = nav
        .session
        .document()
        .and_then(|d| d.parent().map(std::path::Path::to_path_buf))
        .or_else(|| nav.session.prepare_storyboards_folder());
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let mut path = dialog.save_file()?;
    if path.extension().and_then(|e| e.to_str()).is_none_or(|e| !e.eq_ignore_ascii_case(ext)) {
        path.set_extension(ext);
    }
    Some(path)
}

#[cfg(all(not(test), target_arch = "wasm32"))]
fn pick_path(nav: &mut Navigator, _ext: &str) -> Option<std::path::PathBuf> {
    nav.notify("Exporting video is not available on the web yet".to_owned());
    None
}

/// Each frame: notice when a running export ends and say so once.
pub(crate) fn tick(nav: &mut Navigator, ctx: &egui::Context) {
    let progress = nav.session.export_progress();
    if progress.running() {
        nav.state.export_seen = Some(progress.stage);
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        return;
    }
    if let Some(seen) = nav.state.export_seen.take()
        && seen != progress.stage
    {
        // `exportStatus` joins the finished thread and carries the message.
        if let Some(status) = nav.run("storyboard.exportStatus", json!({}))
            && status["stage"] == "failed"
        {
            nav.notify(status["message"].as_str().unwrap_or("Export failed").to_owned());
        }
    }
}

/// Esc while an export runs: stop it. `true` when it was running.
pub(crate) fn cancel_if_running(nav: &mut Navigator) -> bool {
    if !nav.session.export_progress().running() {
        return false;
    }
    nav.run("storyboard.exportCancel", json!({}));
    true
}

/// The progress bar along the Viewer's top while an export runs.
pub(crate) fn progress(nav: &Navigator, ui: &Ui, frame: Rect) {
    let p: ExportProgress = nav.session.export_progress();
    if !p.running() {
        return;
    }
    let painter = ui.painter();
    let bar = Rect::from_min_size(frame.left_top(), vec2(frame.width(), 3.0));
    painter.rect_filled(bar, 0, Color32::from_gray(0x3a));
    painter.rect_filled(
        Rect::from_min_size(bar.min, vec2(bar.width() * p.fraction(), 3.0)),
        0,
        crate::storyboard_view::ACCENT,
    );
    let text = match p.stage {
        ExportStage::Rendering => format!("Exporting {} / {} frames · Esc stops", p.frames_done, p.frames_total),
        ExportStage::Audio => "Exporting · the song".to_owned(),
        _ => "Exporting · writing the file".to_owned(),
    };
    let galley = painter.layout_no_wrap(text, FontId::monospace(10.0), Color32::from_gray(0xe0));
    let band = Rect::from_min_size(frame.left_top() + vec2(8.0, 8.0), galley.size() + vec2(12.0, 4.0));
    painter.rect_filled(band, 2, Color32::from_black_alpha(150));
    painter.galley(band.min + vec2(6.0, 2.0), galley, Color32::from_gray(0xe0));
}

fn quality_word(qp: u8) -> &'static str {
    match qp {
        0..=19 => "Best (largest file)",
        20..=23 => "High",
        24..=27 => "Good (default)",
        28..=31 => "Smaller",
        _ => "Smallest",
    }
}

fn mib(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.1} GiB", bytes as f64 / f64::from(1u32 << 30))
    } else {
        format!("{:.0} MiB", bytes as f64 / f64::from(1u32 << 20))
    }
}
