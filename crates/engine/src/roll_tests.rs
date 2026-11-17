//! `storyboard.roll` (cut-editing.md C1): move a cut through FilmCraft's roll.
//! Paul, 2026-11-14: "I want to provide the user with the ability to adjust the video cuts…"

use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::{ClipId, ShotType, Ticks};
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

/// `storyboard.split` (cut-editing.md C3): FilmCraft's razor on the Cut.
#[test]
fn a_split_makes_two_clips_of_the_same_shot_and_keeps_the_cut_length() {
    let mut s = session();
    let board = s.storyboard().unwrap().clone();
    // The first clip that isn't a title card, and where it starts.
    let (i, shot) = board.clips.iter().enumerate().find(|(_, c)| c.shot_type != ShotType::Ttl).unwrap();
    let start: f64 = board.clips[..i].iter().map(|c| c.duration.as_secs_f64()).sum();
    let len = shot.duration.as_secs_f64();
    let (t0, undo, count) = (total(&s), s.boards.undo_len(), board.clips.len());
    let r = run(&mut s, "storyboard.split", json!({ "at": start + 1.0, "clip": shot.id.0 }));
    let new = r["new"].as_u64().unwrap() as u32;
    let after = s.storyboard().unwrap();
    assert_eq!(after.clips.len(), count + 1);
    let (a, b) = (&after.clips[i], &after.clips[i + 1]);
    assert_eq!((a.id, b.id), (shot.id, ClipId(new)), "the new half sits right after the first");
    assert!((a.duration.as_secs_f64() - 1.0).abs() < 1e-9 && (b.duration.as_secs_f64() - (len - 1.0)).abs() < 1e-9);
    assert_eq!(
        (b.shot_type, &b.asset_ids, &b.description, b.scene_id),
        (shot.shot_type, &shot.asset_ids, &shot.description, shot.scene_id)
    );
    assert_eq!(b.transition_out, shot.transition_out, "the out-transition goes with the second half");
    assert_eq!(a.transition_out, assetcraft_storyboard::Transition::Cut);
    assert!(b.notes.is_empty() && b.overlay.is_none(), "notes and overlay text stay on the first half");
    assert!((total(&s) - t0).abs() < 1e-9, "the Cut is as long as before");
    assert_eq!(s.boards.undo_len(), undo + 1);
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(s.storyboard().unwrap().clips, board.clips, "undo puts it back");
}

#[test]
fn a_split_needs_room_for_both_halves_and_never_cuts_a_title_card() {
    let mut s = session();
    let board = s.storyboard().unwrap().clone();
    let mut starts = Vec::new();
    let mut t = 0.0;
    for c in &board.clips {
        starts.push(t);
        t += c.duration.as_secs_f64();
    }
    let card = board.clips.iter().position(|c| c.shot_type == ShotType::Ttl);
    let shot = board.clips.iter().position(|c| c.shot_type != ShotType::Ttl).unwrap();
    let other = board.clips.iter().position(|c| c.id != board.clips[shot].id).unwrap();
    let mut cases = vec![
        (json!({ "at": starts[shot] + 0.2 }), "a 0.2 s first half"),
        (json!({ "at": starts[1] }), "exactly on a cut"),
        (json!({ "at": t + 5.0 }), "past the end"),
        (json!({ "at": 0.0 }), "at the start"),
        (json!({ "at": "soon" }), "not a number"),
        (json!({}), "no time"),
        (json!({ "at": starts[shot] + 1.0, "clip": board.clips[other].id.0 }), "the wrong clip"),
    ];
    if let Some(c) = card {
        cases.push((json!({ "at": starts[c] + board.clips[c].duration.as_secs_f64() / 2.0 }), "a title card"));
    }
    for (bad, why) in cases {
        assert!(matches!(s.execute("storyboard.split", &bad), Err(CmdError::BadParams(_))), "{why}: {bad}");
    }
    assert_eq!(s.storyboard().unwrap().clips, board.clips, "nothing changed");
}

