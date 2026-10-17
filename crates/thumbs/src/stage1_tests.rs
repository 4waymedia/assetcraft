//! Stage 1 tests: disk cache, decoder registry, background probes.

use crate::decoders::{self, finish};
use crate::disk::{self, DiskCache, cache_key, cache_key_and_head, is_legacy_file, key_from_parts, retire_legacy};
use crate::hash::Hash128;
use crate::*;
use assetcraft_cache::SourceId;
use assetcraft_fs::{FakeFs, RealFs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
}

/// A fresh, empty folder under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("assetcraft-thumbs-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn wait_for(pool: &mut ThumbPool, n: usize) -> Vec<ThumbResult> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut done = Vec::new();
    while done.len() < n && Instant::now() < deadline {
        done.extend(pool.poll_timeout(Duration::from_millis(100)));
    }
    done
}

fn thumb(w: u32, h: u32) -> Thumb {
    Thumb { width: w, height: h, source_width: w * 4, source_height: h * 4, rgba: vec![200; (w * h * 4) as usize] }
}

/// Files with extension `ext` anywhere under `dir`.
fn count_ext(dir: &Path, ext: &str) -> usize {
    let Ok(items) = std::fs::read_dir(dir) else { return 0 };
    items
        .flatten()
        .map(|i| {
            let p = i.path();
            if p.is_dir() { count_ext(&p, ext) } else { usize::from(p.extension().is_some_and(|e| e == ext)) }
        })
        .sum()
}

/// Cache entries under a cache root.
fn cache_files(dir: &Path) -> usize {
    count_ext(dir, "crft")
}

/// The on-disk size of one thumbnail entry.
fn entry_len(t: &Thumb) -> u64 {
    let png = disk::encode(t).unwrap();
    assetcraft_cache::entry::encode(disk::KIND, disk::FORMAT_VERSION, &SourceId::NONE, &png).len() as u64
}

// ---- disk cache ------------------------------------------------------------------------

#[test]
fn disk_cache_hits_after_restart() {
    let dir = TempDir::new("restart");
    let file = library().join("environments/snow_clearing_plate.jpg");
    let key = ThumbKey::new(&file, 128);

    let disk = Arc::new(DiskCache::new(&dir.0, disk::DEFAULT_DISK_BUDGET));
    let mut pool = ThumbPool::with_disk(Arc::new(RealFs), 1, ThumbCache::default(), Some(Arc::clone(&disk)));
    pool.request(key.clone(), 1);
    let first = wait_for(&mut pool, 1);
    let rendered = first[0].1.clone().unwrap();
    assert_eq!(disk.stats(), (0, 1, 1), "first run: a miss and a write");
    drop(pool);

    // "Restart": a new cache object and pool over the same folder.
    let disk = Arc::new(DiskCache::new(&dir.0, disk::DEFAULT_DISK_BUDGET));
    let mut pool = ThumbPool::with_disk(Arc::new(RealFs), 1, ThumbCache::default(), Some(Arc::clone(&disk)));
    pool.request(key, 1);
    let second = wait_for(&mut pool, 1);
    let cached = second[0].1.clone().unwrap();
    assert_eq!(disk.stats(), (1, 0, 0), "second run: a disk hit, nothing rendered");
    assert_eq!(*cached, *rendered, "pixels and source size survive the round trip");
    assert_eq!(cache_files(&dir.0), 1);
}

#[test]
fn corrupt_cache_file_is_re_rendered() {
    let dir = TempDir::new("corrupt");
    let file = library().join("edge-cases/TINY_ICON.JPG");
    let (hash, source, _) = cache_key_and_head(&RealFs, &file, 64).unwrap();
    let disk = Arc::new(DiskCache::new(&dir.0, disk::DEFAULT_DISK_BUDGET));
    let path = disk.path_for(hash);
    assert!(path.starts_with(dir.0.join("v1").join("thumbs")), "{path:?}");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"\x89PNG\r\n\x1a\nnot really").unwrap();

    let mut pool = ThumbPool::with_disk(Arc::new(RealFs), 1, ThumbCache::default(), Some(Arc::clone(&disk)));
    pool.request(ThumbKey::new(&file, 64), 1);
    let done = wait_for(&mut pool, 1);
    let t = done[0].1.clone().unwrap();
    assert_eq!((t.width, t.height), (64, 64));
    assert_eq!(disk.stats(), (0, 1, 1));
    let rewritten = disk.get(hash, &source, 64).unwrap();
    assert_eq!(rewritten, *t, "the bad file was replaced by a good one");
    // The same key from a file that changed since: a miss.
    let changed = SourceId { size: source.size + 1, ..source };
    assert!(disk.get(hash, &changed, 64).is_none());
    // A truncated file and a foreign PNG (no source chunk) are misses too.
    let bytes = disk::encode(&t).unwrap();
    assert!(disk::decode(&bytes[..bytes.len() / 2]).is_none());
    let foreign = std::fs::read(library().join("props/scythe_transparent.png")).unwrap();
    assert!(disk::decode(&foreign).is_none());
}

