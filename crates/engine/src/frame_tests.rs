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

/// F4: the workspace default frame — set like a board's, copied from the open board, or reset —
/// is what new storyboards start with, unless `storyboard.new` names its own; it is kept in prefs.
#[test]
fn new_storyboards_start_with_the_default_frame() {
    let mut s = session();
    let ask =
        |s: &mut Session, p: Value| s.execute("storyboard.defaultFrame", &p).unwrap_or_else(|e| panic!("{p}: {e}"));
    assert_eq!(ask(&mut s, json!({}))["ratio"], "16:9");
    let r = ask(&mut s, json!({ "preset": "vertical", "snap": "top" }));
    assert_eq!(
        (r["changed"].as_bool(), r["ratio"].as_str(), r["snap"].as_str()),
        (Some(true), Some("9:16"), Some("top"))
    );
    let undo = s.boards.undo_len();
    s.execute("storyboard.new", &json!({ "name": "Reel" })).unwrap();
    let f = s.storyboard().unwrap().frame_spec();
    assert_eq!((f.width, f.height, f.fps, f.snap), (1080, 1920, 30, assetcraft_storyboard::Snap::Top));
    assert!(s.boards.undo_len() >= undo, "the setting itself is no undo step");
    // A storyboard's own frame wins.
    s.execute("storyboard.new", &json!({ "frame": "square" })).unwrap();
    assert_eq!(s.storyboard().unwrap().frame_spec().ratio_label(), "1:1");
    assert!(s.execute("storyboard.new", &json!({ "frame": "imax" })).is_err());
    // Copied from the open board.
    s.execute("storyboard.frame", &json!({ "preset": "scope" })).unwrap();
    assert_eq!(ask(&mut s, json!({ "fromBoard": true }))["ratio"], "2.39:1");
    // Kept in prefs, and read back; a bad one in the file is dropped.
    let text = serde_json::to_string(&s.prefs()).unwrap();
    let back = Prefs::parse(&text).0;
    assert_eq!(back.frame.as_ref().map(|f| f.width), Some(2048));
    let mut bad: Value = serde_json::from_str(&text).unwrap();
    bad["frame"]["width"] = json!(2047);
    assert_eq!(Prefs::parse(&bad.to_string()).0.frame, None);
    // Reset: new storyboards are HD 24 again, with no frame in the file.
    ask(&mut s, json!({ "reset": true }));
    s.execute("storyboard.new", &json!({})).unwrap();
    assert_eq!(s.storyboard().unwrap().frame, None);
    assert!(s.prefs().frame.is_none());
}

