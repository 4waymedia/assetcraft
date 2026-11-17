//! `storyboard.frame` / `storyboard.framePresets` (frame-settings.md F1).

use super::*;
use assetcraft_fs::FakeFs;
use serde_json::json;

fn session() -> Session {
    let fs = FakeFs::new().known(KnownFolder::Home, "/home/paul");
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    s.execute("storyboard.new", &json!({ "sample": true })).unwrap();
    s
}

fn run(s: &mut Session, p: Value) -> Value {
    s.execute("storyboard.frame", &p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

#[test]
fn the_presets_are_listed_for_the_ui_cli_and_mcp() {
    let mut s = session();
    let r = s.execute("storyboard.framePresets", &json!({})).unwrap();
    let presets = r["presets"].as_array().unwrap();
    assert_eq!(presets.len(), assetcraft_storyboard::FRAME_PRESETS.len());
    let vertical = presets.iter().find(|p| p["id"] == "vertical").unwrap();
    assert_eq!((vertical["width"].as_u64(), vertical["height"].as_u64()), (Some(1080), Some(1920)));
    assert_eq!(vertical["ratio"], "9:16");
    assert_eq!(r["rates"], json!([24, 25, 30, 50, 60]));
    assert_eq!(r["snaps"], json!(["center", "top", "bottom", "left", "right"]));
}

#[test]
fn a_board_starts_hd_and_takes_a_preset_in_one_undo_step() {
    let mut s = session();
    let r = run(&mut s, json!({}));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64(), r["fps"].as_u64()), (Some(1920), Some(1080), Some(24)));
    assert_eq!(r["changed"], false);
    let undo = s.boards.undo_len();
    let r = run(&mut s, json!({ "preset": "vertical" }));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64(), r["fps"].as_u64()), (Some(1080), Some(1920), Some(30)));
    assert_eq!(
        (r["ratio"].as_str(), r["vertical"].as_bool(), r["preset"].as_str()),
        (Some("9:16"), Some(true), Some("vertical"))
    );
    assert_eq!(s.boards.undo_len(), undo + 1);
    // The same again: no undo step.
    run(&mut s, json!({ "preset": "vertical" }));
    assert_eq!(s.boards.undo_len(), undo + 1);
    // The crop is the board's choice and keeps the preset.
    let r = run(&mut s, json!({ "fill": "horizontal", "snap": "top" }));
    assert_eq!(
        (r["fill"].as_str(), r["snap"].as_str(), r["preset"].as_str()),
        (Some("horizontal"), Some("top"), Some("vertical"))
    );
    // A rate of its own makes it custom.
    let r = run(&mut s, json!({ "fps": 60 }));
    assert_eq!((r["fps"].as_u64(), r["preset"].as_str()), (Some(60), None));
    assert!(r["message"].as_str().unwrap().starts_with("Custom: 1080 × 1920 (9:16) at 60 fps"), "{r}");
    s.execute("storyboard.undo", &json!({})).unwrap();
    assert_eq!(s.storyboard().unwrap().frame_spec().fps, 30);
}

#[test]
fn the_frame_is_saved_and_checked_on_load() {
    let mut s = session();
    let text = serde_json::to_string(s.storyboard().unwrap()).unwrap();
    assert!(!text.contains("\"frame\""), "today's frame is left out of the file");
    run(&mut s, json!({ "preset": "scope" }));
    let text = serde_json::to_string(s.storyboard().unwrap()).unwrap();
    let back = assetcraft_storyboard::Storyboard::from_json(text.as_bytes()).unwrap();
    assert_eq!(back.frame_spec().ratio_label(), "2.39:1");
    let mut odd: Value = serde_json::from_str(&text).unwrap();
    odd["frame"]["width"] = json!(2047);
    assert!(assetcraft_storyboard::Storyboard::from_json(serde_json::to_string(&odd).unwrap().as_bytes()).is_err());
    // Back to HD 24: the field goes again.
    run(&mut s, json!({ "preset": "hd" }));
    run(&mut s, json!({ "fill": "vertical", "snap": "center" }));
    let r = run(&mut s, json!({ "preset": "hd" }));
    assert_eq!(r["preset"], "hd");
}

#[test]
fn a_bad_frame_is_refused_and_changes_nothing() {
    let mut s = session();
    let undo = s.boards.undo_len();
    for bad in [
        json!({ "preset": "imax" }),
        json!({ "preset": 3 }),
        json!({ "width": 1081 }),
        json!({ "height": 4 }),
        json!({ "fps": 29 }),
        json!({ "fps": -1 }),
        json!({ "fill": "stretch" }),
        json!({ "snap": "middle" }),
    ] {
        assert!(matches!(s.execute("storyboard.frame", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert_eq!(s.boards.undo_len(), undo);
    assert_eq!(s.storyboard().unwrap().frame, None);
}

/// F3: a named export size is its short side in the board's shape.
#[test]
fn named_export_sizes_follow_the_boards_shape() {
    use crate::export_job::in_shape;
    let spec = |id| assetcraft_storyboard::frame_preset(id).map(|p| p.spec()).unwrap();
    assert_eq!(in_shape(&spec("hd"), 720), (1280, 720));
    assert_eq!(in_shape(&spec("vertical"), 1080), (1080, 1920));
    assert_eq!(in_shape(&spec("square"), 720), (720, 720));
    assert_eq!(in_shape(&spec("portrait"), 1080), (1080, 1350));
    let (w, h) = in_shape(&spec("scope"), 1080);
    assert!(h == 1080 && w % 2 == 0 && (f64::from(w) / 1080.0 - 2048.0 / 858.0).abs() < 0.01);
}
