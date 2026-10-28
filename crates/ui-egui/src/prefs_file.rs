//! Where `prefs.json` lives, and loading / atomically saving it.
//!
//! The engine turns state into [`Prefs`] and back; this module only moves that JSON to and from
//! the per-user config folder. Prefs are AssetCraft's own file, not the user's assets, so writing
//! them is allowed in Stage 0 (AGENTS.md rule 5 is about the user's files).

use assetcraft_engine::Prefs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// File name inside the config folder.
pub const FILE_NAME: &str = "prefs.json";

/// Saved prefs larger than this are ignored (defaults are used instead).
pub const MAX_PREFS_BYTES: u64 = 1024 * 1024;

/// Shortest gap between two automatic saves (panel drags change the layout every frame).
pub const SAVE_INTERVAL: Duration = Duration::from_secs(1);

/// The prefs file for this platform:
/// - Windows: `%APPDATA%\AssetCraft\prefs.json`
/// - macOS: `~/Library/Application Support/AssetCraft/prefs.json`
/// - elsewhere: `$XDG_CONFIG_HOME/assetcraft/prefs.json`, else `~/.config/assetcraft/prefs.json`
pub fn config_path() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if cfg!(windows) {
        var("APPDATA")?.join("AssetCraft")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library").join("Application Support").join("AssetCraft")
    } else {
        match var("XDG_CONFIG_HOME") {
            Some(xdg) if xdg.is_absolute() => xdg.join("assetcraft"),
            _ => var("HOME")?.join(".config").join("assetcraft"),
        }
    };
    Some(dir.join(FILE_NAME))
}

/// The suite-wide My Folders file, shared by every Crafting app (docs/design/folders-panel.md):
/// - Windows: `%APPDATA%\Crafting\places.json`
/// - macOS: `~/Library/Application Support/Crafting/places.json`
/// - elsewhere: `$XDG_CONFIG_HOME/crafting/places.json`, else `~/.config/crafting/places.json`
pub fn places_path() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if cfg!(windows) {
        var("APPDATA")?.join("Crafting")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library").join("Application Support").join("Crafting")
    } else {
        match var("XDG_CONFIG_HOME") {
            Some(xdg) if xdg.is_absolute() => xdg.join("crafting"),
            _ => var("HOME")?.join(".config").join("crafting"),
        }
    };
    Some(dir.join("places.json"))
}

/// Where thumbnails were cached before the suite cache (C1, 2026-10-16), so the app can delete
/// them once (`assetcraft_thumbs::disk::retire_legacy`):
/// - Windows: `%LOCALAPPDATA%\AssetCraft\thumbs`
/// - macOS: `~/Library/Caches/AssetCraft/thumbs`
/// - elsewhere: `$XDG_CACHE_HOME/assetcraft/thumbs`, else `~/.cache/assetcraft/thumbs`
///
/// The cache itself now lives where `assetcraft_cache::root` says.
pub fn legacy_thumb_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let os = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::MacOs
    } else {
        Os::Other
    };
    cache_dir_for(os, var)
}

/// Platform family for [`cache_dir_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
    /// Linux and other Unix.
    Other,
}

/// [`legacy_thumb_dir`] for `os` with environment lookups from `var` (pure, for tests).
pub fn cache_dir_for(os: Os, var: impl Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    let base = match os {
        Os::Windows => var("LOCALAPPDATA").filter(|p| p.is_absolute())?.join("AssetCraft"),
        Os::MacOs => var("HOME")?.join("Library").join("Caches").join("AssetCraft"),
        Os::Other => match var("XDG_CACHE_HOME") {
            Some(xdg) if xdg.is_absolute() => xdg.join("assetcraft"),
            _ => var("HOME")?.join(".cache").join("assetcraft"),
        },
    };
    Some(base.join("thumbs"))
}

/// Load prefs from `path`. A missing, oversized or unreadable file gives defaults: bad prefs
/// must never stop the app from starting. A file that exists but can't be used at all
/// (oversized, not UTF-8, not a JSON object) is first renamed to `prefs.json.bad`, so the next
/// save doesn't silently destroy what the user (or a newer version) wrote. A file with only some
/// bad fields keeps its good ones (see `Prefs::from_json`) and isn't renamed.
pub fn load(path: &Path) -> Prefs {
    let Ok(meta) = std::fs::metadata(path) else { return Prefs::from_json("") };
    if !meta.is_file() {
        return Prefs::from_json("");
    }
    let text = if meta.len() > MAX_PREFS_BYTES {
        Some(Err(()))
    } else {
        // Read errors (antivirus, another instance holding the file) are transient: use
        // defaults for this run but leave the file alone. Only content that was read and is
        // unusable gets set aside.
        match std::fs::read(path) {
            Ok(bytes) => Some(String::from_utf8(bytes).map_err(drop)),
            Err(_) => None,
        }
    };
    match text {
        None => Prefs::from_json(""),
        Some(Err(())) => {
            let _ = std::fs::rename(path, bad_path(path));
            Prefs::from_json("")
        }
        Some(Ok(text)) => {
            let (prefs, whole) = Prefs::parse(&text);
            if !whole {
                let _ = std::fs::rename(path, bad_path(path));
            }
            prefs
        }
    }
}

/// Where an unusable prefs file is kept: `prefs.json` → `prefs.json.bad` (replacing an older
/// copy).
pub fn bad_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| FILE_NAME.into());
    name.push(".bad");
    path.with_file_name(name)
}

/// Save prefs atomically: write a temporary file in the same folder, flush it to disk, then
/// rename it over `path`, so a crash mid-write leaves the old file intact.
pub fn save(path: &Path, prefs: &Prefs) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("prefs path has no folder"))?;
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| FILE_NAME.into());
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let written = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(prefs.to_json().as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// Saves prefs when the parts the user changes on purpose (favorites, view, panel layout)
/// differ from what is on disk, at most once per [`SAVE_INTERVAL`], and always on exit.
#[derive(Debug)]
pub struct PrefsStore {
    path: Option<PathBuf>,
    saved: Prefs,
    last_write: Option<Instant>,
    last_error: Option<String>,
}

impl PrefsStore {
    /// A store writing to `path` (`None`: never writes), where `loaded` is what is on disk now.
    pub fn new(path: Option<PathBuf>, loaded: Prefs) -> Self {
        Self { path, saved: loaded, last_write: None, last_error: None }
    }

    /// The file this store writes, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The last save error, if the most recent attempt failed.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Call every frame with the session's prefs. Writes when favorites, view or panels
    /// changed (the last folder alone is saved on exit). Returns `true` if it wrote.
    pub fn maybe_save(&mut self, prefs: &Prefs) -> bool {
        let changed = prefs.favorites != self.saved.favorites
            || prefs.view != self.saved.view
            || prefs.panels != self.saved.panels;
        let due = self.last_write.is_none_or(|t| t.elapsed() >= SAVE_INTERVAL);
        changed && due && self.write(prefs)
    }

    /// Write unconditionally if anything differs (call on exit). Returns `true` if it wrote.
    pub fn save_now(&mut self, prefs: &Prefs) -> bool {
        *prefs != self.saved && self.write(prefs)
    }

    fn write(&mut self, prefs: &Prefs) -> bool {
        let Some(path) = &self.path else { return false };
        self.last_write = Some(Instant::now());
        match save(path, prefs) {
            Ok(()) => {
                self.saved = prefs.clone();
                self.last_error = None;
                true
            }
            Err(e) => {
                self.last_error = Some(format!("could not save {}: {e}", path.display()));
                false
            }
        }
    }
}