/// `storyboard.snapping` (C5): on by default, toggles, takes `on`, and is remembered.
#[test]
fn snapping_toggles_and_is_remembered() {
    let mut s = session();
    assert!(s.snapping_on() && s.prefs().snapping, "on by default");
    let r = run(&mut s, "storyboard.snapping", json!({}));
    assert_eq!(r["on"].as_bool(), Some(false), "no `on` toggles: {r}");
    assert!(!s.prefs().snapping, "saved with the prefs");
    run(&mut s, "storyboard.snapping", json!({ "on": true }));
    run(&mut s, "storyboard.snapping", json!({ "on": true }));
    assert!(s.snapping_on(), "`on: true` sets, it doesn't toggle");
    assert!(matches!(s.execute("storyboard.snapping", &json!({ "on": "yes" })), Err(CmdError::BadParams(_))));
    // A session restored from prefs with it off starts off.
    let mut prefs = s.prefs();
    prefs.snapping = false;
    let fs = FakeFs::new().known(KnownFolder::Home, "/home/paul");
    let s2 = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), prefs);
    assert!(!s2.snapping_on());
    let (from_disk, _) = Prefs::parse(r#"{"snapping": false}"#);
    assert!(!from_disk.snapping, "read from prefs.json");
    assert!(Prefs::parse("{}").0.snapping, "and on when the file doesn't say");
}

