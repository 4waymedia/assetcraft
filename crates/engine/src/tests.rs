use super::*;
use assetcraft_fs::{FakeFs, RealFs};
use serde_json::json;

fn fake() -> FakeFs {
    FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .known(KnownFolder::Pictures, "/home/paul/Pictures")
        .file("/home/paul/Pictures/shot_10.png", 300)
        .file("/home/paul/Pictures/shot_2.png", 100)
        .file("/home/paul/Pictures/shot_1.png", 200)
        .dir("/home/paul/Pictures/reapers")
        .file("/home/paul/Pictures/reapers/concept.png", 50)
        .hidden("/home/paul/Pictures/.thumbs.db", 9)
        .volume("F:", "/f")
}

fn session(fs: FakeFs) -> (Session, Arc<RecordingLauncher>) {
    let launcher = Arc::new(RecordingLauncher::default());
    (Session::new(Arc::new(fs), launcher.clone(), Prefs::default()), launcher)
}

fn names(s: &Session) -> Vec<&str> {
    s.items().iter().map(|e| e.name.as_str()).collect()
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn starts_in_pictures_sorted_naturally_with_folders_first_and_hidden_filtered() {
    let (s, _) = session(fake());
    assert_eq!(s.location(), &Location::Folder("/home/paul/Pictures".into()));
    assert_eq!(names(&s), ["reapers", "shot_1.png", "shot_2.png", "shot_10.png"]);
}

#[test]
fn restores_last_folder_and_falls_back_when_it_is_gone() {
    let prefs = Prefs { last_folder: Some("/home/paul/Pictures/reapers".into()), ..Prefs::default() };
    let s = Session::new(Arc::new(fake()), Arc::new(RecordingLauncher::default()), prefs);
    assert_eq!(s.location().path(), Some(Path::new("/home/paul/Pictures/reapers")));
    let prefs = Prefs { last_folder: Some("/deleted".into()), ..Prefs::default() };
    let s = Session::new(Arc::new(fake()), Arc::new(RecordingLauncher::default()), prefs);
    assert_eq!(s.location().path(), Some(Path::new("/home/paul/Pictures")));
    let s = Session::new(Arc::new(FakeFs::new()), Arc::new(RecordingLauncher::default()), Prefs::default());
    assert_eq!(s.location(), &Location::Computer);
}

#[test]
fn back_forward_up_history() {
    let (mut s, _) = session(fake());
    run(&mut s, "nav.open", json!({"path": "/home/paul/Pictures/reapers"}));
    run(&mut s, "nav.up", json!({}));
    assert_eq!(s.location().path(), Some(Path::new("/home/paul/Pictures")));
    assert_eq!(s.selection().focus(), Some(Path::new("/home/paul/Pictures/reapers")), "up focuses the child we left");
    run(&mut s, "nav.back", json!({}));
    assert_eq!(s.location().path(), Some(Path::new("/home/paul/Pictures/reapers")));
    run(&mut s, "nav.back", json!({}));
    assert_eq!(s.location().path(), Some(Path::new("/home/paul/Pictures")));
    assert!(!s.can_back());
    run(&mut s, "nav.forward", json!({}));
    run(&mut s, "nav.forward", json!({}));
    assert_eq!(s.location().path(), Some(Path::new("/home/paul/Pictures")));
    assert!(!s.can_forward());
    // A new navigation clears forward history.
    run(&mut s, "nav.back", json!({}));
    run(&mut s, "nav.open", json!({"path": "/f"}));
    assert!(!s.can_forward());
}

#[test]
fn up_from_a_root_goes_to_computer_and_stops_there() {
    let (mut s, _) = session(fake());
    run(&mut s, "nav.open", json!({"path": "/"}));
    run(&mut s, "nav.up", json!({}));
    assert_eq!(s.location(), &Location::Computer);
    assert_eq!(names(&s), ["/", "F:"]);
    assert!(matches!(s.execute("nav.up", &json!({})), Err(CmdError::Unavailable(_))));
}

#[test]
fn failed_navigation_changes_nothing() {
    let (mut s, _) = session(fake().unreadable_dir("/locked"));
    let before = (s.location().clone(), names(&s).len());
    assert!(matches!(s.execute("nav.open", &json!({"path": "/locked"})), Err(CmdError::Fs(_))));
    assert!(matches!(s.execute("nav.open", &json!({"path": "/nope"})), Err(CmdError::Fs(_))));
    assert_eq!((s.location().clone(), names(&s).len()), before);
    assert!(!s.can_back());
}

#[test]
fn view_commands_sort_and_show_hidden() {
    let (mut s, _) = session(fake());
    run(&mut s, "view.sort", json!({"by": "size", "ascending": false}));
    assert_eq!(names(&s), ["reapers", "shot_10.png", "shot_1.png", "shot_2.png"]);
    run(&mut s, "view.hidden", json!({"show": true}));
    assert!(names(&s).contains(&".thumbs.db"));
    run(&mut s, "view.thumbSize", json!({"px": 10_000}));
    assert_eq!(s.view().thumb_size, prefs::THUMB_SIZE_RANGE.1);
    run(&mut s, "view.mode", json!({"mode": "list"}));
    assert_eq!(s.view().mode, ViewMode::List);
    assert!(matches!(s.execute("view.sort", &json!({"by": "colour"})), Err(CmdError::BadParams(_))));
}

#[test]
fn selection_survives_sort_and_refresh() {
    let (mut s, _) = session(fake());
    run(&mut s, "select.click", json!({"path": "/home/paul/Pictures/shot_2.png"}));
    run(&mut s, "select.click", json!({"path": "/home/paul/Pictures/shot_10.png", "mode": "toggle"}));
    run(&mut s, "view.sort", json!({"by": "name", "ascending": false}));
    run(&mut s, "nav.refresh", json!({}));
    assert_eq!(s.selected_paths().len(), 2);
    run(&mut s, "select.all", json!({}));
    assert_eq!(s.selected_paths().len(), 4);
    run(&mut s, "select.none", json!({}));
    assert!(s.selected_paths().is_empty());
    // Navigating clears the selection.
    run(&mut s, "select.all", json!({}));
    run(&mut s, "nav.open", json!({"path": "/f"}));
    assert!(s.selected_paths().is_empty());
}

#[test]
fn keyboard_moves_and_enter_opens() {
    let (mut s, launcher) = session(fake());
    run(&mut s, "select.move", json!({"delta": 1}));
    run(&mut s, "file.open", json!({}));
    assert_eq!(s.location().path(), Some(Path::new("/home/paul/Pictures/reapers")), "Enter on a folder navigates");
    run(&mut s, "nav.back", json!({}));
    assert_eq!(
        s.selection().focus(),
        Some(Path::new("/home/paul/Pictures/reapers")),
        "back focuses the folder we left"
    );
    run(&mut s, "select.move", json!({"delta": 2}));
    run(&mut s, "file.open", json!({}));
    run(&mut s, "file.reveal", json!({}));
    let calls = launcher.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        [
            ("open", PathBuf::from("/home/paul/Pictures/shot_2.png")),
            ("reveal", PathBuf::from("/home/paul/Pictures/shot_2.png"))
        ]
    );
}

