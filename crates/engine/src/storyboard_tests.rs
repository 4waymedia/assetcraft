use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::Ticks;
use serde_json::json;

fn session() -> Session {
    let fs = FakeFs::new().known(KnownFolder::Home, "/home/paul");
    Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default())
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn starts_in_the_organizer_with_no_storyboard() {
    let mut s = session();
    assert_eq!(s.mode(), Mode::Organizer);
    let st = run(&mut s, "storyboard.state", json!({}));
    assert_eq!(st["mode"], "organizer");
    assert!(st["board"].is_null());
    assert!(matches!(s.execute("storyboard.place", &json!({"asset": "/a.png"})), Err(CmdError::Unavailable(_))));
    assert!(matches!(s.execute("storyboard.undo", &json!({})), Err(CmdError::Unavailable(_))));
    assert!(matches!(s.execute("app.mode", &json!({"mode": "x"})), Err(CmdError::BadParams(_))));
    run(&mut s, "app.mode", json!({"mode": "storyboard"}));
    assert_eq!(run(&mut s, "app.state", json!({}))["mode"], "storyboard");
}

#[test]
fn new_sample_opens_storyboard_mode_on_the_first_empty_clip() {
    let mut s = session();
    let st = run(&mut s, "storyboard.new", json!({"sample": true}));
    assert_eq!(s.mode(), Mode::Storyboard);
    assert_eq!(st["summary"]["clips"], 24);
    assert_eq!(st["summary"]["fillable"], 19);
    assert_eq!(st["selected"], 2, "clip 1 is a title card");
    assert_eq!(st["board"]["clips"][0]["shotType"], "TTL");
    // Manual start: one title card, no track.
    run(&mut s, "storyboard.new", json!({"name": "Mine"}));
    let board = s.storyboard().unwrap();
    assert_eq!((board.name.as_str(), board.clips.len(), board.track.is_none()), ("Mine", 1, true));
    assert!(matches!(s.execute("storyboard.recut", &json!({})), Err(CmdError::Unavailable(_))));
    assert!(matches!(s.execute("storyboard.new", &json!({"name": "  "})), Err(CmdError::BadParams(_))));
}

#[test]
fn quick_fill_undo_and_redo() {
    let mut s = session();
    run(&mut s, "storyboard.new", json!({"sample": true}));
    let r = run(&mut s, "storyboard.place", json!({"asset": "/art/a.png"}));
    assert_eq!((r["placed"].as_u64(), r["next"].as_u64(), r["filled"].as_u64()), (Some(2), Some(3), Some(1)));
    assert_eq!(s.selected_clip().map(|c| c.0), Some(3), "cursor advances");
    run(&mut s, "storyboard.place", json!({"asset": "/art/b.png", "clip": 5}));
    assert_eq!(s.storyboard().unwrap().clip(assetcraft_storyboard::ClipId(5)).unwrap().asset_ids.len(), 1);
    // Refused placements leave no undo step.
    let before = s.boards.undo_len();
    assert!(s.execute("storyboard.place", &json!({"asset": "/x.png", "clip": 1})).is_err());
    assert!(s.execute("storyboard.place", &json!({"asset": "/x.png", "clip": 999})).is_err());
    assert!(s.execute("storyboard.place", &json!({"asset": ""})).is_err());
    assert!(s.execute("storyboard.place", &json!({"asset": "/x.png", "clip": -1})).is_err());
    assert_eq!(s.boards.undo_len(), before);

    run(&mut s, "storyboard.undo", json!({}));
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(s.storyboard().unwrap().fill_progress().0, 0);
    assert_eq!(s.selected_clip().map(|c| c.0), Some(2), "undo restores the selection too");
    run(&mut s, "storyboard.redo", json!({}));
    assert_eq!(s.storyboard().unwrap().fill_progress().0, 1);
    // A new edit clears redo.
    run(&mut s, "storyboard.place", json!({"asset": "/art/c.png"}));
    assert!(!s.can_redo_storyboard());
    // Undo goes all the way back to "no storyboard", and is bounded.
    while s.can_undo_storyboard() {
        run(&mut s, "storyboard.undo", json!({}));
    }
    assert!(s.storyboard().is_none());
    run(&mut s, "storyboard.new", json!({"sample": true}));
    for i in 0..(MAX_UNDO + 20) {
        run(&mut s, "storyboard.place", json!({"asset": format!("/a{i}.png"), "clip": 2}));
    }
    assert_eq!(s.boards.undo_len(), MAX_UNDO);
}

