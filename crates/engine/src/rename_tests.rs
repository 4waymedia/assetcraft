//! Batch rename planning (T1): tokens, sanitizing, collisions, and `file.renamePreview`.

use crate::rename::{TOKENS, expand_tokens, new_name, plan, unknown_tokens};
use crate::{KnownFolder, Prefs, RecordingLauncher, Session};
use assetcraft_fs::FakeFs;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[test]
fn every_listed_token_expands() {
    for t in TOKENS {
        assert_ne!(expand_tokens(t.tag, "IMG_0042.jpg", "reapers", 7), t.tag, "{} is listed but not expanded", t.tag);
    }
}

#[test]
fn tokens_expand_like_lightcraft() {
    let e = |t: &str| expand_tokens(t, "IMG_0042.CR3", "reapers", 7);
    assert_eq!(e("{name}"), "IMG_0042");
    assert_eq!(e("{num}"), "0042");
    assert_eq!(e("{seq}"), "7");
    assert_eq!(e("{seq:3}"), "007");
    assert_eq!(e("{SEQ:2}"), "07", "case-insensitive");
    assert_eq!(e("{folder}_{seq:3}"), "reapers_007");
    assert_eq!(e("{ext}"), "CR3");
    assert_eq!(e("{seq:0}"), "{seq:0}", "width out of 1..9 stays as typed");
    assert_eq!(e("{dtae}x{"), "{dtae}x{", "unknown and unclosed stay as typed");
    assert_eq!(unknown_tokens("{dtae}_{seq}_{dtae}_{zz"), ["{dtae}"]);
}

#[test]
fn new_names_keep_the_extension_and_are_safe_on_windows() {
    assert_eq!(new_name("{folder}_{seq:3}", "IMG_1.jpg", "reapers", 1), "reapers_001.jpg");
    assert_eq!(new_name("a/b:c*?", "x.png", "f", 1), "a-b-c--.png");
    assert_eq!(new_name("  ..  ", "keep.png", "f", 1), "keep.png", "an empty result keeps the name");
    assert_eq!(new_name("draft. ", "x.png", "f", 1), "draft.png", "no trailing dot before the extension");
    assert_eq!(new_name("con", "x.txt", "f", 1), "con_.txt", "reserved device name");
    assert_eq!(new_name("{name}", ".hidden", "f", 1), ".hidden");
    let long = new_name(&"é".repeat(400), "x.jpeg", "f", 1);
    assert_eq!(long.chars().count(), crate::rename::MAX_NAME_CHARS);
    assert!(long.ends_with(".jpeg"));
}

#[test]
fn taken_names_get_a_suffix_and_case_changes_are_allowed() {
    let dir = Path::new("/p");
    let files: Vec<PathBuf> = ["IMG_1.jpg", "IMG_2.jpg", "Shot.jpg"].iter().map(|n| dir.join(n)).collect();
    let on_disk = ["/p/img_1.jpg", "/p/img_2.jpg", "/p/shot.jpg", "/p/out_1.jpg"];
    // `Path::join` writes `\` on Windows: compare with `/` so the test means the same everywhere.
    let exists = |p: &Path| on_disk.iter().any(|d| d.eq_ignore_ascii_case(&p.to_string_lossy().replace('\\', "/")));
    // Lower-casing: each file's own name, so no suffix.
    let lower = plan(&files[..2], "img_{num}", 1, &exists).unwrap_or_default();
    assert_eq!(lower.iter().map(|r| r.to.clone()).collect::<Vec<_>>(), [dir.join("img_1.jpg"), dir.join("img_2.jpg")]);
    assert!(lower.iter().all(|r| r.changes() && !r.suffixed));
    // out_1 exists; the batch's own claims count too.
    let out = plan(&files, "out", 1, &exists).unwrap_or_default();
    let names: Vec<String> =
        out.iter().map(|r| r.to.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["out.jpg", "out-1.jpg", "out-2.jpg"]);
    let out = plan(&files, "out_{seq}", 1, &exists).unwrap_or_default();
    let names: Vec<String> =
        out.iter().map(|r| r.to.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["out_1-1.jpg", "out_2.jpg", "out_3.jpg"]);
    assert!(out.first().is_some_and(|r| r.suffixed));
}

#[test]
fn no_free_name_is_an_error_not_a_hang() {
    let files = vec![PathBuf::from("/p/a.jpg")];
    let r = plan(&files, "b", 1, &|_| true);
    assert!(r.is_err());
}

fn session() -> Session {
    let fs = FakeFs::new()
        .known(KnownFolder::Pictures, "/home/paul/Pictures")
        .file("/home/paul/Pictures/IMG_0042.jpg", 10)
        .file("/home/paul/Pictures/IMG_0043.jpg", 10)
        .file("/home/paul/Pictures/Pictures_002.jpg", 10)
        .dir("/home/paul/Pictures/sub");
    Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default())
}

#[test]
fn rename_preview_plans_the_selection_and_changes_nothing() {
    let mut s = session();
    let before: Vec<String> = s.items().iter().map(|e| e.name.clone()).collect();
    s.execute("select.all", &json!({})).unwrap_or_default();
    let v = s.execute("file.renamePreview", &json!({ "template": "{folder}_{seq:3}" })).unwrap_or_default();
    let new: Vec<&str> =
        v["plans"].as_array().map(|a| a.iter().filter_map(|p| p["newName"].as_str()).collect()).unwrap_or_default();
    // Display order: IMG_0042, IMG_0043, Pictures_002 (the folder `sub` is skipped).
    assert_eq!(new, ["Pictures_001.jpg", "Pictures_002-1.jpg", "Pictures_003.jpg"], "{v}");
    assert_eq!(v["count"], 3);
    assert_eq!(v["suffixed"], 1);
    assert_eq!(v["skipped"].as_array().map(Vec::len), Some(1));
    let after: Vec<String> = s.items().iter().map(|e| e.name.clone()).collect();
    assert_eq!(before, after);
}

#[test]
fn rename_preview_refuses_bad_params() {
    let mut s = session();
    let err = |p: Value, s: &mut Session| s.execute("file.renamePreview", &p).is_err();
    assert!(err(json!({ "template": "x" }), &mut s), "nothing selected");
    assert!(err(json!({ "paths": ["/home/paul/Pictures/IMG_0042.jpg"] }), &mut s), "no template");
    assert!(err(json!({ "template": "x", "paths": ["relative.jpg"] }), &mut s));
    assert!(err(json!({ "template": "x", "start": -1, "paths": ["/home/paul/Pictures/IMG_0042.jpg"] }), &mut s));
    assert!(err(json!({ "template": "x".repeat(600), "paths": ["/home/paul/Pictures/IMG_0042.jpg"] }), &mut s));
    let effects = crate::find("file.renamePreview").map(|c| c.effects.read_only());
    assert_eq!(effects, Some(true), "a preview only reads");
}
