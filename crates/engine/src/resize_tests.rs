//! `storyboard.resize` (SR1): seconds or beats, the limits, the total follows, one undo step.

use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::{ClipId, Ticks};
use serde_json::json;

fn session() -> Session {
    let fs = FakeFs::new().known(KnownFolder::Home, "/home/paul");
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    s.execute("storyboard.new", &json!({"sample": true})).unwrap();
    s
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn secs(s: &Session, clip: u32) -> f64 {
    s.storyboard().unwrap().clip(ClipId(clip)).unwrap().duration.as_secs_f64()
}

#[test]
fn seconds_resize_one_clip_and_the_total_follows_in_one_undo_step() {
    let mut s = session();
    let total = s.storyboard().unwrap().total();
    let track_len = s.storyboard().unwrap().track.as_ref().unwrap().duration;
    let before = secs(&s, 2);
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.resize", json!({"clip": 2, "seconds": 7.5}));
    assert_eq!(r["changed"], true);
    assert_eq!(r["seconds"], 7.5);
    assert!(r["beats"].is_number(), "the sample has a track: beats are reported");
    assert!(r["message"].as_str().unwrap().starts_with("Clip #02: 7.5 s"), "{r}");
    assert_eq!(secs(&s, 2), 7.5);
    let grew = Ticks::from_secs_f64(7.5 - before).unwrap();
    assert_eq!(s.storyboard().unwrap().total(), total.checked_add(grew).unwrap(), "only that clip changed");
    assert_eq!(r["total"], s.storyboard().unwrap().total().timecode());
    assert_eq!(s.boards.undo_len(), undo + 1);
    assert_eq!(s.storyboard().unwrap().track.as_ref().unwrap().duration, track_len, "track untouched");
    // The same length again: no change, no undo step.
    let r = run(&mut s, "storyboard.resize", json!({"clip": 2, "seconds": 7.5}));
    assert_eq!(r["changed"], false);
    assert_eq!(s.boards.undo_len(), undo + 1);
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(secs(&s, 2), before);
    assert_eq!(s.storyboard().unwrap().total(), total);
}

#[test]
fn beats_use_the_tracks_tempo_and_need_a_track() {
    let mut s = session();
    let bpm = s.storyboard().unwrap().track.as_ref().unwrap().bpm;
    let r = run(&mut s, "storyboard.resize", json!({"clip": 3, "beats": 8}));
    assert_eq!(r["beats"], 8);
    assert_eq!(s.storyboard().unwrap().clip(ClipId(3)).unwrap().duration, bpm.beats_to_ticks(8));
    // A manual board has no track: seconds still work, beats are refused.
    run(&mut s, "storyboard.new", json!({"name": "Mine"}));
    let r = run(&mut s, "storyboard.resize", json!({"clip": 1, "seconds": 2}));
    assert!(r["beats"].is_null());
    assert_eq!(r["message"], "Clip #01: 2.0 s · Cut 0:02.0");
    assert!(matches!(s.execute("storyboard.resize", &json!({"clip": 1, "beats": 4})), Err(CmdError::Unavailable(_))));
}

#[test]
fn bad_lengths_and_clips_are_refused_without_an_undo_step() {
    let mut s = session();
    let undo = s.boards.undo_len();
    for (bad, why) in [
        (json!({}), "no clip"),
        (json!({"clip": 2}), "no length"),
        (json!({"clip": 2, "seconds": 3, "beats": 4}), "both"),
        (json!({"clip": 2, "seconds": 0}), "zero"),
        (json!({"clip": 2, "seconds": -1}), "negative"),
        (json!({"clip": 2, "seconds": 0.2}), "shorter than the minimum"),
        (json!({"clip": 2, "seconds": 7.0 * 3600.0}), "longer than 6 hours"),
        (json!({"clip": 2, "seconds": 6.0 * 3600.0 - 1.0}), "the Cut would pass 6 hours"),
        (json!({"clip": 2, "beats": 0}), "zero beats"),
        (json!({"clip": 999, "seconds": 3}), "no such clip"),
        (json!({"clip": "two", "seconds": 3}), "clip not a number"),
    ] {
        assert!(matches!(s.execute("storyboard.resize", &bad), Err(CmdError::BadParams(_))), "{why}: {bad}");
    }
    assert_eq!(s.boards.undo_len(), undo);
    // Exactly the minimum is fine.
    run(&mut s, "storyboard.resize", json!({"clip": 2, "seconds": MIN_CLIP_SECS}));
    assert_eq!(secs(&s, 2), MIN_CLIP_SECS);
    // No board at all.
    let mut s = Session::new(
        Arc::new(FakeFs::new().known(KnownFolder::Home, "/home/paul")),
        Arc::new(RecordingLauncher::default()),
        Prefs::default(),
    );
    assert!(s.enabled("storyboard.resize").is_err());
    assert!(matches!(s.execute("storyboard.resize", &json!({"clip": 1, "seconds": 3})), Err(CmdError::Unavailable(_))));
}
