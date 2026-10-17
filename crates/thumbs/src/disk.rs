//! Rendered thumbnails on disk, in the suite cache (`docs/design/cache.md`, C1): kind `thumbs`,
//! format version [`FORMAT_VERSION`], `<cache root>/v1/thumbs/<2 hex>/<32 hex>.crft`. Each entry
//! is a checked [`assetcraft_cache::entry`] holding an 8-bit RGBA PNG with the source size in a
//! text chunk ([`encode`]).
//!
//! **Key** ([`cache_key`]): SipHash-1-3-128 over [`RENDERER_VERSION`], the thumbnail side bucket,
//! the file size, the modification time, the first [`HASH_WINDOW`] bytes and the last
//! [`HASH_WINDOW`] bytes of the file. At most 128 KiB is read per key, so a 1.4 MB PNG and a 2 GB
//! TIFF cost the same. The same size, time and ends are stored in the entry as its source
//! ([`assetcraft_cache::SourceId`]) and compared on every hit.
//!
//! *Trade-off:* an edit that changes only the middle of a file and keeps its size *and* its
//! modification time (a tool that restores the mtime after an in-place edit) is not seen, and
//! the stale thumbnail stays until the cache is cleared. Normal saves change the mtime; most
//! format edits change the size or the header/trailer too.
//!
//! Writing, budgets (default [`DEFAULT_DISK_BUDGET`], 1 GiB; least recently used down to 80 %),
//! own-files-only scanning and damage handling are the cache crate's ([`assetcraft_cache::Cache`]).
//! Thumbnails from before C1 (`<cache>/thumbs/<2 hex>/<32 hex>.png`) are deleted once by
//! [`retire_legacy`]: they are regenerable, so nothing is migrated.
//!
//! This is AssetCraft's own data, so writing it is allowed before Stage 2 (AGENTS.md rule 5
//! covers the user's files). Cache files go through `std::fs`, not `Services`: `Services` is
//! the read-only view of the user's files.

use crate::Thumb;
use crate::hash::{Hash128, Hasher128};
use assetcraft_cache::{Cache, SourceId};
use assetcraft_fs::{FsError, Services};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Bump when any decoder's output changes, so older cache files are never shown.
/// 2: PSD, camera raw, HEIF (sibling crates), PDF (pdfcraft-render), video poster frames
/// (filmcraft-cli) and GLB/glTF/OBJ models (assetcraft-mesh) added.
pub const RENDERER_VERSION: u64 = 2;

/// The `thumbs` kind's entry format (bump when [`encode`] changes).
pub const FORMAT_VERSION: u16 = 1;

/// The cache kind.
pub const KIND: &str = "thumbs";

/// Bytes hashed from each end of the file.
pub const HASH_WINDOW: usize = assetcraft_cache::source::HASH_WINDOW;

/// Default disk budget: 1 GiB.
pub const DEFAULT_DISK_BUDGET: u64 = 1024 * 1024 * 1024;

/// Payloads larger than this are treated as corrupt (a 1024-px RGBA PNG is at most ~4 MiB).
const MAX_CACHE_FILE: u64 = 16 * 1024 * 1024;

/// Text chunk keyword holding `"<source width>x<source height>"`.
const SOURCE_KEYWORD: &str = "assetcraft-source";

/// The cache key for `path` at `max_side`: see the module docs. Reads at most 128 KiB.
pub fn cache_key(services: &dyn Services, path: &Path, max_side: u32) -> Result<Hash128, FsError> {
    cache_key_and_head(services, path, max_side).map(|(key, _, _)| key)
}