#[test]
fn favorites_add_remove_and_persist() {
    let (mut s, _) = session(fake());
    run(&mut s, "nav.favorite.add", json!({}));
    run(&mut s, "nav.favorite.add", json!({"path": "/f"}));
    run(&mut s, "nav.favorite.add", json!({"path": "/f"})); // no duplicates
    assert!(matches!(
        s.execute("nav.favorite.add", &json!({"path": "/home/paul/Pictures/shot_1.png"})),
        Err(CmdError::BadParams(_))
    ));
    assert_eq!(s.favorites(), [PathBuf::from("/home/paul/Pictures"), PathBuf::from("/f")]);
    run(&mut s, "nav.favorite.remove", json!({"path": "/f"}));
    let prefs = Prefs::from_json(&s.prefs().to_json());
    assert_eq!(prefs.favorites, [PathBuf::from("/home/paul/Pictures")]);
    assert_eq!(prefs.last_folder.as_deref(), Some(Path::new("/home/paul/Pictures")));
}

#[test]
fn known_folders_breadcrumb_and_subfolders() {
    let (mut s, _) = session(fake());
    run(&mut s, "nav.known", json!({"folder": "home"}));
    assert_eq!(s.location().path(), Some(Path::new("/home/paul")));
    assert!(matches!(s.execute("nav.known", &json!({"folder": "videos"})), Err(CmdError::Unavailable(_))));
    assert_eq!(s.breadcrumb(), [PathBuf::from("/"), PathBuf::from("/home"), PathBuf::from("/home/paul")]);
    assert_eq!(s.subfolders(Path::new("/home/paul/Pictures")).unwrap(), [PathBuf::from("/home/paul/Pictures/reapers")]);
}

#[test]
fn every_command_is_registered_once_and_described() {
    let ids: Vec<_> = commands().iter().map(|c| c.id).collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(ids.len(), unique.len());
    assert!(commands().iter().all(|c| !c.label.is_empty() && !c.params.is_empty()));
    let (mut s, _) = session(fake());
    assert!(matches!(s.execute("nope.nope", &json!({})), Err(CmdError::Unknown(_))));
    assert_eq!(run(&mut s, "app.commands", json!({})).as_array().map(Vec::len), Some(commands().len()));
}

#[test]
fn bad_params_never_panic() {
    let (mut s, _) = session(fake());
    for c in commands() {
        for params in [json!(null), json!({}), json!({"path": 5, "px": "big", "delta": "x", "paths": "x", "show": 1})] {
            let _ = s.execute(c.id, &params);
        }
    }
}

