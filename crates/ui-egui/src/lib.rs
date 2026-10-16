//! egui panels: folders, content grid, preview, metadata. Draws only; logic lives in the engine.
//!
//! Layer L6. See `AGENTS.md` → "Layers" for what this crate may depend on.
//!
//! [`Navigator`] owns an engine [`Session`] and a thumbnail pool. Each frame,
//! [`Navigator::ui`] reads state back from the session and draws it; every click and key press
//! becomes a `Session::execute(id, params)` call. The only state kept here is view plumbing:
//! the path-field text, the folder tree's expansion and child cache, textures, scroll offsets
//! and the docking arrangement (which is also saved through the engine's `view.panels`).

mod content;
mod dock;
mod folders;
mod inspector;
mod keys;
mod menu;
mod model_preview;
mod pathbar;
pub mod prefs_file;
mod storyboard_view;
mod strip_view;
mod text_view;
pub mod theme;
mod thumbnails;
mod widgets;

pub use dock::{Panel, default_layout};
pub use menu::{Menu, MenuAction, MenuEntry, NOT_IN_MENUS};
pub use prefs_file::PrefsStore;
pub use thumbnails::Badge;

use assetcraft_engine::{FileInfo, Location, Session, ThemeMode, ViewMode, WatchStatus};
use assetcraft_thumbs::{ThumbCache, ThumbPool, default_workers};
use craft_ui::docking::Layout;
use egui::{RichText, Ui};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// Seconds a status message stays visible.
const MESSAGE_SECONDS: f64 = 6.0;

/// Shown in place of file facts (format, dimensions) still being probed.
pub const PENDING: &str = "…";

/// The navigator window's contents.
pub struct Navigator {
    session: Session,
    thumbs: thumbnails::Thumbnails,
    state: UiState,
}

/// View plumbing that isn't application state.
#[derive(Default)]
struct UiState {
    path_text: String,
    /// Status-bar message, when it was first shown, and whether it is just information
    /// (drawn plainly) rather than an error (drawn red).
    message: Option<(String, Option<f64>)>,
    message_is_info: bool,
    /// The theme last handed to egui (re-applied when `view.theme` changes it).
    applied_theme: Option<ThemeMode>,
    tree: folders::Tree,
    grid_columns: usize,
    seen_focus: Option<PathBuf>,
    reveal_focus: bool,
    scroll: Option<(Location, f32)>,
    layout: Layout<Panel>,
    hidden: dock::Hidden,
    drawn: Vec<(PathBuf, Badge)>,
    fonts_installed: bool,
    show_shortcuts: bool,
    show_about: bool,
    exit_requested: bool,
    menu_rects: Vec<(String, egui::Rect)>,
    /// Files whose header facts were drawn this frame (queued probes for others are dropped).
    probe_wanted: std::collections::HashSet<PathBuf>,
    /// The Metadata panel's rows as last drawn (for tests and diagnostics).
    metadata_rows: Vec<(String, String)>,
    /// The 3D model shown in the Preview panel, with its orbit camera.
    model: Option<model_preview::ModelPreview>,
    /// Open In ▸ entries and the session generation they were built at.
    open_in_cache: std::cell::RefCell<Option<(u64, Vec<MenuEntry>)>>,
    /// Storyboard view plumbing: playback clock and the Re-cut toast.
    storyboard: strip_view::SbUi,
}

impl Navigator {
    /// A navigator over `session`, with a thumbnail pool sized for this machine that uses the
    /// session's disk cache, if it has one.
    pub fn new(session: Session) -> Self {
        let disk = session.thumb_cache().cloned();
        let pool = ThumbPool::with_disk(session.services(), default_workers(), ThumbCache::default(), disk);
        Self::with_pool(session, pool)
    }

    /// A navigator with a given pool (tests use one worker).
    pub fn with_pool(session: Session, pool: ThumbPool) -> Self {
        let layout = dock::restore(session.panels());
        let mut thumbs = thumbnails::Thumbnails::new(pool);
        thumbs.set_options(session.decode_options());
        Self { session, thumbs, state: UiState { layout, ..UiState::default() } }
    }

