//! One AssetCraft at a time (FIND-T2, Paul 2026-11-28): starting it again while it runs brings
//! the running window forward instead of opening a second app.
//!
//! The first AssetCraft holds an OS lock on `instance.lock` beside `prefs.json` (released by
//! the OS when the process ends, crash included, so a stale lock never blocks a start). A
//! second one finds the lock taken, leaves a `raise` request there (the folder it was given, if
//! any) and exits; the first notices within half a second, comes forward and opens that folder.
//! `ASSETCRAFT_NEW_INSTANCE=1` skips all this (two windows on purpose, or testing).

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const LOCK: &str = "instance.lock";
const RAISE: &str = "raise";
/// A raise request is one path; anything longer is not ours.
const MAX_RAISE_BYTES: u64 = 32 * 1024;
const POLL: Duration = Duration::from_millis(400);

/// What starting found.
pub enum Start {
    /// This is the only AssetCraft: run, and listen for later starts.
    First(Primary),
    /// Another AssetCraft runs and was asked to come forward: exit.
    Handed,
    /// The lock couldn't be used (no config folder, a read-only disk…): run as before.
    Alone(String),
}

/// The running AssetCraft's side: holds the lock while it lives.
pub struct Primary {
    _lock: File,
    raise: PathBuf,
    wake: Arc<AtomicBool>,
}

/// Claim `dir`'s lock, or hand `folder` to the AssetCraft that holds it.
pub fn claim(dir: &Path, folder: Option<&Path>) -> Start {
    if std::env::var_os("ASSETCRAFT_NEW_INSTANCE").is_some_and(|v| !v.is_empty()) {
        return Start::Alone("ASSETCRAFT_NEW_INSTANCE is set".into());
    }
    if let Err(e) = std::fs::create_dir_all(dir) {
        return Start::Alone(format!("{}: {e}", dir.display()));
    }
    let lock = match OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(LOCK)) {
        Ok(f) => f,
        Err(e) => return Start::Alone(format!("{}: {e}", dir.join(LOCK).display())),
    };
    match lock.try_lock() {
        Ok(()) => {
            let raise = dir.join(RAISE);
            // A request left by a start that raced our own is stale now.
            let _ = std::fs::remove_file(&raise);
            Start::First(Primary { _lock: lock, raise, wake: Arc::new(AtomicBool::new(false)) })
        }
        Err(TryLockError::WouldBlock) => match request(dir, folder) {
            Ok(()) => Start::Handed,
            Err(e) => Start::Alone(format!("couldn't reach the running AssetCraft: {e}")),
        },
        Err(TryLockError::Error(e)) => Start::Alone(format!("{}: {e}", dir.join(LOCK).display())),
    }
}

/// Leave a raise request: the folder's path, or nothing. Written beside and renamed over, so
/// the running app never reads half of one.
fn request(dir: &Path, folder: Option<&Path>) -> std::io::Result<()> {
    let text = folder.map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = dir.join(format!("{RAISE}.{}.tmp", std::process::id()));
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, dir.join(RAISE)).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

impl Primary {
    /// Watch for raise requests on a small thread that wakes the UI when one arrives.
    pub fn watch(&self, wake_ui: impl Fn() + Send + 'static) {
        let raise = self.raise.clone();
        let wake = self.wake.clone();
        let _ = std::thread::Builder::new().name("assetcraft-single".into()).spawn(move || {
            loop {
                std::thread::sleep(POLL);
                if raise.is_file() && !wake.swap(true, Ordering::AcqRel) {
                    wake_ui();
                }
            }
        });
    }

    /// A raise request, taken (and removed) once: `Some(folder)` with the folder to open, if
    /// the later start was given one. A request that isn't one path is still a raise.
    pub fn take(&self) -> Option<Option<PathBuf>> {
        if !self.wake.swap(false, Ordering::AcqRel) && !self.raise.is_file() {
            return None;
        }
        let mut text = String::new();
        let read = File::open(&self.raise).and_then(|f| f.take(MAX_RAISE_BYTES).read_to_string(&mut text));
        let _ = std::fs::remove_file(&self.raise);
        read.ok()?;
        let path = PathBuf::from(text.trim());
        Some(path.is_absolute().then_some(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("assetcraft-single-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_second_start_hands_its_folder_to_the_first() {
        let dir = temp("hand");
        let Start::First(first) = claim(&dir, None) else { panic!("the first start runs") };
        assert!(first.take().is_none(), "nothing asked yet");
        let folder = std::env::temp_dir().join("pictures");
        assert!(matches!(claim(&dir, Some(&folder)), Start::Handed));
        assert_eq!(first.take(), Some(Some(folder)));
        assert!(first.take().is_none(), "taken once");
        assert!(matches!(claim(&dir, None), Start::Handed));
        assert_eq!(first.take(), Some(None), "a plain start just raises");
        drop(first);
        assert!(matches!(claim(&dir, None), Start::First(_)), "the lock goes with the app");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_raise_that_isnt_a_path_still_raises_and_a_huge_one_is_cut() {
        let dir = temp("hostile");
        let Start::First(first) = claim(&dir, None) else { panic!("the first start runs") };
        std::fs::write(dir.join(RAISE), "relative\\folder").unwrap();
        assert_eq!(first.take(), Some(None));
        std::fs::write(dir.join(RAISE), vec![b'/'; 1 << 20]).unwrap();
        assert!(first.take().is_some());
        assert!(!dir.join(RAISE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
