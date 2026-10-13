//! Opening files in other apps. Behind a trait so tests never spawn processes.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Hands files to the operating system.
pub trait Launcher: Send + Sync {
    /// Open `path` in its default app.
    fn open(&self, path: &Path) -> Result<(), String>;
    /// Show `path` selected in the system file manager.
    fn reveal(&self, path: &Path) -> Result<(), String>;
}

/// The real launcher: Explorer on Windows, `open` on macOS, `xdg-open` elsewhere.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemLauncher;

impl Launcher for SystemLauncher {
    fn open(&self, path: &Path) -> Result<(), String> {
        let mut cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("explorer.exe");
            c.arg(path);
            c
        } else if cfg!(target_os = "macos") {
            let mut c = std::process::Command::new("open");
            c.arg(path);
            c
        } else {
            let mut c = std::process::Command::new("xdg-open");
            c.arg(path);
            c
        };
        cmd.spawn().map(drop).map_err(|e| format!("could not open {}: {e}", path.display()))
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        let mut cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("explorer.exe");
            let mut arg = std::ffi::OsString::from("/select,");
            arg.push(path.as_os_str());
            c.arg(arg);
            c
        } else if cfg!(target_os = "macos") {
            let mut c = std::process::Command::new("open");
            c.arg("-R").arg(path);
            c
        } else {
            let mut c = std::process::Command::new("xdg-open");
            c.arg(path.parent().unwrap_or(path));
            c
        };
        cmd.spawn().map(drop).map_err(|e| format!("could not reveal {}: {e}", path.display()))
    }
}

/// Records calls instead of launching anything. For tests and headless use.
#[derive(Debug, Default)]
pub struct RecordingLauncher {
    /// `("open" | "reveal", path)` in call order.
    pub calls: Mutex<Vec<(&'static str, PathBuf)>>,
}

impl Launcher for RecordingLauncher {
    fn open(&self, path: &Path) -> Result<(), String> {
        self.calls.lock().map_err(|e| e.to_string())?.push(("open", path.to_path_buf()));
        Ok(())
    }
    fn reveal(&self, path: &Path) -> Result<(), String> {
        self.calls.lock().map_err(|e| e.to_string())?.push(("reveal", path.to_path_buf()));
        Ok(())
    }
}