#[test]
fn select_and_recut() {
    let mut s = session();
    run(&mut s, "storyboard.new", json!({"sample": true}));
    run(&mut s, "storyboard.select", json!({"clip": 7}));
    assert_eq!(s.selected_clip().map(|c| c.0), Some(7));
    run(&mut s, "storyboard.select", json!({"clip": null}));
    assert_eq!(s.selected_clip(), None);
    assert!(s.execute("storyboard.select", &json!({"clip": 99})).is_err());
    assert!(s.execute("storyboard.select", &json!({"clip": "7"})).is_err());

    let r = run(&mut s, "storyboard.recut", json!({"bpm": 128, "seconds": 30}));
    assert_eq!(r["fits"], true);
    assert!(r["message"].as_str().unwrap().starts_with("Re-cut to Sample track · 128 BPM"));
    let board = s.storyboard().unwrap();
    assert!((board.track.as_ref().unwrap().bpm.get() - 128.0).abs() < 1e-9);
    assert!(board.total() <= assetcraft_storyboard::Ticks::from_secs(30).unwrap());
    for bad in [json!({"bpm": 5}), json!({"bpm": "fast"}), json!({"seconds": -1}), json!({"seconds": 1e12})] {
        assert!(matches!(s.execute("storyboard.recut", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    run(&mut s, "storyboard.undo", json!({}));
    assert!((s.storyboard().unwrap().track.as_ref().unwrap().bpm.get() - 92.0).abs() < 1e-9);
}

#[test]
fn view_switches_between_map_and_strip() {
    let mut s = session();
    assert_eq!(s.board_view(), BoardView::Map);
    assert_eq!(run(&mut s, "storyboard.view", json!({"view": "strip"}))["view"], "strip");
    assert_eq!(run(&mut s, "storyboard.state", json!({}))["view"], "strip");
    assert!(matches!(s.execute("storyboard.view", &json!({"view": "tracks"})), Err(CmdError::BadParams(_))));
    assert_eq!(s.board_view(), BoardView::Strip);
}

#[test]
fn autofill_and_clear_are_single_undo_steps() {
    let mut s = session();
    assert!(matches!(s.execute("storyboard.autofill", &json!({})), Err(CmdError::Unavailable(_))));
    run(&mut s, "storyboard.new", json!({"sample": true}));
    // The fake home folder has no files: nothing to fill with.
    assert!(matches!(s.execute("storyboard.autofill", &json!({})), Err(CmdError::Unavailable(_))));
    let before = s.boards.undo_len();
    let r = run(&mut s, "storyboard.autofill", json!({"assets": ["/a.png", "/b.png"]}));
    assert_eq!(r["filled"].as_array().map(Vec::len), Some(19));
    assert_eq!(s.boards.undo_len(), before + 1);
    assert_eq!(s.selected_clip(), None, "nothing left to target");
    assert!(matches!(s.execute("storyboard.autofill", &json!({"assets": ["/a.png"]})), Err(CmdError::Unavailable(_))));
    for bad in [json!({"assets": "x"}), json!({"assets": [1]}), json!({"assets": [""]})] {
        assert!(matches!(s.execute("storyboard.autofill", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert_eq!(run(&mut s, "storyboard.clear", json!({}))["cleared"], 19);
    assert!(matches!(s.execute("storyboard.clear", &json!({})), Err(CmdError::Unavailable(_))));
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(s.storyboard().unwrap().fill_progress().0, 19);
}

#[test]
fn autofill_defaults_to_the_open_folder() {
    let fs = FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .known(KnownFolder::Pictures, "/home/paul/Pictures")
        .file("/home/paul/Pictures/one.png", 10)
        .file("/home/paul/Pictures/two.png", 10)
        .dir("/home/paul/Pictures/sub");
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut s, "storyboard.new", json!({"sample": true}));
    run(&mut s, "storyboard.autofill", json!({}));
    let board = s.storyboard().unwrap();
    assert_eq!(board.fill_progress(), (19, 19));
    assert!(board.clips.iter().flat_map(|c| &c.asset_ids).all(|a| a.0.ends_with(".png")), "folders are skipped");
}

#[test]
fn transport_plays_pauses_and_seeks_within_the_cut() {
    let mut s = session();
    assert!(matches!(s.execute("storyboard.play", &json!({})), Err(CmdError::Unavailable(_))));
    run(&mut s, "storyboard.new", json!({"sample": true}));
    let total = s.storyboard().unwrap().total();
    let r = run(&mut s, "storyboard.play", json!({}));
    assert_eq!((r["playing"].as_bool(), r["position"].as_f64()), (Some(true), Some(0.0)));
    run(&mut s, "storyboard.pause", json!({"at": 12.5}));
    assert_eq!(s.transport(), Transport { playing: false, position: Ticks::from_secs_f64(12.5).unwrap() });
    run(&mut s, "storyboard.pause", json!({"at": 1e6}));
    assert_eq!(s.transport().position, total, "clamped to the Cut");
    run(&mut s, "storyboard.play", json!({}));
    assert_eq!(s.transport().position, Ticks::ZERO, "play from the end starts over");
    run(&mut s, "storyboard.seek", json!({"seconds": 3}));
    assert!(!s.transport().playing);
    for bad in [json!({"seconds": -1}), json!({"seconds": "x"}), json!({})] {
        assert!(matches!(s.execute("storyboard.seek", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert_eq!(run(&mut s, "storyboard.state", json!({}))["transport"]["timecode"], "0:03.0");
    run(&mut s, "storyboard.new", json!({"sample": true}));
    assert_eq!(s.transport(), Transport::default(), "a new storyboard rewinds");
}
