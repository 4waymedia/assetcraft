//! Finder's sidebar Favorites, read by the app on the user's Mac (docs/design/folders-panel.md
//! F2; Paul, 2026-10-27: "it should be the app being able to access or read it... every user that
//! installs the app will have unique folders"). Nothing is built in: each user's own file is read.
//!
//! The file is `~/Library/Application Support/com.apple.sharedfilelist/
//! com.apple.LSSharedFileList.FavoriteItems.sfl3` (`.sfl2` before macOS 14): a binary plist
//! written by `NSKeyedArchiver`, whose root holds `items`, each with a `Bookmark` (macOS bookmark
//! data). A bookmark is `book` + header, then a table of contents whose key `0x1004` is the path,
//! as an array of strings (one per folder from `/`).
//!
//! Everything here is hostile input and pure (no I/O): the plist is size-capped by the caller,
//! object references are followed to a fixed depth, and the bookmark parser bounds-checks every
//! offset. Anything unexpected is skipped; a file we can't read gives an empty list, and the
//! Favorites tab shows the standard folders instead.

use crate::SidebarItem;
use plist::Value;
use std::path::PathBuf;

/// Largest sidebar file read.
pub const MAX_FILE_BYTES: u64 = 4 << 20;
/// Most favorites read.
pub const MAX_ITEMS: usize = 200;
/// Deepest chain of archive references followed.
const MAX_DEPTH: usize = 16;
/// Bookmark key for the path components.
const KEY_PATH: u32 = 0x1004;
/// Bookmark item types.
const TYPE_STRING: u32 = 0x0101;
const TYPE_ARRAY: u32 = 0x0601;

/// The favorites in a Finder `.sfl2` / `.sfl3` file, in Finder's order. Items without a usable
/// file bookmark (AirDrop, iCloud, network browse entries) are skipped.
pub fn parse_sfl(bytes: &[u8]) -> Vec<SidebarItem> {
    let Ok(root) = Value::from_reader(std::io::Cursor::new(bytes)) else { return Vec::new() };
    let Some(archive) = root.as_dictionary() else { return Vec::new() };
    let Some(objects) = archive.get("$objects").and_then(Value::as_array) else { return Vec::new() };
    let resolve = |v: &Value| -> Option<Value> { resolve(objects, v, 0) };
    let Some(top) = archive.get("$top").and_then(Value::as_dictionary) else { return Vec::new() };
    let Some(list) = top.get("root").and_then(&resolve) else { return Vec::new() };
    let Some(items) = list.as_dictionary().and_then(|d| d.get("items")).and_then(&resolve) else {
        return Vec::new();
    };
    let Some(items) = ns_array(objects, &items) else { return Vec::new() };
    let mut out: Vec<SidebarItem> = Vec::new();
    for item in items.iter().take(MAX_ITEMS) {
        let Some(item) = resolve(item) else { continue };
        let Some(dict) = item.as_dictionary() else { continue };
        let Some(bookmark) = dict.get("Bookmark").and_then(&resolve).and_then(|b| ns_data(objects, &b)) else {
            continue;
        };
        let Some(path) = bookmark_path(&bookmark) else { continue };
        // Finder shows the folder's own name; a custom one is kept in `Name` when it differs.
        let name = dict.get("Name").and_then(&resolve).and_then(|n| n.as_string().map(str::to_owned));
        let name = name.filter(|n| {
            !n.is_empty() && n.len() <= 255 && Some(n.as_str()) != path.file_name().and_then(|f| f.to_str())
        });
        if !out.iter().any(|o| o.path == path) {
            out.push(SidebarItem { path, name });
        }
    }
    out
}

/// Follow `NSKeyedArchiver` references (`Uid` → `$objects[i]`), at most [`MAX_DEPTH`] deep.
fn resolve(objects: &[Value], v: &Value, depth: usize) -> Option<Value> {
    if depth > MAX_DEPTH {
        return None;
    }
    match v {
        Value::Uid(uid) => {
            let i = usize::try_from(uid.get()).ok()?;
            resolve(objects, objects.get(i)?, depth + 1)
        }
        other => Some(other.clone()),
    }
}

/// An `NSArray` (`{NS.objects: [...]}`) or a plain array.
fn ns_array(objects: &[Value], v: &Value) -> Option<Vec<Value>> {
    match v {
        Value::Array(a) => Some(a.clone()),
        Value::Dictionary(d) => d.get("NS.objects").and_then(|o| resolve(objects, o, 0)).and_then(|o| o.into_array()),
        _ => None,
    }
}

