//! `storyboard.inset` / `storyboard.insetClear` (SR2a): a slot, a file, the options, the
//! section stretched to a recording, the refusals, one undo step each, the file round trip.

use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::{Corner, InsetKind, InsetSize, SceneId, ShotType, Ticks};
use serde_json::json;

fn session() -> Session {
    let fs = FakeFs::new().known(KnownFolder::Home, "/home/paul");
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    s.execute("storyboard.new", &json!({"template": "tutorial", "steps": 2})).unwrap();
    s
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn inset(s: &Session, scene: u32) -> Option<assetcraft_storyboard::Inset> {
    s.storyboard().unwrap().scenes.iter().find(|sc| sc.id == SceneId(scene)).unwrap().inset.clone()
}

#[test]
fn an_inset_is_set_changed_and_cleared_in_one_undo_step_each() {
    let mut s = session();
    let board = s.storyboard().unwrap().clone();
    let scene = board.inset_scenes().next().unwrap().id;
    let undo = s.boards.undo_len();
    // A file in the default corner and size.
    let r = run(&mut s, "storyboard.inset", json!({"scene": scene.0, "asset": "/clips/host_section1.mp4"}));
    assert_eq!(r["changed"], true);
    assert_eq!(r["inset"]["asset"], "/clips/host_section1.mp4");
    assert_eq!(
        (r["inset"]["kind"].as_str(), r["inset"]["corner"].as_str(), r["inset"]["size"].as_str()),
        (Some("video"), Some("bottomRight"), Some("medium"))
    );
    assert_eq!(r["message"], format!("Inset on scene {}: host_section1.mp4", scene.0));
    assert_eq!(s.boards.undo_len(), undo + 1);
    let i = inset(&s, scene.0).unwrap();
    assert_eq!(i.kind, InsetKind::Video);
    assert!(!i.mix_audio && !i.swap);
    // Options, one at a time; the others keep their values.
    run(&mut s, "storyboard.inset", json!({"scene": scene.0, "corner": "TL", "size": "L"}));
    run(&mut s, "storyboard.inset", json!({"scene": scene.0, "mixAudio": true}));
    run(&mut s, "storyboard.inset", json!({"scene": scene.0, "swap": true, "kind": "graphic"}));
    let i = inset(&s, scene.0).unwrap();
    assert_eq!(
        (i.corner, i.size, i.mix_audio, i.swap, i.kind),
        (Corner::TopLeft, InsetSize::Large, true, true, InsetKind::Graphic)
    );
    assert_eq!(i.asset.as_ref().unwrap().0, "/clips/host_section1.mp4");
    assert_eq!(s.boards.undo_len(), undo + 4);
    // The same again: no change, no step.
    let r = run(&mut s, "storyboard.inset", json!({"scene": scene.0, "corner": "topLeft"}));
    assert_eq!(r["changed"], false);
    assert_eq!(s.boards.undo_len(), undo + 4);
    // Clear keeps the slot (and its options); remove drops the lane.
    let r = run(&mut s, "storyboard.insetClear", json!({"scene": scene.0}));
    assert_eq!(r["inset"]["asset"], Value::Null);
    assert_eq!(r["inset"]["corner"], "topLeft");
    assert!(inset(&s, scene.0).unwrap().is_empty());
    run(&mut s, "storyboard.insetClear", json!({"scene": scene.0, "remove": true}));
    assert!(inset(&s, scene.0).is_none());
    assert_eq!(s.boards.undo_len(), undo + 6);
    run(&mut s, "storyboard.undo", json!({}));
    assert!(inset(&s, scene.0).unwrap().is_empty());
    for _ in 0..5 {
        run(&mut s, "storyboard.undo", json!({}));
    }
    assert_eq!(s.storyboard().unwrap(), &board);
    // An empty slot on its own.
    let r = run(&mut s, "storyboard.inset", json!({"scene": scene.0, "kind": "graphic"}));
    assert_eq!(r["inset"]["asset"], Value::Null);
    assert_eq!(r["message"], format!("Empty inset slot on scene {}", scene.0));
    // A file, then `asset: null` empties it again.
    run(&mut s, "storyboard.inset", json!({"scene": scene.0, "asset": "/g/parts.png"}));
    run(&mut s, "storyboard.inset", json!({"scene": scene.0, "asset": null}));
    assert!(inset(&s, scene.0).unwrap().is_empty());
}

#[test]
fn seconds_stretch_the_sections_shots_pro_rata_and_keep_its_title_card() {
    let mut s = session();
    let board = s.storyboard().unwrap().clone();
    let scene = board.inset_scenes().find(|sc| board.inset_span(sc.id).count() >= 2).unwrap().id;
    let title: Vec<Ticks> = board
        .clips
        .iter()
        .filter(|c| c.scene_id == scene && c.shot_type == ShotType::Ttl)
        .map(|c| c.duration)
        .collect();
    let shots: Vec<Ticks> = board.inset_span(scene).map(|c| c.duration).collect();
    assert!(shots.len() >= 2, "the tutorial's step has a shot and an insert");
    let span: f64 = shots.iter().map(|t| t.as_secs_f64()).sum();
    let r =
        run(&mut s, "storyboard.inset", json!({"scene": scene.0, "asset": "/clips/host.mp4", "seconds": span * 3.0}));
    assert!((r["spanSeconds"].as_f64().unwrap() - span * 3.0).abs() < 0.001, "{r}");
    assert!(r["message"].as_str().unwrap().contains("section now"), "{r}");
    let after = s.storyboard().unwrap();
    let now: Vec<Ticks> = after.inset_span(scene).map(|c| c.duration).collect();
    for (a, b) in shots.iter().zip(&now) {
        assert!((b.as_secs_f64() - a.as_secs_f64() * 3.0).abs() < 0.01, "pro rata: {a:?} → {b:?}");
    }
    let title_now: Vec<Ticks> = after
        .clips
        .iter()
        .filter(|c| c.scene_id == scene && c.shot_type == ShotType::Ttl)
        .map(|c| c.duration)
        .collect();
    assert_eq!(title, title_now, "title cards keep their length");
    assert_eq!(after.clips.len(), board.clips.len());
    // Other scenes untouched.
    for (a, b) in board.clips.iter().zip(&after.clips) {
        if a.scene_id != scene {
            assert_eq!(a, b);
        }
    }
    // Shrinking to the floor: every shot at least the minimum.
    let n = now.len() as f64;
    run(&mut s, "storyboard.inset", json!({"scene": scene.0, "seconds": MIN_CLIP_SECS * n}));
    for c in s.storyboard().unwrap().inset_span(scene) {
        assert!((c.duration.as_secs_f64() - MIN_CLIP_SECS).abs() < 0.001);
    }
    assert!(matches!(
        s.execute("storyboard.inset", &json!({"scene": scene.0, "seconds": MIN_CLIP_SECS * n - 0.2})),
        Err(CmdError::BadParams(_))
    ));
    // One undo step for file + stretch together.
    let undo = s.boards.undo_len();
    run(&mut s, "storyboard.inset", json!({"scene": scene.0, "asset": "/clips/other.mp4", "seconds": 12.0}));
    assert_eq!(s.boards.undo_len(), undo + 1);
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(
        s.storyboard().unwrap().inset_span(scene).map(|c| c.duration.as_secs_f64()).sum::<f64>(),
        MIN_CLIP_SECS * n
    );
}

#[test]
fn refusals_leave_no_undo_step_and_the_inset_survives_the_file_and_templates() {
    let mut s = session();
    let board = s.storyboard().unwrap().clone();
    let scene = board.inset_scenes().next().unwrap().id;
    let titles_only = board.scenes.iter().find(|sc| board.inset_span(sc.id).next().is_none()).map(|sc| sc.id);
    let undo = s.boards.undo_len();
    for (bad, why) in [
        (json!({}), "no scene"),
        (json!({"scene": "one"}), "scene not a number"),
        (json!({"scene": 999}), "no such scene"),
        (json!({"scene": scene.0, "kind": "audio"}), "kind"),
        (json!({"scene": scene.0, "corner": "middle"}), "corner"),
        (json!({"scene": scene.0, "size": "huge"}), "size"),
        (json!({"scene": scene.0, "mixAudio": "yes"}), "flag"),
        (json!({"scene": scene.0, "asset": ""}), "empty asset"),
        (json!({"scene": scene.0, "asset": 7}), "asset not a path"),
        (json!({"scene": scene.0, "seconds": 0.1}), "too short"),
        (json!({"scene": scene.0, "seconds": 7.0 * 3600.0}), "too long"),
    ] {
        assert!(matches!(s.execute("storyboard.inset", &bad), Err(CmdError::BadParams(_))), "{why}: {bad}");
    }
    if let Some(t) = titles_only {
        assert!(
            matches!(
                s.execute("storyboard.inset", &json!({"scene": t.0, "asset": "/x.mp4"})),
                Err(CmdError::Unavailable(_))
            ),
            "a title-only scene has nothing to sit over"
        );
    }
    assert!(matches!(s.execute("storyboard.insetClear", &json!({})), Err(CmdError::BadParams(_))));
    assert_eq!(run(&mut s, "storyboard.insetClear", json!({"scene": scene.0}))["changed"], false, "nothing to clear");
    assert_eq!(s.boards.undo_len(), undo);
    assert_eq!(s.storyboard().unwrap(), &board);

    // The file: an inset round-trips; a board without one writes no `inset` key.
    run(
        &mut s,
        "storyboard.inset",
        json!({"scene": scene.0, "asset": "/clips/host.mp4", "corner": "BL", "mixAudio": true}),
    );
    let text = s.storyboard().unwrap().to_json().unwrap();
    assert!(text.contains("\"mixAudio\": true") || text.contains("\"mixAudio\":true"), "{text}");
    let back = assetcraft_storyboard::Storyboard::from_json(text.as_bytes()).unwrap();
    assert_eq!(&back, s.storyboard().unwrap());
    assert_eq!(back.asset_refs().filter(|a| a.0 == "/clips/host.mp4").count(), 1, "an inset's file is an asset ref");
    assert!(!board.to_json().unwrap().contains("inset"), "no lane, no key");
    // A template keeps the slot and drops the file.
    let t = back.as_template("Mine");
    let slot = t.scenes.iter().find(|sc| sc.id == scene).unwrap().inset.clone().unwrap();
    assert!(slot.is_empty());
    assert_eq!((slot.corner, slot.mix_audio), (Corner::BottomLeft, true));
    // No board.
    let mut s = Session::new(
        Arc::new(FakeFs::new().known(KnownFolder::Home, "/home/paul")),
        Arc::new(RecordingLauncher::default()),
        Prefs::default(),
    );
    assert!(s.enabled("storyboard.inset").is_err());
    assert!(matches!(s.execute("storyboard.inset", &json!({"scene": 1})), Err(CmdError::Unavailable(_))));
}
