//! What the app can't show on screen, written to a small log beside `prefs.json` (Paul,
//! 2026-11-14: the window stopped redrawing after a resize while the Cut played, and a release
//! build has no console to say why). Warnings from eframe and wgpu (a dropped frame, a surface
//! error), AssetCraft's own notes (a slow frame, a surface it rebuilt) and panics.
//!
//! `assetcraft.log` is this run; the run before is kept as `assetcraft.prev.log`, so a session
//! that had to be killed is still there after the restart. Each file stops growing at 1 MB.
//! Logging never fails the app: a log that can't be written is skipped.

use log::{Level, LevelFilter, Log, Metadata, Record};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

/// Where a file stops growing.
const MAX_BYTES: u64 = 1 << 20;

struct FileLog {
    file: Mutex<Option<std::fs::File>>,
    start: Instant,
}

impl Log for FileLog {
    fn enabled(&self, m: &Metadata<'_>) -> bool {
        // Everyone's warnings and errors; AssetCraft's own notes too.
        m.level() <= Level::Warn || (m.level() <= Level::Info && m.target().starts_with("assetcraft"))
    }

    fn log(&self, r: &Record<'_>) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line =
            format!("{:>10.3}s {:<5} {}: {}\n", self.start.elapsed().as_secs_f64(), r.level(), r.target(), r.args());
        eprint!("{line}");
        if let Ok(mut file) = self.file.lock()
            && let Some(f) = file.as_mut()
            && f.metadata().is_ok_and(|m| m.len() < MAX_BYTES)
        {
            let _ = f.write_all(line.as_bytes()).and_then(|()| f.flush());
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock()
            && let Some(f) = file.as_mut()
        {
            let _ = f.flush();
        }
    }
}

/// The log file for a folder: `<dir>/assetcraft.log`.
pub(crate) fn log_path(dir: &Path) -> PathBuf {
    dir.join("assetcraft.log")
}

/// Start logging into `dir` (keeping the last run's log as `assetcraft.prev.log`) and record
/// panics there too. Without a folder, or if the file can't be made, only stderr gets the lines.
/// Returns the log's path when there is one.
pub(crate) fn install(dir: Option<&Path>) -> Option<PathBuf> {
    let path = dir.map(log_path);
    let file = path.as_ref().and_then(|p| {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::rename(p, p.with_file_name("assetcraft.prev.log"));
        std::fs::File::create(p).ok()
    });
    let ok = file.is_some();
    let logger = Box::new(FileLog { file: Mutex::new(file), start: Instant::now() });
    if log::set_boxed_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!(target: "assetcraft", "panic: {info}");
        previous(info);
    }));
    log::info!(target: "assetcraft", "AssetCraft {} started", env!("CARGO_PKG_VERSION"));
    path.filter(|_| ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_sits_beside_prefs() {
        assert_eq!(log_path(Path::new("/x/AssetCraft")), PathBuf::from("/x/AssetCraft/assetcraft.log"));
    }

    #[test]
    fn warnings_from_anyone_and_notes_from_assetcraft() {
        let l = FileLog { file: Mutex::new(None), start: Instant::now() };
        let m = |level, target| Metadata::builder().level(level).target(target).build();
        assert!(l.enabled(&m(Level::Warn, "egui_wgpu::winit")));
        assert!(l.enabled(&m(Level::Info, "assetcraft")));
        assert!(!l.enabled(&m(Level::Info, "wgpu_core::device")), "other crates' chatter stays out");
        assert!(!l.enabled(&m(Level::Debug, "assetcraft")));
    }
}