#[test]
fn prune_respects_budget_and_only_touches_its_own_files() {
    let dir = TempDir::new("prune");
    let one = entry_len(&thumb(32, 32));
    let disk = DiskCache::new(&dir.0, one * 10);
    let keys: Vec<Hash128> = (0..20u128).map(|n| Hash128((n + 1) << 120 | n)).collect();
    let base = SystemTime::now() - Duration::from_secs(10_000);
    for (n, key) in keys.iter().enumerate() {
        disk.put(*key, &SourceId::NONE, &thumb(32, 32));
        let f = std::fs::File::options().write(true).open(disk.path_for(*key)).unwrap();
        f.set_modified(base + Duration::from_secs(n as u64 * 10)).unwrap();
    }
    // Someone else's files inside the cache folder.
    let shard = disk.path_for(keys[0]).parent().unwrap().to_path_buf();
    let foreign = [
        disk.dir().join("readme.txt"),
        shard.join("notes.png"),
        shard.join(format!("{}.jpg", keys[0])),
        disk.dir().join("zz").join(format!("{}.crft", keys[1])),
        disk.dir().join("00").join(format!("{}.crft", keys[2])), // right shape, wrong shard
    ];
    for f in &foreign {
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, vec![0u8; 50_000]).unwrap();
    }

    disk.prune();
    let size = disk.size_known().unwrap();
    assert!(size <= one * 8, "pruned to 80 %: {size} > {}", one * 8);
    assert!(size >= one * 7, "but no further than needed: {size}");
    assert!(disk.get(keys[0], &SourceId::NONE, 64).is_none(), "the oldest went first");
    assert!(disk.get(keys[19], &SourceId::NONE, 64).is_some(), "the newest stays");
    assert!(foreign.iter().all(|f| f.exists()), "foreign files are never deleted");

    disk.clear();
    assert_eq!(cache_files(&dir.0), 2, "only the two foreign .crft files are left");
    assert!(foreign.iter().all(|f| f.exists()));
    assert_eq!(disk.size_known(), Some(0));
}

#[test]
fn thumbnails_from_before_c1_are_retired_once_and_nothing_else_is_touched() {
    let dir = TempDir::new("legacy");
    let shard = dir.0.join("ab");
    std::fs::create_dir_all(&shard).unwrap();
    let old = [shard.join(format!("ab{}.png", "0".repeat(30))), shard.join(format!("ab{}.tmp123x4", "1".repeat(30)))];
    let keep = [shard.join("notes.png"), dir.0.join("readme.txt")];
    for f in old.iter().chain(&keep) {
        std::fs::write(f, b"x").unwrap();
    }
    assert_eq!(retire_legacy(&dir.0), 2);
    assert!(old.iter().all(|f| !f.exists()) && keep.iter().all(|f| f.exists()));
    // An emptied folder goes; a missing one is fine.
    let empty = TempDir::new("legacy-empty");
    std::fs::create_dir_all(empty.0.join("cd")).unwrap();
    std::fs::write(empty.0.join("cd").join(format!("cd{}.png", "2".repeat(30))), b"x").unwrap();
    assert_eq!(retire_legacy(&empty.0), 1);
    assert!(!empty.0.exists());
    assert_eq!(retire_legacy(&empty.0), 0);

    assert!(is_legacy_file("ab", &format!("ab{}.png", "0".repeat(30))));
    assert!(is_legacy_file("ab", &format!("ab{}.tmp123x4", "0".repeat(30))));
    assert!(!is_legacy_file("ab", &format!("ab{}.tmp", "0".repeat(30))));
    assert!(!is_legacy_file("AB", &format!("AB{}.png", "0".repeat(30))));
    assert!(!is_legacy_file("ab", &format!("ab{}.png.bak", "0".repeat(30))));
}