#[test]
fn preview_toggles_follows_focus_and_closes_on_navigation() {
    let (mut s, _) = session(fake());
    assert!(matches!(s.execute("view.preview", &json!({})), Err(CmdError::Unavailable(_))), "nothing focused");
    run(&mut s, "select.move", json!({"delta": 2}));
    run(&mut s, "view.preview", json!({}));
    assert!(s.preview_open());
    run(&mut s, "select.move", json!({"delta": 1}));
    assert!(s.preview_open(), "arrow keys keep the preview open on the next item");
    run(&mut s, "view.preview", json!({"open": false}));
    assert!(!s.preview_open());
    run(&mut s, "view.preview", json!({}));
    run(&mut s, "nav.open", json!({"path": "/f"}));
    assert!(!s.preview_open(), "navigating closes the preview");
}

#[test]
fn panel_layout_is_stored_opaquely_and_bounded() {
    let (mut s, _) = session(fake());
    assert!(s.panels().is_none());
    run(&mut s, "view.panels", json!({"layout": {"root": {"Tabs": {"panels": ["content"], "active": 0}}}}));
    let saved = Prefs::from_json(&s.prefs().to_json());
    assert_eq!(saved.panels.as_ref(), s.panels());
    let restored = Session::new(Arc::new(fake()), Arc::new(RecordingLauncher::default()), saved);
    assert_eq!(restored.panels(), s.panels());
    assert!(matches!(s.execute("view.panels", &json!({"layout": [1]})), Err(CmdError::BadParams(_))));
    let huge = "x".repeat(prefs::MAX_PANELS_JSON + 1);
    assert!(matches!(s.execute("view.panels", &json!({"layout": {"big": huge}})), Err(CmdError::BadParams(_))));
    run(&mut s, "view.panels", json!({"layout": null}));
    assert!(s.panels().is_none());
}

#[cfg(unix)]
#[test]
fn non_utf8_file_names_never_panic() {
    use std::os::unix::ffi::OsStrExt;
    let odd = PathBuf::from(std::ffi::OsStr::from_bytes(b"/home/paul/Pictures/bad\xff\xfe.png"));
    let odd_dir = PathBuf::from(std::ffi::OsStr::from_bytes(b"/home/paul/Pictures/dir\xff"));
    let (mut s, launcher) = session(fake().file(&odd, 4).dir(&odd_dir));
    run(&mut s, "nav.favorite.add", json!({})); // Pictures
    run(&mut s, "select.all", json!({}));
    run(&mut s, "app.state", json!({}));
    let last = s.items().len() as i64;
    run(&mut s, "select.move", json!({"delta": last}));
    run(&mut s, "select.move", json!({"delta": -last}));
    for _ in 0..s.items().len() {
        let focus = s.selection().focus().map(Path::to_path_buf);
        if focus.as_deref() == Some(odd.as_path()) {
            run(&mut s, "file.open", json!({}));
        }
        run(&mut s, "select.move", json!({"delta": 1}));
    }
    assert!(launcher.calls.lock().unwrap().iter().any(|(_, p)| *p == odd), "the odd file can still be opened");
    run(&mut s, "select.move", json!({"delta": -last}));
    run(&mut s, "file.open", json!({})); // first item is the odd folder (folders first)
    assert_eq!(s.location().path(), Some(odd_dir.as_path()));
    assert!(s.prefs().last_folder.is_none(), "a non-UTF-8 folder is not written to prefs");
    run(&mut s, "nav.favorite.add", json!({})); // the odd folder itself
    assert_eq!(s.favorites().len(), 2);
    run(&mut s, "view.mode", json!({"mode": "list"}));
    // The saved file still carries everything that can be written, not an empty "{}".
    let saved = Prefs::from_json(&s.prefs().to_json());
    assert_eq!(saved.favorites, [PathBuf::from("/home/paul/Pictures")]);
    assert_eq!(saved.view, *s.view());
    assert_eq!(saved.view.mode, ViewMode::List);
}

/// Services that count how often volumes and known folders are scanned.
struct CountingFs {
    inner: FakeFs,
    volume_scans: std::sync::atomic::AtomicUsize,
    known_scans: std::sync::atomic::AtomicUsize,
}

impl assetcraft_fs::Services for CountingFs {
    fn list_entries(&self, dir: &Path, opts: &ListOptions) -> Result<assetcraft_fs::Listing, FsError> {
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
        self.volume_scans.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.volumes()
    }
    fn home_dir(&self) -> Option<PathBuf> {
        self.inner.home_dir()
    }
    fn known_folder(&self, which: KnownFolder) -> Option<PathBuf> {
        self.known_scans.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.known_folder(which)
    }
}

