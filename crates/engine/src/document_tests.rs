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
    assert!(matches!(
        s.execute("storyboard.saveAs", &json!({ "path": "/x.storyboard.json" })),
        Err(CmdError::Unavailable(_))
    ));
    run(&mut s, "storyboard.new", json!({ "name": "Mine: a/b?" }));
    assert_eq!(s.suggested_document_name(), "Mine- a-b.storyboard.json");
    for bad in [
        json!({}),
        json!({ "path": "" }),
        json!({ "path": "relative.storyboard.json" }),
        json!({ "path": "/tmp/notes.json" }),
        json!({ "path": "/tmp/.storyboard.json" }),
        json!({ "path": 7 }),
    ] {
        assert!(matches!(s.execute("storyboard.saveAs", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert_eq!(s.storyboards_folder(), Some(PathBuf::from("/home/paul/AssetCraft/Storyboards")));
}
