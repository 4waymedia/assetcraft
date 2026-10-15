//! File-system access behind a [`Services`] trait (real disk, fake disk for tests).
//!
//! Layer 0. Everything above this crate reads folders through [`Services`], never through
//! `std::fs` directly, so the engine can be tested against [`FakeFs`] and a web build can plug
//! in its own implementation. Modelled on FilmCraft's Media Browser `Services` split
//! (`crates/engine/src/media_browser.rs`).
//!
//! Rules: read-only; never follow symlinks while walking; every walk is bounded; a single
//! unreadable entry never fails a whole listing.

mod fake;
mod real;
mod walk;

pub use fake::FakeFs;
pub use real::RealFs;
pub use walk::{WalkLimits, WalkSummary, walk};

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// Default cap on entries returned by one [`Services::list_entries`] call.
pub const DEFAULT_MAX_ENTRIES: usize = 100_000;

/// What kind of thing a directory entry is. Symlinks are reported as such and never followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EntryKind {
    /// A directory.
    Dir,
    /// A regular file.
    File,
    /// A symbolic link or junction (not followed).
    Symlink,
    /// Anything else (device, socket, pipe).
    Other,
}

/// One entry in a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// File name as displayed (lossy UTF-8 if the OS name isn't valid Unicode).
    pub name: String,
    /// Full path.
    pub path: PathBuf,
    /// Entry kind.
    pub kind: EntryKind,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Last modification time, when the OS reports one.
    pub modified: Option<SystemTime>,
    /// Creation (birth) time, when the OS and file system report one (many Linux file systems
    /// and network shares don't).
    pub created: Option<SystemTime>,
    /// Hidden by OS convention (dot-file on Unix, hidden attribute on Windows).
    pub hidden: bool,
}

impl Entry {
    /// Lower-cased extension without the dot, if any (`"JPG"` → `"jpg"`).
    pub fn extension(&self) -> Option<String> {
        Path::new(&self.name).extension().map(|e| e.to_string_lossy().to_lowercase())
    }
}

/// Options for one listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListOptions {
    /// Include hidden entries.
    pub include_hidden: bool,
    /// Stop after this many entries and set [`Listing::truncated`].
    pub max_entries: usize,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self { include_hidden: false, max_entries: DEFAULT_MAX_ENTRIES }
    }
}

/// The result of listing one directory. Entries are in the order the OS returned them; sorting
/// is a view concern and lives in the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    /// The entries.
    pub entries: Vec<Entry>,
    /// `true` if [`ListOptions::max_entries`] cut the listing short.
    pub truncated: bool,
    /// Entries that could not be read and were skipped.
    pub skipped: usize,
}

/// A mounted volume or drive root shown in the Folders panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// Display name, e.g. `"F:"` or `"/"`.
    pub name: String,
    /// Root path.
    pub path: PathBuf,
}

/// Well-known user folders shown at the top of the Folders panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnownFolder {
    /// The user's home folder.
    Home,
    /// Desktop.
    Desktop,
    /// Documents.
    Documents,
    /// Downloads.
    Downloads,
    /// Pictures.
    Pictures,
    /// Videos (Movies on macOS).
    Videos,
}

impl KnownFolder {
    /// Every known folder, in Folders-panel order.
    pub const ALL: [KnownFolder; 6] =
        [Self::Home, Self::Desktop, Self::Documents, Self::Downloads, Self::Pictures, Self::Videos];

    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Desktop => "Desktop",
            Self::Documents => "Documents",
            Self::Downloads => "Downloads",
            Self::Pictures => "Pictures",
            Self::Videos => "Videos",
        }
    }
}

/// Why a file-system call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    /// The path does not exist.
    NotFound(PathBuf),
    /// The OS refused access.
    PermissionDenied(PathBuf),
    /// The path exists but is not a directory.
    NotADirectory(PathBuf),
    /// Any other I/O failure.
    Io {
        /// The path involved.
        path: PathBuf,
        /// The OS error text.
        message: String,
    },
}

impl FsError {
    /// Map a `std::io::Error` for `path` to an [`FsError`].
    pub fn from_io(path: &Path, err: &std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound(path.to_path_buf()),
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied(path.to_path_buf()),
            std::io::ErrorKind::NotADirectory => Self::NotADirectory(path.to_path_buf()),
            _ => Self::Io { path: path.to_path_buf(), message: err.to_string() },
        }
    }
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(p) => write!(f, "not found: {}", p.display()),
            Self::PermissionDenied(p) => write!(f, "permission denied: {}", p.display()),
            Self::NotADirectory(p) => write!(f, "not a folder: {}", p.display()),
            Self::Io { path, message } => write!(f, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for FsError {}

/// Read-only file-system services. Implemented by [`RealFs`] (the disk) and [`FakeFs`] (tests).
pub trait Services: Send + Sync {
    /// List one directory. Fails only if the directory itself can't be read; unreadable entries
    /// are skipped and counted in [`Listing::skipped`].
    fn list_entries(&self, dir: &Path, opts: &ListOptions) -> Result<Listing, FsError>;

    /// Metadata for one path, without following a final symlink.
    fn entry(&self, path: &Path) -> Result<Entry, FsError>;

    /// Read at most `max` bytes from the start of a file (for type sniffing and headers).
    /// Returns fewer bytes for short files. Never reads past `max`.
    fn read_head(&self, path: &Path, max: usize) -> Result<Vec<u8>, FsError>;

    /// Read at most `len` bytes starting at byte `offset`. Returns fewer bytes (possibly none)
    /// when the file ends first.
    fn read_range(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, FsError>;

    /// Mounted volumes / drive roots.
    fn volumes(&self) -> Vec<Volume>;

    /// The user's home folder.
    fn home_dir(&self) -> Option<PathBuf>;

    /// A well-known folder, if it exists.
    fn known_folder(&self, which: KnownFolder) -> Option<PathBuf>;

    /// `true` for a regular file with an execute permission bit set (Unix). Windows has no such
    /// bit (programs are recognized by extension), so the default and the Windows answer is
    /// `false`. Never follows a final symlink.
    fn is_executable(&self, path: &Path) -> bool {
        let _ = path;
        false
    }
}

/// Display name for a path's last component, falling back to the whole path for roots.
pub fn display_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_prefixed() {
        assert!(CRATE.starts_with("assetcraft-"));
    }

    #[test]
    fn extension_is_lowercased() {
        let e = Entry {
            name: "TINY_ICON.JPG".into(),
            path: "/x/TINY_ICON.JPG".into(),
            kind: EntryKind::File,
            size: 1,
            modified: None,
            created: None,
            hidden: false,
        };
        assert_eq!(e.extension().as_deref(), Some("jpg"));
    }

    #[test]
    fn display_name_of_root_is_the_path() {
        assert_eq!(display_name(Path::new("/")), "/");
        assert_eq!(display_name(Path::new("/a/b.png")), "b.png");
    }
}