#[test]
fn volumes_and_known_folders_are_cached_not_rescanned_per_read() {
    use std::sync::atomic::Ordering::Relaxed;
    let fs = Arc::new(CountingFs { inner: fake(), volume_scans: 0.into(), known_scans: 0.into() });
    let mut s = Session::new(fs.clone(), Arc::new(RecordingLauncher::default()), Prefs::default());
    let (v0, k0) = (fs.volume_scans.load(Relaxed), fs.known_scans.load(Relaxed));
    // The Folders panel reads these every frame.
    for _ in 0..100 {
        assert_eq!(s.volumes().len(), 2);
        assert_eq!(s.known_folders().len(), 2);
    }
    assert_eq!((fs.volume_scans.load(Relaxed), fs.known_scans.load(Relaxed)), (v0, k0), "no scan per read");
    run(&mut s, "nav.refresh", json!({}));
    assert_eq!(fs.volume_scans.load(Relaxed), v0 + 1, "F5 rescans");
    run(&mut s, "nav.computer", json!({}));
    assert_eq!(fs.volume_scans.load(Relaxed), v0 + 2, "Computer rescans");
    assert_eq!(names(&s), ["/", "F:"]);
}

#[test]
fn nav_open_trims_quotes_and_makes_paths_absolute() {
    let (mut s, _) = session(fake());
    run(&mut s, "nav.open", json!({"path": "  \"/home/paul/Pictures/reapers\"  "}));
    assert_eq!(s.location().path(), Some(Path::new("/home/paul/Pictures/reapers")));
    assert!(matches!(s.execute("nav.open", &json!({"path": " \"\" "})), Err(CmdError::BadParams(_))));
    // A relative path is resolved against the working directory, never stored relative.
    // (Tests run in the crate folder; this one exists, so the navigation really happens.)
    let mut real = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut real, "nav.open", json!({"path": "../../test_data/library"}));
    assert!(real.location().path().is_some_and(Path::is_absolute), "{:?}", real.location());
    assert!(real.location().path().is_some_and(|p| p.ends_with("test_data/library")));
    let _ = s.execute("nav.open", &json!({"path": "some/relative/dir"}));
    // (`has_root`, not `is_absolute`: on Windows `/home/...` has a root but no drive.)
    assert!(s.location().path().is_some_and(Path::has_root));
    for loc in s.back.iter().chain(&s.forward) {
        assert!(loc.path().is_none_or(Path::has_root));
    }
    // Rooted paths are kept exactly as typed on every platform.
    run(&mut s, "nav.open", json!({"path": "/f"}));
    assert_eq!(s.location().path(), Some(Path::new("/f")));
}

// ---- against the real test library ---------------------------------------------------------

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
}

#[test]
fn browses_the_real_test_library_by_keyboard() {
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut s, "nav.open", json!({"path": library()}));
    assert_eq!(names(&s), ["characters", "edge-cases", "environments", "palettes", "props", "storyboard"]);
    // Down into characters/reaper/turnaround with Enter only.
    for _ in 0..3 {
        run(&mut s, "select.move", json!({"delta": 1}));
        run(&mut s, "file.open", json!({}));
    }
    assert!(s.location().path().is_some_and(|p| p.ends_with("characters/reaper/turnaround")));
    assert_eq!(names(&s), ["reaper_back.jpg", "reaper_front.jpg", "reaper_side.jpg"]);
    run(&mut s, "nav.up", json!({}));
    assert_eq!(
        names(&s),
        [
            "turnaround",
            "reaper_concept_final.png",
            "reaper_concept_v01.png",
            "reaper_concept_v02.png",
            "reaper_concept_v03.png"
        ]
    );
}

#[test]
fn probes_the_real_library_and_flags_the_broken_file() {
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    let edge = library().join("edge-cases");
    run(&mut s, "nav.open", json!({"path": edge}));
    let broken = run(&mut s, "item.probe", json!({"path": edge.join("broken_not_an_image.jpg")}));
    assert_eq!(broken["unreadable"], json!(true));
    let pano = run(&mut s, "item.probe", json!({"path": edge.join("panorama_20to1.jpg")}));
    assert_eq!((pano["width"].clone(), pano["height"].clone()), (json!(4000), json!(200)));
    let state = run(&mut s, "app.state", json!({}));
    assert_eq!(state["items"].as_array().map(Vec::len), Some(4));
}

