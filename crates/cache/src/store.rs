//! One kind of cache entry in one root (R4–R16).
//!
//! Layout: `<root>/v<N>/<kind>/<2 hex>/<32 hex>.crft`, plus `CACHEDIR.TAG` at `<root>` (R5) so
//! backup tools skip it. A file is written to `<32 hex>.tmp.<pid>.<n>` in its shard and renamed
//! into place (R10); a failed write (a full disk, Windows refusing a rename onto a file another
//! process has open) is simply dropped (R11). Entries are immutable once named, so processes and
//! apps sharing a root need no locks (R13).
//!
//! The cache lists, counts and deletes only names of its own shape, inside its own kind folder
//! (and older `v<M>` folders of the same kind, emptied on the next prune, R4), and never follows
//! links (R14). Over budget, the least recently used files go until 80 % is left; a hit refreshes
//! a file's time at most once a day (R15). The total is learnt by a background scan (R16).
//!
//! With no folder ([`Cache::memory`]), entries live in memory under the same rules.

use crate::entry;
use crate::hash::Hash128;
use crate::source::SourceId;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// File extension of an entry.
pub const EXT: &str = "crft";
/// Largest entry by default (a bigger file is damaged, or not ours).
pub const DEFAULT_MAX_ENTRY: u64 = 64 * 1024 * 1024;
/// A hit refreshes a file's time when it is older than this.
const REFRESH_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
/// Temp files older than this are leftovers of a crash and are deleted by a prune.
const STALE_TMP: Duration = Duration::from_secs(60 * 60);

/// `CACHEDIR.TAG` (bford.info/cachedir).
const CACHEDIR_TAG: &str = "Signature: 8a477f597d28d172789f06886806bc55\n\
# This file is a cache directory tag created by a Crafting app (AssetCraft).\n\
# For information about cache directory tags see https://bford.info/cachedir/\n";

/// What `cache.info` reports for one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindInfo {
    /// Kind name.
    pub kind: String,
    /// Format version.
    pub version: u16,
    /// The kind's folder (`None` = memory only).
    pub dir: Option<PathBuf>,
    /// Bytes held, if known yet.
    pub bytes: Option<u64>,
    /// Budget, bytes.
    pub budget: u64,
    /// `(hits, misses, writes)` since start.
    pub hits: u64,
    /// See `hits`.
    pub misses: u64,
    /// See `hits`.
    pub writes: u64,
}

#[derive(Debug, Default)]
struct Memory {
    entries: HashMap<Hash128, (Vec<u8>, u64)>,
    tick: u64,
}

#[derive(Debug)]
enum Backend {
    Dir { root: PathBuf, dir: PathBuf },
    Memory(Mutex<Memory>),
}

