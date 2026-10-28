//! [`FakeFs`]: an in-memory file system for tests. Paths are plain `Path`s; parents are created
//! automatically. Build it with the chained helpers:
//!
//! ```
//! use assetcraft_fs::{FakeFs, ListOptions, Services};
//! use std::path::Path;
//!
//! let fs = FakeFs::new().file("/shots/010.png", 1200).dir("/empty").hidden("/shots/.cache", 0);
//! let listing = fs.list_entries(Path::new("/shots"), &ListOptions::default()).unwrap_or_default();
//! assert_eq!(listing.entries.len(), 1); // the hidden file is filtered out
//! ```

use crate::{Entry, EntryKind, FsError, KnownFolder, ListOptions, Listing, Services, SidebarItem, Volume};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
struct Node {
    kind: EntryKind,
    size: u64,
    hidden: bool,
    modified: SystemTime,
    unreadable: bool,
    executable: bool,
    data: Option<Arc<Vec<u8>>>,
}

/// In-memory file system. Cheap to clone; immutable once built.
#[derive(Debug, Clone, Default)]
pub struct FakeFs {
    nodes: BTreeMap<PathBuf, Node>,
    known: HashMap<KnownFolder, PathBuf>,
    volumes: Vec<Volume>,
    sidebar: Vec<SidebarItem>,
    clock: u64,
}

impl FakeFs {
    /// An empty file system with a `/` volume.
    pub fn new() -> Self {
        let mut fs = Self {
            volumes: vec![Volume { name: "/".into(), path: PathBuf::from("/"), network: false }],
            ..Self::default()
        };
        fs.insert(Path::new("/"), EntryKind::Dir, 0, false, false);
        fs
    }

    /// Add a folder (and its parents).
    pub fn dir(mut self, path: impl AsRef<Path>) -> Self {
        self.insert(path.as_ref(), EntryKind::Dir, 0, false, false);
        self
    }

    /// Add a folder that exists but can't be listed (permission denied).
    pub fn unreadable_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.insert(path.as_ref(), EntryKind::Dir, 0, false, true);
        self
    }

    /// Add a file of `size` bytes (and its parent folders).
    pub fn file(mut self, path: impl AsRef<Path>, size: u64) -> Self {
        self.insert(path.as_ref(), EntryKind::File, size, false, false);
        self
    }

    /// Add a file with real contents (its size is the content length).
    pub fn file_with(mut self, path: impl AsRef<Path>, bytes: impl Into<Vec<u8>>) -> Self {
        let bytes: Vec<u8> = bytes.into();
        let path = path.as_ref();
        self.insert(path, EntryKind::File, bytes.len() as u64, false, false);
        if let Some(node) = self.nodes.get_mut(path) {
            node.data = Some(Arc::new(bytes));
        }
        self
    }

    /// Add a file with its execute bit set (as on Unix).
    pub fn executable(mut self, path: impl AsRef<Path>, size: u64) -> Self {
        let path = path.as_ref();
        self.insert(path, EntryKind::File, size, false, false);
        if let Some(node) = self.nodes.get_mut(path) {
            node.executable = true;
        }
        self
    }

    /// Add a hidden file.
    pub fn hidden(mut self, path: impl AsRef<Path>, size: u64) -> Self {
        self.insert(path.as_ref(), EntryKind::File, size, true, false);
        self
    }

    /// Add a symlink. The target is recorded nowhere: symlinks are never followed.
    pub fn symlink(mut self, path: impl AsRef<Path>, _target: impl AsRef<Path>) -> Self {
        self.insert(path.as_ref(), EntryKind::Symlink, 0, false, false);
        self
    }

    /// Register a known folder (creating it).
    pub fn known(mut self, which: KnownFolder, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        self = self.dir(&path);
        self.known.insert(which, path);
        self
    }

    /// Add a volume root (creating it).
    pub fn volume(mut self, name: &str, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        self = self.dir(&path);
        self.volumes.push(Volume { name: name.into(), path, network: false });
        self
    }

    /// Add a network share's root (creating it).
    pub fn network_volume(mut self, name: &str, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        self = self.dir(&path);
        self.volumes.push(Volume { name: name.into(), path, network: true });
        self
    }

    /// Add a folder to the fake OS sidebar (creating it).
    pub fn sidebar(mut self, path: impl AsRef<Path>, name: Option<&str>) -> Self {
        let path = path.as_ref().to_path_buf();
        self = self.dir(&path);
        self.sidebar.push(SidebarItem { path, name: name.map(str::to_owned) });
        self
    }

    fn insert(&mut self, path: &Path, kind: EntryKind, size: u64, hidden: bool, unreadable: bool) {
        for ancestor in path.ancestors().skip(1) {
            if !self.nodes.contains_key(ancestor) {
                let node = self.node(EntryKind::Dir, 0, false, false);
                self.nodes.insert(ancestor.to_path_buf(), node);
            }
        }
        let node = self.node(kind, size, hidden, unreadable);
        self.nodes.insert(path.to_path_buf(), node);
    }

    /// Each node gets a distinct, increasing modification time so date sorts are testable.
    fn node(&mut self, kind: EntryKind, size: u64, hidden: bool, unreadable: bool) -> Node {
        self.clock += 1;
        let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000 + self.clock * 60);
        Node { kind, size, hidden, modified, unreadable, executable: false, data: None }
    }

    fn to_entry(path: &Path, node: &Node) -> Entry {
        Entry {
            name: crate::display_name(path),
            path: path.to_path_buf(),
            kind: node.kind,
            size: node.size,
            modified: Some(node.modified),
            // A fixed hour before the modification time, so "Created" is testable and distinct.
            created: node.modified.checked_sub(Duration::from_secs(3600)),
            hidden: node.hidden,
        }
    }
}