/// [`cache_key`], the file's identity as stored in the entry, and the first bytes it read (the
/// worker sniffs the format from them to decide whether the disk cache may be consulted at all).
pub(crate) fn cache_key_and_head(
    services: &dyn Services,
    path: &Path,
    max_side: u32,
) -> Result<(Hash128, SourceId, Vec<u8>), FsError> {
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
    let source = SourceId::from_parts(entry.size, entry.modified, &head, &tail);
    Ok((key, source, head))
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

/// Thumbnails in the suite cache. Share it as `Arc<DiskCache>` between the pool and the engine.
#[derive(Debug)]
pub struct DiskCache {
    cache: Arc<Cache>,
    dir: PathBuf,
}

impl DiskCache {
    /// Thumbnails under the cache `root` (the app's cache folder; created on first write),
    /// holding at most about `budget` bytes.
    pub fn new(root: impl Into<PathBuf>, budget: u64) -> Self {
        let root = root.into();
        let cache = Cache::new(Some(&root), KIND, FORMAT_VERSION, budget).with_max_entry(MAX_CACHE_FILE);
        let dir = cache.dir().map(Path::to_path_buf).unwrap_or(root);
        Self { cache: Arc::new(cache), dir }
    }

    /// [`DiskCache::new`] behind an `Arc`, with the first size scan (and a prune, if a smaller
    /// budget is now in force, and the emptying of older formats) on a background thread.
    pub fn open(root: impl Into<PathBuf>, budget: u64) -> Arc<Self> {
        let cache = Arc::new(Self::new(root, budget));
        cache.prune_in_background();
        cache
    }

    /// The thumbnails' folder (`<root>/v1/thumbs`).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The underlying cache (for `cache.info`).
    pub fn cache(&self) -> &Arc<Cache> {
        &self.cache
    }

    /// Bytes the cache may hold.
    pub fn budget(&self) -> u64 {
        self.cache.budget()
    }

    /// Bytes on disk if already known; never scans (safe on the UI thread).
    pub fn size_known(&self) -> Option<u64> {
        self.cache.size_known()
    }

    /// Bytes on disk, scanning the folder the first time. Blocking.
    pub fn size(&self) -> u64 {
        self.cache.size()
    }

    /// `(hits, misses, writes)` since start.
    pub fn stats(&self) -> (u64, u64, u64) {
        self.cache.stats()
    }

    /// `true` while a background prune or clear runs.
    pub fn is_busy(&self) -> bool {
        self.cache.is_busy()
    }

    /// Where the thumbnail for `key` lives.
    pub fn path_for(&self, key: Hash128) -> PathBuf {
        self.cache.path_for(key).unwrap_or_else(|| self.dir.join(key.to_string()))
    }

    /// The cached thumbnail, if a valid one made from `source` exists. A malformed entry is
    /// deleted (a miss).
    pub fn get(&self, key: Hash128, source: &SourceId, max_side: u32) -> Option<Thumb> {
        let payload = self.cache.get(key, source)?;
        let thumb = decode(&payload).filter(|t| t.width.max(t.height) <= max_side);
        if thumb.is_none() {
            // A good entry with a bad PNG inside: never ours to show again.
            let _ = std::fs::remove_file(self.path_for(key));
        }
        thumb
    }

    /// Store a thumbnail made from `source`. Returns `true` when the cache is now over budget
    /// (the caller should call [`DiskCache::prune_in_background`]). Failures are silent.
    pub fn put(&self, key: Hash128, source: &SourceId, thumb: &Thumb) -> bool {
        encode(thumb).is_some_and(|png| self.cache.put(key, source, &png))
    }

    /// Scan and prune on a new thread unless one is already running. Never blocks.
    pub fn prune_in_background(&self) {
        self.cache.prune_in_background();
    }

    /// Delete every thumbnail on a new thread unless a prune or clear is already running (then
    /// the call is dropped and returns `false`). Never blocks.
    pub fn clear_in_background(&self) -> bool {
        self.cache.clear_in_background()
    }

    /// Delete least recently used thumbnails until the cache is at most 80 % of its budget.
    /// Blocking: call from a background thread.
    pub fn prune(&self) {
        self.cache.prune();
    }

    /// Delete every thumbnail (and nothing else). Blocking.
    pub fn clear(&self) {
        self.cache.clear();
    }
}

fn is_lower_hex(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether `name` inside shard folder `shard` is a thumbnail file from before C1:
/// `<32 hex>.png` whose first two digits are the shard, or a `<32 hex>.tmp<alphanumeric>` temp
/// file. Everything else in that folder belongs to someone else.
pub fn is_legacy_file(shard: &str, name: &str) -> bool {
    if shard.len() != 2 || !is_lower_hex(shard) {
        return false;
    }
    let Some((stem, ext)) = name.split_once('.') else { return false };
    if stem.len() != 32 || !is_lower_hex(stem) || !stem.starts_with(shard) {
        return false;
    }
    ext == "png"
        || ext.strip_prefix("tmp").is_some_and(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Delete the thumbnails from before C1 in `dir` (the old `thumbs` folder): only names of their
/// shape, then the shard folders and `dir` itself if left empty. Never follows links. Returns
/// the number of files deleted. Blocking: run it on a background thread.
pub fn retire_legacy(dir: &Path) -> usize {
    let mut deleted = 0;
    let Ok(shards) = std::fs::read_dir(dir) else { return 0 };
    for shard in shards.flatten() {
        let shard_name = shard.file_name();
        let Some(shard_name) = shard_name.to_str() else { continue };
        if !shard.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(shard.path()) else { continue };
        for file in files.flatten() {
            let name = file.file_name();
            if name.to_str().is_some_and(|n| is_legacy_file(shard_name, n))
                && file.file_type().is_ok_and(|t| t.is_file())
                && std::fs::remove_file(file.path()).is_ok()
            {
                deleted += 1;
            }
        }
        let _ = std::fs::remove_dir(shard.path()); // only if empty
    }
    let _ = std::fs::remove_dir(dir);
    deleted
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
