//! Headless UI tests: drive `Navigator::ui` through `egui::Context::run_ui` with synthetic
//! input, then check the engine state it produced. No window, no GPU.

use super::*;
use assetcraft_engine::{Prefs, RecordingLauncher, SortKey};
use assetcraft_fs::{FakeFs, KnownFolder, RealFs, Services};
use egui::{Context, Event, Key, Modifiers, Pos2, RawInput, Rect};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const SCREEN: Rect = Rect { min: Pos2::ZERO, max: Pos2 { x: 1280.0, y: 800.0 } };

fn navigator(fs: impl Services + 'static, prefs: Prefs) -> (Navigator, Arc<RecordingLauncher>) {
    let launcher = Arc::new(RecordingLauncher::default());
    let session = Session::new(Arc::new(fs), launcher.clone(), prefs);
    (Navigator::with_one_worker(session), launcher)
}

/// One frame with `events`.
fn frame_with(ctx: &Context, nav: &mut Navigator, events: Vec<Event>) {
    let input = RawInput { screen_rect: Some(SCREEN), events, focused: true, ..RawInput::default() };
    let mut output = ctx.run_ui(input, |ui| nav.ui(ui));
    output.textures_delta.clear(); // no renderer here
}

fn frame(ctx: &Context, nav: &mut Navigator) {
    frame_with(ctx, nav, Vec::new());
}

fn press(ctx: &Context, nav: &mut Navigator, key: Key, modifiers: Modifiers) {
    let event = Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
    frame_with(ctx, nav, vec![event]);
}

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
}

fn fake() -> FakeFs {
    FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .known(KnownFolder::Pictures, "/home/paul/Pictures")
        .dir("/home/paul/Pictures/reapers")
        .file("/home/paul/Pictures/reapers/concept.png", 50)
        .dir("/home/paul/Pictures/shots")
        .file("/home/paul/Pictures/a.png", 10)
        .file("/home/paul/Pictures/b.png", 10)
        .file("/home/paul/Pictures/c.png", 10)
        .file("/home/paul/Pictures/d.png", 10)
        .file("/home/paul/Pictures/e.png", 10)
        .file("/home/paul/Pictures/f.png", 10)
        .volume("F:", "/f")
}

/// Frames until no thumbnail work is left (bounded), so badges settle.
fn settle(ctx: &Context, nav: &mut Navigator) {
    for _ in 0..200 {
        frame(ctx, nav);
        if nav.thumbs.busy() == 0 && !nav.drawn().iter().any(|(_, b)| *b == Badge::Loading) {
            return;
        }
        nav.wait_thumbs(Duration::from_millis(50));
    }
}

#[test]
fn renders_library_root_edge_cases_and_computer_without_panic() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(RealFs, Prefs::default());
    assert!(nav.open_folder(&library()));
    frame(&ctx, &mut nav);
    frame(&ctx, &mut nav);
    let folders = nav.drawn().iter().filter(|(_, b)| *b == Badge::Folder).count();
    assert_eq!(folders, 6, "six library folders drawn with folder glyphs: {:?}", nav.drawn());

    assert!(nav.open_folder(&library().join("edge-cases")));
    settle(&ctx, &mut nav);
    let badge_of = |nav: &Navigator, name: &str| {
        nav.drawn().iter().find(|(p, _)| p.file_name().is_some_and(|n| n == name)).map(|(_, b)| *b)
    };
    assert_eq!(badge_of(&nav, "broken_not_an_image.jpg"), Some(Badge::Unreadable));
    assert_eq!(badge_of(&nav, "TINY_ICON.JPG"), Some(Badge::Image));
    assert_eq!(badge_of(&nav, "panorama_20to1.jpg"), Some(Badge::Image));

    // Select the broken file so preview and metadata draw it too, then the list view.
    let broken = library().join("edge-cases/broken_not_an_image.jpg");
    nav.run("select.click", json!({ "path": broken }));
    nav.run("view.mode", json!({ "mode": "list" }));
    settle(&ctx, &mut nav);
    assert_eq!(nav.drawn().len(), 4);

    nav.run("nav.computer", json!({}));
    frame(&ctx, &mut nav);
    assert!(matches!(nav.session().location(), Location::Computer));
    assert!(nav.message().is_none(), "no errors: {:?}", nav.message());
}