impl Services for FakeFs {
    fn list_entries(&self, dir: &Path, opts: &ListOptions) -> Result<Listing, FsError> {
        let node = self.nodes.get(dir).ok_or_else(|| FsError::NotFound(dir.to_path_buf()))?;
        if node.kind != EntryKind::Dir {
            return Err(FsError::NotADirectory(dir.to_path_buf()));
        }
        if node.unreadable {
            return Err(FsError::PermissionDenied(dir.to_path_buf()));
        }
        let children: BTreeSet<&PathBuf> =
            self.nodes.keys().filter(|p| p.parent() == Some(dir) && p.as_path() != dir).collect();
        let mut listing = Listing::default();
        for path in children {
            let Some(child) = self.nodes.get(path) else { continue };
            if child.hidden && !opts.include_hidden {
                continue;
            }
            if listing.entries.len() >= opts.max_entries {
                listing.truncated = true;
                break;
            }
            listing.entries.push(Self::to_entry(path, child));
        }
        Ok(listing)
    }

    fn entry(&self, path: &Path) -> Result<Entry, FsError> {
        self.nodes.get(path).map(|n| Self::to_entry(path, n)).ok_or_else(|| FsError::NotFound(path.to_path_buf()))
    }

    fn read_head(&self, path: &Path, max: usize) -> Result<Vec<u8>, FsError> {
        let node = self.nodes.get(path).ok_or_else(|| FsError::NotFound(path.to_path_buf()))?;
        if node.kind != EntryKind::File {
            return Err(FsError::Io { path: path.to_path_buf(), message: "not a file".into() });
        }
        // Files added without contents read as zeros, up to their declared size.
        let len = usize::try_from(node.size).unwrap_or(usize::MAX).min(max);
        Ok(match &node.data {
            Some(data) => data.iter().take(max).copied().collect(),
            None => vec![0; len],
        })
    }

    fn read_range(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        let node = self.nodes.get(path).ok_or_else(|| FsError::NotFound(path.to_path_buf()))?;
        if node.kind != EntryKind::File {
            return Err(FsError::Io { path: path.to_path_buf(), message: "not a file".into() });
        }
        let size = usize::try_from(node.size).unwrap_or(usize::MAX);
        let start = usize::try_from(offset).unwrap_or(usize::MAX).min(size);
        let end = start.saturating_add(len).min(size);
        // Slice instead of copying the whole file: probes hop through large files in small reads.
        Ok(match &node.data {
            Some(data) => data.get(start..end.min(data.len())).map(<[u8]>::to_vec).unwrap_or_default(),
            None => vec![0; end - start],
        })
    }

    fn is_executable(&self, path: &Path) -> bool {
        self.nodes.get(path).is_some_and(|n| n.kind == EntryKind::File && n.executable)
    }

