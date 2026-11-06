//! ST1–ST3: templates listed, saved and used for New.

use super::*;
use crate::launch::RecordingLauncher;
use assetcraft_fs::RealFs;
use serde_json::json;
use std::path::PathBuf;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A real session whose working folder is a temp dir, so Templates/ is real too.
fn session(tag: &str) -> (Session, PathBuf) {
    let dir = std::env::temp_dir().join(format!("assetcraft-st-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let prefs = Prefs { working_folder: Some(dir.clone()), ..Prefs::default() };
    (Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), prefs), dir)
}

#[test]
fn new_from_a_bundled_template_gives_its_skeleton_with_no_assets() {
    let (mut s, dir) = session("bundled");
    let list = run(&mut s, "storyboard.templates", json!({}));
    let ids: Vec<&str> = list["templates"].as_array().unwrap().iter().filter_map(|t| t["id"].as_str()).collect();
    assert_eq!(ids, ["review", "tutorial", "instructions"]);
    assert_eq!(list["folder"].as_str().map(PathBuf::from), Some(dir.join("Templates")));
    let st = run(&mut s, "storyboard.new", json!({ "template": "tutorial" }));
    let board = s.storyboard().unwrap();
    assert_eq!(board.name, "Tutorial");
    assert!(!board.template && board.track.is_none());
    assert!(board.clips.len() > 8 && board.assets_assigned() == 0);
    assert_eq!(st["document"], Value::Null);
    assert_eq!(s.mode(), Mode::Storyboard);
    run(&mut s, "storyboard.new", json!({ "template": "review", "name": "Lamp review" }));
    assert_eq!(s.storyboard().unwrap().name, "Lamp review");
    for bad in
        [json!({ "template": "nope" }), json!({ "template": "relative.storyboard.json" }), json!({ "template": 7 })]
    {
        assert!(matches!(s.execute("storyboard.new", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_as_template_writes_the_shape_and_new_reads_it_back() {
    let (mut s, dir) = session("user");
    run(&mut s, "storyboard.new", json!({ "sample": true }));
    let still = dir.join("still.png");
    std::fs::write(&still, b"png").unwrap();
    run(&mut s, "storyboard.place", json!({ "asset": still.to_string_lossy() }));
    let shape: Vec<(assetcraft_storyboard::ShotType, String)> =
        s.storyboard().unwrap().clips.iter().map(|c| (c.shot_type, c.description.clone())).collect();
    assert!(s.enabled("storyboard.saveTemplate").is_ok());
    let out = run(&mut s, "storyboard.saveTemplate", json!({ "name": "Teaser: 24 shots?" }));
    let path = PathBuf::from(out["path"].as_str().unwrap());
    assert_eq!(path, dir.join("Templates").join("Teaser- 24 shots.storyboard.json"));
    assert_eq!(out["message"], "Saved template Teaser- 24 shots");
    assert_eq!(s.storyboard().unwrap().assets_assigned(), 1, "the open board is untouched");
    assert!(s.storyboard_dirty(), "and still dirty");
    let saved = assetcraft_storyboard::Storyboard::from_json(&std::fs::read(&path).unwrap()).unwrap();
    assert!(saved.template && saved.track.is_none() && saved.asset_refs().next().is_none());
    assert_eq!(saved.name, "Teaser: 24 shots?");
    // Exists now: refused without overwrite.
    assert!(matches!(
        s.execute("storyboard.saveTemplate", &json!({ "name": "Teaser: 24 shots?" })),
        Err(CmdError::Unavailable(_))
    ));
    run(&mut s, "storyboard.saveTemplate", json!({ "name": "Teaser: 24 shots?", "overwrite": true }));
    for bad in [json!({}), json!({ "name": "" }), json!({ "name": "???" })] {
        assert!(matches!(s.execute("storyboard.saveTemplate", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    // Listed under the bundled three, and usable for New.
    let list = run(&mut s, "storyboard.templates", json!({}));
    let user: Vec<&Value> = list["templates"].as_array().unwrap().iter().filter(|t| t["source"] == "user").collect();
    assert_eq!(user.len(), 1);
    assert_eq!(user[0]["name"], "Teaser- 24 shots");
    assert_eq!(user[0]["path"].as_str().map(PathBuf::from), Some(path.clone()));
    run(&mut s, "storyboard.new", json!({ "template": path.to_string_lossy() }));
    let fresh = s.storyboard().unwrap();
    assert_eq!(fresh.name, "Teaser: 24 shots?");
    assert!(!fresh.template && fresh.assets_assigned() == 0 && fresh.track.is_none());
    let fresh_shape: Vec<(assetcraft_storyboard::ShotType, String)> =
        fresh.clips.iter().map(|c| (c.shot_type, c.description.clone())).collect();
    assert_eq!(fresh_shape, shape);
    // A plain (non-template) board file works as a template too, named after the file.
    let plain = dir.join("Templates").join("Plain.storyboard.json");
    run(&mut s, "storyboard.saveAs", json!({ "path": plain.to_string_lossy() }));
    run(&mut s, "storyboard.new", json!({ "template": plain.to_string_lossy() }));
    assert_eq!(s.storyboard().unwrap().name, "Plain");
    assert_eq!(s.storyboard().unwrap().assets_assigned(), 0, "its assets are left behind");
    let _ = std::fs::remove_dir_all(&dir);
}
