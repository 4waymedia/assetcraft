//! Rendered thumbnails on disk: `<dir>/<2 hex>/<32 hex>.png`, bounded in bytes.
//!
//! **Key** ([`cache_key`]): SipHash-1-3-128 (fixed keys, see [`crate::hash`]) over
//! [`RENDERER_VERSION`], the thumbnail side bucket, the file size, the modification time, the
//! first [`HASH_WINDOW`] bytes and the last [`HASH_WINDOW`] bytes of the file. At most 128 KiB
//! is read per key, so a 1.4 MB PNG and a 2 GB TIFF cost the same.
//!
//! *Trade-off:* an edit that changes only the middle of a file and keeps its size *and* its
//! modification time (a tool that restores the mtime after an in-place edit) is not seen, and
//! the stale thumbnail stays until the cache is cleared. Normal saves change the mtime; most
//! format edits change the size or the header/trailer too. Identical copies with the same mtime
//! share one cache file, which is fine.
//!
//! **Files** are written to a temp file in the same shard folder and renamed into place, so a
//! reader never sees a partial file; they are not fsynced (the cache is disposable). A missing,
//! unreadable or malformed cache file is a miss: it is deleted and the thumbnail re-rendered.
//! Source dimensions ride in a PNG `tEXt` chunk.
//!
//! **Budget**: when the total passes the budget ([`DEFAULT_DISK_BUDGET`], 1 GiB), the least
//! recently used files (by modification time, refreshed on every hit) are deleted down to 80 %
//! on a background thread ([`DiskCache::prune_in_background`]); worker threads only note that
//! the budget was crossed.
//!
//! **Own files only**: the cache lists, counts and deletes only names of its own shape
//! ([`is_cache_file`]: `<2 hex>/<32 hex>.png` and its `<32 hex>.tmp…` temp files) directly
//! inside its own folder, and never follows links. Anything else in the folder is left alone.
//!
//! This is AssetCraft's own data, so writing it is allowed before Stage 2 (AGENTS.md rule 5
//! covers the user's files). Cache files go through `std::fs`, not `Services`: `Services` is
//! the read-only view of the user's files.
//!
//! Adapted from LightCraft `crates/preview/src/disk.rs` (see NOTICE).

use crate::Thumb;
use crate::hash::{Hash128, Hasher128};
use assetcraft_fs::{FsError, Services};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

/// Bump when any decoder's output changes, so older cache files are never shown.
/// 2: PSD, camera raw, HEIF (sibling crates), PDF (pdfcraft-render), video poster frames
/// (filmcraft-cli) and GLB/glTF/OBJ models (assetcraft-mesh) added.
pub const RENDERER_VERSION: u64 = 2;

/// Bytes hashed from each end of the file.
pub const HASH_WINDOW: usize = 64 * 1024;

/// Default disk budget: 1 GiB.
pub const DEFAULT_DISK_BUDGET: u64 = 1024 * 1024 * 1024;

/// Cache files larger than this are treated as corrupt (a 1024-px RGBA PNG is at most ~4 MiB).
const MAX_CACHE_FILE: u64 = 16 * 1024 * 1024;

/// Text chunk keyword holding `"<source width>x<source height>"`.
const SOURCE_KEYWORD: &str = "assetcraft-source";

/// The cache key for `path` at `max_side`: see the module docs. Reads at most 128 KiB.
pub fn cache_key(services: &dyn Services, path: &Path, max_side: u32) -> Result<Hash128, FsError> {
    cache_key_and_head(services, path, max_side).map(|(key, _)| key)
}

/// [`cache_key`] plus the file's first bytes it read (the worker sniffs the format from them
/// to decide whether the disk cache may be consulted at all).
pub(crate) fn cache_key_and_head(
    services: &dyn Services,
    path: &Path,
    max_side: u32,
) -> Result<(Hash128, Vec<u8>), FsError> {
    let entry = services.entry(path)?;
    let head = services.read_head(path, HASH_WINDOW)?;
    let window = HASH_WINDOW as u64;
    let tail = if entry.size > window {
        // The tail never overlaps the head: for files under 128 KiB it is what follows it.
        let start = entry.size.saturating_sub(window).max(window);
        let len = usize::try_from(entry.size - start).unwrap_or(HASH_WINDOW);
        services.read_range(path, start, len)?
    } else {
        Vec::new()
    };
    let key = key_from_parts(max_side, entry.size, entry.modified, &head, &tail);
    Ok((key, head))
}

