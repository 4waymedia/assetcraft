//! Where a cache lives (R1–R3, R19–R22; decisions D-C2 and D-C4, Paul 2026-10-16).
//!
//! | Platform | App cache                                   | Shared cache                     |
//! |----------|---------------------------------------------|----------------------------------|
//! | Windows  | `%LOCALAPPDATA%\AssetCraft\Cache` (never roams) | `%LOCALAPPDATA%\Crafting\Cache` |
//! | macOS    | `~/Library/Caches/AssetCraft`               | `~/Library/Caches/Crafting`      |
//! | Linux    | `$XDG_CACHE_HOME/assetcraft` (`~/.cache`)   | `$XDG_CACHE_HOME/crafting`       |
//!
//! First match wins: the user's setting (Settings ▸ Cache), then `<ID>_CACHE_DIR`
//! (`ASSETCRAFT_CACHE_DIR`, `CRAFTING_CACHE_DIR`; set but empty = **memory only**), then portable
//! mode (a `portable` file beside the executable → `<exe dir>/<Display>Data/Cache`), then the
//! platform folder, then memory only. Folder names follow the apps (D-C2): the display name on
//! Windows and macOS, the lower-case id on Linux. Paths must be absolute; anything else is
//! skipped. Pure functions over an environment reader, so every rule is tested on every OS.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// An app's names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct App {
    /// Shown in Explorer and Finder (`AssetCraft`).
    pub display: &'static str,
    /// Lower-case id (`assetcraft`), for Linux folders and the environment variable.
    pub id: &'static str,
}

/// AssetCraft.
pub const ASSETCRAFT: App = App { display: "AssetCraft", id: "assetcraft" };
/// The shared cache every Crafting app may use (D-C4).
pub const SHARED: App = App { display: "Crafting", id: "crafting" };

/// Platform family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
    /// Linux and other Unix.
    Other,
}

impl Os {
    /// The one this build runs on.
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Other
        }
    }
}

/// Which rule chose the folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Rule {
    /// The user's setting.
    Setting,
    /// The named environment variable.
    EnvVar(String),
    /// The named environment variable is set but empty: memory only.
    Isolated(String),
    /// A `portable` file beside the executable.
    Portable,
    /// The platform's cache folder.
    Platform,
    /// No folder could be found: memory only.
    Memory,
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rule::Setting => f.write_str("your setting"),
            Rule::EnvVar(n) => write!(f, "environment variable {n}"),
            Rule::Isolated(n) => write!(f, "{n} is empty: memory only"),
            Rule::Portable => f.write_str("portable mode"),
            Rule::Platform => f.write_str("platform default"),
            Rule::Memory => f.write_str("no folder available: memory only"),
        }
    }
}

/// A chosen cache folder (`None` = memory only) and the rule behind it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolved {
    /// The folder.
    pub dir: Option<PathBuf>,
    /// Why.
    pub rule: Rule,
}

/// `ASSETCRAFT_CACHE_DIR` for `assetcraft`.
pub fn env_var_name(app: &App) -> String {
    let mut n: String =
        app.id.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    n.push_str("_CACHE_DIR");
    n
}

/// The platform cache folder for `app` (see the module table).
pub fn platform(os: Os, app: &App, var: &dyn Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    let abs = |p: PathBuf| p.is_absolute().then_some(p);
    match os {
        Os::Windows => Some(abs(var("LOCALAPPDATA")?)?.join(app.display).join("Cache")),
        Os::MacOs => Some(abs(var("HOME")?)?.join("Library").join("Caches").join(app.display)),
        Os::Other => match var("XDG_CACHE_HOME").and_then(abs) {
            Some(xdg) => Some(xdg.join(app.id)),
            None => Some(abs(var("HOME")?)?.join(".cache").join(app.id)),
        },
    }
}

/// Apply the rules for `app`. `var` reads environment variables (an empty value is returned as
/// an empty path); `exe_dir` is the folder holding the executable, if known.
pub fn resolve(
    os: Os,
    app: &App,
    setting: Option<&Path>,
    var: &dyn Fn(&str) -> Option<PathBuf>,
    exe_dir: Option<&Path>,
) -> Resolved {
    if let Some(dir) = setting.filter(|p| p.is_absolute()) {
        return Resolved { dir: Some(dir.to_path_buf()), rule: Rule::Setting };
    }
    let name = env_var_name(app);
    if let Some(v) = var(&name) {
        if v.as_os_str().is_empty() {
            return Resolved { dir: None, rule: Rule::Isolated(name) };
        }
        if v.is_absolute() {
            return Resolved { dir: Some(v), rule: Rule::EnvVar(name) };
        }
    }
    if let Some(exe) = exe_dir.filter(|d| d.join("portable").is_file()) {
        return Resolved { dir: Some(exe.join(format!("{}Data", app.display)).join("Cache")), rule: Rule::Portable };
    }
    match platform(os, app, var) {
        Some(dir) => Resolved { dir: Some(dir), rule: Rule::Platform },
        None => Resolved { dir: None, rule: Rule::Memory },
    }
}

/// [`resolve`] against this process: real environment, real executable folder.
pub fn resolve_here(app: &App, setting: Option<&Path>) -> Resolved {
    let var = |n: &str| std::env::var_os(n).map(PathBuf::from);
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    resolve(Os::current(), app, setting, &var, exe.as_deref())
}
