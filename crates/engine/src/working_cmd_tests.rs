//! W1: the working folder setting, and `storyboard.musicImport` copying songs into it.

use super::*;
use crate::launch::RecordingLauncher;
use assetcraft_fs::{FakeFs, RealFs};
use serde_json::json;
use std::path::PathBuf;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetcraft-w1-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn the_working_folder_defaults_to_documents_assetcraft_and_is_saved() {
    let fake = FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .known(KnownFolder::Documents, "/home/paul/Documents")
        .dir("/home/paul/Documents")
        .file("/home/paul/notes.txt", 10);
    let mut s = Session::new(Arc::new(fake), Arc::new(RecordingLauncher::default()), Prefs::default());
    let v = run(&mut s, "app.workingFolder", json!({}));
    assert_eq!(v["workingFolder"], "/home/paul/Documents/AssetCraft");
    assert_eq!(v["music"], "/home/paul/Documents/AssetCraft/Music");
    assert_eq!(v["custom"], false);
    assert_eq!(s.prefs().working_folder, None);
    let v = run(&mut s, "app.workingFolder", json!({ "path": "/home/paul/Studio" }));
    assert_eq!((&v["workingFolder"], &v["custom"]), (&json!("/home/paul/Studio"), &json!(true)));
    assert_eq!(s.prefs().working_folder, Some(PathBuf::from("/home/paul/Studio")), "saved");
    for bad in [json!({ "path": "relative" }), json!({ "path": "/home/paul/notes.txt" }), json!({ "path": 3 })] {
        assert!(matches!(s.execute("app.workingFolder", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert_eq!(run(&mut s, "app.workingFolder", json!({ "path": null }))["custom"], false, "back to the default");
}

#[test]
fn import_copies_songs_into_music_never_overwrites_and_leaves_the_originals() {
    let src = temp("src");
    let other = temp("other");
    let work = temp("work").join("AssetCraft");
    std::fs::write(src.join("a.mp3"), b"song a bytes").unwrap();
    std::fs::write(src.join("b.wav"), b"song b").unwrap();
    std::fs::write(src.join("notes.txt"), b"not music").unwrap();
    std::fs::write(other.join("a.mp3"), b"a different song, longer").unwrap();
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut s, "app.workingFolder", json!({ "path": work.to_string_lossy() }));
    run(&mut s, "storyboard.musicFolder", json!({ "path": src.to_string_lossy() }));

    // Dry run: the plan, nothing written (the working folder isn't even created).
    let plan = run(&mut s, "storyboard.musicImport", json!({ "dryRun": true }));
    assert_eq!((plan["copied"].as_u64(), plan["failed"].as_u64()), (Some(2), Some(0)), "{plan}");
    assert!(plan["message"].as_str().unwrap().starts_with("Would copy 2 songs"));
    assert!(!work.exists());
    assert_eq!(s.music_folder(), Some(src.as_path()), "a dry run changes nothing");

    // Copy: both songs (not the .txt) land in Music; the originals are untouched.
    let r = run(&mut s, "storyboard.musicImport", json!({}));
    assert_eq!((r["copied"].as_u64(), r["present"].as_u64(), r["failed"].as_u64()), (Some(2), Some(0), Some(0)));
    let music = work.join("Music");
    assert_eq!(std::fs::read(music.join("a.mp3")).unwrap(), b"song a bytes");
    assert_eq!(std::fs::read(music.join("b.wav")).unwrap(), b"song b");
    assert!(src.join("a.mp3").exists() && src.join("b.wav").exists(), "copy, not move");
    assert!(std::fs::read_dir(&music).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
    assert!(r["message"].as_str().unwrap().starts_with("Copied 2 songs to "), "{}", r["message"]);
    assert_eq!(s.music_folder(), Some(music.as_path()), "the Start screen now lists Music");
    assert_eq!(r["tracks"].as_array().unwrap().len(), 2);

    // Again: already there (same name and size), nothing copied; a different a.mp3 gets a (2).
    let again = run(
        &mut s,
        "storyboard.musicImport",
        json!({ "paths": [src.join("a.mp3").to_string_lossy(), src.join("b.wav").to_string_lossy(),
                          other.join("a.mp3").to_string_lossy(), src.join("notes.txt").to_string_lossy()] }),
    );
    assert_eq!(
        (again["copied"].as_u64(), again["present"].as_u64(), again["failed"].as_u64()),
        (Some(1), Some(2), Some(1)),
        "{again}"
    );
    assert_eq!(std::fs::read(music.join("a (2).mp3")).unwrap(), b"a different song, longer");
    assert_eq!(std::fs::read(music.join("a.mp3")).unwrap(), b"song a bytes", "never overwritten");
    let txt =
        again["items"].as_array().unwrap().iter().find(|i| i["from"].as_str().unwrap().ends_with(".txt")).cloned();
    assert_eq!(txt.unwrap()["error"], "not an audio file");
    assert!(again["message"].as_str().unwrap().contains("(2 already there), 1 couldn't be copied"));

    // Songs already in Music import as "present", in place.
    let inside = run(&mut s, "storyboard.musicImport", json!({}));
    assert_eq!((inside["copied"].as_u64(), inside["present"].as_u64()), (Some(0), Some(3)));

    // Bad params.
    for bad in [json!({ "paths": "x" }), json!({ "paths": ["rel.mp3"] }), json!({ "dryRun": "yes" })] {
        assert!(matches!(s.execute("storyboard.musicImport", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    for d in [&src, &other, &work.parent().unwrap().to_path_buf()] {
        let _ = std::fs::remove_dir_all(d);
    }
}
