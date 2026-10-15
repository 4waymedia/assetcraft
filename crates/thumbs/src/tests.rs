use super::*;
use assetcraft_fs::{FakeFs, RealFs};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
}

fn library_files() -> Vec<PathBuf> {
    assetcraft_fs::walk(&RealFs, &library(), &assetcraft_fs::WalkLimits::default()).file_paths
}

#[test]
fn every_library_image_renders_within_bounds() {
    let (mut rendered, mut tool_missing, mut heic_off) = (0, 0, 0);
    let files = library_files();
    for path in &files {
        let result = render_path(&RealFs, path, 256);
        if result == Err(ThumbError::Unsupported(Format::Heif)) {
            heic_off += 1; // HEIC is off by default (feature `heic` + the `formats.heic` switch)
            continue;
        }
        if path.ends_with("broken_not_an_image.jpg") {
            assert_eq!(result, Err(ThumbError::Unreadable(Status::Unrecognized)));
            continue;
        }
        if result == Err(ThumbError::ToolMissing("filmcraft-cli")) {
            tool_missing += 1; // video without filmcraft-cli installed
            continue;
        }
        let t = result.unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(t.width.max(t.height) <= 256, "{}", path.display());
        assert_eq!(t.rgba.len(), (t.width * t.height * 4) as usize);
        // Aspect ratio survives: the short side is within one pixel of the exact value.
        let (sw, sh) = (f64::from(t.source_width), f64::from(t.source_height));
        let (w, h) = (f64::from(t.width), f64::from(t.height));
        let error = if sw >= sh { (h - sh * w / sw).abs() } else { (w - sw * h / sh).abs() };
        assert!(error <= 1.0, "{}: {t:?}", path.display());
        rendered += 1;
    }
    assert_eq!(rendered + tool_missing + heic_off, files.len() - 1);
    assert_eq!(heic_off, 1, "lantern_dusk.heic stays a type icon by default");
    assert!(rendered >= 29, "23 images, PSD, DNG, a PDF and 3 models render without external tools: {rendered}");
}

#[test]
fn small_images_are_not_upscaled() {
    let t = render_path(&RealFs, &library().join("edge-cases/TINY_ICON.JPG"), 256).unwrap();
    assert_eq!((t.width, t.height), (64, 64));
}

#[test]
fn panorama_keeps_its_shape() {
    let t = render_path(&RealFs, &library().join("edge-cases/panorama_20to1.jpg"), 256).unwrap();
    assert_eq!((t.width, t.height, t.source_width, t.source_height), (256, 13, 4000, 200));
}

#[test]
fn transparency_survives() {
    let t = render_path(&RealFs, &library().join("props/scythe_transparent.png"), 128).unwrap();
    assert!(t.rgba.chunks(4).any(|px| px[3] == 0), "some pixels stay fully transparent");
    assert!(t.rgba.chunks(4).any(|px| px[3] == 255), "the scythe itself is opaque");
}

#[test]
fn unsupported_and_missing_files_are_typed_errors() {
    let wav = b"RIFF\x24\0\0\0WAVEfmt ".to_vec();
    let fs = FakeFs::new().file_with("/a.wav", wav).file_with("/b.png", b"\x89PNG\r\n\x1a\n".to_vec());
    assert_eq!(render_path(&fs, Path::new("/a.wav"), 128), Err(ThumbError::Unsupported(Format::Wav)));
    // A damaged header in a decodable format goes to the (limited) decoder, which rejects it.
    assert!(matches!(render_path(&fs, Path::new("/b.png"), 128), Err(ThumbError::Decode(_))));
    assert!(matches!(render_path(&fs, Path::new("/nope.png"), 128), Err(ThumbError::Io(_))));
}

#[test]
fn decompression_bomb_header_is_refused_without_allocating() {
    // A real PNG with its IHDR rewritten to claim 100,000 × 100,000 pixels.
    let mut png = std::fs::read(library().join("props/scythe_transparent.png")).unwrap();
    png[16..20].copy_from_slice(&100_000u32.to_be_bytes());
    png[20..24].copy_from_slice(&100_000u32.to_be_bytes());
    let fs = FakeFs::new().file_with("/bomb.png", png);
    assert_eq!(render_path(&fs, Path::new("/bomb.png"), 128), Err(ThumbError::TooLarge));
}