    /// A navigator with a single-worker pool and the default cache.
    pub fn with_one_worker(session: Session) -> Self {
        let pool = ThumbPool::new(session.services(), 1, ThumbCache::default());
        Self::with_pool(session, pool)
    }

    /// The engine session (read-only; change it with [`Navigator::run`]).
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Run an engine command. Failures are shown in the status bar, never fatal; a result
    /// carrying a `message` (e.g. "Revealed setup.exe …") is shown as information.
    pub fn run(&mut self, id: &str, params: Value) -> Option<Value> {
        match self.session.execute(id, &params) {
            Ok(value) => {
                if matches!(id, "nav.refresh" | "view.hidden") {
                    self.state.tree.clear_cache();
                }
                if matches!(id, "nav.refresh" | "cache.clear") {
                    self.thumbs.invalidate_all();
                }
                if id == "nav.changed" {
                    self.apply_folder_changes(&value);
                }
                if id == "formats.heic" {
                    self.thumbs.set_options(self.session.decode_options());
                }
                if id == "storyboard.recut" {
                    // Re-cut reports as a toast over the timeline (design 4a), not the status bar.
                    if let Some(text) = value.get("message").and_then(Value::as_str) {
                        self.state.storyboard.toast = Some((text.to_owned(), None));
                    }
                } else if let Some(text) = value.get("message").and_then(Value::as_str) {
                    self.inform(text.to_owned());
                }
                Some(value)
            }
            Err(error) => {
                self.notify(error.to_string());
                None
            }
        }
    }

    /// Show an error in the status bar for a few seconds.
    pub fn notify(&mut self, text: impl Into<String>) {
        self.state.message = Some((text.into(), None));
        self.state.message_is_info = false;
    }

    /// Show information (not an error) in the status bar for a few seconds.
    pub fn inform(&mut self, text: impl Into<String>) {
        self.state.message = Some((text.into(), None));
        self.state.message_is_info = true;
    }

    /// Give the session a folder watcher (live updates for the open folder).
    pub fn set_watcher(&mut self, watcher: Box<dyn assetcraft_watch::Watcher>) {
        self.session.set_watcher(watcher);
    }

    /// After `nav.changed`: forget thumbnails of changed or removed files, and folder-tree
    /// children that may have changed.
    fn apply_folder_changes(&mut self, result: &Value) {
        let paths = ["modified", "removed"]
            .into_iter()
            .filter_map(|k| result.get(k).and_then(Value::as_array))
            .flatten()
            .filter_map(Value::as_str);
        for path in paths {
            let path = std::path::Path::new(path);
            let modified = self.session.items().iter().find(|e| e.path == path).and_then(|e| e.modified);
            self.thumbs.invalidate_path(path, modified);
        }
        let added_or_removed = ["added", "removed"]
            .into_iter()
            .any(|k| result.get(k).and_then(Value::as_array).is_some_and(|a| !a.is_empty()));
        if added_or_removed {
            self.state.tree.clear_cache();
        }
    }

    /// Apply whatever the folder watcher has ready, and schedule a repaint for what's pending.
    fn pump_watcher(&mut self, ctx: &egui::Context) {
        if let Some(params) = self.session.pending_changes() {
            self.run("nav.changed", params);
        }
        if let Some(wait) = self.session.changes_due_in() {
            ctx.request_repaint_after(wait.max(std::time::Duration::from_millis(16)));
        }
    }

    /// Hand the theme to egui when it changes. `System` follows the OS (eframe reports it).
    fn apply_theme(&mut self, ctx: &egui::Context) {
        let want = self.session.view().theme;
        if self.state.applied_theme != Some(want) {
            ctx.set_theme(theme::preference(want));
            self.state.applied_theme = Some(want);
            ctx.request_repaint();
        }
    }

    /// The current status-bar message, if one is showing.
    pub fn message(&self) -> Option<&str> {
        self.state.message.as_ref().map(|(text, _)| text.as_str())
    }