/// `storyboard.trackLevel` (V1): the song's level, one undo step, saved only when not 0 dB.
#[test]
fn the_song_level_is_one_undo_step_and_saved_with_the_board() {
    let mut s = session();
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.trackLevel", json!({ "db": -6 }));
    assert_eq!(r["db"], -6, "{r}");
    assert_eq!(s.storyboard().unwrap().track.as_ref().unwrap().level_db, -6);
    assert_eq!(s.boards.undo_len(), undo + 1);
    run(&mut s, "storyboard.trackLevel", json!({ "db": -6 }));
    assert_eq!(s.boards.undo_len(), undo + 1, "the same level again is no step");
    for bad in [json!({}), json!({ "db": 7 }), json!({ "db": -31 }), json!({ "db": 1.5 }), json!({ "db": "loud" })] {
        assert!(matches!(s.execute("storyboard.trackLevel", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    // In the file: present when set, left out at 0 dB (old files load unchanged), checked on load.
    let board = s.storyboard().unwrap().clone();
    let text = serde_json::to_string(&board).unwrap();
    assert!(text.contains("\"levelDb\":-6"), "saved");
    let back = assetcraft_storyboard::Storyboard::from_json(text.as_bytes()).unwrap();
    assert_eq!(back.track.unwrap().level_db, -6);
    let loud = text.replace("\"levelDb\":-6", "\"levelDb\":40");
    assert!(assetcraft_storyboard::Storyboard::from_json(loud.as_bytes()).is_err());
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(s.storyboard().unwrap().track.as_ref().unwrap().level_db, 0);
    assert!(!serde_json::to_string(s.storyboard().unwrap()).unwrap().contains("levelDb"), "0 dB is left out");
    // No song, no level.
    s.boards.board.as_mut().unwrap().track = None;
    assert!(matches!(s.execute("storyboard.trackLevel", &json!({ "db": -3 })), Err(CmdError::Unavailable(_))));
}

/// `storyboard.titleLevel` (B-R3): the dip under title cards, one undo step, 0 = none.
#[test]
fn the_title_level_dips_the_music_under_title_cards() {
    let mut s = session();
    let default = assetcraft_storyboard::TITLE_DB_DEFAULT;
    assert_eq!(s.storyboard().unwrap().track.as_ref().unwrap().title_db, default);
    let windows = s.storyboard().unwrap().title_windows();
    assert!(!windows.is_empty(), "the sample has title cards");
    let dips = crate::export_job::title_dips(s.storyboard().unwrap());
    assert_eq!(dips.len(), windows.len(), "a dip per stretch of title cards");
    assert!((dips[0].gain - assetcraft_audio::mix::db(f64::from(default))).abs() < 1e-6);
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.titleLevel", json!({ "db": 0 }));
    assert_eq!(r["db"], 0, "{r}");
    assert!(crate::export_job::title_dips(s.storyboard().unwrap()).is_empty(), "0 dB: no dip");
    assert_eq!(s.boards.undo_len(), undo + 1);
    run(&mut s, "storyboard.titleLevel", json!({ "db": 0 }));
    assert_eq!(s.boards.undo_len(), undo + 1, "the same level is no step");
    for bad in [json!({}), json!({ "db": 1 }), json!({ "db": -31 }), json!({ "db": "soft" })] {
        assert!(matches!(s.execute("storyboard.titleLevel", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(s.storyboard().unwrap().track.as_ref().unwrap().title_db, default);
}

/// `storyboard.zoom` (Z1): set or multiply, kept within the range; a view setting, no undo step.
#[test]
fn the_strip_zoom_is_set_or_multiplied_within_its_range() {
    let mut s = session();
    let undo = s.boards.undo_len();
    assert_eq!(s.strip_zoom(), 1.0);
    run(&mut s, "storyboard.zoom", json!({ "by": 2 }));
    run(&mut s, "storyboard.zoom", json!({ "by": 2 }));
    assert_eq!(s.strip_zoom(), 4.0);
    assert_eq!(run(&mut s, "storyboard.zoom", json!({ "zoom": 0.01 }))["zoom"], 0.25, "clamped to a quarter");
    assert_eq!(run(&mut s, "storyboard.zoom", json!({ "zoom": 99 }))["zoom"], 16.0, "and to sixteen");
    for bad in [json!({}), json!({ "zoom": 2, "by": 2 }), json!({ "zoom": -1 }), json!({ "by": "in" })] {
        assert!(matches!(s.execute("storyboard.zoom", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert_eq!(s.boards.undo_len(), undo, "no undo step");
}

/// `storyboard.volume` (V3): the whole line in one undo step; points sorted, a repeated time
/// replaced, levels clamped; with points the line sets the level, `[]` returns to the song level.
#[test]
fn the_volume_line_is_set_whole_and_overrides_the_song_level() {
    let mut s = session();
    run(&mut s, "storyboard.trackLevel", json!({ "db": -6 }));
    assert!(crate::export_job::song_envelope(s.storyboard().unwrap()).is_none());
    assert!((crate::export_job::song_gain(s.storyboard().unwrap()) - assetcraft_audio::mix::db(-6.0)).abs() < 1e-6);
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.volume", json!({ "points": [[4.0, -3], [1.0, 0], [4.0, -9], [2.0, -99]] }));
    assert_eq!(r["points"], json!([[1.0, 0.0], [2.0, -60.0], [4.0, -9.0]]), "sorted, 4 s replaced, -99 clamped");
    assert_eq!(s.boards.undo_len(), undo + 1);
    let b = s.storyboard().unwrap();
    assert_eq!(crate::export_job::song_gain(b), 1.0, "the line sets the level now");
    let e = crate::export_job::song_envelope(b).unwrap();
    assert_eq!(e.db_at(3.0), -34.5, "halfway from -60 to -9");
    // Saved, and checked on load.
    let text = serde_json::to_string(b).unwrap();
    assert!(text.contains("\"volume\""));
    assert!(assetcraft_storyboard::Storyboard::from_json(text.as_bytes()).is_ok());
    let loud = text.replace("\"db\":-9.0", "\"db\":40.0");
    assert!(assetcraft_storyboard::Storyboard::from_json(loud.as_bytes()).is_err(), "+40 dB is refused on load");
    run(&mut s, "storyboard.volume", json!({ "points": [] }));
    assert!(crate::export_job::song_envelope(s.storyboard().unwrap()).is_none(), "cleared: back to the song level");
    for bad in [json!({}), json!({ "points": [[1.0]] }), json!({ "points": [[-1, 0]] }), json!({ "points": "flat" })] {
        assert!(matches!(s.execute("storyboard.volume", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(s.storyboard().unwrap().track.as_ref().unwrap().volume.len(), 3, "undo brings the line back");
}

/// `storyboard.fade` (V4): a section's fade in and out, each held to what the section has left,
/// played and exported as quarter-sine fades at the section's ends; saved, checked on load, one
/// undo step.
#[test]
fn a_section_fades_in_and_out() {
    let mut s = session();
    // The sample's song has no sections: give it two.
    let secs = |x: f64| Ticks::from_secs_f64(x).unwrap();
    let section = |name: &str, a: f64, b: f64| assetcraft_storyboard::SongSection {
        name: name.into(),
        start: secs(a),
        end: secs(b),
        fade_in: Ticks::ZERO,
        fade_out: Ticks::ZERO,
    };
    let track = s.boards.board.as_mut().and_then(|b| b.track.as_mut()).unwrap();
    assert!(track.duration.as_secs_f64() > 12.0);
    track.song_sections = vec![section("Intro", 0.0, 4.0), section("Chorus", 4.0, 12.0)];
    let sec = s.storyboard().unwrap().track.as_ref().unwrap().song_sections[1].clone();
    let (a, b) = (sec.start.as_secs_f64(), sec.end.as_secs_f64());
    let len = b - a;
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.fade", json!({ "section": 1, "in": 1.0 }));
    assert_eq!((r["in"].as_f64(), r["out"].as_f64()), (Some(1.0), Some(0.0)), "{r}");
    assert_eq!(s.boards.undo_len(), undo + 1);
    // Out leaves in alone; asking for more than is left is held.
    let r = run(&mut s, "storyboard.fade", json!({ "section": 1, "out": len * 10.0 }));
    assert_eq!(r["in"].as_f64(), Some(1.0));
    assert!((r["out"].as_f64().unwrap() - (len - 1.0)).abs() < 1e-6, "{r}");
    run(&mut s, "storyboard.fade", json!({ "section": 1, "out": 0.5 }));
    let fades = crate::export_job::section_fades(s.storyboard().unwrap());
    assert_eq!(fades.len(), 2);
    assert_eq!((fades[0].start, fades[0].end, fades[0].rising), (a, a + 1.0, true));
    assert!((fades[1].start - (b - 0.5)).abs() < 1e-9 && fades[1].end == b && !fades[1].rising);
    // The same again: no undo step.
    let undo = s.boards.undo_len();
    run(&mut s, "storyboard.fade", json!({ "section": 1, "in": 1.0, "out": 0.5 }));
    assert_eq!(s.boards.undo_len(), undo);
    // Saved, and checked on load.
    let text = serde_json::to_string(s.storyboard().unwrap()).unwrap();
    assert!(text.contains("\"fadeIn\"") && text.contains("\"fadeOut\""));
    assert!(assetcraft_storyboard::Storyboard::from_json(text.as_bytes()).is_ok());
    let mut long: Value = serde_json::from_str(&text).unwrap();
    long["track"]["songSections"][1]["fadeIn"] = json!((len as u64 + 1) * assetcraft_storyboard::TICKS_PER_SECOND);
    let long = serde_json::to_string(&long).unwrap();
    assert!(
        assetcraft_storyboard::Storyboard::from_json(long.as_bytes()).is_err(),
        "a fade longer than its section is refused on load"
    );
    for bad in [
        json!({ "section": 1 }),
        json!({ "section": 99, "in": 1 }),
        json!({ "section": -1, "in": 1 }),
        json!({ "section": 1, "in": -1 }),
        json!({ "section": 1, "out": "slow" }),
    ] {
        assert!(matches!(s.execute("storyboard.fade", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    run(&mut s, "storyboard.fade", json!({ "section": 1, "in": 0, "out": 0 }));
    assert!(crate::export_job::section_fades(s.storyboard().unwrap()).is_empty());
    assert!(!serde_json::to_string(s.storyboard().unwrap()).unwrap().contains("fadeIn"), "no fades: left out");
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(crate::export_job::section_fades(s.storyboard().unwrap()).len(), 2, "undo brings them back");
}