#[test]
fn keys_drive_the_engine() {
    let ctx = Context::default();
    let (mut nav, launcher) = navigator(fake(), Prefs::default());
    frame(&ctx, &mut nav);
    assert_eq!(nav.session().location().path(), Some(Path::new("/home/paul/Pictures")));
    let focus = |nav: &Navigator| nav.session().selection().focus().map(Path::to_path_buf);

    press(&ctx, &mut nav, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(focus(&nav), Some(PathBuf::from("/home/paul/Pictures/reapers")), "first press focuses the first item");
    press(&ctx, &mut nav, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(focus(&nav), Some(PathBuf::from("/home/paul/Pictures/shots")));
    press(&ctx, &mut nav, Key::ArrowLeft, Modifiers::NONE);

    // Down moves by a whole grid row.
    let columns = nav.grid_columns();
    assert!(columns > 1, "the grid has several columns at 1280 px");
    press(&ctx, &mut nav, Key::ArrowDown, Modifiers::NONE);
    let index = nav.session().items().iter().position(|e| Some(&e.path) == focus(&nav).as_ref());
    assert_eq!(index, Some(columns.min(nav.session().items().len() - 1)));
    press(&ctx, &mut nav, Key::ArrowUp, Modifiers::NONE);
    assert_eq!(focus(&nav), Some(PathBuf::from("/home/paul/Pictures/reapers")));

    // Enter on a folder navigates; Alt+Left comes back and focuses it.
    press(&ctx, &mut nav, Key::Enter, Modifiers::NONE);
    assert_eq!(nav.session().location().path(), Some(Path::new("/home/paul/Pictures/reapers")));
    press(&ctx, &mut nav, Key::ArrowLeft, Modifiers::ALT);
    assert_eq!(nav.session().location().path(), Some(Path::new("/home/paul/Pictures")));
    assert_eq!(focus(&nav), Some(PathBuf::from("/home/paul/Pictures/reapers")));
    press(&ctx, &mut nav, Key::ArrowRight, Modifiers::ALT);
    assert_eq!(nav.session().location().path(), Some(Path::new("/home/paul/Pictures/reapers")));

    // Backspace goes up.
    press(&ctx, &mut nav, Key::Backspace, Modifiers::NONE);
    assert_eq!(nav.session().location().path(), Some(Path::new("/home/paul/Pictures")));

    // Ctrl+A selects everything; Shift+Right extends from the focus.
    press(&ctx, &mut nav, Key::A, Modifiers::COMMAND);
    assert_eq!(nav.session().selection().len(), nav.session().items().len());
    press(&ctx, &mut nav, Key::ArrowRight, Modifiers::NONE);
    press(&ctx, &mut nav, Key::ArrowRight, Modifiers::SHIFT);
    assert_eq!(nav.session().selection().len(), 2);

    // Enter on a file opens it through the launcher; Space opens the preview, Esc closes it.
    press(&ctx, &mut nav, Key::End, Modifiers::NONE);
    press(&ctx, &mut nav, Key::Enter, Modifiers::NONE);
    let calls = launcher.calls.lock().map(|c| c.clone()).unwrap_or_default();
    assert_eq!(calls, [("open", PathBuf::from("/home/paul/Pictures/f.png"))]);
    press(&ctx, &mut nav, Key::Space, Modifiers::NONE);
    assert!(nav.session().preview_open());
    press(&ctx, &mut nav, Key::Escape, Modifiers::NONE);
    assert!(!nav.session().preview_open());

    // F5 refreshes without losing the focus.
    press(&ctx, &mut nav, Key::F5, Modifiers::NONE);
    assert_eq!(focus(&nav), Some(PathBuf::from("/home/paul/Pictures/f.png")));
}

#[test]
fn typing_in_the_path_field_does_not_trigger_shortcuts_and_bad_paths_are_reported() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(fake(), Prefs::default());
    frame(&ctx, &mut nav);
    ctx.memory_mut(|m| m.request_focus(pathbar::edit_id()));
    frame(&ctx, &mut nav);
    press(&ctx, &mut nav, Key::Backspace, Modifiers::NONE);
    assert_eq!(nav.session().location().path(), Some(Path::new("/home/paul/Pictures")), "Backspace edits text");
    nav.state.path_text = "/does/not/exist".into();
    press(&ctx, &mut nav, Key::Enter, Modifiers::NONE);
    assert!(nav.message().is_some_and(|m| m.contains("not found")), "{:?}", nav.message());
    assert_eq!(nav.session().location().path(), Some(Path::new("/home/paul/Pictures")));
}

#[test]
fn a_huge_folder_only_asks_for_visible_thumbnails() {
    let ctx = Context::default();
    let mut fs = FakeFs::new().known(KnownFolder::Pictures, "/big");
    for i in 0..2_500 {
        fs = fs.file(format!("/big/shot_{i:05}.png"), 64);
    }
    let (mut nav, _) = navigator(fs, Prefs::default());
    frame(&ctx, &mut nav);
    frame(&ctx, &mut nav);
    assert_eq!(nav.session().items().len(), 2_500);
    let wanted = nav.thumbs_wanted_last_frame();
    assert!(wanted > 0 && wanted < 100, "only visible cells ask for thumbnails, got {wanted}");
    assert!(nav.drawn().len() < 100, "only visible cells are laid out, got {}", nav.drawn().len());
    // Jump to the end (the first key press focuses the first item): the view follows the focus
    // and still only draws a screenful.
    press(&ctx, &mut nav, Key::ArrowRight, Modifiers::NONE);
    press(&ctx, &mut nav, Key::End, Modifiers::NONE);
    frame(&ctx, &mut nav);
    assert!(
        nav.drawn().iter().any(|(p, _)| p.ends_with("shot_02499.png")),
        "the focused last item is scrolled into view"
    );
    assert!(nav.thumbs_requested_total() < 250, "requested {}", nav.thumbs_requested_total());
    nav.run("view.mode", json!({ "mode": "list" }));
    frame(&ctx, &mut nav);
    assert!(nav.drawn().len() < 100, "list rows are virtualized too, got {}", nav.drawn().len());
}

#[test]
fn folder_tree_lists_only_expanded_nodes() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(fake(), Prefs::default());
    for _ in 0..3 {
        frame(&ctx, &mut nav);
    }
    assert_eq!(nav.state.tree.listings, 0, "nothing is listed until a node is expanded");
    nav.state.tree.expand_place(Path::new("/home/paul"));
    for _ in 0..3 {
        frame(&ctx, &mut nav);
    }
    assert_eq!(nav.state.tree.listings, 1, "one listing for the expanded node, then cached");
    nav.run("nav.refresh", json!({}));
    frame(&ctx, &mut nav);
    assert_eq!(nav.state.tree.listings, 2, "refresh re-lists expanded nodes only");
}