fn wait_file_info(s: &mut Session, path: &Path) -> Probe {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        s.poll_file_info();
        match s.file_info(path) {
            FileInfo::Ready(p) => return p.clone(),
            FileInfo::Failed(e) => panic!("{}: {e}", path.display()),
            FileInfo::Pending => {
                assert!(std::time::Instant::now() < deadline, "no probe for {}", path.display());
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

#[test]
fn file_info_is_pending_until_polled_and_carries_created_time() {
    let png = std::fs::read(library().join("props/scythe_transparent.png")).unwrap();
    let fs = FakeFs::new().known(KnownFolder::Pictures, "/pics").file_with("/pics/a.png", png);
    let (mut s, _) = session(fs);
    let path = Path::new("/pics/a.png");
    assert_eq!(s.file_info(path), FileInfo::Pending, "first ask only queues the probe");
    let probe = wait_file_info(&mut s, path);
    assert!(probe.dimensions.is_some());
    let entry = s.items()[0].clone();
    assert!(entry.created.is_some() && entry.created < entry.modified, "FakeFs gives a created time");
    assert_eq!(probe.created, entry.created);
    // Created time is in the probe and state JSON (unix seconds).
    let info = run(&mut s, "item.probe", json!({ "path": "/pics/a.png" }));
    let secs = |t: Option<std::time::SystemTime>| {
        t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs())
    };
    assert_eq!(info["created"].as_u64(), secs(entry.created));
    let state = run(&mut s, "app.state", json!({}));
    assert_eq!(state["items"][0]["created"].as_u64(), secs(entry.created));
    // A refresh forgets cached probes (the file may have changed).
    run(&mut s, "nav.refresh", json!({}));
    assert_eq!(s.file_info(path), FileInfo::Pending);
    let missing = Path::new("/pics/missing.png");
    s.file_info(missing);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !matches!(s.file_info(missing), FileInfo::Failed(_)) && std::time::Instant::now() < deadline {
        s.poll_file_info();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(matches!(s.file_info(missing), FileInfo::Failed(_)));
}

#[test]
fn cache_clear_needs_a_cache_and_empties_it_in_the_background() {
    let (mut s, _) = session(fake());
    assert!(matches!(s.execute("cache.clear", &json!({})), Err(CmdError::Unavailable(_))));
    assert!(run(&mut s, "app.state", json!({}))["thumb_cache"].is_null());

    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("assetcraft-engine-cache-{}-{nanos}", std::process::id()));
    let cache = Arc::new(DiskCache::new(&dir, 1 << 20));
    let thumb = assetcraft_thumbs::Thumb { width: 2, height: 2, source_width: 2, source_height: 2, rgba: vec![1; 16] };
    let key = assetcraft_thumbs::hash::Hash128(0xab << 120);
    cache.put(key, &assetcraft_thumbs::SourceId::NONE, &thumb);
    assert!(cache.path_for(key).exists());
    let mut s = s.with_thumb_cache(Arc::clone(&cache));
    let state = run(&mut s, "app.state", json!({}));
    assert_eq!(state["thumb_cache"]["budget"], 1 << 20);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while s.execute("cache.clear", &json!({})).is_err() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    while (cache.is_busy() || cache.path_for(key).exists()) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!cache.path_for(key).exists());
    assert_eq!(cache.size_known(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}
// ---- folder watching -------------------------------------------------------------------

/// A fake disk whose contents a test can swap while a session holds it.
#[derive(Clone)]
struct SwapFs(Arc<std::sync::RwLock<FakeFs>>);

impl SwapFs {
    fn new(fs: FakeFs) -> Self {
        Self(Arc::new(std::sync::RwLock::new(fs)))
    }
    fn change(&self, f: impl FnOnce(FakeFs) -> FakeFs) {
        let mut guard = self.0.write().unwrap();
        let old = std::mem::take(&mut *guard);
        *guard = f(old);
    }
    fn get(&self) -> std::sync::RwLockReadGuard<'_, FakeFs> {
        self.0.read().unwrap()
    }
}

impl Services for SwapFs {
    fn list_entries(&self, dir: &Path, opts: &ListOptions) -> Result<assetcraft_fs::Listing, FsError> {
        self.get().list_entries(dir, opts)
    }
    fn entry(&self, path: &Path) -> Result<Entry, FsError> {
        self.get().entry(path)
    }
    fn read_head(&self, path: &Path, max: usize) -> Result<Vec<u8>, FsError> {
        self.get().read_head(path, max)
    }
    fn read_range(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.get().read_range(path, offset, len)
    }
    fn volumes(&self) -> Vec<Volume> {
        self.get().volumes()
    }
    fn home_dir(&self) -> Option<PathBuf> {
        self.get().home_dir()
    }
    fn known_folder(&self, which: KnownFolder) -> Option<PathBuf> {
        self.get().known_folder(which)
    }
}

const PICS: &str = "/home/paul/Pictures";

fn watched_session() -> (Session, SwapFs, assetcraft_watch::FakeWatcher) {
    let disk = SwapFs::new(fake());
    let mut s = Session::new(Arc::new(disk.clone()), Arc::new(RecordingLauncher::default()), Prefs::default());
    let watcher = assetcraft_watch::FakeWatcher::new();
    s.set_watcher(Box::new(watcher.clone()));
    (s, disk, watcher)
}

/// Poll the session once and apply whatever came, like the UI does each frame.
fn pump(s: &mut Session) -> Option<Value> {
    let params = s.pending_changes()?;
    Some(run(s, "nav.changed", params))
}

#[test]
fn watcher_follows_navigation_and_turns_off_at_computer() {
    let (mut s, _, watcher) = watched_session();
    assert_eq!(s.watch_status(), &WatchStatus::Live);
    assert_eq!(watcher.watching(), Some(PathBuf::from(PICS)));
    run(&mut s, "nav.open", json!({"path": "/home/paul/Pictures/reapers"}));
    assert_eq!(watcher.watching(), Some(PathBuf::from("/home/paul/Pictures/reapers")));
    run(&mut s, "select.all", json!({}));
    let calls = watcher.watch_calls();
    run(&mut s, "nav.refresh", json!({}));
    // F5 re-arms even a "live" watch (it may have died silently on Windows; review M1).
    assert_eq!(watcher.watch_calls(), calls + 1, "F5 re-creates the watch");
    run(&mut s, "select.none", json!({}));
    assert_eq!(watcher.watch_calls(), calls + 1, "other commands leave it alone");
    run(&mut s, "nav.computer", json!({}));
    assert_eq!(watcher.watching(), None);
    assert_eq!(s.watch_status(), &WatchStatus::Off);
    assert_eq!(s.pending_changes(), None);
}

#[test]
fn a_file_copied_in_appears_and_a_deleted_one_disappears_keeping_selection() {
    let (mut s, disk, watcher) = watched_session();
    run(&mut s, "select.click", json!({"path": "/home/paul/Pictures/shot_2.png"}));
    run(&mut s, "select.click", json!({"path": "/home/paul/Pictures/shot_10.png", "mode": "toggle"}));
    assert_eq!(pump(&mut s), None, "nothing happened yet");

    disk.change(|fs| fs.file("/home/paul/Pictures/shot_3.png", 42));
    watcher.emit("/home/paul/Pictures/shot_3.png");
    let result = pump(&mut s).unwrap();
    assert_eq!(result["added"], json!(["/home/paul/Pictures/shot_3.png"]));
    assert_eq!(result["rescan"], json!(false));
    assert_eq!(names(&s), ["reapers", "shot_1.png", "shot_2.png", "shot_3.png", "shot_10.png"], "sorted in place");
    assert_eq!(s.selection().len(), 2, "selection kept");
    assert_eq!(s.selection().focus(), Some(Path::new("/home/paul/Pictures/shot_10.png")));

    // Delete the focused file: it disappears, the other selected item stays selected.
    disk.change(|_| {
        // FakeFs can't delete, so rebuild it without shot_10.png.
        FakeFs::new()
            .known(KnownFolder::Home, "/home/paul")
            .known(KnownFolder::Pictures, PICS)
            .file("/home/paul/Pictures/shot_2.png", 100)
            .file("/home/paul/Pictures/shot_1.png", 200)
            .file("/home/paul/Pictures/shot_3.png", 42)
            .dir("/home/paul/Pictures/reapers")
            .hidden("/home/paul/Pictures/.new_hidden", 1)
    });
    watcher.emit("/home/paul/Pictures/shot_10.png");
    watcher.emit("/home/paul/Pictures/.new_hidden");
    watcher.emit("/elsewhere/ignored.png");
    let result = pump(&mut s).unwrap();
    assert_eq!(result["removed"], json!(["/home/paul/Pictures/shot_10.png"]));
    assert_eq!(result["added"], json!([]), "hidden files stay hidden; other folders are ignored");
    assert_eq!(names(&s), ["reapers", "shot_1.png", "shot_2.png", "shot_3.png"]);
    assert_eq!(s.selected_paths(), [PathBuf::from("/home/paul/Pictures/shot_2.png")]);
    assert_eq!(s.selection().focus(), None);
}

#[test]
fn a_modified_file_is_reported_for_thumbnail_invalidation() {
    let (mut s, disk, watcher) = watched_session();
    run(&mut s, "select.click", json!({"path": "/home/paul/Pictures/shot_1.png"}));
    // Re-adding the file gives it a new size and a later modification time.
    disk.change(|fs| fs.file("/home/paul/Pictures/shot_1.png", 999));
    watcher.emit("/home/paul/Pictures/shot_1.png");
    watcher.emit("/home/paul/Pictures/shot_2.png"); // touched but unchanged
    let result = pump(&mut s).unwrap();
    assert_eq!(result["modified"], json!(["/home/paul/Pictures/shot_1.png"]));
    assert_eq!(result["added"], json!([]));
    assert_eq!(s.items().iter().find(|e| e.name == "shot_1.png").map(|e| e.size), Some(999));
    assert_eq!(s.selection().focus(), Some(Path::new("/home/paul/Pictures/shot_1.png")));
}

#[test]
fn an_event_storm_coalesces_into_one_rescan() {
    let (mut s, disk, watcher) = watched_session();
    run(&mut s, "select.click", json!({"path": "/home/paul/Pictures/shot_2.png"}));
    disk.change(|mut fs| {
        for i in 0..1000 {
            fs = fs.file(format!("/home/paul/Pictures/copy_{i:04}.png"), 10);
        }
        fs
    });
    for i in 0..1000 {
        watcher.emit(format!("/home/paul/Pictures/copy_{i:04}.png"));
    }
    let result = pump(&mut s).unwrap();
    assert_eq!(result["rescan"], json!(true));
    assert_eq!(result["added"].as_array().map(Vec::len), Some(1000));
    assert_eq!(pump(&mut s), None, "one batch only");
    assert_eq!(watcher.batches(), 1);
    assert_eq!(s.items().len(), 1004);
    assert_eq!(s.selected_paths(), [PathBuf::from("/home/paul/Pictures/shot_2.png")]);
    // A long path list sent directly is also one re-list, not 300 stats.
    let many: Vec<String> = (0..300).map(|i| format!("/home/paul/Pictures/copy_{i:04}.png")).collect();
    assert_eq!(run(&mut s, "nav.changed", json!({ "paths": many }))["rescan"], json!(true));
    assert!(s.execute("nav.changed", &json!({})).is_err(), "needs paths or rescan");
}

#[test]
fn watch_failure_degrades_quietly_and_f5_retries() {
    let disk = SwapFs::new(fake());
    let mut s = Session::new(Arc::new(disk.clone()), Arc::new(RecordingLauncher::default()), Prefs::default());
    let watcher = assetcraft_watch::FakeWatcher::new();
    watcher.fail_with(Some("network share: watching not supported"));
    s.set_watcher(Box::new(watcher.clone()));
    assert!(matches!(s.watch_status(), WatchStatus::Unavailable(r) if r.contains("network")));
    // Navigation still works, nothing pends, and nothing is reported as an error.
    run(&mut s, "nav.open", json!({"path": "/home/paul/Pictures/reapers"}));
    assert!(matches!(s.watch_status(), WatchStatus::Unavailable(_)));
    assert_eq!(s.pending_changes(), None);
    // F5 still re-lists, and retries the watch.
    disk.change(|fs| fs.file("/home/paul/Pictures/reapers/new.png", 1));
    watcher.fail_with(None);
    run(&mut s, "nav.refresh", json!({}));
    assert!(names(&s).contains(&"new.png"));
    assert_eq!(s.watch_status(), &WatchStatus::Live);
    // A watch that breaks down later turns live updates off with one final re-list.
    watcher.break_down("too many errors");
    assert_eq!(s.pending_changes(), Some(json!({"rescan": true})));
    assert!(matches!(s.watch_status(), WatchStatus::Unavailable(r) if r == "too many errors"));
    assert_eq!(s.pending_changes(), None, "no repeats");
}

#[test]
fn f5_re_arms_a_watch_that_died_silently_and_a_vanished_folder_turns_live_updates_off() {
    let (mut s, disk, watcher) = watched_session();
    assert_eq!(s.watch_status(), &WatchStatus::Live);
    let calls = watcher.watch_calls();
    // Windows: the watch can vanish with no error while the status says Live. F5 re-arms it.
    run(&mut s, "nav.refresh", json!({}));
    assert_eq!(watcher.watch_calls(), calls + 1, "F5 watches the folder again");
    assert_eq!(s.watch_status(), &WatchStatus::Live);
    // The open folder is deleted: the rescan fails and the status says why.
    disk.change(|_| FakeFs::new().known(KnownFolder::Home, "/home/paul"));
    assert!(s.execute("nav.changed", &json!({ "rescan": true })).is_err());
    assert!(
        matches!(s.watch_status(), WatchStatus::Unavailable(r) if r.contains("can't be read")),
        "{:?}",
        s.watch_status()
    );
    assert_eq!(watcher.watching(), None, "unwatched");
    // It comes back: F5 lists it and goes live again.
    disk.change(|_| fake());
    run(&mut s, "nav.refresh", json!({}));
    assert_eq!(s.watch_status(), &WatchStatus::Live);
}

#[test]
fn real_notify_watcher_sees_a_copied_file_through_the_session() {
    let dir = std::env::temp_dir().join(format!("assetcraft-engine-watch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.png"), b"x").unwrap();
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut s, "nav.open", json!({"path": dir}));
    run(&mut s, "select.click", json!({"path": dir.join("a.png")}));
    s.set_watcher(Box::new(assetcraft_watch::NotifyWatcher::new(None)));
    assert_eq!(s.watch_status(), &WatchStatus::Live);
    std::fs::write(dir.join("b.png"), b"y").unwrap();
    let start = std::time::Instant::now();
    while s.items().len() < 2 && start.elapsed() < std::time::Duration::from_secs(5) {
        pump(&mut s);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(names(&s), ["a.png", "b.png"], "the new file appeared without F5");
    assert_eq!(s.selected_paths(), [dir.join("a.png")]);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- programs: reveal on Enter, run on Shift+Enter --------------------------------------

#[test]
fn programs_are_revealed_by_open_and_run_by_run() {
    let fs = fake()
        .file("/home/paul/Pictures/setup.EXE", 10)
        .file("/home/paul/Pictures/Shortcut.lnk", 1)
        .file("/home/paul/Pictures/deploy.sh", 1)
        .executable("/home/paul/Pictures/tool", 1);
    let (mut s, launcher) = session(fs);
    for name in ["setup.EXE", "Shortcut.lnk", "deploy.sh", "tool"] {
        let path = format!("{PICS}/{name}");
        let result = run(&mut s, "file.open", json!({ "path": path }));
        assert_eq!(result["program"], json!(true), "{name}");
        let message = result["message"].as_str().unwrap_or_default();
        assert!(message.starts_with(&format!("Revealed {name}")) && message.contains("Shift+Enter"), "{message}");
    }
    let calls = launcher.calls.lock().unwrap().clone();
    assert!(calls.iter().all(|(verb, _)| *verb == "reveal"), "{calls:?}");
    assert_eq!(calls.len(), 4);

    // file.run (Shift+Enter) really opens it; on the focused item when no path is given.
    run(&mut s, "select.click", json!({"path": "/home/paul/Pictures/setup.EXE"}));
    assert_eq!(run(&mut s, "file.run", json!({}))["opened"], json!("/home/paul/Pictures/setup.EXE"));
    // Ordinary files open either way, folders navigate.
    run(&mut s, "file.open", json!({"path": "/home/paul/Pictures/shot_1.png"}));
    let calls = launcher.calls.lock().unwrap().clone();
    assert_eq!(
        calls[4..],
        [
            ("open", PathBuf::from("/home/paul/Pictures/setup.EXE")),
            ("open", PathBuf::from("/home/paul/Pictures/shot_1.png"))
        ]
    );
    run(&mut s, "file.run", json!({"path": "/home/paul/Pictures/reapers"}));
    assert_eq!(s.location().path(), Some(Path::new("/home/paul/Pictures/reapers")));
    assert!(s.is_program(Path::new("/x/App.app")) && !s.is_program(Path::new("/x/a.png")));
}

#[test]
fn model_command_sets_and_persists_the_3d_preview_settings() {
    let (mut s, _) = session(fake());
    assert_eq!(s.model(), ModelPrefs::default());
    let out = run(&mut s, "view.model", json!({"mode": "wireframe", "turntable": true}));
    assert_eq!(out["model"]["mode"], "wireframe");
    assert_eq!(s.model().mode, ModelDisplay::Wireframe);
    assert!(s.model().turntable);
    run(&mut s, "view.model", json!({"background": "grey", "exposure": 150}));
    assert_eq!((s.model().background, s.model().exposure), (ModelBackground::Grey, 150));
    assert_eq!(s.model().mode, ModelDisplay::Wireframe, "fields not named are kept");
    let reloaded =
        Session::new(Arc::new(fake()), Arc::new(RecordingLauncher::default()), Prefs::from_json(&s.prefs().to_json()));
    assert_eq!(reloaded.model(), s.model());
    for bad in [json!({"mode": "xray"}), json!({"background": 3}), json!({"exposure": 9}), json!({"turntable": "yes"})] {
        assert!(s.execute("view.model", &bad).is_err(), "{bad}");
    }
    assert_eq!(s.model().exposure, 150, "a bad call changes nothing");
    assert_eq!(run(&mut s, "app.state", json!({}))["model"]["background"], "grey");
    let junk = Prefs::from_json(r#"{"model":{"mode":"purple","exposure":9000,"turntable":true}}"#).model;
    assert_eq!((junk.mode, junk.exposure, junk.turntable), (ModelDisplay::Shaded, 400, true));
}

#[test]
fn theme_command_sets_and_persists_the_theme() {
    let (mut s, _) = session(fake());
    assert_eq!(s.view().theme, ThemeMode::Dark, "dark by default");
    run(&mut s, "view.theme", json!({"mode": "light"}));
    assert_eq!(s.prefs().view.theme, ThemeMode::Light);
    let reloaded =
        Session::new(Arc::new(fake()), Arc::new(RecordingLauncher::default()), Prefs::from_json(&s.prefs().to_json()));
    assert_eq!(reloaded.view().theme, ThemeMode::Light);
    run(&mut s, "view.theme", json!({"mode": "system"}));
    assert_eq!(s.view().theme, ThemeMode::System);
    assert!(s.execute("view.theme", &json!({"mode": "neon"})).is_err());
    assert_eq!(s.view().theme, ThemeMode::System, "a bad mode changes nothing");
}
