//! AssetCraft desktop app: a Bridge-style, read-only navigator window.
//!
//! `assetcraft [FOLDER]` opens FOLDER (else the last folder, else Pictures, else home).
//! Prefs live in the per-user config folder (see `assetcraft_ui_egui::prefs_file::config_path`)
//! and are saved when favorites, view settings or the panel layout change, and on exit.
//! Thumbnails are cached on disk in the per-user cache folder (see
//! `assetcraft_ui_egui::prefs_file::thumb_cache_dir`), up to 1 GiB.
//! The open folder is watched (`assetcraft-watch`); where watching fails, F5 still refreshes.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use assetcraft_engine::{Session, SystemLauncher};
use assetcraft_fs::RealFs;
use assetcraft_thumbs::{DEFAULT_DISK_BUDGET, DiskCache};
use assetcraft_ui_egui::{Navigator, PrefsStore, prefs_file};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

struct App {
    navigator: Navigator,
    store: PrefsStore,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.navigator.ui(ui);
        if !self.store.maybe_save(&self.navigator.session().prefs())
            && let Some(error) = self.store.last_error().map(str::to_owned)
            && self.navigator.message().is_none()
        {
            self.navigator.notify(error);
        }
    }

    fn on_exit(&mut self) {
        if !self.store.save_now(&self.navigator.session().prefs())
            && let Some(error) = self.store.last_error()
        {
            eprintln!("assetcraft: {error}");
        }
    }
}

fn main() -> ExitCode {
    let path = prefs_file::config_path();
    let prefs = path.as_deref().map(prefs_file::load).unwrap_or_default();
    let store = PrefsStore::new(path, prefs.clone());
    let mut session = Session::new(Arc::new(RealFs::new()), Arc::new(SystemLauncher), prefs);
    if let Some(dir) = prefs_file::thumb_cache_dir() {
        session = session.with_thumb_cache(DiskCache::open(dir, DEFAULT_DISK_BUDGET));
    }
    // Watch your Cut plays the track on the default output device (opened on the first ▶).
    session.set_player(Arc::new(assetcraft_playback::DevicePlayer::new()));
    let mut navigator = Navigator::new(session);
    if let Some(folder) = std::env::args_os().nth(1).map(PathBuf::from) {
        navigator.open_folder(&folder);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AssetCraft")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([720.0, 420.0]),
        ..eframe::NativeOptions::default()
    };
    let mut app = App { navigator, store };
    let create = move |cc: &eframe::CreationContext<'_>| -> Result<Box<dyn eframe::App>, _> {
        // Folder events arrive on the watcher's thread; wake the UI so they show without input.
        let ctx = cc.egui_ctx.clone();
        let waker: assetcraft_watch::Waker = Arc::new(move || ctx.request_repaint());
        app.navigator.set_watcher(Box::new(assetcraft_watch::NotifyWatcher::new(Some(waker))));
        Ok(Box::new(app))
    };
    match eframe::run_native("AssetCraft", options, Box::new(create)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("assetcraft: could not open a window: {error}");
            ExitCode::FAILURE
        }
    }
}
