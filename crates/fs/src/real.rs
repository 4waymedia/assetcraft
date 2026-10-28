//! [`RealFs`]: the actual disk, through `std::fs`. Read-only.

use crate::{Entry, EntryKind, FsError, KnownFolder, ListOptions, Listing, Services, SidebarItem, Volume};
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

    fn read_head(&self, path: &Path, max: usize) -> Result<Vec<u8>, FsError> {
        use std::io::Read;
        let file = fs::File::open(path).map_err(|e| FsError::from_io(path, &e))?;
        let mut buf = Vec::with_capacity(max.min(1 << 20));
        file.take(max as u64).read_to_end(&mut buf).map_err(|e| FsError::from_io(path, &e))?;
        Ok(buf)
    }

    fn read_range(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = fs::File::open(path).map_err(|e| FsError::from_io(path, &e))?;
        file.seek(SeekFrom::Start(offset)).map_err(|e| FsError::from_io(path, &e))?;
        let mut buf = Vec::with_capacity(len.min(1 << 20));
        file.take(len as u64).read_to_end(&mut buf).map_err(|e| FsError::from_io(path, &e))?;
        Ok(buf)
    }

    fn volumes(&self) -> Vec<Volume> {
        volumes()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        home_dir()
    }

    fn sidebar(&self) -> Vec<SidebarItem> {
        crate::sidebar::read(home_dir().as_deref())
    }

    fn is_executable(&self, path: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            false
        }
    }

    fn known_folder(&self, which: KnownFolder) -> Option<PathBuf> {
        // Windows: ask the shell (SHGetKnownFolderPath), which knows where the user or OneDrive
        // redirected the folder. Elsewhere, and if that fails, guess below the home folder.
        if let Some(path) = os_known_folder(which).filter(|p| p.is_dir()) {
            return Some(path);
        }
        let home = home_dir()?;
        let names: &[&str] = match which {
            KnownFolder::Home => return Some(home),
            KnownFolder::Desktop => &["Desktop"],
            KnownFolder::Documents => &["Documents"],
            KnownFolder::Downloads => &["Downloads"],
            KnownFolder::Pictures => &["Pictures"],
            KnownFolder::Videos => &["Videos", "Movies"],
            KnownFolder::Music => &["Music"],
        };
        // The guess: `<home>\<Name>`, then OneDrive's usual redirect `<home>\OneDrive\<Name>`.
        let bases = [home.clone(), home.join("OneDrive")];
        bases.iter().flat_map(|base| names.iter().map(move |n| base.join(n))).find(|p| p.is_dir())
    }
}

/// The folder's real location from the Windows Known Folder API (through the `known-folders`
/// crate, which owns the `unsafe` FFI; this workspace forbids it). Home stays `%USERPROFILE%`.
#[cfg(windows)]
fn os_known_folder(which: KnownFolder) -> Option<PathBuf> {
    use known_folders::{KnownFolder as Id, get_known_folder_path};
    let id = match which {
        KnownFolder::Home => return None,
        KnownFolder::Desktop => Id::Desktop,
        KnownFolder::Documents => Id::Documents,
        KnownFolder::Downloads => Id::Downloads,
        KnownFolder::Pictures => Id::Pictures,
        KnownFolder::Videos => Id::Videos,
        KnownFolder::Music => Id::Music,
    };
    get_known_folder_path(id)
}

#[cfg(not(windows))]
fn os_known_folder(_which: KnownFolder) -> Option<PathBuf> {
    None
}

fn to_entry(name: String, path: PathBuf, meta: &Metadata) -> Entry {
    let ft = meta.file_type();
    let kind = classify(ft.is_symlink(), ft.is_dir(), ft.is_file(), file_attributes(meta));
    let size = if kind == EntryKind::File { meta.len() } else { 0 };
    let hidden = name.starts_with('.') || file_attributes(meta) & FILE_ATTRIBUTE_HIDDEN != 0;
    Entry { name, path, kind, size, modified: meta.modified().ok(), created: meta.created().ok(), hidden }
}

/// Windows `FILE_ATTRIBUTE_HIDDEN`.
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

