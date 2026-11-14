//! `storyboard.roll` (cut-editing.md C1): move a cut through FilmCraft's roll.
//! Paul, 2026-11-14: "I want to provide the user with the ability to adjust the video cuts…"

use super::*;
use assetcraft_fs::FakeFs;
use serde_json::json;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

/// The clips' lengths, in seconds, in Cut order.
fn lengths(s: &Session) -> Vec<f64> {
    s.storyboard().unwrap().clips.iter().map(|c| c.duration.as_secs_f64()).collect()
}

fn session() -> Session {
    let fs = FakeFs::new().known(KnownFolder::Home, "/home/paul");
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut s, "storyboard.new", json!({ "sample": true }));
    s
}

fn total(s: &Session) -> f64 {
    s.storyboard().unwrap().total().as_secs_f64()
}

#[test]
fn moving_a_cut_keeps_the_cut_length_and_everything_after_it() {
    let mut s = session();
    let before = lengths(&s);
    let (t0, undo) = (total(&s), s.boards.undo_len());
    let first = s.storyboard().unwrap().clips[0].id.0;
    let r = run(&mut s, "storyboard.roll", json!({ "left": first, "seconds": 0.5 }));
    assert_eq!((r["changed"].as_bool(), r["limited"].as_bool()), (Some(true), Some(false)), "{r}");
    assert!((r["moved"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    let after = lengths(&s);
    assert!((after[0] - before[0] - 0.5).abs() < 1e-9, "the first clip grew");
    assert!((before[1] - after[1] - 0.5).abs() < 1e-9, "the second gave it up");
    assert_eq!(&after[2..], &before[2..], "nothing after the cut moved");
    assert!((total(&s) - t0).abs() < 1e-9, "the Cut's length (and the music under it) is unchanged");
    assert_eq!(s.boards.undo_len(), undo + 1, "one undo step");
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(lengths(&s), before);
}

#[test]
fn a_cut_moves_earlier_too_and_to_a_time() {
    let mut s = session();
    let before = lengths(&s);
    let first = s.storyboard().unwrap().clips[0].id.0;
    // Earlier: FilmCraft limits a roll by the media before the next clip's in-point, so the
    // adapter's headroom has to make this possible.
    let r = run(&mut s, "storyboard.roll", json!({ "left": first, "seconds": -0.25 }));
    assert!((r["moved"].as_f64().unwrap() + 0.25).abs() < 1e-9, "{r}");
    assert!((lengths(&s)[1] - before[1] - 0.25).abs() < 1e-9);
    // To an absolute time in the Cut.
    let want = before[0] + 0.75;
    let r = run(&mut s, "storyboard.roll", json!({ "left": first, "to": want }));
    assert!((r["cut"].as_f64().unwrap() - want).abs() < 1e-9, "{r}");
    assert!((lengths(&s)[0] - want).abs() < 1e-9);
}

#[test]
fn a_cut_stops_where_a_clip_would_go_under_half_a_second() {
    let mut s = session();
    let before = lengths(&s);
    let first = s.storyboard().unwrap().clips[0].id.0;
    let r = run(&mut s, "storyboard.roll", json!({ "left": first, "seconds": 3600.0 }));
    assert_eq!(r["limited"].as_bool(), Some(true), "{r}");
    let after = lengths(&s);
    assert!((after[1] - 0.5).abs() < 1e-9, "the second clip keeps half a second: {after:?}");
    assert!((after[0] - (before[0] + before[1] - 0.5)).abs() < 1e-9);
    // Already at the limit: nothing moves and no undo step.
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.roll", json!({ "left": first, "seconds": 1.0 }));
    assert_eq!(r["changed"].as_bool(), Some(false));
    assert_eq!(s.boards.undo_len(), undo);
}

#[test]
fn a_cut_moves_by_beats_of_the_track_and_bad_requests_are_refused() {
    let mut s = session();
    let first = s.storyboard().unwrap().clips[0].id.0;
    let last = s.storyboard().unwrap().clips.last().unwrap().id.0;
    let beat = s.storyboard().unwrap().track.as_ref().expect("the sample has a track").bpm.beat().as_secs_f64();
    let before = lengths(&s);
    let r = run(&mut s, "storyboard.roll", json!({ "left": first, "beats": -1 }));
    assert!((r["moved"].as_f64().unwrap() + beat).abs() < 1e-6, "one beat earlier: {r}");
    assert!((lengths(&s)[0] - (before[0] - beat)).abs() < 1e-6);
    for (bad, why) in [
        (json!({ "left": last, "seconds": 1.0 }), "no cut after the last clip"),
        (json!({ "left": 9999, "seconds": 1.0 }), "no such clip"),
        (json!({ "left": first }), "no amount"),
        (json!({ "left": first, "seconds": 1.0, "beats": 2 }), "two amounts"),
        (json!({ "left": first, "to": -1.0 }), "a negative time"),
        (json!({ "left": first, "seconds": "far" }), "not a number"),
        (json!({ "left": first, "beats": 1.5 }), "half a beat"),
    ] {
        assert!(matches!(s.execute("storyboard.roll", &bad), Err(CmdError::BadParams(_))), "{why}: {bad}");
    }
}