#[test]
fn favorites_and_panel_layout_reach_prefs() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(fake(), Prefs::default());
    frame(&ctx, &mut nav);
    nav.run("nav.favorite.add", json!({ "path": "/f" }));
    frame(&ctx, &mut nav);
    assert_eq!(nav.session().prefs().favorites, [PathBuf::from("/f")]);
    assert_eq!(nav.layout(), &default_layout());
    // A saved custom layout is restored by a new navigator.
    let mut custom = default_layout();
    let swap = craft_ui::docking::Action::Move {
        panel: Panel::Metadata,
        anchor: Panel::Preview,
        placement: craft_ui::docking::Placement::Tab { before: None },
    };
    assert!(custom.apply(swap).is_ok());
    let value = serde_json::to_value(&custom).unwrap();
    nav.run("view.panels", json!({ "layout": value }));
    let (restored, _) = navigator(fake(), nav.session().prefs());
    assert_eq!(restored.layout(), &custom);
}

#[test]
fn prefs_save_and_load_round_trip_atomically() {
    let dir = std::env::temp_dir().join(format!(
        "assetcraft-prefs-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("nested").join(prefs_file::FILE_NAME);
    assert_eq!(prefs_file::load(&path), Prefs::from_json("{}"), "missing file gives defaults");

    let mut prefs = Prefs::default();
    prefs.favorites.push("/shots".into());
    prefs.view.mode = ViewMode::List;
    prefs.panels = Some(serde_json::to_value(default_layout()).unwrap());
    prefs_file::save(&path, &prefs).unwrap();
    assert_eq!(prefs_file::load(&path), Prefs::from_json(&prefs.to_json()));
    let leftovers: Vec<_> =
        std::fs::read_dir(path.parent().unwrap()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(leftovers, [std::ffi::OsString::from(prefs_file::FILE_NAME)], "no temp file left behind");

    // The store writes only when something the user chose changed.
    let mut store = PrefsStore::new(Some(path.clone()), prefs_file::load(&path));
    assert!(!store.maybe_save(&prefs_file::load(&path)));
    let mut changed = prefs.clone();
    changed.favorites.push("/more".into());
    assert!(store.maybe_save(&changed));
    assert_eq!(prefs_file::load(&path).favorites, changed.favorites);
    changed.last_folder = Some("/elsewhere".into());
    assert!(!store.maybe_save(&changed), "the last folder alone waits for exit");
    assert!(store.save_now(&changed));
    assert_eq!(prefs_file::load(&path).last_folder, changed.last_folder);

    // Junk on disk falls back to defaults, and is kept aside before anything overwrites it.
    std::fs::write(&path, b"{ not json").unwrap();
    assert_eq!(prefs_file::load(&path), Prefs::from_json("{}"));
    assert!(!path.exists(), "the unreadable file is moved out of the way");
    assert_eq!(std::fs::read(prefs_file::bad_path(&path)).unwrap(), b"{ not json");
    assert!(PrefsStore::new(Some(path.clone()), Prefs::default()).save_now(&changed));
    assert_eq!(std::fs::read(prefs_file::bad_path(&path)).unwrap(), b"{ not json", "the save left the copy alone");
    // One bad field: the good ones survive and the file is not renamed.
    std::fs::write(&path, br#"{"favorites":["/keep"],"view":{"thumb_size":"big","mode":"list"}}"#).unwrap();
    let partial = prefs_file::load(&path);
    assert_eq!((partial.favorites.clone(), partial.view.mode), (vec![PathBuf::from("/keep")], ViewMode::List));
    assert!(path.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn folders_panel_does_not_rescan_drives_every_frame() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Counting(FakeFs, Arc<AtomicUsize>);
    impl Services for Counting {
        fn list_entries(
            &self,
            dir: &Path,
            opts: &assetcraft_fs::ListOptions,
        ) -> Result<assetcraft_fs::Listing, assetcraft_fs::FsError> {
            self.0.list_entries(dir, opts)
        }
        fn entry(&self, path: &Path) -> Result<assetcraft_fs::Entry, assetcraft_fs::FsError> {
            self.0.entry(path)
        }
        fn read_head(&self, path: &Path, max: usize) -> Result<Vec<u8>, assetcraft_fs::FsError> {
            self.0.read_head(path, max)
        }
        fn read_range(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, assetcraft_fs::FsError> {
            self.0.read_range(path, offset, len)
        }
        fn volumes(&self) -> Vec<assetcraft_fs::Volume> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.volumes()
        }
        fn home_dir(&self) -> Option<PathBuf> {
            self.0.home_dir()
        }
        fn known_folder(&self, which: KnownFolder) -> Option<PathBuf> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.known_folder(which)
        }
    }
    let scans = Arc::new(AtomicUsize::new(0));
    let ctx = Context::default();
    let (mut nav, _) = navigator(Counting(fake(), scans.clone()), Prefs::default());
    frame(&ctx, &mut nav);
    let after_first = scans.load(Ordering::Relaxed);
    for _ in 0..30 {
        frame(&ctx, &mut nav);
    }
    assert_eq!(scans.load(Ordering::Relaxed), after_first, "30 frames, zero drive / known-folder scans");
    press(&ctx, &mut nav, Key::F5, Modifiers::NONE);
    assert!(scans.load(Ordering::Relaxed) > after_first, "F5 rescans");
}

#[test]
fn refresh_retries_failed_thumbnails() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(RealFs, Prefs::default());
    assert!(nav.open_folder(&library().join("edge-cases")));
    settle(&ctx, &mut nav);
    assert!(nav.thumbs.pool().failed_len() > 0, "broken_not_an_image.jpg failed");
    nav.run("nav.refresh", json!({}));
    assert_eq!(nav.thumbs.pool().failed_len(), 0, "F5 forgets failures so they are retried");
    assert_eq!(nav.thumbs.texture_bytes(), 0);
    settle(&ctx, &mut nav);
    assert!(nav.thumbs.pool().failed_len() > 0, "and the broken file is tried (and fails) again");
}

#[test]
fn texture_memory_stays_within_its_byte_budget() {
    let ctx = Context::default();
    let launcher = Arc::new(RecordingLauncher::default());
    let session = Session::new(Arc::new(RealFs), launcher, Prefs::default());
    let mut nav = Navigator::with_one_worker(session);
    let pool = ThumbPool::new(nav.session().services(), 1, ThumbCache::default());
    nav.thumbs = thumbnails::Thumbnails::with_budget(pool, 1);
    assert!(nav.open_folder(&library().join("characters/reaper")));
    settle(&ctx, &mut nav);
    let shown = nav.thumbs.texture_bytes();
    assert!(shown > 0, "textures drawn this frame are kept even over budget");
    assert!(nav.open_folder(&library().join("props")));
    settle(&ctx, &mut nav);
    frame(&ctx, &mut nav);
    let drawn = nav.drawn().iter().filter(|(_, b)| *b == Badge::Image).count();
    assert_eq!(drawn, 2, "{:?}", nav.drawn());
    // Only the two props textures (<= 256 px buckets at the default size) are left; a count
    // limit would still hold the four reaper concepts too.
    assert!(nav.thumbs.texture_bytes() <= 2 * 256 * 256 * 4, "{} of {shown}", nav.thumbs.texture_bytes());
}

// ---- menu bar ---------------------------------------------------------------------------

fn walk<'a>(entries: &'a [MenuEntry], out: &mut Vec<&'a MenuEntry>) {
    for e in entries {
        match e {
            MenuEntry::Sub { entries, .. } => walk(entries, out),
            MenuEntry::Item { .. } => out.push(e),
            MenuEntry::Separator => {}
        }
    }
}

fn find_item(nav: &Navigator, menu: &str, label: &str) -> MenuEntry {
    let menus = nav.menus();
    let mut items = Vec::new();
    for m in menus.iter().filter(|m| m.title == menu) {
        walk(&m.entries, &mut items);
    }
    items
        .into_iter()
        .find(|e| matches!(e, MenuEntry::Item { label: l, .. } if l == label))
        .cloned()
        .unwrap_or_else(|| panic!("no {menu} › {label}"))
}

fn pointer_click(ctx: &Context, nav: &mut Navigator, pos: Pos2) {
    let button = |pressed| Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame_with(ctx, nav, vec![Event::PointerMoved(pos)]);
    frame_with(ctx, nav, vec![button(true)]);
    frame_with(ctx, nav, vec![button(false)]);
    frame(ctx, nav);
}

fn menu_rect(nav: &Navigator, label: &str) -> Rect {
    nav.state
        .menu_rects
        .iter()
        .find(|(l, _)| l == label)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("{label} not drawn"))
}

#[test]
fn every_menu_item_is_a_registered_command_or_a_window_action() {
    let ctx = Context::default();
    let prefs = Prefs { favorites: vec!["/f".into()], ..Prefs::default() };
    let (mut nav, _) = navigator(fake(), prefs);
    frame(&ctx, &mut nav);
    nav.run("select.move", json!({ "delta": 1 }));
    let menus = nav.menus();
    let titles: Vec<_> = menus.iter().map(|m| m.title).collect();
    assert_eq!(titles, ["File", "Edit", "View", "Go", "Tools", "Window", "Help"]);
    let mut items = Vec::new();
    for m in &menus {
        walk(&m.entries, &mut items);
    }
    let mut used = std::collections::HashSet::new();
    for entry in items {
        let MenuEntry::Item { action, shortcut, label, .. } = entry else { continue };
        match action {
            MenuAction::Command(id, _) => {
                let spec = assetcraft_engine::find(id).unwrap_or_else(|| panic!("{label}: `{id}` is not registered"));
                if shortcut.is_some() {
                    assert_eq!(*shortcut, spec.shortcut, "{label}: shortcut text comes from the registry");
                }
                used.insert(*id);
            }
            MenuAction::FocusPath => {
                used.insert("nav.open");
            }
            MenuAction::TogglePanel(p) => assert_ne!(*p, Panel::Content),
            MenuAction::BatchRename => {
                used.insert("file.renamePreview");
            }
            MenuAction::ResetLayout
            | MenuAction::Shortcuts
            | MenuAction::About
            | MenuAction::Exit
            | MenuAction::NotYet => {}
        }
    }
    for spec in assetcraft_engine::commands() {
        assert!(
            used.contains(spec.id) || NOT_IN_MENUS.contains(&spec.id),
            "`{}` is neither in a menu nor listed in NOT_IN_MENUS",
            spec.id
        );
    }
    // Shortcuts appear on the right of their items.
    assert!(matches!(find_item(&nav, "Go", "Back"), MenuEntry::Item { shortcut: Some("Alt+Left"), .. }));
    assert!(matches!(find_item(&nav, "View", "Show Hidden Files"), MenuEntry::Item { checked: Some(false), .. }));
    assert!(matches!(find_item(&nav, "Window", "Folders"), MenuEntry::Item { checked: Some(true), .. }));
}

#[test]
fn window_menu_hides_and_restores_panels_through_view_panels() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(fake(), Prefs::default());
    frame(&ctx, &mut nav);
    assert!(nav.activate(&ctx, &find_item(&nav, "Window", "Preview")));
    frame(&ctx, &mut nav);
    assert!(!nav.layout().contains(&Panel::Preview));
    assert_eq!(nav.session().panels(), serde_json::to_value(nav.layout()).ok().as_ref(), "saved via view.panels");
    assert!(matches!(find_item(&nav, "Window", "Preview"), MenuEntry::Item { checked: Some(false), .. }));
    // A restart keeps it closed.
    let (reloaded, _) = navigator(fake(), nav.session().prefs());
    assert!(!reloaded.layout().contains(&Panel::Preview));
    // Showing it again puts it back where it was.
    assert!(nav.activate(&ctx, &find_item(&nav, "Window", "Preview")));
    frame(&ctx, &mut nav);
    assert!(nav.layout().contains(&Panel::Preview));
    assert_eq!(nav.layout(), &default_layout());
    assert_eq!(nav.session().panels(), serde_json::to_value(default_layout()).ok().as_ref());
    // A restarted app with Folders closed reopens it beside the content.
    assert!(nav.activate(&ctx, &find_item(&nav, "Window", "Folders")));
    let (mut reloaded, _) = navigator(fake(), nav.session().prefs());
    frame(&ctx, &mut reloaded);
    assert!(reloaded.activate(&ctx, &find_item(&reloaded, "Window", "Folders")));
    assert!(reloaded.layout().contains(&Panel::Folders));
}

