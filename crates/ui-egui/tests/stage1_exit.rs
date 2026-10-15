//! Stage 1 exit criteria (plan step 8): a mixed folder with every supported format shows a
//! thumbnail for each file, and a file copied in appears without a refresh, through the real
//! `notify` watcher. Videos need `filmcraft-cli`; without it they show their type icon.
//! Measured times are printed (`cargo test -p assetcraft-ui-egui --test stage1_exit -- --nocapture`).
// Test-only crate: helpers outside `#[test]` fns may fail loudly.
#![allow(clippy::unwrap_used)]

use assetcraft_engine::{Prefs, RecordingLauncher, Session};
use assetcraft_fs::RealFs;
use assetcraft_ui_egui::{Badge, Navigator};
use egui::{Context, Pos2, RawInput, Rect};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// One sample per supported format, from `test_data/library`.
const SAMPLES: &[&str] = &[
    "characters/reaper/reaper_concept_v01.png",
    "characters/reaper/turnaround/reaper_front.jpg",
    "props/soul_orb_small.gif",
    "palettes/reapers_palette.bmp",
    "environments/frozen_lake_wide.webp",
    "storyboard/shot_010_intro.tiff",
    "palettes/reapers_palette_board.psd",
    "environments/clearing_raw_orient6.dng",
    "environments/lantern_dusk.heic",
    "environments/location_breakdown.pdf",
    "props/models/soul_orb_icosphere.glb",
    "props/models/scythe_ring.gltf",
    "props/models/tombstone.obj",
    "environments/snow_clearing_flythrough.mp4",
];

/// Videos may be a type icon: their poster frame needs `filmcraft-cli`.
const VIDEO: &str = "snow_clearing_flythrough.mp4";

/// HEIC is a type icon until `formats.heic` is on (and the build has `--features heic`).
const HEIC: &str = "lantern_dusk.heic";

/// Generous limits: on Windows, Defender scans each freshly written file during the test.
const THUMBNAILS_WITHIN: Duration = Duration::from_secs(90);
const NEW_FILE_WITHIN: Duration = Duration::from_secs(15);

/// A unique temp folder, removed on drop (also when an assertion fails).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("assetcraft-stage1-exit-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // macOS: canonical, so paths match what FSEvents reports (/var → /private/var). Not on
        // Windows, where canonicalize returns a `\\?\` verbatim path.
        if cfg!(target_os = "macos") { Self(dir.canonicalize().unwrap_or(dir)) } else { Self(dir) }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
}

fn frame(ctx: &Context, nav: &mut Navigator) {
    let screen = Rect::from_min_max(Pos2::ZERO, Pos2::new(1600.0, 1000.0));
    let input = RawInput { screen_rect: Some(screen), focused: true, ..RawInput::default() };
    let mut output = ctx.run_ui(input, |ui| nav.ui(ui));
    output.textures_delta.clear();
}

fn badge(nav: &Navigator, name: &str) -> Option<Badge> {
    nav.drawn().iter().find(|(p, _)| p.file_name().is_some_and(|n| n == name)).map(|(_, b)| *b)
}

fn settled(nav: &Navigator, name: &str) -> bool {
    !matches!(badge(nav, name), None | Some(Badge::Loading))
}

#[test]
fn stage1_exit_mixed_folder_thumbnails_and_live_updates() {
    let tmp = TempDir::new();
    let names: Vec<String> = SAMPLES
        .iter()
        .map(|rel| {
            let src = library().join(rel);
            let name = src.file_name().unwrap().to_string_lossy().into_owned();
            std::fs::copy(&src, tmp.0.join(&name)).unwrap_or_else(|e| panic!("{rel}: {e}"));
            name
        })
        .collect();

    let session = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    let mut nav = Navigator::new(session);
    nav.set_watcher(Box::new(assetcraft_watch::NotifyWatcher::new(None)));
    assert!(nav.open_folder(&tmp.0), "{:?}", nav.message());
    assert_eq!(nav.session().items().len(), SAMPLES.len());
    assert_eq!(nav.session().watch_status(), &assetcraft_engine::WatchStatus::Live);

    // (a) Every file gets a thumbnail (a video may get its type icon).
    let ctx = Context::default();
    let t = Instant::now();
    while !names.iter().all(|n| settled(&nav, n)) && t.elapsed() < THUMBNAILS_WITHIN {
        frame(&ctx, &mut nav);
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("(a) {} thumbnails settled in {:?}", names.len(), t.elapsed());
    for name in &names {
        let got = badge(&nav, name);
        let ok = match name.as_str() {
            HEIC => got == Some(Badge::TypeIcon),
            VIDEO => matches!(got, Some(Badge::Image | Badge::TypeIcon)),
            _ => got == Some(Badge::Image),
        };
        assert!(ok, "{name}: {got:?}");
    }
    println!("    video: {:?} (TypeIcon = filmcraft-cli not installed)", badge(&nav, VIDEO));

    // HEIC opt-in: with the decoder built in, switching it on re-renders the file.
    let switched = nav.run("formats.heic", serde_json::json!({ "enabled": true }));
    assert_eq!(switched.is_some(), assetcraft_thumbs::HEIC_BUILT, "{:?}", nav.message());
    if assetcraft_thumbs::HEIC_BUILT {
        let t = Instant::now();
        while badge(&nav, HEIC) != Some(Badge::Image) && t.elapsed() < THUMBNAILS_WITHIN {
            frame(&ctx, &mut nav);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(badge(&nav, HEIC), Some(Badge::Image), "HEIC renders once switched on");
        nav.run("formats.heic", serde_json::json!({ "enabled": false }));
        frame(&ctx, &mut nav);
    }

    // (b) A file copied in appears without nav.refresh, and gets its thumbnail.
    let new_name = "copied_in_later.png";
    let t = Instant::now();
    std::fs::copy(library().join("environments/ruins_vertical.png"), tmp.0.join(new_name)).unwrap();
    let listed = |nav: &Navigator| nav.session().items().iter().any(|e| e.name == new_name);
    while !listed(&nav) && t.elapsed() < NEW_FILE_WITHIN {
        frame(&ctx, &mut nav);
        std::thread::sleep(Duration::from_millis(20));
    }
    let appeared = t.elapsed();
    println!("(b) the copied file appeared in {appeared:?}");
    assert!(listed(&nav), "{new_name} did not appear within {NEW_FILE_WITHIN:?}");
    assert_eq!(nav.session().items().len(), SAMPLES.len() + 1);
    while !settled(&nav, new_name) && t.elapsed() < NEW_FILE_WITHIN + THUMBNAILS_WITHIN {
        frame(&ctx, &mut nav);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(badge(&nav, new_name), Some(Badge::Image));
}