/// The key from already-read parts (pure; see [`cache_key`]).
pub fn key_from_parts(max_side: u32, size: u64, modified: Option<SystemTime>, head: &[u8], tail: &[u8]) -> Hash128 {
    let (tag, secs, nanos) = match modified.map(|t| t.duration_since(UNIX_EPOCH)) {
        None => (0, 0, 0),
        Some(Ok(d)) => (1, d.as_secs(), d.subsec_nanos()),
        Some(Err(before)) => (2, before.duration().as_secs(), before.duration().subsec_nanos()),
    };
    Hasher128::new()
        .bytes(b"assetcraft-thumb")
        .u64(RENDERER_VERSION)
        .u64(u64::from(max_side))
        .u64(size)
        .u64(tag)
        .u64(secs)
        .u64(u64::from(nanos))
        .bytes(head)
        .bytes(tail)
        .finish()
}

/// Thumbnails on disk. Share it as `Arc<DiskCache>` between the pool and the engine.
#[derive(Debug)]
pub struct DiskCache {
    dir: PathBuf,
    budget: AtomicU64,
    /// Bytes on disk (`None` until first scanned).
    total: Mutex<Option<u64>>,
    /// A background prune or clear is running.
    busy: AtomicBool,
    hits: AtomicU64,
    misses: AtomicU64,
    writes: AtomicU64,
}

impl DiskCache {
    /// A cache in `dir` (created on first write) holding at most about `budget` bytes.
    pub fn new(dir: impl Into<PathBuf>, budget: u64) -> Self {
        Self {
            dir: dir.into(),
            budget: AtomicU64::new(budget),
            total: Mutex::new(None),
            busy: AtomicBool::new(false),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            writes: AtomicU64::new(0),
        }
    }

    /// [`DiskCache::new`] behind an `Arc`, with the first size scan (and a prune, if a smaller
    /// budget is now in force) started on a background thread.
    pub fn open(dir: impl Into<PathBuf>, budget: u64) -> Arc<Self> {
        let cache = Arc::new(Self::new(dir, budget));
        cache.prune_in_background();
        cache
    }

    /// The cache folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Bytes the cache may hold.
    pub fn budget(&self) -> u64 {
        self.budget.load(Ordering::Relaxed)
    }

    /// Bytes on disk if already known; never scans (safe on the UI thread).
    pub fn size_known(&self) -> Option<u64> {
        *self.lock_total()
    }

    /// Bytes on disk, scanning the folder the first time. Blocking.
    pub fn size(&self) -> u64 {
        let mut total = self.lock_total();
        *total.get_or_insert_with(|| self.scan().iter().map(|f| f.1).sum())
    }