#[test]
fn reset_panel_layout_restores_the_default() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(fake(), Prefs::default());
    frame(&ctx, &mut nav);
    for label in ["Folders", "Metadata", "Preview"] {
        assert!(nav.activate(&ctx, &find_item(&nav, "Window", label)));
    }
    assert_eq!(craft_ui::docking::Layout::panels(nav.layout()), [&Panel::Content]);
    assert!(nav.activate(&ctx, &find_item(&nav, "Window", "Reset Panel Layout")));
    frame(&ctx, &mut nav);
    assert_eq!(nav.layout(), &default_layout());
    assert_eq!(nav.session().panels(), serde_json::to_value(default_layout()).ok().as_ref());
}

#[test]
fn clicking_menus_runs_commands_and_disabled_items_do_nothing() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(fake(), Prefs::default());
    frame(&ctx, &mut nav);
    frame(&ctx, &mut nav);
    let before = nav.session().location().clone();
    assert!(!nav.session().can_back());

    // Real pointer clicks: Go, then the greyed-out Back.
    let at = menu_rect(&nav, "Go").center();
    pointer_click(&ctx, &mut nav, at);
    let back = menu_rect(&nav, "Back");
    pointer_click(&ctx, &mut nav, back.center());
    assert_eq!(nav.session().location(), &before, "disabled Back does nothing");
    assert!(nav.message().is_none(), "and reports nothing: {:?}", nav.message());
    assert!(!nav.activate(&ctx, &find_item(&nav, "Go", "Back")), "disabled items don't run");

    // An enabled item does run: View › List.
    let at = menu_rect(&nav, "View").center();
    pointer_click(&ctx, &mut nav, at);
    let at = menu_rect(&nav, "List").center();
    pointer_click(&ctx, &mut nav, at);
    assert_eq!(nav.session().view().mode, ViewMode::List);

    // Help windows and Exit.
    assert!(nav.activate(&ctx, &find_item(&nav, "Help", "Keyboard Shortcuts…")));
    assert!(nav.activate(&ctx, &find_item(&nav, "Help", "About AssetCraft")));
    frame(&ctx, &mut nav);
    assert!(nav.state.show_shortcuts && nav.state.show_about);
    assert!(nav.activate(&ctx, &find_item(&nav, "File", "Exit")));
    assert!(nav.exit_requested());
}