#[test]
fn background_prune_and_clear_run_off_the_calling_thread() {
    let dir = TempDir::new("background");
    let one = entry_len(&thumb(16, 16));
    let disk = DiskCache::open(&dir.0, one * 4);
    let deadline = Instant::now() + Duration::from_secs(10);
    while disk.size_known().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(disk.size_known(), Some(0), "open() scans in the background");
    let mut over = false;
    for n in 0..6u128 {
        over |= disk.put(Hash128(n << 120 | n), &SourceId::NONE, &thumb(16, 16));
    }
    assert!(over, "put reports crossing the budget");
    disk.prune_in_background();
    while (disk.is_busy() || disk.size_known().unwrap_or(0) > one * 4) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(disk.size_known().unwrap_or(u64::MAX) <= one * 4);
    while !disk.clear_in_background() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    while disk.is_busy() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(cache_files(&dir.0), 0);
}

#[test]
fn key_changes_with_mtime_size_side_and_ends_of_the_file() {
    let t0 = Some(UNIX_EPOCH + Duration::from_secs(1_790_000_000));
    let t1 = Some(UNIX_EPOCH + Duration::from_secs(1_790_000_001));
    let base = key_from_parts(256, 1000, t0, b"head", b"tail");
    assert_eq!(base, key_from_parts(256, 1000, t0, b"head", b"tail"), "deterministic");
    assert_ne!(base, key_from_parts(256, 1000, t1, b"head", b"tail"), "mtime");
    assert_ne!(base, key_from_parts(256, 1001, t0, b"head", b"tail"), "size");
    assert_ne!(base, key_from_parts(128, 1000, t0, b"head", b"tail"), "side bucket");
    assert_ne!(base, key_from_parts(256, 1000, None, b"head", b"tail"), "missing mtime");
    assert_ne!(base, key_from_parts(256, 1000, t0, b"heat", b"tail"), "head bytes");
    assert_ne!(base, key_from_parts(256, 1000, t0, b"head", b"tall"), "tail bytes");
    assert_ne!(key_from_parts(256, 8, t0, b"ab", b"c"), key_from_parts(256, 8, t0, b"a", b"bc"));

    // Through Services: only the first and last 64 KiB are read and hashed.
    let size = 300 * 1024;
    let data: Vec<u8> = (0..size).map(|n| (n % 251) as u8).collect();
    let key_of = |bytes: Vec<u8>| {
        let fs = FakeFs::new().file_with("/big.tif", bytes);
        cache_key(&fs, Path::new("/big.tif"), 256).unwrap()
    };
    let original = key_of(data.clone());
    let mut middle = data.clone();
    middle[150 * 1024] ^= 0xFF;
    assert_eq!(original, key_of(middle), "documented trade-off: a same-size, same-mtime middle edit is not seen");
    let mut last = data.clone();
    last[size - 1] ^= 0xFF;
    assert_ne!(original, key_of(last));
    let mut first = data;
    first[0] ^= 0xFF;
    assert_ne!(original, key_of(first));
    assert_eq!(disk::HASH_WINDOW, 64 * 1024);
}

#[test]
fn cache_png_round_trips_pixels_and_source_size() {
    let mut t = thumb(5, 3);
    t.rgba.iter_mut().enumerate().for_each(|(i, b)| *b = (i * 7 % 256) as u8);
    let back = disk::decode(&disk::encode(&t).unwrap()).unwrap();
    assert_eq!(back, t);
    assert_eq!((back.source_width, back.source_height), (20, 12));
}

// ---- decoder registry ------------------------------------------------------------------

#[test]
fn every_format_a_decoder_claims_renders_from_the_library() {
    let files = assetcraft_fs::walk(&RealFs, &library(), &assetcraft_fs::WalkLimits::default()).file_paths;
    let probes: Vec<_> = files.iter().filter_map(|p| assetcraft_probe::probe(&RealFs, p).ok()).collect();
    for decoder in decoders::registry() {
        for format in decoder.formats() {
            let samples: Vec<_> = probes.iter().filter(|p| p.format == *format && p.status == Status::Ok).collect();
            assert!(!samples.is_empty(), "{}: no {} sample in test_data/library", decoder.name(), format.label());
            for probe in samples {
                // Every runtime switch on: a registered decoder must work when enabled.
                let key = ThumbKey::new(&probe.path, 128);
                let t = match crate::render_with(&RealFs, &key, &crate::DecodeOptions { heic: true }) {
                    // An external tool (filmcraft-cli for video) that isn't installed here.
                    Err(ThumbError::ToolMissing(_)) => continue,
                    r => r.unwrap_or_else(|e| panic!("{} via {}: {e}", probe.path.display(), decoder.name())),
                };
                assert!(t.width.max(t.height) <= 128);
            }
        }
    }
    assert!(supports(Format::Png) && supports(Format::Tiff));
}

