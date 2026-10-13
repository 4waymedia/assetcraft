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
    let mut rendered = 0;
    for path in library_files() {
        let result = render_path(&RealFs, &path, 256);
        if path.ends_with("broken_not_an_image.jpg") {
            assert_eq!(result, Err(ThumbError::Unreadable(Status::Unrecognized)));
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
    assert_eq!(rendered, 23);
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
    let psd = {
        let mut b = b"8BPS".to_vec();
        b.extend(1u16.to_be_bytes());
        b.extend([0u8; 8]);
        b.extend(64u32.to_be_bytes());
        b.extend(64u32.to_be_bytes());
        b
    };
    let fs = FakeFs::new().file_with("/a.psd", psd).file_with("/b.png", b"\x89PNG\r\n\x1a\n".to_vec());
    assert_eq!(render_path(&fs, Path::new("/a.psd"), 128), Err(ThumbError::Unsupported(Format::Psd)));
    assert_eq!(render_path(&fs, Path::new("/b.png"), 128), Err(ThumbError::Unreadable(Status::Damaged)));
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
    assert_eq!(pool.cache().len(), 23);
    assert!(keys.iter().filter(|k| k.path.ends_with("broken_not_an_image.jpg")).all(|k| pool.error(k).is_some()));
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