/// One kind of entry in one root. Share it as `Arc<Cache>`.
#[derive(Debug)]
pub struct Cache {
    backend: Backend,
    kind: String,
    version: u16,
    max_entry: u64,
    budget: AtomicU64,
    total: Mutex<Option<u64>>,
    busy: AtomicBool,
    hits: AtomicU64,
    misses: AtomicU64,
    writes: AtomicU64,
    seq: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn is_lower_hex(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_shard(s: &str) -> bool {
    s.len() == 2 && is_lower_hex(s)
}

/// Whether `name` in shard `shard` is a file a cache wrote: `<32 hex>.crft` starting with the
/// shard, or a temp file `<32 hex>.tmp.<pid>.<n>`. Returns `Some(true)` for a temp file.
pub fn own_file(shard: &str, name: &str) -> Option<bool> {
    if !is_shard(shard) {
        return None;
    }
    let (stem, ext) = name.split_once('.')?;
    if stem.len() != 32 || !is_lower_hex(stem) || !stem.starts_with(shard) {
        return None;
    }
    if ext == EXT {
        return Some(false);
    }
    let rest = ext.strip_prefix("tmp.")?;
    let (pid, n) = rest.split_once('.')?;
    (!pid.is_empty() && !n.is_empty() && pid.bytes().chain(n.bytes()).all(|b| b.is_ascii_digit())).then_some(true)
}

/// Own-shaped files under a kind folder: `(path, size, modified, temp)`. Never follows links.
fn scan_kind(dir: &Path) -> Vec<(PathBuf, u64, SystemTime, bool)> {
    let mut out = Vec::new();
    let Ok(shards) = std::fs::read_dir(dir) else { return out };
    for shard in shards.flatten() {
        let shard_name = shard.file_name();
        let Some(shard_name) = shard_name.to_str() else { continue };
        if !is_shard(shard_name) || !shard.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(shard.path()) else { continue };
        for file in files.flatten() {
            let name = file.file_name();
            let Some(tmp) = name.to_str().and_then(|n| own_file(shard_name, n)) else { continue };
            if let Ok(meta) = file.metadata()
                && meta.is_file()
            {
                out.push((file.path(), meta.len(), meta.modified().unwrap_or(UNIX_EPOCH), tmp));
            }
        }
    }
    out
}

impl Cache {
    /// `kind` (1–32 of `a-z0-9-`) at format `version` under `root`; `None` for memory only.
    /// An invalid kind name gives a memory-only cache (it can't name a folder safely).
    pub fn new(root: Option<&Path>, kind: &str, version: u16, budget: u64) -> Cache {
        let kind_ok = entry::valid_kind(kind);
        let backend = match root {
            Some(root) if kind_ok => {
                Backend::Dir { root: root.to_path_buf(), dir: root.join(format!("v{version}")).join(kind) }
            }
            _ => Backend::Memory(Mutex::new(Memory::default())),
        };
        Cache {
            backend,
            kind: if kind_ok { kind.to_owned() } else { "invalid".to_owned() },
            version,
            max_entry: DEFAULT_MAX_ENTRY,
            budget: AtomicU64::new(budget),
            total: Mutex::new(None),
            busy: AtomicBool::new(false),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            writes: AtomicU64::new(0),
            seq: AtomicU64::new(0),
        }
    }

    /// A memory-only cache.
    pub fn memory(kind: &str, version: u16, budget: u64) -> Cache {
        Cache::new(None, kind, version, budget)
    }

    /// [`Cache::new`] behind an `Arc`, with the first scan (and a prune if needed, and the
    /// emptying of older format versions) on a background thread.
    pub fn open(root: Option<&Path>, kind: &str, version: u16, budget: u64) -> Arc<Cache> {
        let cache = Arc::new(Cache::new(root, kind, version, budget));
        cache.prune_in_background();
        cache
    }

    /// Refuse entries larger than `bytes` (default [`DEFAULT_MAX_ENTRY`]).
    pub fn with_max_entry(mut self, bytes: u64) -> Cache {
        self.max_entry = bytes;
        self
    }

    /// The kind's folder (`None` = memory only).
    pub fn dir(&self) -> Option<&Path> {
        match &self.backend {
            Backend::Dir { dir, .. } => Some(dir),
            Backend::Memory(_) => None,
        }
    }

    /// The kind name.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Bytes this kind may hold.
    pub fn budget(&self) -> u64 {
        self.budget.load(Ordering::Relaxed)
    }

    /// Change the budget (the next prune applies it).
    pub fn set_budget(&self, bytes: u64) {
        self.budget.store(bytes, Ordering::Relaxed);
    }

    /// Bytes held, if already known; never scans (safe on the UI thread).
    pub fn size_known(&self) -> Option<u64> {
        match &self.backend {
            Backend::Memory(m) => Some(lock(m).entries.values().map(|(b, _)| b.len() as u64).sum()),
            Backend::Dir { .. } => *lock(&self.total),
        }
    }

    /// Bytes held, scanning the folder the first time. Blocking.
    pub fn size(&self) -> u64 {
        match &self.backend {
            Backend::Memory(_) => self.size_known().unwrap_or(0),
            Backend::Dir { dir, .. } => {
                *lock(&self.total).get_or_insert_with(|| scan_kind(dir).iter().filter(|f| !f.3).map(|f| f.1).sum())
            }
        }
    }

    /// `(hits, misses, writes)` since start.
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.hits.load(Ordering::Relaxed), self.misses.load(Ordering::Relaxed), self.writes.load(Ordering::Relaxed))
    }

    /// What `cache.info` shows.
    pub fn info(&self) -> KindInfo {
        let (hits, misses, writes) = self.stats();
        KindInfo {
            kind: self.kind.clone(),
            version: self.version,
            dir: self.dir().map(Path::to_path_buf),
            bytes: self.size_known(),
            budget: self.budget(),
            hits,
            misses,
            writes,
        }
    }

