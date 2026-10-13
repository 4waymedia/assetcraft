//! Session, command registry and navigation state. Every action is a command.
//!
//! Layer 5. The UI, keyboard, CLI and (later) MCP all call [`Session::execute`] with a command
//! id and JSON params; nothing else changes state. The UI then reads state back through the
//! getters ([`Session::location`], [`Session::items`], [`Session::selection`] …) and draws it.
//! Commands are listed in [`commands`]; `assetcraft-cli commands` prints them.

mod commands;
pub mod launch;
pub mod prefs;
pub mod select;
pub mod sort;

pub use commands::{CommandSpec, commands, find};
pub use launch::{Launcher, RecordingLauncher, SystemLauncher};
pub use prefs::{Prefs, ViewMode, ViewPrefs};
pub use select::{ClickMode, Selection};
pub use sort::SortKey;

use assetcraft_fs::{Entry, EntryKind, FsError, KnownFolder, ListOptions, Services};
use assetcraft_probe::Probe;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// Most entries shown for one folder. Beyond this the listing is marked truncated.
pub const MAX_ITEMS: usize = 50_000;

/// Back-history depth.
pub const MAX_HISTORY: usize = 100;

/// Where the content panel is pointed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    /// "Computer": the list of drives / volumes.
    Computer,
    /// A folder on disk.
    Folder(PathBuf),
}

impl Location {
    /// The folder path, if this is a folder.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Computer => None,
            Self::Folder(p) => Some(p),
        }
    }

    /// Display label (folder name, or "Computer").
    pub fn label(&self) -> String {
        match self {
            Self::Computer => "Computer".into(),
            Self::Folder(p) => assetcraft_fs::display_name(p),
        }
    }
}

