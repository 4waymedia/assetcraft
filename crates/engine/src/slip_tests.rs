//! `storyboard.slip` (cut-editing.md C6a): change where in its video a clip starts, through
//! FilmCraft's slip. Paul, 2026-11-16: "is there a way to change the in point for the video
//! clip?"

use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::{Clip, ClipId, ShotType, Storyboard};
use serde_json::json;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

/// A 2-second video from the test library.
fn video() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_data/library/environments/snow_clearing_flythrough.mp4")
        .to_string_lossy()
        .into_owned()
}

/// The sample board with the video in its first shot, `secs` long. Returns the clip's id.
fn session(secs: f64) -> (Session, u32) {
    let fs = FakeFs::new().known(KnownFolder::Home, "/home/paul");
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut s, "storyboard.new", json!({ "sample": true }));
    let board = s.storyboard().unwrap();
    let id = board.clips.iter().find(|c| c.shot_type.accepts_drop() && c.shot_type != ShotType::Ttl).unwrap().id.0;
    run(&mut s, "storyboard.place", json!({ "asset": video(), "clip": id }));
    run(&mut s, "storyboard.resize", json!({ "clip": id, "seconds": secs }));
    (s, id)
}

fn clip(s: &Session, id: u32) -> Clip {
    s.storyboard().unwrap().clip(ClipId(id)).unwrap().clone()
}

fn in_secs(s: &Session, id: u32) -> f64 {
    clip(s, id).source_in.as_secs_f64()
}

#[test]
fn a_slip_moves_the_in_point_and_nothing_else() {
    let (mut s, id) = session(0.5);
    let (before, undo) = (s.storyboard().unwrap().clone(), s.boards.undo_len());
    let r = run(&mut s, "storyboard.slip", json!({ "clip": id, "seconds": 0.5 }));
    assert_eq!(r["changed"], true);
    assert!((r["in"].as_f64().unwrap() - 0.5).abs() < 1e-6, "{r}");
    assert!((r["video"].as_f64().unwrap() - 2.0).abs() < 0.05, "{r}");
    assert!((in_secs(&s, id) - 0.5).abs() < 1e-6);
    assert_eq!(s.boards.undo_len(), undo + 1, "one undo step");
    // Every clip keeps its place and length.
    let after = s.storyboard().unwrap();
    let lens = |b: &Storyboard| b.clips.iter().map(|c| (c.id, c.duration)).collect::<Vec<_>>();
    assert_eq!(lens(&before), lens(after));
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(in_secs(&s, id), 0.0, "undo puts it back");
}

#[test]
fn a_slip_stops_at_the_video_s_start_and_where_it_would_run_out() {
    let (mut s, id) = session(0.5);
    // 2 s of video, 0.5 s of clip: the latest in-point is 1.5 s.
    let r = run(&mut s, "storyboard.slip", json!({ "clip": id, "seconds": 10.0 }));
    assert_eq!(r["limited"], true);
    assert!((in_secs(&s, id) - 1.5).abs() < 0.05, "{r}");
    let r = run(&mut s, "storyboard.slip", json!({ "clip": id, "to": 0.25 }));
    assert!((in_secs(&s, id) - 0.25).abs() < 1e-6, "{r}");
    run(&mut s, "storyboard.slip", json!({ "clip": id, "seconds": -5.0 }));
    assert_eq!(in_secs(&s, id), 0.0);
    // Already at the start: nothing to do, and no undo step.
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.slip", json!({ "clip": id, "seconds": -1.0 }));
    assert_eq!(r["changed"], false);
    assert_eq!(s.boards.undo_len(), undo);
}

#[test]
fn a_clip_as_long_as_its_video_can_t_slip() {
    let (mut s, id) = session(3.0);
    let r = run(&mut s, "storyboard.slip", json!({ "clip": id, "seconds": 0.5 }));
    assert_eq!(r["changed"], false, "{r}");
    assert_eq!(in_secs(&s, id), 0.0);
}

#[test]
fn only_a_video_clip_slips_and_params_are_checked() {
    let (mut s, id) = session(0.5);
    let still = s.storyboard().unwrap().clips.iter().find(|c| c.id.0 != id).unwrap().id.0;
    assert!(matches!(
        s.execute("storyboard.slip", &json!({ "clip": still, "seconds": 0.5 })),
        Err(CmdError::BadParams(_))
    ));
    for bad in [
        json!({ "clip": id }),
        json!({ "clip": id, "seconds": 0.5, "to": 1.0 }),
        json!({ "clip": id, "to": -1.0 }),
        json!({ "clip": id, "seconds": "far" }),
        json!({ "clip": 9999, "seconds": 0.5 }),
    ] {
        assert!(matches!(s.execute("storyboard.slip", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
}

#[test]
fn a_new_hero_starts_at_its_video_s_start() {
    let (mut s, id) = session(0.5);
    run(&mut s, "storyboard.slip", json!({ "clip": id, "seconds": 1.0 }));
    // The same video again keeps the in-point; a different one starts over.
    run(&mut s, "storyboard.place", json!({ "asset": video(), "clip": id }));
    assert!((in_secs(&s, id) - 1.0).abs() < 1e-6);
    run(&mut s, "storyboard.place", json!({ "asset": "/art/a.png", "clip": id }));
    assert_eq!(in_secs(&s, id), 0.0);
}

#[test]
fn the_second_half_of_a_split_carries_on_in_the_video() {
    let (mut s, id) = session(1.0);
    run(&mut s, "storyboard.slip", json!({ "clip": id, "seconds": 0.5 }));
    let board = s.storyboard().unwrap();
    let index = board.index_of(ClipId(id)).unwrap();
    let start: f64 = board.clips[..index].iter().map(|c| c.duration.as_secs_f64()).sum();
    run(&mut s, "storyboard.split", json!({ "at": start + 0.5, "clip": id }));
    let board = s.storyboard().unwrap();
    let (a, b) = (&board.clips[index], &board.clips[index + 1]);
    assert_eq!(a.id.0, id);
    assert!((a.source_in.as_secs_f64() - 0.5).abs() < 1e-6);
    assert!((b.source_in.as_secs_f64() - (0.5 + a.duration.as_secs_f64())).abs() < 1e-6, "{:?}", b.source_in);
}

#[test]
fn a_zero_in_point_stays_out_of_the_file() {
    let (mut s, id) = session(0.5);
    let file = |s: &Session| serde_json::to_value(clip(s, id)).unwrap();
    assert!(file(&s).get("sourceIn").is_none());
    run(&mut s, "storyboard.slip", json!({ "clip": id, "seconds": 0.5 }));
    assert!(file(&s).get("sourceIn").is_some());
}
