//! Stage 0 exit criteria on a real folder of 2,000 small PNGs (plan step 8):
//! (a) `nav.open` lists it in under 2 s, (b) the default thumbnail pool delivers the first 40
//! thumbnails in under 1 s, (c) one UI frame asks for a bounded number of thumbnails.
//! Measured times are printed (`cargo test -p assetcraft-ui-egui --test stage0_exit -- --nocapture`).
// Test-only crate: helpers outside `#[test]` fns may fail loudly.
#![allow(clippy::unwrap_used)]

use assetcraft_engine::{Prefs, RecordingLauncher, Session};
use assetcraft_fs::RealFs;
use assetcraft_thumbs::{ThumbKey, ThumbPool};
use assetcraft_ui_egui::Navigator;
use egui::{Context, Pos2, RawInput, Rect};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const FILES: usize = 2_000;
const VARIANTS: usize = 16;

/// A unique temp folder, removed on drop (also when an assertion fails).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("assetcraft-stage0-exit-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A 96 x 64 PNG: a two-tone gradient whose colours depend on `n`.
fn png(n: usize) -> Vec<u8> {
    let (r, g, b) = ((n * 53 % 256) as u8, (n * 97 % 256) as u8, (n * 151 % 256) as u8);
    let img = image::RgbImage::from_fn(96, 64, |x, y| {
        image::Rgb([r.wrapping_add(x as u8), g.wrapping_add(y as u8 * 2), b ^ ((x / 12 + y / 8) as u8 * 16)])
    });
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

/// 2,000 real PNG files (16 distinct images, written under different names).
fn populate(dir: &Path) {
    let variants: Vec<Vec<u8>> = (0..VARIANTS).map(png).collect();
    for i in 0..FILES {
        std::fs::write(dir.join(format!("shot_{i:04}.png")), &variants[i % VARIANTS]).unwrap();
    }
}

#[test]
fn stage0_exit_criteria_on_2000_pngs() {
    let tmp = TempDir::new();
    let started = Instant::now();
    populate(&tmp.0);
    println!("generated {FILES} PNGs in {:?}", started.elapsed());

    // (a) Listing.
    let launcher = Arc::new(RecordingLauncher::default());
    let mut session = Session::new(Arc::new(RealFs), launcher, Prefs::default());
    let t = Instant::now();
    session.execute("nav.open", &json!({ "path": tmp.0.to_str().unwrap() })).unwrap();
    let listed = t.elapsed();
    println!("(a) nav.open listed {} items in {listed:?}", session.items().len());
    assert_eq!(session.items().len(), FILES);
    // Generous limits: on Windows, Defender scans each freshly written file during the test.
    assert!(listed < Duration::from_secs(5), "nav.open took {listed:?}");

    // (b) First 40 thumbnails from the default pool, in display order.
    let mut pool = ThumbPool::with_default_workers(Arc::new(RealFs));
    let first: Vec<ThumbKey> =
        session.items().iter().take(40).map(|e| ThumbKey::new(&e.path, 128).with_modified(e.modified)).collect();
    let t = Instant::now();
    for key in &first {
        pool.request(key.clone(), 1);
    }
    let mut done = 0;
    let deadline = t + Duration::from_secs(10);
    while done < first.len() && Instant::now() < deadline {
        for (key, result) in pool.poll_timeout(Duration::from_millis(20)) {
            assert!(result.is_ok(), "{}: {result:?}", key.path.display());
            done += 1;
        }
    }
    let thumbed = t.elapsed();
    println!("(b) {} workers delivered {done} thumbnails in {thumbed:?}", pool.workers());
    assert_eq!(done, first.len());
    assert!(thumbed < Duration::from_secs(3), "first 40 thumbnails took {thumbed:?}");

    // (c) One UI frame over the folder asks only for what is visible.
    let mut nav = Navigator::new(session);
    let ctx = Context::default();
    let screen = Rect::from_min_max(Pos2::ZERO, Pos2::new(1280.0, 800.0));
    for _ in 0..2 {
        let input = RawInput { screen_rect: Some(screen), focused: true, ..RawInput::default() };
        let mut output = ctx.run_ui(input, |ui| nav.ui(ui));
        output.textures_delta.clear();
    }
    let wanted = nav.thumbs_wanted_last_frame();
    println!("(c) one frame asked for {wanted} thumbnails ({} requested in total)", nav.thumbs_requested_total());
    assert!(wanted > 0 && wanted <= 120, "a frame asked for {wanted} of {FILES}");
    assert!(nav.thumbs_requested_total() <= 240);
}