    /// `(hits, misses, writes)` since start.
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.hits.load(Ordering::Relaxed), self.misses.load(Ordering::Relaxed), self.writes.load(Ordering::Relaxed))
    }

    /// `true` while a background prune or clear runs.
    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }

    /// Where the thumbnail for `key` lives.
    pub fn path_for(&self, key: Hash128) -> PathBuf {
        let hex = key.to_string();
        self.dir.join(&hex[..2]).join(format!("{hex}.png"))
    }

    /// The cached thumbnail, if a valid one exists. A malformed file is deleted (a miss).
    pub fn get(&self, key: Hash128, max_side: u32) -> Option<Thumb> {
        let path = self.path_for(key);
        let found = std::fs::symlink_metadata(&path)
            .ok()
            .filter(|m| m.is_file() && m.len() <= MAX_CACHE_FILE)
            .and_then(|_| std::fs::read(&path).ok());
        let Some(bytes) = found else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        match decode(&bytes).filter(|t| t.width.max(t.height) <= max_side) {
            Some(thumb) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                // Refresh recency for pruning; failure only makes this file older.
                if let Ok(file) = std::fs::File::options().write(true).open(&path) {
                    let _ = file.set_modified(SystemTime::now());
                }
                Some(thumb)
            }
            None => {
                if std::fs::remove_file(&path).is_ok() {
                    self.subtract(bytes.len() as u64);
                }
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Store a thumbnail. Returns `true` when the cache is now over budget (the caller should
    /// call [`DiskCache::prune_in_background`]). Failures are silent: the cache is optional.
    pub fn put(&self, key: Hash128, thumb: &Thumb) -> bool {
        let Some(bytes) = encode(thumb) else { return false };
        let path = self.path_for(key);
        let Some(parent) = path.parent() else { return false };
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
        let tid = format!("{:?}", std::thread::current().id()).replace(|c: char| !c.is_ascii_alphanumeric(), "");
        let tmp = path.with_extension(format!("tmp{}x{tid}", std::process::id()));
        if std::fs::write(&tmp, &bytes).is_err() || std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return false;
        }
        self.writes.fetch_add(1, Ordering::Relaxed);
        let mut total = self.lock_total();
        // Unknown total: the next background scan counts this file; don't double count it.
        match total.as_mut() {
            Some(t) => {
                *t += bytes.len() as u64;
                *t > self.budget()
            }
            None => false,
        }
    }

    /// Scan and prune on a new thread unless one is already running. Never blocks.
    pub fn prune_in_background(self: &Arc<Self>) {
        self.in_background("assetcraft-thumb-prune", Self::prune);
    }

    /// Delete every cache file on a new thread unless a prune or clear is already running
    /// (then the call is dropped and returns `false`). Never blocks.
    pub fn clear_in_background(self: &Arc<Self>) -> bool {
        self.in_background("assetcraft-thumb-clear", Self::clear)
    }

    fn in_background(self: &Arc<Self>, name: &str, job: fn(&Self)) -> bool {
        if self.busy.swap(true, Ordering::AcqRel) {
            return false;
        }
        let cache = Arc::clone(self);
        let spawned = std::thread::Builder::new().name(name.into()).spawn(move || {
            job(&cache);
            cache.busy.store(false, Ordering::Release);
        });
        if spawned.is_err() {
            self.busy.store(false, Ordering::Release);
            return false;
        }
        true
    }

    /// Delete least recently used files until the cache is at most 80 % of its budget.
    /// Blocking (scans the folder): call from a background thread.
    pub fn prune(&self) {
        let mut files = self.scan();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        let target = self.budget() / 5 * 4;
        if total > self.budget() {
            files.sort_by_key(|f| f.2);
            for (path, len, _) in files {
                if total <= target {
                    break;
                }
                if std::fs::remove_file(&path).is_ok() {
                    total -= len;
                }
            }
        }
        *self.lock_total() = Some(total);
    }

    /// Delete every cache file (and nothing else). Blocking.
    pub fn clear(&self) {
        let mut left = 0;
        for (path, len, _) in self.scan() {
            if std::fs::remove_file(&path).is_err() {
                left += len;
            }
        }
        *self.lock_total() = Some(left);
    }

    /// All cache files: `(path, size, modified)`. Only names of the cache's own shape.
    fn scan(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        let mut out = Vec::new();
        let Ok(shards) = std::fs::read_dir(&self.dir) else { return out };
        for shard in shards.flatten() {
            let shard_name = shard.file_name();
            let Some(shard_name) = shard_name.to_str() else { continue };
            // `DirEntry::file_type` does not follow links: a linked shard is never walked.
            if !is_shard_name(shard_name) || !shard.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Ok(files) = std::fs::read_dir(shard.path()) else { continue };
            for file in files.flatten() {
                let name = file.file_name();
                let Some(name) = name.to_str() else { continue };
                if !is_cache_file(shard_name, name) {
                    continue;
                }
                if let Ok(meta) = file.metadata()
                    && meta.is_file()
                {
                    out.push((file.path(), meta.len(), meta.modified().unwrap_or(UNIX_EPOCH)));
                }
            }
        }
        out
    }

    fn subtract(&self, bytes: u64) {
        if let Some(total) = self.lock_total().as_mut() {
            *total = total.saturating_sub(bytes);
        }
    }

    fn lock_total(&self) -> MutexGuard<'_, Option<u64>> {
        self.total.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn is_lower_hex(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_shard_name(s: &str) -> bool {
    s.len() == 2 && is_lower_hex(s)
}

/// Whether `name` inside shard folder `shard` is a file this cache wrote: `<32 hex>.png` whose
/// first two digits are the shard, or a `<32 hex>.tmp<alphanumeric>` temp file of
/// [`DiskCache::put`]. Everything else in the cache folder belongs to someone else.
pub fn is_cache_file(shard: &str, name: &str) -> bool {
    if !is_shard_name(shard) {
        return false;
    }
    let Some((stem, ext)) = name.split_once('.') else { return false };
    if stem.len() != 32 || !is_lower_hex(stem) || !stem.starts_with(shard) {
        return false;
    }
    ext == "png"
        || ext.strip_prefix("tmp").is_some_and(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Encode a thumbnail as the cache stores it: 8-bit RGBA PNG plus a source-size text chunk.
pub fn encode(thumb: &Thumb) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, thumb.width, thumb.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.add_text_chunk(SOURCE_KEYWORD.into(), format!("{}x{}", thumb.source_width, thumb.source_height)).ok()?;
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&thumb.rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}

/// Decode a cache file (see [`encode`]); `None` for anything this cache didn't write.
pub fn decode(bytes: &[u8]) -> Option<Thumb> {
    let limits = png::Limits { bytes: 2 * MAX_CACHE_FILE as usize };
    let mut reader = png::Decoder::new_with_limits(Cursor::new(bytes), limits).read_info().ok()?;
    let info = reader.info();
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let (width, height) = (info.width, info.height);
    let (source_width, source_height) = info
        .uncompressed_latin1_text
        .iter()
        .find(|c| c.keyword == SOURCE_KEYWORD)
        .and_then(|c| c.text.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut rgba).ok()?;
    rgba.truncate(frame.buffer_size());
    let thumb = Thumb { width, height, source_width, source_height, rgba };
    let expected = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
    (width > 0 && height > 0 && thumb.rgba.len() == expected).then_some(thumb)
}