    /// The current panel arrangement.
    pub fn layout(&self) -> &Layout<Panel> {
        &self.state.layout
    }

    /// Distinct thumbnails the last frame asked for (only visible cells ask).
    pub fn thumbs_wanted_last_frame(&self) -> usize {
        self.thumbs.last_wanted()
    }

    /// Thumbnail jobs handed to the pool since start.
    pub fn thumbs_requested_total(&self) -> usize {
        self.thumbs.total_requested()
    }

    /// Every item cell drawn in the last frame and how (thumbnail, badge, folder…).
    pub fn drawn(&self) -> &[(PathBuf, Badge)] {
        &self.state.drawn
    }

    /// The Metadata panel's `(label, value)` rows as last drawn.
    pub fn metadata_rows(&self) -> &[(String, String)] {
        &self.state.metadata_rows
    }

    /// Header facts for a file being drawn: never blocks (see `Session::file_info`).
    pub(crate) fn file_info(&mut self, path: &std::path::Path) -> FileInfo<'_> {
        self.state.probe_wanted.insert(path.to_path_buf());
        self.session.file_info(path)
    }

    /// The menu model as the bar would draw it now.
    pub fn menus(&self) -> Vec<menu::Menu> {
        menu::menus(self)
    }

    /// Run a menu entry as if clicked. Disabled items and submenus do nothing; returns whether
    /// anything ran.
    pub fn activate(&mut self, ctx: &egui::Context, entry: &MenuEntry) -> bool {
        match entry {
            MenuEntry::Item { enabled: true, action, .. } => {
                menu::apply(self, ctx, action.clone());
                true
            }
            _ => false,
        }
    }

    /// `true` once File → Exit was chosen (the window has been asked to close).
    pub fn exit_requested(&self) -> bool {
        self.state.exit_requested
    }

    /// Grid columns in the last frame (Up/Down move by this many).
    pub fn grid_columns(&self) -> usize {
        self.state.grid_columns.max(1)
    }

    /// Draw one frame. Call from `eframe::App::ui`, or from `Context::run_ui` in tests.
    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if !std::mem::replace(&mut self.state.fonts_installed, true) {
            theme::install(&ctx);
        }
        self.apply_theme(&ctx);
        self.pump_watcher(&ctx);
        self.thumbs.begin_frame();
        self.session.poll_file_info();
        self.state.probe_wanted.clear();
        self.state.drawn.clear();
        self.handle_keys(&ctx);
        self.track_focus();

        let storyboard = self.session.mode() == assetcraft_engine::Mode::Storyboard;
        egui::Panel::top("assetcraft-menubar").show(ui, |ui| menu::bar(self, ui));
        egui::Panel::top("assetcraft-modebar").show(ui, |ui| storyboard_view::mode_bar(self, ui));
        if storyboard {
            egui::Panel::bottom("assetcraft-status").show(ui, |ui| {
                ui.horizontal(|ui| {
                    storyboard_view::status(self, ui);
                    self.message_label(ui);
                })
            });
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(ui.visuals().panel_fill)).show(ui, |ui| {
                storyboard_view::show(self, ui);
            });
        } else {
            egui::Panel::top("assetcraft-pathbar").show(ui, |ui| pathbar::show(self, ui));
            egui::Panel::bottom("assetcraft-status").show(ui, |ui| self.status_bar(ui));
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(ui.visuals().panel_fill)).show(ui, |ui| {
                dock::show(self, ui);
            });
        }
        if !storyboard && self.session.preview_open() {
            inspector::overlay(self, &ctx);
        }
        menu::windows(self, &ctx);
        self.thumbs.end_frame(&ctx);
        let wanted = &self.state.probe_wanted;
        self.session.retain_file_info(|p| wanted.contains(p));
        if self.session.file_info_busy() > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        self.expire_message(&ctx);
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let focused = ctx.memory(|m| m.focused());
        if focused == Some(pathbar::edit_id()) {
            return; // typing a path
        }
        // Our shortcuts own the keyboard: a button keeping focus after a click must not also
        // react to Space / Enter.
        if let Some(id) = focused {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        if self.session.mode() == assetcraft_engine::Mode::Storyboard {
            let (undo, redo, space) = ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Space),
                )
            });
            if space && self.session.storyboard().is_some() {
                let at = strip_view::playhead(self, ctx);
                strip_view::toggle_play(self, at);
            }
            if undo && self.session.can_undo_storyboard() {
                self.run("storyboard.undo", json!({}));
            }
            if redo && self.session.can_redo_storyboard() {
                self.run("storyboard.redo", json!({}));
            }
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let cx = keys::KeyContext {
            step: if self.session.view().mode == ViewMode::Grid { self.grid_columns() } else { 1 },
            preview_open: self.session.preview_open(),
            show_hidden: self.session.view().show_hidden,
        };
        for (id, params) in keys::commands(&events, cx) {
            // Enter / Space with nothing focused are no-ops, not errors worth a message.
            let quiet =
                matches!(id, "file.open" | "file.run" | "view.preview") && self.session.selection().focus().is_none();
            if quiet {
                continue;
            }
            self.run(id, params);
        }
    }

    /// When the focus moves (keys, back/up landing on a folder…), scroll it into view.
    fn track_focus(&mut self) {
        let focus = self.session.selection().focus().map(|p| p.to_path_buf());
        if focus != self.state.seen_focus {
            self.state.reveal_focus = focus.is_some();
            self.state.seen_focus = focus;
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let items = self.session.items().len();
            ui.label(format!("{items} item{}", if items == 1 { "" } else { "s" }));
            let selected = self.session.selection().len();
            if selected > 0 {
                ui.separator();
                ui.label(format!("{selected} selected"));
            }
            if self.session.truncated() {
                ui.separator();
                ui.colored_label(
                    theme::warn_color(ui.visuals()),
                    format!("Truncated: showing the first {} entries", assetcraft_engine::MAX_ITEMS),
                );
            }
            let skipped = self.session.skipped();
            if skipped > 0 {
                ui.separator();
                ui.colored_label(theme::warn_color(ui.visuals()), format!("{skipped} entries could not be read"));
            }
            let busy = self.thumbs.busy();
            if busy > 0 {
                ui.separator();
                ui.weak(format!("{busy} thumbnails loading"));
            }
            if let WatchStatus::Unavailable(reason) = self.session.watch_status() {
                ui.separator();
                ui.weak("Live updates off").on_hover_text(format!("{reason}\nPress F5 to refresh."));
            }
            self.message_label(ui);
        });
    }

    /// The status-bar message (info or error), if one is showing.
    fn message_label(&self, ui: &mut Ui) {
        if let Some((text, _)) = &self.state.message {
            ui.separator();
            if self.state.message_is_info {
                ui.label(RichText::new(text).color(ui.visuals().strong_text_color()));
            } else {
                ui.label(RichText::new(text).color(theme::error_color(ui.visuals())));
            }
        }
    }

    fn expire_message(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if let Some((_, shown)) = &mut self.state.message {
            let since = *shown.get_or_insert(now);
            if now - since > MESSAGE_SECONDS {
                self.state.message = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(MESSAGE_SECONDS));
            }
        }
    }

    /// Test helper: wait for thumbnail results.
    #[cfg(test)]
    fn wait_thumbs(&mut self, timeout: std::time::Duration) -> usize {
        self.thumbs.wait(timeout)
    }
}

impl Navigator {
    /// Open a folder (e.g. from the command line). Errors go to the status bar.
    pub fn open_folder(&mut self, path: &std::path::Path) -> bool {
        self.run("nav.open", json!({ "path": path.to_string_lossy() })).is_some()
    }
}

#[cfg(test)]
mod screenshot;
#[cfg(test)]
mod stage1_tests;
#[cfg(test)]
mod storyboard_tests;
#[cfg(test)]
mod tests;
