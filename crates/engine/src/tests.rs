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
