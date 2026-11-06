//! SF1: `storyboard.save` / `storyboard.saveAs`, the document path and the dirty flag.

use super::*;
use crate::launch::RecordingLauncher;
use assetcraft_fs::{FakeFs, RealFs};
use assetcraft_storyboard::Storyboard;
use serde_json::json;
use std::path::PathBuf;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetcraft-sf1-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn real() -> Session {
    Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default())
}

fn leftovers(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect()
}

#[test]
fn save_as_writes_the_document_round_trips_and_clears_dirty() {
    let dir = temp("save");
    let mut s = real();
    let st = run(&mut s, "storyboard.new", json!({ "sample": true }));
    assert_eq!((&st["document"], &st["dirty"]), (&Value::Null, &json!(false)), "a new storyboard is clean");
    // A never-saved storyboard needs a path.
    assert!(matches!(s.execute("storyboard.save", &json!({})), Err(CmdError::BadParams(_))));
    assert!(s.enabled("storyboard.save").is_err());
    assert!(s.enabled("storyboard.saveAs").is_ok());

    run(&mut s, "storyboard.place", json!({ "asset": "/art/a.png" }));
    assert!(s.storyboard_dirty(), "an edit marks the document");
    let path = dir.join("Reaper.storyboard.json");
    let out = run(&mut s, "storyboard.saveAs", json!({ "path": path.to_string_lossy() }));
    assert_eq!(out["dirty"], false);
    assert_eq!(PathBuf::from(out["document"].as_str().unwrap()), path);
    assert!(out["message"].as_str().unwrap().starts_with("Saved "));
    assert_eq!(s.document(), Some(path.as_path()));
    assert!(leftovers(&dir).is_empty(), "no temp file left behind");

    // What is on disk is the open board, and reads back through the validating parser.
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len() as u64, out["bytes"].as_u64().unwrap());
    let back = Storyboard::from_json(&bytes).unwrap();
    assert_eq!(&back, s.storyboard().unwrap());

    // Save (no path) goes to the same file; undo back past the save marks it again, redo clears it.
    run(&mut s, "storyboard.place", json!({ "asset": "/art/b.png" }));
    assert!(s.storyboard_dirty());
    run(&mut s, "storyboard.undo", json!({}));
    assert!(!s.storyboard_dirty(), "undone back to the saved state is clean");
    run(&mut s, "storyboard.redo", json!({}));
    assert!(s.storyboard_dirty());
    assert!(s.enabled("storyboard.save").is_ok());
    let out = run(&mut s, "storyboard.save", json!({}));
    assert_eq!((out["dirty"].as_bool(), s.document()), (Some(false), Some(path.as_path())));
    let back = Storyboard::from_json(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(back.assets_assigned(), 2, "the second save holds the second edit");

    // New forgets the document.
    run(&mut s, "storyboard.new", json!({}));
    assert_eq!(s.document(), None);
    assert!(!s.storyboard_dirty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_different_existing_file_is_never_replaced_without_overwrite() {
    let dir = temp("overwrite");
    let other = dir.join("Other.storyboard.json");
    std::fs::write(&other, b"someone else's file").unwrap();
    let mut s = real();
    run(&mut s, "storyboard.new", json!({ "name": "Mine" }));
    let refused = s.execute("storyboard.saveAs", &json!({ "path": other.to_string_lossy() }));
    assert!(matches!(refused, Err(CmdError::Unavailable(_))), "{refused:?}");
    assert_eq!(std::fs::read(&other).unwrap(), b"someone else's file", "untouched");
    assert_eq!(s.document(), None, "a refusal changes nothing");
    run(&mut s, "storyboard.saveAs", json!({ "path": other.to_string_lossy(), "overwrite": true }));
    assert!(Storyboard::from_json(&std::fs::read(&other).unwrap()).is_ok());
    // Now it is this storyboard's own file: Save needs no `overwrite`.
    run(&mut s, "storyboard.save", json!({ "path": other.to_string_lossy() }));
    assert!(leftovers(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_write_leaves_the_old_file_and_no_temp() {
    let dir = temp("fail");
    let path = dir.join("Keep.storyboard.json");
    let mut s = real();
    run(&mut s, "storyboard.new", json!({ "name": "Keep" }));
    run(&mut s, "storyboard.saveAs", json!({ "path": path.to_string_lossy() }));
    let before = std::fs::read(&path).unwrap();
    // The target's folder is now a file: the temp file cannot be created there.
    let blocked = dir.join("blocked");
    std::fs::write(&blocked, b"not a folder").unwrap();
    let inside = blocked.join("x.storyboard.json");
    let failed = s.execute("storyboard.saveAs", &json!({ "path": inside.to_string_lossy() }));
    assert!(matches!(failed, Err(CmdError::Unavailable(_))), "{failed:?}");
    assert_eq!(s.document(), Some(path.as_path()), "still the old document");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(leftovers(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn paths_are_checked_before_anything_is_written() {
    let mut s = Session::new(
        Arc::new(FakeFs::new().known(KnownFolder::Home, "/home/paul")),
        Arc::new(RecordingLauncher::default()),
        Prefs::default(),
    );
    // Absolute on every OS (a Unix-style `/x` has no drive on Windows and would fail as relative).
    let abs = |name: &str| std::env::temp_dir().join(name).to_string_lossy().into_owned();
    let no_board = s.execute("storyboard.saveAs", &json!({ "path": abs("x.storyboard.json") }));
    assert!(matches!(no_board, Err(CmdError::Unavailable(_))), "{no_board:?}");
    run(&mut s, "storyboard.new", json!({ "name": "Mine: a/b?" }));
    assert_eq!(s.suggested_document_name(), "Mine- a-b.storyboard.json");
    for bad in [
        json!({}),
        json!({ "path": "" }),
        json!({ "path": "relative.storyboard.json" }),
        json!({ "path": abs("notes.json") }),
        json!({ "path": abs(".storyboard.json") }),
        json!({ "path": 7 }),
    ] {
        assert!(matches!(s.execute("storyboard.saveAs", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert_eq!(s.storyboards_folder(), Some(PathBuf::from("/home/paul/AssetCraft/Storyboards")));
}

#[test]
fn open_reads_a_saved_storyboard_back_as_one_undo_step_and_remembers_it() {
    let dir = temp("open");
    let mut s = real();
    run(&mut s, "storyboard.new", json!({ "sample": true }));
    let still =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/environments/soul_light_forest.jpg");
    let still = still.canonicalize().unwrap();
    run(&mut s, "storyboard.place", json!({ "asset": still.to_string_lossy() }));
    let saved_board = s.storyboard().unwrap().clone();
    let path = dir.join("Reaper.storyboard.json");
    run(&mut s, "storyboard.saveAs", json!({ "path": path.to_string_lossy() }));
    assert_eq!(s.recent_storyboards(), std::slice::from_ref(&path), "saving remembers the file");

    // A different board is open; Open replaces it, undo brings it back.
    run(&mut s, "storyboard.new", json!({ "name": "Scratch" }));
    run(&mut s, "app.mode", json!({ "mode": "organizer" }));
    let out = run(&mut s, "storyboard.open", json!({ "path": path.to_string_lossy() }));
    assert_eq!(out["opened"].as_str().map(PathBuf::from), Some(path.clone()));
    assert_eq!(out["missing"].as_array().map(Vec::len), Some(0));
    assert_eq!(out["dirty"], false);
    assert_eq!(s.storyboard(), Some(&saved_board));
    assert_eq!(s.document(), Some(path.as_path()));
    assert_eq!(s.mode(), Mode::Storyboard, "Open switches to Storyboard mode");
    assert!(out["message"].as_str().unwrap().starts_with("Opened "));
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(s.storyboard().unwrap().name, "Scratch", "one undo step");
    run(&mut s, "storyboard.redo", json!({}));
    assert_eq!(s.storyboard(), Some(&saved_board));

    // Recent: newest first, no duplicates, capped.
    let recent = run(&mut s, "storyboard.recent", json!({}));
    assert_eq!(recent["recent"].as_array().map(Vec::len), Some(1));
    assert_eq!(recent["recent"][0]["name"], "Reaper");
    assert_eq!(recent["recent"][0]["exists"], true);
    for i in 0..12 {
        let p = dir.join(format!("B{i}.storyboard.json"));
        run(&mut s, "storyboard.saveAs", json!({ "path": p.to_string_lossy() }));
    }
    assert_eq!(s.recent_storyboards().len(), crate::document_cmd::MAX_RECENT);
    assert!(s.recent_storyboards()[0].ends_with("B11.storyboard.json"));
    run(&mut s, "storyboard.open", json!({ "path": path.to_string_lossy() }));
    assert_eq!(s.recent_storyboards()[0], path);
    assert_eq!(s.recent_storyboards().iter().filter(|p| **p == path).count(), 1);
    assert_eq!(s.prefs().recent_storyboards, s.recent_storyboards(), "saved in prefs");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn open_refuses_bad_files_and_reports_missing_assets() {
    let dir = temp("open-bad");
    let mut s = real();
    let abs = |name: &str| dir.join(name).to_string_lossy().into_owned();
    // Refusals change nothing.
    assert!(matches!(s.execute("storyboard.open", &json!({})), Err(CmdError::BadParams(_))));
    assert!(matches!(
        s.execute("storyboard.open", &json!({ "path": "x.storyboard.json" })),
        Err(CmdError::BadParams(_))
    ));
    assert!(matches!(s.execute("storyboard.open", &json!({ "path": abs("notes.json") })), Err(CmdError::BadParams(_))));
    assert!(matches!(
        s.execute("storyboard.open", &json!({ "path": abs("gone.storyboard.json") })),
        Err(CmdError::Unavailable(_))
    ));
    std::fs::write(dir.join("junk.storyboard.json"), b"{ not json").unwrap();
    assert!(matches!(
        s.execute("storyboard.open", &json!({ "path": abs("junk.storyboard.json") })),
        Err(CmdError::Unavailable(_))
    ));
    let newer = format!(
        r#"{{"version": {}, "name": "x", "scenes": [], "clips": []}}"#,
        assetcraft_storyboard::FORMAT_VERSION + 1
    );
    std::fs::write(dir.join("newer.storyboard.json"), newer).unwrap();
    let err = s.execute("storyboard.open", &json!({ "path": abs("newer.storyboard.json") })).unwrap_err();
    assert!(err.to_string().contains("newer AssetCraft"), "{err}");
    std::fs::write(dir.join("huge.storyboard.json"), vec![b' '; (16 << 20) + 1]).unwrap();
    let err = s.execute("storyboard.open", &json!({ "path": abs("huge.storyboard.json") })).unwrap_err();
    assert!(err.to_string().contains("larger than"), "{err}");
    assert!(s.storyboard().is_none() && s.recent_storyboards().is_empty(), "nothing opened, nothing remembered");

    // Assets that moved are reported, the board keeps them.
    run(&mut s, "storyboard.new", json!({ "sample": true }));
    let gone = dir.join("moved.png");
    run(&mut s, "storyboard.place", json!({ "asset": gone.to_string_lossy() }));
    let path = dir.join("Moved.storyboard.json");
    run(&mut s, "storyboard.saveAs", json!({ "path": path.to_string_lossy() }));
    let out = run(&mut s, "storyboard.open", json!({ "path": path.to_string_lossy() }));
    assert_eq!(out["missing"].as_array().unwrap(), &[json!(gone.to_string_lossy())]);
    assert!(out["message"].as_str().unwrap().ends_with("1 asset is missing"));
    assert!(s.storyboard().unwrap().clips.iter().any(|c| c.asset_ids.iter().any(|a| a.0 == gone.to_string_lossy())));
    assert_eq!(crate::document_cmd::document_name(&path), "Moved");
    assert_eq!(crate::document_cmd::document_name(Path::new("/x/plain.json")), "plain.json");
    let _ = std::fs::remove_dir_all(&dir);
}