#[test]
fn view_and_go_wrappers_pass_the_right_parameters() {
    let ctx = Context::default();
    let prefs = Prefs { favorites: vec!["/f".into()], ..Prefs::default() };
    let (mut nav, _) = navigator(fake(), prefs);
    frame(&ctx, &mut nav);
    assert!(nav.activate(&ctx, &find_item(&nav, "View", "Size")));
    assert_eq!(nav.session().view().sort, SortKey::Size);
    assert!(nav.activate(&ctx, &find_item(&nav, "View", "Descending")));
    assert!(!nav.session().view().ascending);
    let px = nav.session().view().thumb_size;
    assert!(nav.activate(&ctx, &find_item(&nav, "View", "Larger")));
    assert!(nav.session().view().thumb_size > px);
    assert!(nav.activate(&ctx, &find_item(&nav, "View", "Show Hidden Files")));
    assert!(nav.session().view().show_hidden);
    assert!(nav.activate(&ctx, &find_item(&nav, "Go", "Home")));
    assert_eq!(nav.session().location().path(), Some(Path::new("/home/paul")));
    assert!(nav.activate(&ctx, &find_item(&nav, "Go", "f")));
    assert_eq!(nav.session().location().path(), Some(Path::new("/f")));
    assert!(nav.activate(&ctx, &find_item(&nav, "Go", "Unpin from My Folders (Current Folder)")));
    assert!(nav.session().favorites().is_empty());
    assert!(nav.activate(&ctx, &find_item(&nav, "File", "Pin to My Folders")));
    assert_eq!(nav.session().favorites(), [PathBuf::from("/f")]);
}