#[test]
fn corrupted_pixel_data_is_a_decode_error_not_a_crash() {
    let mut jpg = std::fs::read(library().join("environments/snow_clearing_plate.jpg")).unwrap();
    let mid = jpg.len() / 2;
    for b in &mut jpg[mid..mid + 4000] {
        *b = 0xFF;
    }
    jpg.truncate(mid + 4000);
    let fs = FakeFs::new().file_with("/bad.jpg", jpg);
    // Either a decode error or a (partially grey) image is acceptable; a panic is not.
    let _ = render_path(&fs, Path::new("/bad.jpg"), 128);
}

#[test]
fn key_side_is_clamped() {
    assert_eq!(ThumbKey::new("/x", 1).max_side, SIDE_RANGE.0);
    assert_eq!(ThumbKey::new("/x", 99_999).max_side, SIDE_RANGE.1);
}

#[test]
fn pool_renders_everything_and_caches_it() {
    let mut pool = ThumbPool::new(Arc::new(RealFs), 3, ThumbCache::default());
    let keys: Vec<_> = library_files().into_iter().map(|p| ThumbKey::new(p, 128)).collect();
    for key in &keys {
        pool.request(key.clone(), 1);
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut done = 0;
    while done < keys.len() && Instant::now() < deadline {
        done += pool.poll_timeout(Duration::from_millis(200)).len();
    }
    assert_eq!(done, keys.len());
    let failed: Vec<_> = keys.iter().filter_map(|k| pool.error(k).map(|e| (k, e))).collect();
    assert_eq!(pool.cache().len(), keys.len() - failed.len());
    assert!(keys.iter().filter(|k| k.path.ends_with("broken_not_an_image.jpg")).all(|k| pool.error(k).is_some()));
    // Besides the broken file, only videos without filmcraft-cli installed and HEIC (off by
    // default) may fail.
    assert!(
        failed.iter().all(|(k, e)| k.path.ends_with("broken_not_an_image.jpg")
            || matches!(e, ThumbError::ToolMissing(_) | ThumbError::Unsupported(Format::Heif))),
        "{failed:?}"
    );
    // Cached and failed keys are not queued again.
    for key in &keys {
        pool.request(key.clone(), 9);
    }
    assert_eq!(pool.pending(), 0);
}

#[test]
fn retain_drops_queued_work_and_drop_stops_workers() {
    let fs = (0..200).fold(FakeFs::new(), |fs, n| fs.file(format!("/many/{n}.png"), 10));
    let mut pool = ThumbPool::new(Arc::new(fs), 1, ThumbCache::default());
    for n in 0..200 {
        pool.request(ThumbKey::new(format!("/many/{n}.png"), 64), 1);
    }
    pool.retain(|_| false);
    assert_eq!(pool.pending(), 0);
    drop(pool); // must return promptly (joins the worker)
}

/// A real JPEG with five maximum-size APP2 segments (~320 KiB, like a big ICC profile) after
/// SOI, so its SOF marker sits beyond the probe's head read.
fn jpeg_with_big_metadata() -> Vec<u8> {
    let jpg = std::fs::read(library().join("edge-cases/TINY_ICON.JPG")).unwrap();
    let mut out = jpg[..2].to_vec();
    for _ in 0..5 {
        out.extend([0xFF, 0xE2, 0xFF, 0xFF]);
        out.extend(std::iter::repeat_n(0x42, 65_533));
    }
    out.extend(&jpg[2..]);
    out
}

#[test]
fn jpeg_with_more_than_256k_of_metadata_renders() {
    let big = jpeg_with_big_metadata();
    assert!(big.len() > assetcraft_probe::HEAD_BYTES);
    let fs = FakeFs::new().file_with("/icc.jpg", big);
    let t = render_path(&fs, Path::new("/icc.jpg"), 128).unwrap();
    assert_eq!((t.source_width, t.source_height), (64, 64));
}

#[test]
fn damaged_header_in_a_decodable_format_is_still_decoded() {
    // 70 maximum-size APP2 segments (~4.5 MiB) outrun the probe's bounded segment hopping, so
    // the probe reports Damaged; the decoder reads the whole file and succeeds.
    let jpg = std::fs::read(library().join("edge-cases/TINY_ICON.JPG")).unwrap();
    let mut deep = jpg[..2].to_vec();
    for _ in 0..70 {
        deep.extend([0xFF, 0xE2, 0xFF, 0xFF]);
        deep.extend(std::iter::repeat_n(0x42, 65_533));
    }
    deep.extend(&jpg[2..]);
    let fs = FakeFs::new().file_with("/deep.jpg", deep);
    assert_eq!(assetcraft_probe::probe(&fs, Path::new("/deep.jpg")).unwrap().status, Status::Damaged);
    let t = render_path(&fs, Path::new("/deep.jpg"), 64).unwrap();
    assert_eq!((t.source_width, t.source_height), (64, 64));
}

#[test]
fn modified_time_is_part_of_the_key() {
    let t0 = std::time::UNIX_EPOCH + Duration::from_secs(1_000);
    let t1 = t0 + Duration::from_secs(1);
    let a = ThumbKey::new("/x.png", 128).with_modified(Some(t0));
    let b = ThumbKey::new("/x.png", 128).with_modified(Some(t1));
    assert_ne!(a, b, "an edited file is a different key, so it re-renders");
    let mut cache = ThumbCache::default();
    cache.insert(a.clone(), Arc::new(render_path(&RealFs, &library().join("edge-cases/TINY_ICON.JPG"), 32).unwrap()));
    assert!(cache.get(&a).is_some() && cache.get(&b).is_none());
}

fn wait_for(pool: &mut ThumbPool, n: usize) -> usize {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut done = 0;
    while done < n && Instant::now() < deadline {
        done += pool.poll_timeout(Duration::from_millis(100)).len();
    }
    done
}

#[test]
fn clear_retries_failures_and_drops_cached_and_queued_work() {
    let good = library().join("edge-cases/TINY_ICON.JPG");
    let fs = FakeFs::new()
        .file_with("/bad.png", b"\x89PNG\r\n\x1a\n".to_vec())
        .file_with("/good.jpg", std::fs::read(&good).unwrap());
    let mut pool = ThumbPool::new(Arc::new(fs), 1, ThumbCache::default());
    let (bad, ok) = (ThumbKey::new("/bad.png", 64), ThumbKey::new("/good.jpg", 64));
    pool.request(bad.clone(), 1);
    pool.request(ok.clone(), 1);
    assert_eq!(wait_for(&mut pool, 2), 2);
    assert!(pool.error(&bad).is_some() && pool.get(&ok).is_some());
    pool.request(bad.clone(), 1);
    assert_eq!(pool.pending() + pool.running(), 0, "a failed key is not retried by itself");
    pool.clear();
    assert!(pool.error(&bad).is_none() && pool.get(&ok).is_none() && pool.cache().is_empty());
    pool.request(bad.clone(), 1);
    assert_eq!(wait_for(&mut pool, 1), 1, "after clear the failed key is rendered again");
    assert!(pool.error(&bad).is_some());
}

#[test]
fn a_pool_without_workers_fails_requests_instead_of_queueing_forever() {
    let mut pool = ThumbPool::without_workers(Arc::new(FakeFs::new().file("/a.png", 10)));
    assert_eq!(pool.workers(), 0);
    let key = ThumbKey::new("/a.png", 64);
    pool.request(key.clone(), 1);
    assert_eq!(pool.pending(), 0);
    assert!(pool.error(&key).is_some(), "the UI gets a badge, not an endless placeholder");
}

#[test]
fn remembered_failures_are_bounded() {
    let mut pool = ThumbPool::without_workers(Arc::new(FakeFs::new()));
    for n in 0..MAX_FAILED + 5 {
        pool.request(ThumbKey::new(format!("/missing/{n}.png"), 64), 1);
    }
    assert!(pool.failed_len() <= MAX_FAILED, "{}", pool.failed_len());
}

#[test]
fn default_pool_worst_case_memory_is_bounded() {
    const GIB: u64 = 1024 * 1024 * 1024;
    const { assert!(WORST_CASE_BYTES <= 3 * GIB, "workers x (file + decode) must stay under 3 GiB") };
    assert!(default_workers() <= MAX_DEFAULT_WORKERS);
    assert_eq!((MAX_FILE_BYTES, MAX_DECODE_ALLOC), (128 << 20, 512 << 20));
}
