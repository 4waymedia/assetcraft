//! Stage 1 UI tests: background file info, created dates, the disk thumbnail cache.

use super::*;
use crate::widgets::format_time;
use assetcraft_engine::{DiskCache, Prefs, RecordingLauncher};
use assetcraft_fs::{Entry, FakeFs, FsError, KnownFolder, ListOptions, Listing, RealFs, Services, Volume};
use egui::{Context, Pos2, RawInput, Rect};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

const SCREEN: Rect = Rect { min: Pos2::ZERO, max: Pos2 { x: 1280.0, y: 800.0 } };

fn frame(ctx: &Context, nav: &mut Navigator) {
    let input = RawInput { screen_rect: Some(SCREEN), focused: true, ..RawInput::default() };
    let mut output = ctx.run_ui(input, |ui| nav.ui(ui));
    output.textures_delta.clear();
}

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
}

/// Services that count file reads made on one thread (the UI thread under test).
struct Watched {
    inner: FakeFs,
    ui_thread: ThreadId,
    ui_reads: Arc<AtomicUsize>,
}

impl Watched {
    fn note(&self) {
        if std::thread::current().id() == self.ui_thread {
            self.ui_reads.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Services for Watched {
    fn list_entries(&self, dir: &Path, opts: &ListOptions) -> Result<Listing, FsError> {
        self.inner.list_entries(dir, opts)
    }
    fn entry(&self, path: &Path) -> Result<Entry, FsError> {
        self.inner.entry(path)
    }
    fn read_head(&self, path: &Path, max: usize) -> Result<Vec<u8>, FsError> {
        self.note();
        self.inner.read_head(path, max)
    }
    fn read_range(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.note();
        self.inner.read_range(path, offset, len)
    }
    fn volumes(&self) -> Vec<Volume> {
        self.inner.volumes()
    }
    fn home_dir(&self) -> Option<PathBuf> {
        self.inner.home_dir()
    }
    fn known_folder(&self, which: KnownFolder) -> Option<PathBuf> {
        self.inner.known_folder(which)
    }
}

fn metadata_value(nav: &Navigator, key: &str) -> Option<String> {
    nav.metadata_rows().iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

#[test]
fn ui_frames_never_probe_on_the_ui_thread_and_facts_arrive_by_polling() {
    let png = std::fs::read(library().join("props/scythe_transparent.png")).unwrap_or_default();
    let fs = (0..30).fold(FakeFs::new().known(KnownFolder::Pictures, "/pics"), |fs, n| {
        fs.file_with(format!("/pics/{n:02}.png"), png.clone())
    });
    let ui_reads = Arc::new(AtomicUsize::new(0));
    let watched = Watched { inner: fs, ui_thread: std::thread::current().id(), ui_reads: Arc::clone(&ui_reads) };
    let session = Session::new(Arc::new(watched), Arc::new(RecordingLauncher::default()), Prefs::default());
    let mut nav = Navigator::with_one_worker(session);
    nav.run("view.mode", json!({ "mode": "list" }));
    nav.run("select.click", json!({ "path": "/pics/00.png" }));
    let ctx = Context::default();

    // (A frame can already show the facts: egui may run more than one pass per frame, and the
    // next pass polls. The engine test covers `FileInfo::Pending` itself.)
    frame(&ctx, &mut nav);
    let deadline = Instant::now() + Duration::from_secs(10);
    while metadata_value(&nav, "Dimensions").as_deref() == Some(PENDING) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        frame(&ctx, &mut nav);
    }
    let dims = metadata_value(&nav, "Dimensions").unwrap_or_default();
    assert!(dims.ends_with(" px") && dims.contains('×'), "probe arrived: {dims}");
    for _ in 0..5 {
        frame(&ctx, &mut nav);
    }
    assert_eq!(ui_reads.load(Ordering::Relaxed), 0, "no file was read on the UI thread");
    assert!(nav.session().file_info_busy() <= 30);
}

#[test]
fn created_date_appears_in_the_metadata_panel() {
    let fs = FakeFs::new().known(KnownFolder::Pictures, "/pics").file("/pics/a.txt", 5).dir("/pics/sub");
    let session = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    let mut nav = Navigator::with_one_worker(session);
    let ctx = Context::default();
    for path in ["/pics/a.txt", "/pics/sub"] {
        nav.run("select.click", json!({ "path": path }));
        let entry = nav.session().items().iter().find(|e| e.path == Path::new(path)).cloned().unwrap();
        let expected = format_time(entry.created);
        assert!(!expected.is_empty());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            frame(&ctx, &mut nav);
            let ready = metadata_value(&nav, "Format").as_deref() != Some(PENDING);
            if ready || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(metadata_value(&nav, "Created"), Some(expected), "{path}");
    }
}

#[test]
fn the_navigator_uses_the_sessions_disk_cache_and_clear_empties_it() {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("assetcraft-ui-thumbs-{}-{nanos}", std::process::id()));
    let cache = Arc::new(DiskCache::new(&dir, 64 << 20));
    let session = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default())
        .with_thumb_cache(Arc::clone(&cache));
    let mut nav = Navigator::new(session);
    assert!(nav.open_folder(&library().join("props")));
    let ctx = Context::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    while cache.stats().2 < 2 && Instant::now() < deadline {
        frame(&ctx, &mut nav);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(cache.stats().2, 2, "both props thumbnails were written to disk");
    let item = crate::menu::menus(&nav)
        .into_iter()
        .flat_map(|m| m.entries)
        .find(|e| matches!(e, MenuEntry::Item { label, .. } if label == "Clear Thumbnail Cache"));
    assert!(matches!(item, Some(MenuEntry::Item { enabled: true, .. })), "View has Clear Thumbnail Cache");
    while cache.is_busy() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(nav.run("cache.clear", json!({})).is_some());
    while (cache.is_busy() || cache.size_known() != Some(0)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(cache.size_known(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}

fn open_in_menu(nav: &Navigator) -> Vec<MenuEntry> {
    crate::menu::menus(nav)
        .into_iter()
        .filter(|m| m.title == "File")
        .flat_map(|m| m.entries)
        .find_map(|e| match e {
            MenuEntry::Sub { label, entries } if label == "Open In" => Some(entries),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn file_open_in_lists_sibling_apps_greys_missing_ones_and_launches_installed_ones() {
    use assetcraft_engine::FakeLocator;
    let fs = FakeFs::new().known(KnownFolder::Pictures, "/pics").file("/pics/a.png", 5).file("/pics/b.pdf", 5);
    let launcher = Arc::new(RecordingLauncher::default());
    let locator = FakeLocator { installed: vec![("photocraft", PathBuf::from("/opt/photocraft"))] };
    let session = Session::new(Arc::new(fs), launcher.clone(), Prefs::default()).with_app_locator(Arc::new(locator));
    let mut nav = Navigator::with_one_worker(session);
    let ctx = Context::default();

    let none = open_in_menu(&nav);
    assert!(matches!(none.as_slice(), [MenuEntry::Item { enabled: false, .. }]), "nothing selected: {none:?}");

    nav.run("select.click", json!({ "path": "/pics/a.png" }));
    let entries = open_in_menu(&nav);
    let labels: Vec<(&str, bool)> = entries
        .iter()
        .filter_map(|e| match e {
            MenuEntry::Item { label, enabled, .. } => Some((label.as_str(), *enabled)),
            _ => None,
        })
        .collect();
    assert_eq!(labels, [("PhotoCraft", true), ("LightCraft", false), ("FilmCraft", false)]);
    let Some(MenuEntry::Item { tooltip: Some(tip), .. }) = entries.get(1) else { panic!("{entries:?}") };
    assert!(tip.contains("not installed") && tip.contains("ASSETCRAFT_LIGHTCRAFT_EXE"), "{tip}");
    assert!(!nav.activate(&ctx, &entries[1]), "a missing app can't be chosen");
    assert!(nav.activate(&ctx, &entries[0]));
    let spawned = launcher.spawned.lock().map(|s| s.clone()).unwrap_or_default();
    assert_eq!(spawned, [(PathBuf::from("/opt/photocraft"), vec![std::ffi::OsString::from("/pics/a.png")])]);
    assert_eq!(nav.message(), Some("Opened a.png in PhotoCraft"));

    // The context menu offers the same submenu; one frame with it closed draws fine.
    frame(&ctx, &mut nav);
    nav.run("select.set", json!({ "paths": ["/pics/a.png", "/pics/b.pdf"] }));
    let mixed = open_in_menu(&nav);
    assert!(
        matches!(mixed.as_slice(), [MenuEntry::Item { enabled: false, label, .. }] if label.starts_with("No sibling"))
    );
}

/// Services that count `is_executable` calls (a stat on Unix).
struct CountExec {
    inner: FakeFs,
    calls: Arc<AtomicUsize>,
}

impl Services for CountExec {
    fn list_entries(&self, dir: &Path, opts: &ListOptions) -> Result<Listing, FsError> {
        self.inner.list_entries(dir, opts)
    }
    fn entry(&self, path: &Path) -> Result<Entry, FsError> {
        self.inner.entry(path)
    }
    fn read_head(&self, path: &Path, max: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_head(path, max)
    }
    fn read_range(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_range(path, offset, len)
    }
    fn volumes(&self) -> Vec<Volume> {
        self.inner.volumes()
    }
    fn home_dir(&self) -> Option<PathBuf> {
        self.inner.home_dir()
    }
    fn known_folder(&self, which: KnownFolder) -> Option<PathBuf> {
        self.inner.known_folder(which)
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.is_executable(path)
    }
}

#[test]
fn drawing_the_grid_never_stats_files_for_the_context_menu() {
    let fs = (0..40)
        .fold(FakeFs::new().known(KnownFolder::Pictures, "/pics"), |fs, n| fs.file(format!("/pics/{n:02}.png"), 5));
    let calls = Arc::new(AtomicUsize::new(0));
    let services = CountExec { inner: fs, calls: Arc::clone(&calls) };
    let session = Session::new(Arc::new(services), Arc::new(RecordingLauncher::default()), Prefs::default());
    let mut nav = Navigator::with_one_worker(session);
    let ctx = Context::default();
    for _ in 0..5 {
        frame(&ctx, &mut nav);
    }
    assert!(
        nav.drawn().len() >= 10,
        "cells were drawn: {} of {} at {:?}",
        nav.drawn().len(),
        nav.session().items().len(),
        nav.session().location()
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0, "is_program runs only when a context menu opens");
}

#[test]
fn open_in_entries_are_cached_until_a_command_runs() {
    let fs = FakeFs::new().known(KnownFolder::Pictures, "/pics").file("/pics/a.png", 5).file("/pics/b.pdf", 5);
    let session = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    let mut nav = Navigator::with_one_worker(session);
    nav.run("select.click", json!({ "path": "/pics/a.png" }));
    let first = open_in_menu(&nav);
    let generation = nav.session().generation();
    assert_eq!(open_in_menu(&nav), first);
    assert!(nav.state.open_in_cache.borrow().as_ref().is_some_and(|(g, _)| *g == generation));
    let Some(MenuEntry::Item { action: MenuAction::Command(_, params), .. }) = first.first() else {
        panic!("{first:?}")
    };
    assert!(params.get("paths").is_none(), "the selection is the default; no path list per frame");
    nav.run("select.click", json!({ "path": "/pics/b.pdf" }));
    let labels: Vec<String> = open_in_menu(&nav)
        .into_iter()
        .filter_map(|e| match e {
            MenuEntry::Item { label, .. } => Some(label),
            _ => None,
        })
        .collect();
    assert_eq!(labels, ["PdfCraft"], "rebuilt for the new selection");
}

#[test]
fn view_formats_heic_is_a_check_item_greyed_without_the_decoder() {
    let fs = FakeFs::new().known(KnownFolder::Pictures, "/pics").file("/pics/a.png", 5);
    let session = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    let mut nav = Navigator::with_one_worker(session);
    let ctx = Context::default();
    let find = |nav: &Navigator| {
        crate::menu::menus(nav)
            .into_iter()
            .filter(|m| m.title == "View")
            .flat_map(|m| m.entries)
            .find_map(|e| match e {
                MenuEntry::Sub { label, entries } if label == "Formats" => entries.into_iter().next(),
                _ => None,
            })
            .unwrap()
    };
    let item = find(&nav);
    let MenuEntry::Item { label, enabled, checked, tooltip, .. } = &item else { panic!("{item:?}") };
    assert_eq!(label, "HEIC / HEIF thumbnails (HEVC)");
    assert_eq!((*enabled, *checked), (assetcraft_thumbs::HEIC_BUILT, Some(false)));
    if !assetcraft_thumbs::HEIC_BUILT {
        assert_eq!(tooltip.as_deref(), Some(assetcraft_engine::NO_HEIC_BUILD));
        assert!(!nav.activate(&ctx, &item), "greyed");
        assert!(nav.run("formats.heic", json!({ "enabled": true })).is_none());
        assert_eq!(nav.message(), Some(assetcraft_engine::NO_HEIC_BUILD));
        return;
    }
    assert!(nav.activate(&ctx, &item));
    assert!(nav.session().decode_options().heic);
    assert!(matches!(find(&nav), MenuEntry::Item { checked: Some(true), .. }));
    assert!(nav.session().prefs().formats.heic, "saved");
}