// ---- Stage 1: live folder updates, programs, theme ---------------------------------------

/// A fake disk a test can change while the navigator holds it.
#[derive(Clone)]
struct SwapFs(Arc<std::sync::RwLock<FakeFs>>);

impl SwapFs {
    fn add_file(&self, path: &str, size: u64) {
        let mut guard = self.0.write().unwrap();
        let old = std::mem::take(&mut *guard);
        *guard = old.file(path, size);
    }
}

impl Services for SwapFs {
    fn list_entries(
        &self,
        dir: &Path,
        opts: &assetcraft_fs::ListOptions,
    ) -> Result<assetcraft_fs::Listing, assetcraft_fs::FsError> {
        self.0.read().unwrap().list_entries(dir, opts)
    }
    fn entry(&self, path: &Path) -> Result<assetcraft_fs::Entry, assetcraft_fs::FsError> {
        self.0.read().unwrap().entry(path)
    }
    fn read_head(&self, path: &Path, max: usize) -> Result<Vec<u8>, assetcraft_fs::FsError> {
        self.0.read().unwrap().read_head(path, max)
    }
    fn read_range(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, assetcraft_fs::FsError> {
        self.0.read().unwrap().read_range(path, offset, len)
    }
    fn volumes(&self) -> Vec<assetcraft_fs::Volume> {
        self.0.read().unwrap().volumes()
    }
    fn home_dir(&self) -> Option<PathBuf> {
        self.0.read().unwrap().home_dir()
    }
    fn known_folder(&self, which: KnownFolder) -> Option<PathBuf> {
        self.0.read().unwrap().known_folder(which)
    }
}

#[test]
fn a_watched_file_appears_without_losing_selection_or_scroll() {
    let ctx = Context::default();
    let mut fs = FakeFs::new().known(KnownFolder::Pictures, "/big");
    for i in 0..600 {
        fs = fs.file(format!("/big/shot_{i:05}.png"), 64);
    }
    let disk = SwapFs(Arc::new(std::sync::RwLock::new(fs)));
    let (mut nav, _) = navigator(disk.clone(), Prefs::default());
    let watcher = assetcraft_watch::FakeWatcher::new();
    nav.set_watcher(Box::new(watcher.clone()));
    frame(&ctx, &mut nav);
    press(&ctx, &mut nav, Key::ArrowRight, Modifiers::NONE);
    press(&ctx, &mut nav, Key::End, Modifiers::NONE);
    frame(&ctx, &mut nav);
    let scrolled = nav.state.scroll.as_ref().map(|(_, y)| *y).unwrap_or_default();
    assert!(scrolled > 100.0, "scrolled to the end: {scrolled}");

    disk.add_file("/big/aaa_new.png", 10); // sorts first, so everything shifts by one cell
    watcher.emit("/big/aaa_new.png");
    frame(&ctx, &mut nav);
    frame(&ctx, &mut nav);
    assert_eq!(nav.session().items().len(), 601);
    assert_eq!(nav.session().items()[0].name, "aaa_new.png");
    assert_eq!(nav.session().selection().focus(), Some(Path::new("/big/shot_00599.png")), "focus kept");
    let after = nav.state.scroll.as_ref().map(|(_, y)| *y).unwrap_or_default();
    assert!((after - scrolled).abs() < 1.0, "scroll kept: {scrolled} -> {after}");
    assert!(nav.message().is_none(), "{:?}", nav.message());
}

#[test]
fn live_updates_off_is_a_quiet_status_note() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(fake(), Prefs::default());
    let watcher = assetcraft_watch::FakeWatcher::new();
    watcher.fail_with(Some("\\\\nas\\share: not supported"));
    nav.set_watcher(Box::new(watcher));
    frame(&ctx, &mut nav);
    frame(&ctx, &mut nav);
    assert!(matches!(nav.session().watch_status(), assetcraft_engine::WatchStatus::Unavailable(_)));
    assert!(nav.message().is_none(), "no error message: {:?}", nav.message());
}

