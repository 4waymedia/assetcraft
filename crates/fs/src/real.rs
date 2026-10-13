//! [`RealFs`]: the actual disk, through `std::fs`. Read-only.

use crate::{Entry, EntryKind, FsError, KnownFolder, ListOptions, Listing, Services, Volume};
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};

/// The real file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealFs;

impl RealFs {
    /// Create a handle. Holds no state.
    pub fn new() -> Self {
        Self
    }
}

impl Services for RealFs {
    fn list_entries(&self, dir: &Path, opts: &ListOptions) -> Result<Listing, FsError> {
        let meta = fs::metadata(dir).map_err(|e| FsError::from_io(dir, &e))?;
        if !meta.is_dir() {
            return Err(FsError::NotADirectory(dir.to_path_buf()));
        }
        let reader = fs::read_dir(dir).map_err(|e| FsError::from_io(dir, &e))?;
        let mut listing = Listing::default();
        for item in reader {
            let Ok(item) = item else {
                listing.skipped += 1;
                continue;
            };
            let path = item.path();
            // `DirEntry::metadata` does not follow symlinks on any platform.
            let Ok(meta) = item.metadata() else {
                listing.skipped += 1;
                continue;
            };
            let name = item.file_name().to_string_lossy().into_owned();
            let entry = to_entry(name, path, &meta);
            if entry.hidden && !opts.include_hidden {
                continue;
            }
            if listing.entries.len() >= opts.max_entries {
                listing.truncated = true;
                break;
            }
            listing.entries.push(entry);
        }
        Ok(listing)
    }

    fn entry(&self, path: &Path) -> Result<Entry, FsError> {
        let meta = fs::symlink_metadata(path).map_err(|e| FsError::from_io(path, &e))?;
        Ok(to_entry(crate::display_name(path), path.to_path_buf(), &meta))
    }

    fn volumes(&self) -> Vec<Volume> {
        volumes()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        home_dir()
    }

    fn known_folder(&self, which: KnownFolder) -> Option<PathBuf> {
        let home = home_dir()?;
        let names: &[&str] = match which {
            KnownFolder::Home => return Some(home),
            KnownFolder::Desktop => &["Desktop"],
            KnownFolder::Documents => &["Documents"],
            KnownFolder::Downloads => &["Downloads"],
            KnownFolder::Pictures => &["Pictures"],
            KnownFolder::Videos => &["Videos", "Movies"],
        };
        // Windows may redirect Desktop/Documents/Pictures into OneDrive; check there second.
        // (The exact location needs the Known Folder API, which is planned for later.)
        let bases = [home.clone(), home.join("OneDrive")];
        bases.iter().flat_map(|base| names.iter().map(move |n| base.join(n))).find(|p| p.is_dir())
    }
}

fn to_entry(name: String, path: PathBuf, meta: &Metadata) -> Entry {
    let ft = meta.file_type();
    let kind = if ft.is_symlink() || is_reparse_point(meta) {
        EntryKind::Symlink
    } else if ft.is_dir() {
        EntryKind::Dir
    } else if ft.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    };
    let size = if kind == EntryKind::File { meta.len() } else { 0 };
    let hidden = name.starts_with('.') || has_hidden_attribute(meta);
    Entry { name, path, kind, size, modified: meta.modified().ok(), hidden }
}

#[cfg(windows)]
fn has_hidden_attribute(meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(not(windows))]
fn has_hidden_attribute(_meta: &Metadata) -> bool {
    false
}

/// Junctions and other reparse points are treated like symlinks: shown, never followed.
#[cfg(windows)]
fn is_reparse_point(meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_meta: &Metadata) -> bool {
    false
}

fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_dir())
}

#[cfg(windows)]
fn volumes() -> Vec<Volume> {
    (b'A'..=b'Z')
        .map(|letter| format!("{}:\\", letter as char))
        .filter(|root| fs::metadata(root).is_ok())
        .map(|root| Volume { name: root.trim_end_matches('\\').to_string(), path: PathBuf::from(root) })
        .collect()
}

#[cfg(not(windows))]
fn volumes() -> Vec<Volume> {
    let mut out = vec![Volume { name: "/".into(), path: PathBuf::from("/") }];
    let mut mount_parents = vec![PathBuf::from("/Volumes"), PathBuf::from("/mnt"), PathBuf::from("/media")];
    if let Some(user) = std::env::var_os("USER") {
        mount_parents.push(Path::new("/media").join(&user));
        mount_parents.push(Path::new("/run/media").join(&user));
    }
    for parent in mount_parents {
        let Ok(children) = fs::read_dir(&parent) else { continue };
        for child in children.flatten() {
            let path = child.path();
            if path.is_dir() && !out.iter().any(|v| v.path == path) {
                out.push(Volume { name: crate::display_name(&path), path });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WalkLimits, walk};

    fn library() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
    }

    #[test]
    fn lists_the_test_library_root() {
        let listing = RealFs.list_entries(&library(), &ListOptions::default()).unwrap();
        let mut names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["characters", "edge-cases", "environments", "palettes", "props", "storyboard"]);
        assert!(listing.entries.iter().all(|e| e.kind == EntryKind::Dir));
        assert!(!listing.truncated);
    }

    #[test]
    fn reads_edge_case_names_and_sizes() {
        let listing = RealFs.list_entries(&library().join("edge-cases"), &ListOptions::default()).unwrap();
        let unicode = listing.entries.iter().find(|e| e.name == "Réaper ünïcode name.png").unwrap();
        assert_eq!(unicode.kind, EntryKind::File);
        assert!(unicode.size > 0);
        let broken = listing.entries.iter().find(|e| e.name == "broken_not_an_image.jpg").unwrap();
        assert_eq!(broken.size, 18);
        assert!(listing.entries.iter().any(|e| e.extension().as_deref() == Some("jpg") && e.name.ends_with(".JPG")));
    }

    #[test]
    fn walk_counts_every_test_file() {
        let summary = walk(&RealFs, &library(), &WalkLimits::default());
        assert_eq!(summary.files, 24);
        assert_eq!(summary.dirs, 10); // root + 9 folders
        assert!(!summary.truncated);
    }

    #[test]
    fn errors_are_typed() {
        let missing = library().join("does-not-exist");
        assert!(matches!(RealFs.list_entries(&missing, &ListOptions::default()), Err(FsError::NotFound(_))));
        let file = library().join("palettes/reapers_palette.bmp");
        assert!(matches!(RealFs.list_entries(&file, &ListOptions::default()), Err(FsError::NotADirectory(_))));
    }

    #[test]
    fn max_entries_truncates() {
        let opts = ListOptions { max_entries: 2, ..ListOptions::default() };
        let listing = RealFs.list_entries(&library(), &opts).unwrap();
        assert_eq!(listing.entries.len(), 2);
        assert!(listing.truncated);
    }

    #[test]
    fn has_at_least_one_volume() {
        assert!(!RealFs.volumes().is_empty());
    }
}