/// `NSData` (`{NS.data: <bytes>}`) or plain data.
fn ns_data(objects: &[Value], v: &Value) -> Option<Vec<u8>> {
    match v {
        Value::Data(d) => Some(d.clone()),
        Value::Dictionary(d) => d.get("NS.data").and_then(|o| resolve(objects, o, 0)).and_then(|o| o.into_data()),
        _ => None,
    }
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

/// The path a macOS bookmark points at, from its path components (`0x1004`). `None` for anything
/// that isn't a well-formed bookmark of an absolute path.
pub fn bookmark_path(b: &[u8]) -> Option<PathBuf> {
    if b.get(0..4)? != b"book" {
        return None;
    }
    let header = usize::try_from(u32_at(b, 12)?).ok()?;
    let data = b.get(header..)?;
    // The data area starts with the offset of the first table of contents.
    let mut toc = usize::try_from(u32_at(data, 0)?).ok()?;
    let mut tocs_seen = 0;
    while toc != 0 && tocs_seen < 16 {
        tocs_seen += 1;
        // TOC: length, magic 0xfffffffe, identifier, next TOC, count, then (key, offset, reserved).
        if u32_at(data, toc + 4)? != 0xffff_fffe {
            return None;
        }
        let next = usize::try_from(u32_at(data, toc + 12)?).ok()?;
        let count = usize::try_from(u32_at(data, toc + 16)?).ok()?.min(1024);
        for e in 0..count {
            let at = toc + 20 + e * 12;
            if u32_at(data, at)? == KEY_PATH {
                let offset = usize::try_from(u32_at(data, at + 4)?).ok()?;
                return path_from_components(data, offset);
            }
        }
        toc = next;
    }
    None
}

/// An item: `length, type`, then `length` bytes.
fn item(data: &[u8], offset: usize) -> Option<(u32, &[u8])> {
    let len = usize::try_from(u32_at(data, offset)?).ok()?;
    let kind = u32_at(data, offset + 4)?;
    let body = data.get(offset.checked_add(8)?..offset.checked_add(8)?.checked_add(len)?)?;
    Some((kind, body))
}

fn path_from_components(data: &[u8], offset: usize) -> Option<PathBuf> {
    let (kind, body) = item(data, offset)?;
    if kind != TYPE_ARRAY || !body.len().is_multiple_of(4) || body.len() / 4 > 256 {
        return None;
    }
    let mut path = PathBuf::from("/");
    for i in 0..body.len() / 4 {
        let at = usize::try_from(u32_at(body, i * 4)?).ok()?;
        let (kind, s) = item(data, at)?;
        let part = std::str::from_utf8(s).ok().filter(|_| kind == TYPE_STRING)?;
        if part.is_empty() || part == "." || part == ".." || part.contains('/') {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

/// Builders for tests: a bookmark with a path, and an archive of items, in the formats above.
#[cfg(test)]
pub(crate) mod build {
    use plist::{Dictionary, Uid, Value};

    /// A minimal bookmark of `path` (header 48 bytes; one TOC with key `0x1004`).
    pub(crate) fn bookmark(path: &str) -> Vec<u8> {
        let mut data: Vec<u8> = vec![0; 4]; // first-TOC offset, patched below
        let mut item_offsets = Vec::new();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            item_offsets.push(data.len() as u32);
            data.extend((part.len() as u32).to_le_bytes());
            data.extend(super::TYPE_STRING.to_le_bytes());
            data.extend(part.as_bytes());
            while !data.len().is_multiple_of(4) {
                data.push(0);
            }
        }
        let array_at = data.len() as u32;
        data.extend((item_offsets.len() as u32 * 4).to_le_bytes());
        data.extend(super::TYPE_ARRAY.to_le_bytes());
        for o in &item_offsets {
            data.extend(o.to_le_bytes());
        }
        let toc_at = data.len() as u32;
        // A decoy entry first, then the path.
        let entries: [(u32, u32); 2] = [(0x1005, array_at), (super::KEY_PATH, array_at)];
        data.extend((20 + 12 * entries.len() as u32 - 8).to_le_bytes());
        data.extend(0xffff_fffeu32.to_le_bytes());
        data.extend(1u32.to_le_bytes());
        data.extend(0u32.to_le_bytes());
        data.extend((entries.len() as u32).to_le_bytes());
        for (k, o) in entries {
            data.extend(k.to_le_bytes());
            data.extend(o.to_le_bytes());
            data.extend(0u32.to_le_bytes());
        }
        data[0..4].copy_from_slice(&toc_at.to_le_bytes());
        let mut b = b"book".to_vec();
        b.extend(((48 + data.len()) as u32).to_le_bytes());
        b.extend(0x1004_0000u32.to_le_bytes());
        b.extend(48u32.to_le_bytes());
        b.resize(48, 0);
        b.extend(data);
        b
    }

    /// An `NSKeyedArchiver` binary plist like Finder's: root → items (an NSArray) → item dicts with
    /// a `Bookmark` (NSData) and an optional `Name`.
    pub(crate) fn sfl(items: &[(Option<&str>, Vec<u8>)]) -> Vec<u8> {
        let mut objects: Vec<Value> = vec![Value::String("$null".into())];
        let mut item_uids = Vec::new();
        for (name, bookmark) in items {
            let data_at = objects.len() as u64;
            objects.push(Value::Data(bookmark.clone()));
            let mut d = Dictionary::new();
            d.insert("Bookmark".into(), Value::Uid(Uid::new(data_at)));
            if let Some(n) = name {
                let n_at = objects.len() as u64;
                objects.push(Value::String((*n).into()));
                d.insert("Name".into(), Value::Uid(Uid::new(n_at)));
            }
            d.insert("visibility".into(), Value::Integer(0.into()));
            item_uids.push(Value::Uid(Uid::new(objects.len() as u64)));
            objects.push(Value::Dictionary(d));
        }
        let mut arr = Dictionary::new();
        arr.insert("NS.objects".into(), Value::Array(item_uids));
        let arr_at = objects.len() as u64;
        objects.push(Value::Dictionary(arr));
        let mut root = Dictionary::new();
        root.insert("items".into(), Value::Uid(Uid::new(arr_at)));
        let root_at = objects.len() as u64;
        objects.push(Value::Dictionary(root));
        let mut top = Dictionary::new();
        top.insert("root".into(), Value::Uid(Uid::new(root_at)));
        let mut archive = Dictionary::new();
        archive.insert("$archiver".into(), Value::String("NSKeyedArchiver".into()));
        archive.insert("$version".into(), Value::Integer(100000.into()));
        archive.insert("$objects".into(), Value::Array(objects));
        archive.insert("$top".into(), Value::Dictionary(top));
        let mut out = Vec::new();
        let _ = Value::Dictionary(archive).to_writer_binary(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_users_own_favorites_in_finders_order() {
        let file = build::sfl(&[
            (None, build::bookmark("/Users/ana/Desktop")),
            (None, build::bookmark("/Applications")),
            (Some("Client work"), build::bookmark("/Users/ana/Projects/Acme Co")),
            (None, b"not a bookmark".to_vec()),
            (None, build::bookmark("/Users/ana/Desktop")),
            (Some("Artcraft"), build::bookmark("/Users/ana/Documents/Artcraft")),
        ]);
        let items = parse_sfl(&file);
        assert_eq!(
            items,
            [
                SidebarItem { path: "/Users/ana/Desktop".into(), name: None },
                SidebarItem { path: "/Applications".into(), name: None },
                SidebarItem { path: "/Users/ana/Projects/Acme Co".into(), name: Some("Client work".into()) },
                SidebarItem { path: "/Users/ana/Documents/Artcraft".into(), name: None },
            ],
            "bad and repeated items skipped; a Name equal to the folder's own name is dropped"
        );
    }

    #[test]
    fn hostile_files_and_bookmarks_give_nothing_never_a_panic() {
        assert!(parse_sfl(b"").is_empty());
        assert!(parse_sfl(b"bplist00 garbage").is_empty());
        assert!(parse_sfl(&build::sfl(&[])).is_empty());
        let good = build::bookmark("/Users/ana/Art");
        assert_eq!(bookmark_path(&good), Some(PathBuf::from("/Users/ana/Art")));
        // Every truncation, and every single-byte corruption, is handled.
        for n in 0..good.len() {
            let _ = bookmark_path(&good[..n]);
        }
        for i in 0..good.len() {
            let mut bad = good.clone();
            bad[i] ^= 0xff;
            if let Some(p) = bookmark_path(&bad) {
                assert!(p.is_absolute() && !p.components().any(|c| c.as_os_str() == ".."));
            }
        }
        assert_eq!(bookmark_path(&build::bookmark("/a/../etc")), None, "no `..` in a sidebar path");
        // A reference loop in the archive ends.
        let mut objects = vec![plist::Value::Uid(plist::Uid::new(1)), plist::Value::Uid(plist::Uid::new(0))];
        assert!(resolve(&objects, &objects[0].clone(), 0).is_none());
        objects.clear();
    }
}