/// Why a command failed. Shown to the user as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdError {
    /// No command with that id.
    Unknown(String),
    /// Missing or invalid parameter.
    BadParams(String),
    /// The file system refused.
    Fs(FsError),
    /// The command can't run right now (e.g. "no folder above this one").
    Unavailable(String),
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(id) => write!(f, "unknown command `{id}`"),
            Self::BadParams(m) => write!(f, "bad parameters: {m}"),
            Self::Fs(e) => write!(f, "{e}"),
            Self::Unavailable(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for CmdError {}

impl From<FsError> for CmdError {
    fn from(e: FsError) -> Self {
        Self::Fs(e)
    }
}

/// The result of a command: a JSON value describing what changed.
pub type CmdResult = Result<Value, CmdError>;

/// One window's worth of navigator state.
pub struct Session {
    services: Arc<dyn Services>,
    launcher: Arc<dyn Launcher>,
    location: Location,
    back: Vec<Location>,
    forward: Vec<Location>,
    items: Vec<Entry>,
    item_paths: Vec<PathBuf>,
    truncated: bool,
    skipped: usize,
    selection: Selection,
    favorites: Vec<PathBuf>,
    view: ViewPrefs,
    probes: HashMap<PathBuf, Result<Probe, String>>,
}

impl Session {
    /// A session reading through `services`, launching through `launcher`, restoring `prefs`.
    /// Opens the last folder if it still exists, else Pictures, else home, else Computer.
    pub fn new(services: Arc<dyn Services>, launcher: Arc<dyn Launcher>, prefs: Prefs) -> Self {
        let mut session = Self {
            services,
            launcher,
            location: Location::Computer,
            back: Vec::new(),
            forward: Vec::new(),
            items: Vec::new(),
            item_paths: Vec::new(),
            truncated: false,
            skipped: 0,
            selection: Selection::default(),
            favorites: prefs.favorites,
            view: prefs.view,
            probes: HashMap::new(),
        };
        let start = prefs
            .last_folder
            .into_iter()
            .chain(session.services.known_folder(KnownFolder::Pictures))
            .chain(session.services.home_dir())
            .map(Location::Folder)
            .find(|loc| session.load(loc).is_ok());
        match start {
            Some(loc) => session.location = loc,
            None => {
                let _ = session.load(&Location::Computer);
            }
        }
        session
    }

    /// Run a command. This is the only way state changes.
    pub fn execute(&mut self, id: &str, params: &Value) -> CmdResult {
        let spec = find(id).ok_or_else(|| CmdError::Unknown(id.to_string()))?;
        (spec.run)(self, params)
    }

    /// Current location.
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Items in the current location, filtered and sorted for display.
    pub fn items(&self) -> &[Entry] {
        &self.items
    }

    /// `true` if the folder had more than [`MAX_ITEMS`] entries.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Entries the OS wouldn't let us read in the current folder.
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// The selection.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Selected paths in display order.
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selection.paths(&self.item_paths)
    }

    /// Favorites in the user's order.
    pub fn favorites(&self) -> &[PathBuf] {
        &self.favorites
    }

    /// View settings.
    pub fn view(&self) -> &ViewPrefs {
        &self.view
    }

    /// `true` if `nav.back` would do something.
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    /// `true` if `nav.forward` would do something.
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// `true` if `nav.up` would do something.
    pub fn can_up(&self) -> bool {
        matches!(self.location, Location::Folder(_))
    }

    /// Breadcrumb from the volume root to the current folder.
    pub fn breadcrumb(&self) -> Vec<PathBuf> {
        let Some(path) = self.location.path() else { return Vec::new() };
        let mut crumbs: Vec<PathBuf> = path.ancestors().map(Path::to_path_buf).collect();
        crumbs.reverse();
        crumbs
    }

    /// Drives / volumes for the Folders panel.
    pub fn volumes(&self) -> Vec<assetcraft_fs::Volume> {
        self.services.volumes()
    }

    /// Known folders that exist, for the Folders panel.
    pub fn known_folders(&self) -> Vec<(KnownFolder, PathBuf)> {
        KnownFolder::ALL.iter().filter_map(|k| self.services.known_folder(*k).map(|p| (*k, p))).collect()
    }

    /// Sub-folders of `dir` for the folder tree (lazy: call when a node is expanded).
    pub fn subfolders(&self, dir: &Path) -> Result<Vec<PathBuf>, CmdError> {
        let opts = ListOptions { include_hidden: self.view.show_hidden, max_entries: MAX_ITEMS };
        let mut dirs: Vec<Entry> =
            self.services.list_entries(dir, &opts)?.entries.into_iter().filter(|e| e.kind == EntryKind::Dir).collect();
        sort::sort_entries(&mut dirs, SortKey::Name, true);
        Ok(dirs.into_iter().map(|e| e.path).collect())
    }

    /// Header facts for a file (format, dimensions …), cached until the folder is refreshed.
    pub fn probe(&mut self, path: &Path) -> Result<&Probe, CmdError> {
        let services = Arc::clone(&self.services);
        let entry = self
            .probes
            .entry(path.to_path_buf())
            .or_insert_with(|| assetcraft_probe::probe(services.as_ref(), path).map_err(|e| e.to_string()));
        entry.as_ref().map_err(|e| CmdError::Unavailable(e.clone()))
    }

    /// Everything that should be saved on exit.
    pub fn prefs(&self) -> Prefs {
        Prefs {
            version: prefs::PREFS_VERSION,
            favorites: self.favorites.clone(),
            last_folder: self.location.path().map(Path::to_path_buf),
            view: self.view.clone(),
        }
    }

    /// The services this session reads through (the thumbnail pool shares them).
    pub fn services(&self) -> Arc<dyn Services> {
        Arc::clone(&self.services)
    }

    // ---- internals used by commands --------------------------------------------------------

    /// Navigate, recording history. On failure nothing changes.
    fn go(&mut self, target: Location) -> CmdResult {
        if target == self.location {
            return self.refresh();
        }
        self.load(&target)?;
        let previous = std::mem::replace(&mut self.location, target);
        self.back.push(previous);
        if self.back.len() > MAX_HISTORY {
            self.back.remove(0);
        }
        self.forward.clear();
        Ok(self.location_json())
    }

    /// Re-list the current location, keeping the selection where items still exist.
    fn refresh(&mut self) -> CmdResult {
        let loc = self.location.clone();
        let keep = self.selection.clone();
        self.load(&loc)?;
        self.selection = keep;
        self.selection.retain_existing(&self.item_paths);
        Ok(self.location_json())
    }

    /// List `loc` into `items` (the caller sets `location`). Clears selection and probe cache.
    fn load(&mut self, loc: &Location) -> Result<(), CmdError> {
        let (mut items, truncated, skipped) = match loc {
            Location::Computer => {
                let items = self
                    .services
                    .volumes()
                    .into_iter()
                    .map(|v| Entry {
                        name: v.name,
                        path: v.path,
                        kind: EntryKind::Dir,
                        size: 0,
                        modified: None,
                        hidden: false,
                    })
                    .collect();
                (items, false, 0)
            }
            Location::Folder(path) => {
                let opts = ListOptions { include_hidden: self.view.show_hidden, max_entries: MAX_ITEMS };
                let listing = self.services.list_entries(path, &opts)?;
                (listing.entries, listing.truncated, listing.skipped)
            }
        };
        if matches!(loc, Location::Folder(_)) {
            sort::sort_entries(&mut items, self.view.sort, self.view.ascending);
        }
        self.item_paths = items.iter().map(|e| e.path.clone()).collect();
        self.items = items;
        self.truncated = truncated;
        self.skipped = skipped;
        self.selection.clear();
        self.probes.clear();
        Ok(())
    }

    fn resort(&mut self) {
        sort::sort_entries(&mut self.items, self.view.sort, self.view.ascending);
        self.item_paths = self.items.iter().map(|e| e.path.clone()).collect();
    }

    fn location_json(&self) -> Value {
        serde_json::json!({
            "location": match &self.location { Location::Computer => Value::Null, Location::Folder(p) => p.display().to_string().into() },
            "label": self.location.label(),
            "items": self.items.len(),
            "truncated": self.truncated,
        })
    }
}

#[cfg(test)]
mod tests;
