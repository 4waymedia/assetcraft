//! `storyboard.motion` (SB-3D1 step 1): presets, partial cameras in degrees, swap, clear, undo.

use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::{ClipId, MotionPreset};
use serde_json::json;

fn session() -> Session {
    let fs = FakeFs::new().known(KnownFolder::Home, "/home/paul");
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    s.execute("storyboard.new", &json!({"sample": true})).unwrap();
    // Clip 2: a model; clip 3: a still.
    s.execute("storyboard.place", &json!({"asset": "/art/orc-warrior.glb", "clip": 2})).unwrap();
    s.execute("storyboard.place", &json!({"asset": "/art/concept.png", "clip": 3})).unwrap();
    s
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn motion(s: &Session, clip: u32) -> Option<assetcraft_storyboard::Motion> {
    s.storyboard().unwrap().clip(ClipId(clip)).unwrap().motion
}

#[test]
fn a_preset_is_one_undo_step_and_reports_degrees() {
    let mut s = session();
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.motion", json!({"clip": 2, "preset": "turntable"}));
    assert_eq!((r["changed"].as_bool(), r["kind"].as_str()), (Some(true), Some("Orbit camera")));
    assert_eq!(r["motion"]["preset"], "turntable");
    assert!((r["motion"]["amount"]["yaw"].as_f64().unwrap() - 360.0).abs() < 1e-3, "{r}");
    assert!((r["motion"]["end"]["yaw"].as_f64().unwrap() - 360.0).abs() < 1e-3);
    assert_eq!((r["motion"]["ease"].as_str(), r["motion"]["glyph"].as_str()), (Some("linear"), Some("↻")));
    assert!(r["message"].as_str().unwrap().contains("Turntable"), "{r}");
    assert_eq!(s.boards.undo_len(), undo + 1);
    assert_eq!(motion(&s, 2).map(|m| m.preset), Some(MotionPreset::Turntable));

    // The same preset again: no change, no undo step.
    let r = run(&mut s, "storyboard.motion", json!({"clip": 2, "preset": "turntable"}));
    assert_eq!(r["changed"], false);
    assert_eq!(s.boards.undo_len(), undo + 1);

    // Undo removes it.
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(motion(&s, 2), None);
    let st = run(&mut s, "storyboard.state", json!({}));
    assert!(st["board"]["clips"][1].get("motion").is_none(), "no motion is left out of the saved board");
}

#[test]
fn partial_cameras_land_on_the_current_motion() {
    let mut s = session();
    run(&mut s, "storyboard.motion", json!({"clip": 3, "preset": "pushIn"}));
    // A zoom edit alone: pan stays, chip flips to Custom.
    let r = run(&mut s, "storyboard.motion", json!({"clip": 3, "amount": {"zoom": 2.0}}));
    assert_eq!((r["motion"]["preset"].as_str(), r["motion"]["amount"]["zoom"].as_f64()), (Some("custom"), Some(2.0)));
    assert_eq!(r["motion"]["amount"]["pan"], json!([0.0, 0.0]));
    assert_eq!(r["motion"]["glyph"], "⤢");
    // A start in degrees with a preset in the same call: the preset's amount, the given start.
    let r = run(
        &mut s,
        "storyboard.motion",
        json!({"clip": 2, "preset": "rise", "start": {"yaw": 34.4, "pitch": 24.1}, "ease": "out"}),
    );
    let m = motion(&s, 2).unwrap();
    assert_eq!((m.preset, m.ease.id()), (MotionPreset::Custom, "out"), "an ease on top of a preset makes it Custom");
    assert!((m.start.yaw.to_degrees() - 34.4).abs() < 1e-3 && (m.start.pitch.to_degrees() - 24.1).abs() < 1e-3);
    assert!((r["motion"]["amount"]["pitch"].as_f64().unwrap() - 20.0).abs() < 1e-3, "Rise = +20°: {r}");
    assert_eq!(r["motion"]["start"]["zoom"], 1.0);
    // Swap, then clear.
    let r = run(&mut s, "storyboard.motion", json!({"clip": 2, "swap": true}));
    assert!((r["motion"]["start"]["pitch"].as_f64().unwrap() - 44.1).abs() < 1e-3, "{r}");
    assert!((r["motion"]["amount"]["pitch"].as_f64().unwrap() + 20.0).abs() < 1e-3);
    let r = run(&mut s, "storyboard.motion", json!({"clip": 2, "clear": true}));
    assert_eq!((r["changed"].as_bool(), r["motion"].is_null()), (Some(true), true));
    assert!(r["message"].as_str().unwrap().ends_with("no motion"));
    assert_eq!(run(&mut s, "storyboard.motion", json!({"clip": 2, "clear": true}))["changed"], false);
}

#[test]
fn refusals_leave_the_board_alone() {
    let mut s = session();
    let before = s.storyboard().unwrap().clone();
    let undo = s.boards.undo_len();
    let bad = |s: &mut Session, p: Value| s.execute("storyboard.motion", &p).unwrap_err().to_string();
    assert!(bad(&mut s, json!({"clip": 1, "preset": "turntable"})).contains("no shot to move"), "title card");
    assert!(bad(&mut s, json!({"clip": 3, "preset": "turntable"})).contains("does not apply"), "still");
    assert!(bad(&mut s, json!({"clip": 2, "amount": {"yaw": 400}})).contains("out of range"));
    assert!(bad(&mut s, json!({"clip": 2, "amount": {"zoom": "big"}})).contains("must be a number"));
    assert!(bad(&mut s, json!({"clip": 2, "amount": {"pan": [1]}})).contains("[x, y]"));
    assert!(bad(&mut s, json!({"clip": 2, "preset": "spin"})).contains("`preset`"));
    assert!(bad(&mut s, json!({"clip": 2, "ease": "bounce"})).contains("`ease`"));
    assert!(bad(&mut s, json!({"clip": 2})).contains("at least one"));
    assert!(bad(&mut s, json!({"clip": 99, "preset": "rise"})).contains("no clip"));
    assert!(bad(&mut s, json!({"preset": "rise"})).contains("`clip`"));
    assert_eq!(s.storyboard().unwrap(), &before);
    assert_eq!(s.boards.undo_len(), undo);
    // Disabled without a board.
    let fs = FakeFs::new().known(KnownFolder::Home, "/home/paul");
    let s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    assert!(s.enabled("storyboard.motion").is_err());
}