#[test]
fn enter_reveals_a_program_and_shift_enter_runs_it() {
    let ctx = Context::default();
    let fs = FakeFs::new().known(KnownFolder::Pictures, "/p").file("/p/setup.exe", 10);
    let (mut nav, launcher) = navigator(fs, Prefs::default());
    frame(&ctx, &mut nav);
    press(&ctx, &mut nav, Key::ArrowRight, Modifiers::NONE);
    press(&ctx, &mut nav, Key::Enter, Modifiers::NONE);
    assert_eq!(
        nav.message(),
        Some("Revealed setup.exe (programs aren't run on Enter; use Shift+Enter)"),
        "shown as information"
    );
    assert!(nav.state.message_is_info);
    press(&ctx, &mut nav, Key::Enter, Modifiers::SHIFT);
    let calls = launcher.calls.lock().map(|c| c.clone()).unwrap_or_default();
    assert_eq!(calls, [("reveal", PathBuf::from("/p/setup.exe")), ("open", PathBuf::from("/p/setup.exe"))]);
    assert!(matches!(
        find_item(&nav, "File", "Run…"),
        MenuEntry::Item { shortcut: Some("Shift+Enter"), enabled: true, .. }
    ));
}

#[test]
fn theme_is_dark_by_default_and_the_view_menu_switches_and_saves_it() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(fake(), Prefs::default());
    frame(&ctx, &mut nav);
    assert_eq!(ctx.theme(), egui::Theme::Dark);
    assert!(matches!(find_item(&nav, "View", "Dark"), MenuEntry::Item { checked: Some(true), .. }));
    assert!(nav.activate(&ctx, &find_item(&nav, "View", "Light")));
    frame(&ctx, &mut nav);
    assert_eq!(ctx.theme(), egui::Theme::Light);
    assert_eq!(nav.session().prefs().view.theme, assetcraft_engine::ThemeMode::Light);
    assert_eq!(ctx.global_style().visuals.panel_fill, theme::light_visuals().panel_fill);
    // System follows what the OS reports (here: light, then dark).
    assert!(nav.activate(&ctx, &find_item(&nav, "View", "System")));
    for (os, want) in [(egui::Theme::Light, egui::Theme::Light), (egui::Theme::Dark, egui::Theme::Dark)] {
        let input = RawInput { screen_rect: Some(SCREEN), system_theme: Some(os), ..RawInput::default() };
        let mut out = ctx.run_ui(input, |ui| nav.ui(ui));
        out.textures_delta.clear();
        assert_eq!(ctx.theme(), want);
    }
    // A restart keeps the choice.
    let (reloaded, _) = navigator(fake(), nav.session().prefs());
    assert_eq!(reloaded.session().view().theme, assetcraft_engine::ThemeMode::System);
}