/// Entry kind from the file type flags. `attributes` (Windows file attributes, 0 elsewhere) are
/// deliberately *not* consulted: a reparse point is not necessarily a link. OneDrive
/// Files-On-Demand placeholders and redirected Pictures/Desktop folders are reparse points
/// that behave like ordinary files and folders, while real symlinks and junctions are already
/// reported by `FileType::is_symlink()` on Windows (name-surrogate reparse tags).
fn classify(is_symlink: bool, is_dir: bool, is_file: bool, attributes: u32) -> EntryKind {
    let _ = attributes;
    if is_symlink {
        EntryKind::Symlink
    } else if is_dir {
        EntryKind::Dir
    } else if is_file {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

#[cfg(windows)]
fn file_attributes(meta: &Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes()
}

#[cfg(not(windows))]
fn file_attributes(_meta: &Metadata) -> u32 {
    0
}

fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_dir())
}

#[cfg(windows)]
fn volumes() -> Vec<Volume> {
    // Mapped network drives, from `net use` (drive letters only; no `unsafe` Win32 calls here).
    use std::os::windows::process::CommandExt;
    /// `CREATE_NO_WINDOW`: no console flashes up from the GUI app.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let net = std::process::Command::new("net")
        .arg("use")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| crate::mounts::windows_mapped(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();
    (b'A'..=b'Z')
        .map(|letter| format!("{}:\\", letter as char))
        .filter(|root| fs::metadata(root).is_ok())
        .map(|root| {
            let name = root.trim_end_matches('\\').to_string();
            let network = net.iter().any(|d| d.eq_ignore_ascii_case(&name));
            Volume { name, path: PathBuf::from(root), network }
        })
        .collect()
}

#[cfg(not(windows))]
fn volumes() -> Vec<Volume> {
    let network = crate::mounts::network_mounts();
    let mut out = vec![Volume { name: "/".into(), path: PathBuf::from("/"), network: false }];
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
                let net = network.contains(&path);
                out.push(Volume { name: crate::display_name(&path), path, network: net });
            }
        }
    }
    // Network shares mounted elsewhere (say `~/nas`) are listed too.
    for path in network {
        if path.is_dir() && !out.iter().any(|v| v.path == path) {
            out.push(Volume { name: crate::display_name(&path), path, network: true });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WalkLimits, walk};

    #[cfg(unix)]
    #[test]
    fn execute_bit_is_read_without_following_links() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("assetcraft-fs-exec-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let (tool, text) = (dir.join("tool.sh"), dir.join("notes.txt"));
        fs::write(&tool, b"#!/bin/sh\n").unwrap();
        fs::write(&text, b"hi").unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        let real = RealFs;
        assert!(real.is_executable(&tool));
        assert!(!real.is_executable(&text));
        assert!(!real.is_executable(&dir), "a searchable folder is not a program");
        let _ = fs::remove_dir_all(&dir);
    }

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
        assert_eq!(summary.files, 37); // 24 images + PSD, DNG, HEIC, a PDF, 4 videos and 4 models (Stage 1; 3D viewer V0.1 added the textured cube)
        assert_eq!(summary.dirs, 11); // root + 10 folders
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
    fn read_head_is_bounded() {
        let png = library().join("props/scythe_transparent.png");
        let head = RealFs.read_head(&png, 8).unwrap();
        assert_eq!(head, b"\x89PNG\r\n\x1a\n");
        let broken = library().join("edge-cases/broken_not_an_image.jpg");
        assert_eq!(RealFs.read_head(&broken, 4096).unwrap(), b"this is not a jpeg");
        assert!(RealFs.read_head(&library(), 8).is_err());
        assert_eq!(RealFs.read_range(&broken, 8, 3).unwrap(), b"not");
        assert!(RealFs.read_range(&broken, 1_000, 3).unwrap().is_empty());
    }

    #[test]
    fn reparse_points_are_not_links_unless_the_os_says_symlink() {
        const REPARSE_POINT: u32 = 0x400;
        const DIRECTORY: u32 = 0x10;
        const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000; // OneDrive cloud-only placeholder
        // A OneDrive-redirected Pictures folder: a directory with the reparse-point attribute.
        assert_eq!(classify(false, true, false, REPARSE_POINT | DIRECTORY), EntryKind::Dir);
        // A Files-On-Demand placeholder file.
        assert_eq!(classify(false, false, true, REPARSE_POINT | RECALL_ON_DATA_ACCESS), EntryKind::File);
        // Symlinks and junctions (std reports both through is_symlink) stay links.
        assert_eq!(classify(true, false, false, REPARSE_POINT), EntryKind::Symlink);
        assert_eq!(classify(true, true, false, REPARSE_POINT | DIRECTORY), EntryKind::Symlink);
        assert_eq!(classify(false, false, false, 0), EntryKind::Other);
    }

    #[test]
    fn has_at_least_one_volume() {
        assert!(!RealFs.volumes().is_empty());
    }
}