#[test]
fn unregistered_formats_are_unsupported() {
    for format in [Format::Avif, Format::Wav] {
        assert!(!supports(format), "{} has no decoder yet", format.label());
    }
    // HEIF/HEIC: registered only with the `heic` feature, and even then off until switched on.
    assert_eq!(supports(Format::Heif), crate::HEIC_BUILT);
    let heic = library().join("environments/lantern_dusk.heic");
    assert_eq!(render_path(&RealFs, &heic, 64), Err(ThumbError::Unsupported(Format::Heif)));
    let on = crate::render_with(&RealFs, &ThumbKey::new(&heic, 64), &crate::DecodeOptions { heic: true });
    assert_eq!(on.is_ok(), crate::HEIC_BUILT, "{on:?}");
    let wav = FakeFs::new().file_with("/a.wav", b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0".to_vec());
    assert_eq!(render_path(&wav, Path::new("/a.wav"), 64), Err(ThumbError::Unsupported(Format::Wav)));
}

#[test]
fn decoder_output_is_checked_and_shrunk() {
    let bad = Thumb { width: 4, height: 4, source_width: 4, source_height: 4, rgba: vec![0; 10] };
    assert!(matches!(finish(bad, 64), Err(ThumbError::Decode(_))));
    let empty = Thumb { width: 0, height: 4, source_width: 0, source_height: 4, rgba: Vec::new() };
    assert!(matches!(finish(empty, 64), Err(ThumbError::Decode(_))));
    let big = Thumb { width: 400, height: 100, source_width: 4000, source_height: 1000, rgba: vec![9; 400 * 100 * 4] };
    let small = finish(big, 64).unwrap();
    assert_eq!((small.width, small.height, small.source_width, small.source_height), (64, 16, 4000, 1000));
}

// ---- background probes -----------------------------------------------------------------

#[test]
fn probe_pool_delivers_results_after_polling_and_clear_discards_old_work() {
    let png = std::fs::read(library().join("props/scythe_transparent.png")).unwrap();
    let fs = (0..50).fold(FakeFs::new(), |fs, n| fs.file_with(format!("/p/{n}.png"), png.clone()));
    let pool = ProbePool::new(Arc::new(fs), 2);
    assert_eq!(pool.workers(), 2);
    for n in 0..5 {
        pool.request(PathBuf::from(format!("/p/{n}.png")));
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut got = Vec::new();
    while got.len() < 5 && Instant::now() < deadline {
        got.extend(pool.poll_timeout(Duration::from_millis(50)));
    }
    assert_eq!(got.len(), 5);
    assert!(got.iter().all(|(_, r)| r.as_ref().is_ok_and(|p| p.format == Format::Png && p.dimensions.is_some())));
    for n in 5..50 {
        pool.request(PathBuf::from(format!("/p/{n}.png")));
    }
    pool.clear();
    std::thread::sleep(Duration::from_millis(50));
    assert!(pool.poll().is_empty(), "results from before clear() are dropped");
    assert_eq!(pool.busy(), 0);
}

// ---- HEIC runtime switch -----------------------------------------------------------------

#[test]
fn the_heic_switch_gates_rendering_and_the_disk_cache() {
    let dir = TempDir::new("heic-switch");
    let file = library().join("environments/lantern_dusk.heic");
    let key = ThumbKey::new(&file, 128);
    let disk = Arc::new(DiskCache::new(&dir.0, disk::DEFAULT_DISK_BUDGET));
    let mut pool = ThumbPool::with_disk(Arc::new(RealFs), 1, ThumbCache::default(), Some(Arc::clone(&disk)));
    assert_eq!(pool.options(), DecodeOptions::default(), "off by default");
    pool.request(key.clone(), 1);
    assert_eq!(wait_for(&mut pool, 1)[0].1, Err(ThumbError::Unsupported(Format::Heif)));
    assert_eq!(disk.stats().2, 0, "nothing written while off");

    // Switched on: renders when the decoder is built in, and lands in the disk cache.
    pool.set_options(DecodeOptions { heic: true });
    assert!(pool.error(&key).is_none(), "toggling forgets the old failure");
    pool.request(key.clone(), 1);
    let on = wait_for(&mut pool, 1)[0].1.clone();
    assert_eq!(on.is_ok(), HEIC_BUILT, "{on:?}");
    assert_eq!(disk.stats().2, u64::from(HEIC_BUILT));

    // Off again: the disk copy is never served.
    pool.set_options(DecodeOptions::default());
    assert!(pool.get(&key).is_none(), "toggling drops cached thumbnails");
    pool.request(key.clone(), 1);
    assert_eq!(wait_for(&mut pool, 1)[0].1, Err(ThumbError::Unsupported(Format::Heif)));
    assert_eq!(disk.stats().0, 0, "no disk hit for a disabled format");
}