    fn volumes(&self) -> Vec<Volume> {
        self.volumes.clone()
    }

    fn sidebar(&self) -> Vec<SidebarItem> {
        self.sidebar.clone()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.known.get(&KnownFolder::Home).cloned()
    }

    fn known_folder(&self, which: KnownFolder) -> Option<PathBuf> {
        self.known.get(&which).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents_are_created_and_listed() {
        let fs = FakeFs::new().file("/a/b/c.png", 5);
        let root = fs.list_entries(Path::new("/"), &ListOptions::default()).unwrap();
        assert_eq!(root.entries.len(), 1);
        assert_eq!(root.entries[0].kind, EntryKind::Dir);
        let b = fs.list_entries(Path::new("/a/b"), &ListOptions::default()).unwrap();
        assert_eq!(b.entries[0].name, "c.png");
        assert_eq!(b.entries[0].size, 5);
    }

    #[test]
    fn hidden_entries_follow_the_option() {
        let fs = FakeFs::new().file("/d/x", 1).hidden("/d/.y", 1);
        let shown = fs.list_entries(Path::new("/d"), &ListOptions::default()).unwrap();
        assert_eq!(shown.entries.len(), 1);
        let all =
            fs.list_entries(Path::new("/d"), &ListOptions { include_hidden: true, ..ListOptions::default() }).unwrap();
        assert_eq!(all.entries.len(), 2);
    }

    #[test]
    fn errors_match_the_real_fs() {
        let fs = FakeFs::new().file("/f", 1).unreadable_dir("/locked");
        let o = ListOptions::default();
        assert!(matches!(fs.list_entries(Path::new("/nope"), &o), Err(FsError::NotFound(_))));
        assert!(matches!(fs.list_entries(Path::new("/f"), &o), Err(FsError::NotADirectory(_))));
        assert!(matches!(fs.list_entries(Path::new("/locked"), &o), Err(FsError::PermissionDenied(_))));
    }

    #[test]
    fn known_folders_and_volumes() {
        let fs = FakeFs::new()
            .known(KnownFolder::Home, "/home/paul")
            .known(KnownFolder::Pictures, "/home/paul/Pictures")
            .volume("F:", "/f");
        assert_eq!(fs.home_dir().as_deref(), Some(Path::new("/home/paul")));
        assert_eq!(fs.known_folder(KnownFolder::Pictures).as_deref(), Some(Path::new("/home/paul/Pictures")));
        assert_eq!(fs.known_folder(KnownFolder::Videos), None);
        assert_eq!(fs.volumes().len(), 2);
    }

    #[test]
    fn execute_bits_are_per_file() {
        let fs = FakeFs::new().executable("/bin/tool", 10).file("/bin/readme", 1);
        assert!(fs.is_executable(Path::new("/bin/tool")));
        assert!(!fs.is_executable(Path::new("/bin/readme")));
        assert!(!fs.is_executable(Path::new("/bin")), "folders are never programs");
        assert!(!fs.is_executable(Path::new("/missing")));
    }

    #[test]
    fn read_head_returns_contents_or_zeros() {
        let fs = FakeFs::new().file_with("/a.png", b"\x89PNG\r\n\x1a\nrest".to_vec()).file("/z", 3).dir("/d");
        assert_eq!(fs.read_head(Path::new("/a.png"), 4).unwrap(), b"\x89PNG");
        assert_eq!(fs.entry(Path::new("/a.png")).unwrap().size, 12);
        assert_eq!(fs.read_head(Path::new("/z"), 99).unwrap(), vec![0, 0, 0]);
        assert!(fs.read_head(Path::new("/d"), 4).is_err());
        assert!(matches!(fs.read_head(Path::new("/nope"), 4), Err(FsError::NotFound(_))));
        assert_eq!(fs.read_range(Path::new("/a.png"), 8, 99).unwrap(), b"rest");
        assert!(fs.read_range(Path::new("/a.png"), 99, 4).unwrap().is_empty());
    }

    #[test]
    fn modification_times_increase() {
        let fs = FakeFs::new().file("/a", 1).file("/b", 1);
        let a = fs.entry(Path::new("/a")).unwrap().modified;
        let b = fs.entry(Path::new("/b")).unwrap().modified;
        assert!(a < b);
    }
}