/// F5 (L1): `storyboard.layout` splits the stage and `storyboard.region` puts clips in A or B —
/// one undo step each, title cards left alone, saved and checked on load.
#[test]
fn a_split_stage_and_its_regions() {
    use assetcraft_storyboard::{ClipId, Layout, Region, ShotType};
    let mut s = session();
    let undo = s.boards.undo_len();
    let r = s.execute("storyboard.layout", &json!({ "kind": "split" })).unwrap();
    assert_eq!((r["stack"].as_bool(), r["share"].as_u64()), (Some(true), Some(50)), "{r}");
    assert_eq!(s.boards.undo_len(), undo + 1);
    let r = s.execute("storyboard.layout", &json!({ "share": 70 })).unwrap();
    assert_eq!((r["stack"].as_bool(), r["share"].as_u64()), (Some(true), Some(70)), "keeps stacked");
    s.execute("storyboard.layout", &json!({ "stack": false })).unwrap();
    assert_eq!(s.storyboard().unwrap().layout, Some(Layout::Split { stack: false, share: 70 }));
    let undo = s.boards.undo_len();
    s.execute("storyboard.layout", &json!({ "kind": "split", "stack": false, "share": 70 })).unwrap();
    assert_eq!(s.boards.undo_len(), undo, "the same layout: no step");
    for bad in [json!({ "kind": "grid" }), json!({ "share": 55 }), json!({ "stack": "yes" }), json!({ "kind": 3 })] {
        assert!(matches!(s.execute("storyboard.layout", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    // Regions: shots move, a title card stays.
    let board = s.storyboard().unwrap().clone();
    let title = board.clips.iter().find(|c| c.shot_type == ShotType::Ttl).map(|c| c.id.0);
    let shots: Vec<u32> = board.clips.iter().filter(|c| c.shot_type != ShotType::Ttl).take(2).map(|c| c.id.0).collect();
    let mut ids = shots.clone();
    ids.extend(title);
    let undo = s.boards.undo_len();
    let r = s.execute("storyboard.region", &json!({ "clips": ids, "region": "b" })).unwrap();
    assert_eq!(r["changed"], true, "{r}");
    assert_eq!(s.boards.undo_len(), undo + 1, "one step for all");
    let b = s.storyboard().unwrap();
    for id in &shots {
        assert_eq!(b.clip(ClipId(*id)).unwrap().region(), Region::B);
    }
    if let Some(t) = title {
        assert_eq!(b.clip(ClipId(t)).unwrap().region, None, "a title card takes the frame");
    }
    let r = s.execute("storyboard.region", &json!({ "clip": shots[0], "region": "B" })).unwrap();
    assert_eq!(r["changed"], false);
    for bad in
        [json!({ "clip": shots[0], "region": "c" }), json!({ "region": "a" }), json!({ "clip": 9999, "region": "a" })]
    {
        assert!(matches!(s.execute("storyboard.region", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    // Saved, and checked on load.
    let text = serde_json::to_string(s.storyboard().unwrap()).unwrap();
    let back = assetcraft_storyboard::Storyboard::from_json(text.as_bytes()).unwrap();
    assert_eq!(back.layout, Some(Layout::Split { stack: false, share: 70 }));
    let mut bad: Value = serde_json::from_str(&text).unwrap();
    bad["layout"]["share"] = json!(55);
    assert!(assetcraft_storyboard::Storyboard::from_json(bad.to_string().as_bytes()).is_err());
    // Back to full: the field leaves the file; regions stay on the clips for next time.
    s.execute("storyboard.layout", &json!({ "kind": "full" })).unwrap();
    assert!(!serde_json::to_string(s.storyboard().unwrap()).unwrap().contains("\"layout\""));
}

/// F5 (L2): `storyboard.layout {kind: "framed"}` frames the post — headline (the board's name to
/// start with) and caption bands in a text theme — one undo step, fields kept when not given.
#[test]
fn a_framed_post() {
    use assetcraft_storyboard::{Layout, TextTheme};
    let mut s = session();
    let name = s.storyboard().unwrap().name.clone();
    let r = s.execute("storyboard.layout", &json!({ "kind": "framed" })).unwrap();
    assert_eq!(r["kind"], "framed", "{r}");
    assert_eq!(
        s.storyboard().unwrap().layout,
        Some(Layout::Framed { top: 14, bottom: 12, headline: name, caption: String::new(), theme: TextTheme::Bold })
    );
    let undo = s.boards.undo_len();
    s.execute("storyboard.layout", &json!({ "caption": "Swipe for more", "theme": "paper" })).unwrap();
    assert_eq!(s.boards.undo_len(), undo + 1);
    let Some(Layout::Framed { top, caption, theme, .. }) = s.storyboard().unwrap().layout.clone() else { panic!() };
    assert_eq!((top, caption.as_str(), theme), (14, "Swipe for more", TextTheme::Paper), "kept and changed");
    for bad in [
        json!({ "kind": "framed", "top": 40 }),
        json!({ "kind": "framed", "headline": "x".repeat(201) }),
        json!({ "kind": "framed", "theme": "rainbow" }),
        json!({ "kind": "framed", "caption": 3 }),
    ] {
        assert!(matches!(s.execute("storyboard.layout", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    let text = serde_json::to_string(s.storyboard().unwrap()).unwrap();
    let back = assetcraft_storyboard::Storyboard::from_json(text.as_bytes()).unwrap();
    assert_eq!(back.layout, s.storyboard().unwrap().layout);
    // From framed to split and back to full.
    s.execute("storyboard.layout", &json!({ "kind": "split" })).unwrap();
    assert!(matches!(s.storyboard().unwrap().layout, Some(Layout::Split { .. })));
    s.execute("storyboard.layout", &json!({ "kind": "full" })).unwrap();
    assert_eq!(s.storyboard().unwrap().layout, None);
}
