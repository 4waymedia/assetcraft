//! The OS's own sidebar folders (Folders panel → System → Favorites; docs/design/folders-panel.md
//! FP-2). Linux reads GTK bookmarks (F1); macOS Finder Favorites (F2) and Windows Quick Access
//! pins (F3) come next and return nothing until then, so the panel shows the standard list.
//! macOS: Finder's Favorites (F2, `crate::finder`).
//!
//! Bookmark files are hostile input: size-capped, `file://` URIs only, percent-decoded, absolute
//! paths that exist as folders, at most [`MAX_ITEMS`].

use crate::SidebarItem;
use std::path::{Path, PathBuf};

/// Most sidebar folders read.
pub const MAX_ITEMS: usize = 100;
/// Largest bookmarks file read.
const MAX_BYTES: u64 = 256 * 1024;

/// GTK 3 bookmarks: one `file:///path/with%20escapes Optional Label` per line.
pub fn parse_gtk_bookmarks(text: &str) -> Vec<SidebarItem> {
    text.lines()
        .filter_map(|line| {
            let (uri, label) = match line.trim().split_once(' ') {
                Some((u, l)) => (u, Some(l.trim()).filter(|l| !l.is_empty())),
                None => (line.trim(), None),
            };
            let raw = uri.strip_prefix("file://")?;
            let path = percent_decode(raw)?;
            path.starts_with('/').then(|| SidebarItem { path: PathBuf::from(path), name: label.map(str::to_owned) })
        })
        .take(MAX_ITEMS)
        .collect()
}

/// `%20` → space; `None` for bad escapes or non-UTF-8 results.
fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Finder's Favorites for the user whose home is `home` (macOS 14+ writes `.sfl3`, older `.sfl2`).
/// Empty when macOS refuses access or the file can't be used.
pub fn finder_favorites(home: &Path) -> Vec<SidebarItem> {
    let dir = home.join("Library").join("Application Support").join("com.apple.sharedfilelist");
    for name in ["com.apple.LSSharedFileList.FavoriteItems.sfl3", "com.apple.LSSharedFileList.FavoriteItems.sfl2"] {
        let file = dir.join(name);
        let ok = std::fs::metadata(&file).is_ok_and(|m| m.is_file() && m.len() <= crate::finder::MAX_FILE_BYTES);
        if let Some(bytes) = ok.then(|| std::fs::read(&file).ok()).flatten() {
            let items = crate::finder::parse_sfl(&bytes);
            if !items.is_empty() {
                return items;
            }
        }
    }
    Vec::new()
}

/// This machine's sidebar folders that exist (empty when there's no readable sidebar).
pub fn read(home: Option<&Path>) -> Vec<SidebarItem> {
    let items = if cfg!(target_os = "macos") {
        home.map(finder_favorites).unwrap_or_default()
    } else if cfg!(windows) {
        Vec::new()
    } else {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".config")));
        config
            .map(|c| c.join("gtk-3.0").join("bookmarks"))
            .filter(|f| std::fs::metadata(f).is_ok_and(|m| m.is_file() && m.len() <= MAX_BYTES))
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|t| parse_gtk_bookmarks(&t))
            .unwrap_or_default()
    };
    items.into_iter().filter(|i| i.path.is_dir()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtk_bookmarks_parse_as_hostile_input() {
        let text = "file:///home/paul/Art%20Work Art\n\
                    file:///home/paul/Music\n\
                    sftp://box/home/paul remote\n\
                    file:///bad%zzescape\n\
                    file://relative/path\n\
                    \n";
        let items = parse_gtk_bookmarks(text);
        assert_eq!(
            items,
            [
                SidebarItem { path: "/home/paul/Art Work".into(), name: Some("Art".into()) },
                SidebarItem { path: "/home/paul/Music".into(), name: None },
            ]
        );
        let many = "file:///x\n".repeat(500);
        assert_eq!(parse_gtk_bookmarks(&many).len(), MAX_ITEMS);
    }
}