    /// `true` while a background prune or clear runs.
    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }

    /// Where the entry for `key` lives (`None` = memory only).
    pub fn path_for(&self, key: Hash128) -> Option<PathBuf> {
        let hex = key.to_string();
        self.dir().map(|d| d.join(&hex[..2]).join(format!("{hex}.{EXT}")))
    }

    /// The payload stored for `key`, if there is a valid entry made from a source that still
    /// looks like `source`. A damaged or foreign entry is deleted (a miss).
    pub fn get(&self, key: Hash128, source: &SourceId) -> Option<Vec<u8>> {
        let found = match &self.backend {
            Backend::Memory(m) => {
                let mut m = lock(m);
                m.tick += 1;
                let tick = m.tick;
                m.entries.get_mut(&key).map(|(bytes, used)| {
                    *used = tick;
                    bytes.clone()
                })
            }
            Backend::Dir { .. } => {
                let path = self.path_for(key)?;
                let meta = std::fs::symlink_metadata(&path).ok().filter(|m| m.is_file());
                match meta {
                    Some(m) if m.len() <= self.max_entry.saturating_add(128) => {
                        let bytes = std::fs::read(&path).ok();
                        // Recency for pruning, at most once a day; failure only ages the file.
                        if bytes.is_some()
                            && m.modified().ok().and_then(|t| t.elapsed().ok()).is_none_or(|age| age > REFRESH_AFTER)
                            && let Ok(f) = std::fs::File::options().write(true).open(&path)
                        {
                            let _ = f.set_modified(SystemTime::now());
                        }
                        bytes
                    }
                    Some(m) => {
                        self.forget(&path, m.len());
                        None
                    }
                    None => None,
                }
            }
        };
        let Some(bytes) = found else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        match entry::decode(&bytes, &self.kind, self.version) {
            Some((src, payload)) if src == *source => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(payload.to_vec())
            }
            Some(_) => {
                // Another source's entry under this key (a collision, or the file changed in a
                // way the key didn't see): not ours to show. It stays for its own source.
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
            None => {
                self.remove(key, bytes.len() as u64);
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Store `payload` for `key` made from `source`. Returns `true` when the kind is now over
    /// budget (call [`Cache::prune_in_background`]). Failures are silent: the cache is optional.
    pub fn put(&self, key: Hash128, source: &SourceId, payload: &[u8]) -> bool {
        if payload.len() as u64 > self.max_entry {
            return false;
        }
        let bytes = entry::encode(&self.kind, self.version, source, payload);
        let len = bytes.len() as u64;
        match &self.backend {
            Backend::Memory(m) => {
                let mut m = lock(m);
                m.tick += 1;
                let tick = m.tick;
                m.entries.insert(key, (bytes, tick));
                self.writes.fetch_add(1, Ordering::Relaxed);
                let total: u64 = m.entries.values().map(|(b, _)| b.len() as u64).sum();
                if total > self.budget() {
                    // Memory prunes at once: least recently used down to 80 %.
                    let target = self.budget() / 5 * 4;
                    let mut order: Vec<(Hash128, u64, u64)> =
                        m.entries.iter().map(|(k, (b, t))| (*k, b.len() as u64, *t)).collect();
                    order.sort_by_key(|e| e.2);
                    let mut left = total;
                    for (k, l, _) in order {
                        if left <= target {
                            break;
                        }
                        m.entries.remove(&k);
                        left -= l;
                    }
                }
                false
            }
            Backend::Dir { root, .. } => {
                let Some(path) = self.path_for(key) else { return false };
                let Some(shard) = path.parent() else { return false };
                if std::fs::create_dir_all(shard).is_err() {
                    return false;
                }
                tag_root(root);
                let n = self.seq.fetch_add(1, Ordering::Relaxed);
                let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else { return false };
                let tmp = shard.join(format!("{stem}.tmp.{}.{n}", std::process::id()));
                if std::fs::write(&tmp, &bytes).is_err() || std::fs::rename(&tmp, &path).is_err() {
                    let _ = std::fs::remove_file(&tmp);
                    return false;
                }
                self.writes.fetch_add(1, Ordering::Relaxed);
                let mut total = lock(&self.total);
                match total.as_mut() {
                    Some(t) => {
                        *t += len;
                        *t > self.budget()
                    }
                    None => false,
                }
            }
        }
    }

    fn remove(&self, key: Hash128, len: u64) {
        match &self.backend {
            Backend::Memory(m) => {
                lock(m).entries.remove(&key);
            }
            Backend::Dir { .. } => {
                if let Some(path) = self.path_for(key) {
                    self.forget(&path, len);
                }
            }
        }
    }

    fn forget(&self, path: &Path, len: u64) {
        if std::fs::remove_file(path).is_ok()
            && let Some(t) = lock(&self.total).as_mut()
        {
            *t = t.saturating_sub(len);
        }
    }

    /// Prune on a new thread unless one is already running. Never blocks.
    pub fn prune_in_background(self: &Arc<Self>) {
        self.in_background(Self::prune);
    }

    /// Clear on a new thread unless a prune or clear is running (then `false`). Never blocks.
    pub fn clear_in_background(self: &Arc<Self>) -> bool {
        self.in_background(|c| {
            c.clear();
        })
    }

    fn in_background(self: &Arc<Self>, job: fn(&Self)) -> bool {
        if self.busy.swap(true, Ordering::AcqRel) {
            return false;
        }
        let cache = Arc::clone(self);
        let name = format!("cache-{}", self.kind);
        let spawned = std::thread::Builder::new().name(name).spawn(move || {
            job(&cache);
            cache.busy.store(false, Ordering::Release);
        });
        if spawned.is_err() {
            self.busy.store(false, Ordering::Release);
            return false;
        }
        true
    }

    /// Scan; delete stale temp files and older format versions of this kind; then the least
    /// recently used entries down to 80 % if over budget. Blocking: call from a background thread.
    /// Files newer than the prune's start (written meanwhile) are never deleted.
    pub fn prune(&self) {
        let Backend::Dir { root, dir } = &self.backend else { return };
        let started = SystemTime::now();
        self.retire_old_versions(root);
        let mut files = scan_kind(dir);
        let stale = |t: SystemTime| started.duration_since(t).is_ok_and(|age| age > STALE_TMP);
        files.retain(|(path, _, modified, tmp)| {
            if *tmp {
                if stale(*modified) {
                    let _ = std::fs::remove_file(path);
                }
                return false;
            }
            true
        });
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        if total > self.budget() {
            let target = self.budget() / 5 * 4;
            files.sort_by_key(|f| f.2);
            for (path, len, modified, _) in files {
                if total <= target {
                    break;
                }
                if modified <= started && std::fs::remove_file(&path).is_ok() {
                    total -= len;
                }
            }
        }
        *lock(&self.total) = Some(total);
    }

    /// Delete every entry of this kind (and nothing else). Returns the bytes freed; with
    /// `dry_run`, deletes nothing and returns what would be freed. Blocking.
    pub fn clear_with(&self, dry_run: bool) -> u64 {
        match &self.backend {
            Backend::Memory(m) => {
                let mut m = lock(m);
                let freed = m.entries.values().map(|(b, _)| b.len() as u64).sum();
                if !dry_run {
                    m.entries.clear();
                }
                freed
            }
            Backend::Dir { dir, .. } => {
                let (mut freed, mut left) = (0, 0);
                for (path, len, _, tmp) in scan_kind(dir) {
                    let gone = dry_run || std::fs::remove_file(&path).is_ok();
                    match (gone, tmp) {
                        (true, false) => freed += len,
                        (false, false) => left += len,
                        (_, true) => {}
                    }
                }
                if !dry_run {
                    *lock(&self.total) = Some(left);
                }
                freed
            }
        }
    }

    /// Delete every entry of this kind. Returns the bytes freed.
    pub fn clear(&self) -> u64 {
        self.clear_with(false)
    }

    /// Empty `<root>/v<M>/<kind>` for every other format version `M` (own files only), and
    /// remove the folders left empty.
    fn retire_old_versions(&self, root: &Path) {
        let Ok(versions) = std::fs::read_dir(root) else { return };
        for v in versions.flatten() {
            let name = v.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(num) = name.strip_prefix('v').and_then(|n| n.parse::<u16>().ok()) else { continue };
            if num == self.version || !v.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let kind_dir = v.path().join(&self.kind);
            if !std::fs::symlink_metadata(&kind_dir).is_ok_and(|m| m.is_dir()) {
                continue;
            }
            for (path, ..) in scan_kind(&kind_dir) {
                let _ = std::fs::remove_file(path);
            }
            if let Ok(shards) = std::fs::read_dir(&kind_dir) {
                for s in shards.flatten() {
                    if s.file_name().to_str().is_some_and(is_shard) {
                        let _ = std::fs::remove_dir(s.path()); // only if empty
                    }
                }
            }
            let _ = std::fs::remove_dir(&kind_dir);
            let _ = std::fs::remove_dir(v.path());
        }
    }
}

/// Write `CACHEDIR.TAG` at `root` unless something is already there.
fn tag_root(root: &Path) {
    let tag = root.join("CACHEDIR.TAG");
    if std::fs::symlink_metadata(&tag).is_err()
        && let Ok(mut f) = std::fs::File::options().write(true).create_new(true).open(&tag)
    {
        let _ = std::io::Write::write_all(&mut f, CACHEDIR_TAG.as_bytes());
    }
}