#[test]
fn tools_batch_rename_opens_a_preview_that_changes_nothing() {
    let ctx = Context::default();
    let (mut nav, _) = navigator(fake(), Prefs::default());
    frame(&ctx, &mut nav);
    // Greyed until something is selected.
    assert!(matches!(find_item(&nav, "Tools", "Batch Rename…"), MenuEntry::Item { enabled: false, .. }));
    assert!(matches!(find_item(&nav, "Tools", "Append Metadata"), MenuEntry::Item { enabled: false, .. }));
    nav.run("select.set", json!({ "paths": ["/home/paul/Pictures/a.png", "/home/paul/Pictures/b.png"] }));
    assert!(nav.activate(&ctx, &find_item(&nav, "Tools", "Batch Rename…")));
    frame(&ctx, &mut nav);
    let dialog = nav.state.rename.clone().unwrap_or_default();
    let preview = dialog.preview.unwrap_or_default();
    let new: Vec<&str> = preview["plans"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p["newName"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(new, ["Pictures_001.png", "Pictures_002.png"]);
    assert!(nav.message().is_none(), "previews don't use the status bar");
    // The preview is cached: another frame doesn't run it again.
    let generation = nav.session().generation();
    frame(&ctx, &mut nav);
    assert_eq!(nav.session().generation(), generation);
    // Nothing was renamed.
    assert!(nav.session().items().iter().any(|e| e.name == "a.png"));
}
