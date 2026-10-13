//! Bounded, symlink-safe recursive walk. Never call it on the UI thread.

use crate::{EntryKind, ListOptions, Services};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Limits for [`walk`]. The defaults suit a "count what's in this folder" pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkLimits {
    /// Deepest folder level visited below the root (root is depth 0).
    pub max_depth: usize,
    /// Stop after this many entries in total (files + folders + others).
    pub max_entries: usize,
    /// Include hidden entries.
    pub include_hidden: bool,
}

impl Default for WalkLimits {
    fn default() -> Self {
        Self { max_depth: 32, max_entries: 1_000_000, include_hidden: false }
    }
}

/// What a walk found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalkSummary {
    /// Folders visited, including the root.
    pub dirs: usize,
    /// Regular files seen.
    pub files: usize,
    /// Symlinks seen (never followed).
    pub symlinks: usize,
    /// Total bytes in regular files.
    pub bytes: u64,
    /// Folders that could not be listed, plus entries skipped inside listings.
    pub unreadable: usize,
    /// `true` if a limit stopped the walk early.
    pub truncated: bool,
    /// Every file path seen, in visit order.
    pub file_paths: Vec<PathBuf>,
}

/// Breadth-first walk from `root`, bounded by `limits`. Symlinks are counted but never followed,
/// so loops are impossible. Unreadable folders are counted, not fatal.
pub fn walk(services: &dyn Services, root: &Path, limits: &WalkLimits) -> WalkSummary {
    let mut summary = WalkSummary::default();
    let mut queue = VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut seen = 0usize;
    let opts = ListOptions { include_hidden: limits.include_hidden, max_entries: limits.max_entries };
    while let Some((dir, depth)) = queue.pop_front() {
        let listing = match services.list_entries(&dir, &opts) {
            Ok(listing) => listing,
            Err(_) => {
                summary.unreadable += 1;
                continue;
            }
        };
        summary.dirs += 1;
        summary.unreadable += listing.skipped;
        summary.truncated |= listing.truncated;
        for entry in listing.entries {
            seen += 1;
            if seen > limits.max_entries {
                summary.truncated = true;
                return summary;
            }
            match entry.kind {
                EntryKind::Dir if depth < limits.max_depth => queue.push_back((entry.path, depth + 1)),
                EntryKind::Dir => summary.truncated = true,
                EntryKind::File => {
                    summary.files += 1;
                    summary.bytes = summary.bytes.saturating_add(entry.size);
                    summary.file_paths.push(entry.path);
                }
                EntryKind::Symlink => summary.symlinks += 1,
                EntryKind::Other => {}
            }
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeFs;

    fn tree() -> FakeFs {
        FakeFs::new()
            .file("/r/a.png", 10)
            .file("/r/sub/b.png", 20)
            .file("/r/sub/deeper/c.png", 30)
            .symlink("/r/loop", "/r")
            .unreadable_dir("/r/locked")
    }

    #[test]
    fn walks_everything_and_never_follows_symlinks() {
        let s = walk(&tree(), Path::new("/r"), &WalkLimits::default());
        assert_eq!((s.files, s.symlinks, s.bytes), (3, 1, 60));
        assert_eq!(s.dirs, 3); // r, sub, deeper; locked is unreadable
        assert_eq!(s.unreadable, 1);
        assert!(!s.truncated);
    }

    #[test]
    fn depth_limit_truncates() {
        let s = walk(&tree(), Path::new("/r"), &WalkLimits { max_depth: 1, ..WalkLimits::default() });
        assert_eq!(s.files, 2);
        assert!(s.truncated);
    }

    #[test]
    fn entry_limit_truncates() {
        let s = walk(&tree(), Path::new("/r"), &WalkLimits { max_entries: 2, ..WalkLimits::default() });
        assert!(s.truncated);
        assert!(s.files <= 2);
    }

    #[test]
    fn missing_root_is_unreadable_not_a_crash() {
        let s = walk(&tree(), Path::new("/nope"), &WalkLimits::default());
        assert_eq!((s.dirs, s.unreadable), (0, 1));
    }
}
