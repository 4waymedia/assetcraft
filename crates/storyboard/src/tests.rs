use crate::*;

const BPM: f64 = 92.0;

fn bpm(v: f64) -> Bpm {
    Bpm::new(v).unwrap()
}

/// The Quick fill sample: 24 clips, 5 title cards (one per scene), 19 to fill.
fn sample() -> Storyboard {
    let b = bpm(BPM);
    let mut board = Storyboard::new("Hollow Saint teaser");
    board.format = Some("Movie trailer".into());
    board.preset = Some("Prebuilt trailer".into());
    let pattern = [ShotType::Est, ShotType::Ws, ShotType::Ms, ShotType::Cu];
    let mut id = 1;
    for s in 0..5u32 {
        board.scenes.push(Scene {
            id: SceneId(s + 1),
            song_section: Some(["Intro", "Verse", "Pre-chorus", "Chorus", "Outro"][s as usize].into()),
        });
        let mut t = Clip::new(ClipId(id), SceneId(s + 1), ShotType::Ttl, b.beats_to_ticks(4));
        t.description = format!("Title {}", s + 1);
        board.clips.push(t);
        id += 1;
        let shots = if s == 4 { 3 } else { 4 };
        for k in 0..shots {
            board.clips.push(Clip::new(
                ClipId(id),
                SceneId(s + 1),
                pattern[k % 4],
                b.beats_to_ticks(2 + (k as u32 % 3) * 2),
            ));
            id += 1;
        }
    }
    board.track = Some(Track {
        id: AssetId("F:/music/hollow.wav".into()),
        name: "Hollow".into(),
        bpm: b,
        duration: Ticks::from_secs(60).unwrap(),
        beat_grid: vec![],
        song_sections: vec![],
        edit: None,
    });
    board
}

#[test]
fn ticks_and_timecode() {
    assert_eq!(Ticks::from_secs(2).unwrap().0, 2 * TICKS_PER_SECOND);
    assert_eq!(Ticks::from_secs_f64(2.6).unwrap().timecode(), "0:02.6");
    assert_eq!(Ticks::from_secs(75).unwrap().timecode(), "1:15.0");
    assert!(Ticks::from_secs_f64(-1.0).is_none());
    assert!(Ticks::from_secs_f64(f64::NAN).is_none());
    assert!(Ticks::from_secs_f64(f64::INFINITY).is_none());
    assert!(Ticks::from_secs_f64(1e30).is_none());
    assert!(Ticks::from_secs(u64::MAX).is_none());
}

#[test]
fn bpm_bounds_and_beats() {
    assert!(Bpm::new(19.9).is_none() && Bpm::new(400.1).is_none() && Bpm::new(f64::NAN).is_none());
    let b = bpm(120.0);
    assert_eq!(b.beat(), Ticks(TICKS_PER_SECOND / 2));
    assert_eq!(b.ticks_to_beats(Ticks::from_secs_f64(2.6).unwrap()), 5);
    assert_eq!(b.ticks_to_beats(Ticks::from_secs_f64(1.24).unwrap()), 2);
    assert_eq!(b.ticks_to_beats(b.beats_to_ticks(7)), 7);
    // The design's example: 2.6 s at 92 BPM is 4 beats.
    assert_eq!(bpm(92.0).ticks_to_beats(Ticks::from_secs_f64(2.6).unwrap()), 4);
}

#[test]
fn shot_types_follow_the_design() {
    assert_eq!(ShotType::Ttl.fill_rgb(), Some(0xe5243b));
    assert_eq!(ShotType::Ins.fill_rgb(), None);
    assert!(!ShotType::Ttl.accepts_assets());
    assert_eq!(serde_json::to_string(&ShotType::Cu).unwrap(), "\"CU\"");
    assert_eq!(serde_json::to_string(&Transition::SmashCut).unwrap(), "\"Smash cut\"");
}

#[test]
fn sample_shape_and_queries() {
    let board = sample();
    assert_eq!(board.clips.len(), 24);
    assert_eq!(board.fill_progress(), (0, 19));
    assert_eq!(board.scene_title(SceneId(3)), Some("Title 3"));
    let starts = board.starts();
    assert_eq!(starts[0], Ticks::ZERO);
    assert_eq!(board.clip_at(starts[5]).map(|c| c.id), Some(board.clips[5].id));
    assert!(board.clip_at(board.total()).is_none());
    assert_eq!(board.next_clip_id(), Some(ClipId(25)));
}

#[test]
fn quick_fill_places_in_order_and_skips_title_cards() {
    let mut board = sample();
    assert_eq!(next_empty(&board), Some(ClipId(2)));
    for n in 0..19 {
        let got = place(&mut board, AssetId(format!("a{n}.png")), None).unwrap();
        assert_ne!(board.clip(got).unwrap().shot_type, ShotType::Ttl);
    }
    assert_eq!(board.fill_progress(), (19, 19));
    assert_eq!(place(&mut board, AssetId("x".into()), None), Err(FillError::AllFilled));
    // Targeting replaces.
    assert_eq!(place(&mut board, AssetId("new.png".into()), Some(ClipId(3))), Ok(ClipId(3)));
    assert_eq!(board.clip(ClipId(3)).unwrap().asset_ids, vec![AssetId("new.png".into())]);
    // Targeting a title card sets its background (B5); Quick fill progress doesn't count it.
    assert_eq!(place(&mut board, AssetId("bg.mp4".into()), Some(ClipId(1))), Ok(ClipId(1)));
    assert_eq!(board.clip(ClipId(1)).unwrap().background(), Some(&AssetId("bg.mp4".into())));
    assert_eq!(board.fill_progress(), (19, 19));
    assert_eq!(place(&mut board, AssetId("x".into()), Some(ClipId(99))), Err(FillError::NoSuchClip(ClipId(99))));
    assert!(Storyboard::from_json(board.to_json().unwrap().as_bytes()).is_ok());
}

#[test]
fn a_title_card_holds_at_most_one_background() {
    let mut board = sample();
    board.clips[0].asset_ids = vec![AssetId("a".into()), AssetId("b".into())];
    let err = Storyboard::from_json(board.to_json().unwrap().as_bytes()).unwrap_err();
    assert!(err.to_string().contains("more than one background"), "{err}");
    assert_eq!(board.clips[1].background(), None, "shots have no background");
}

#[test]
fn delete_takes_shots_first_then_clips() {
    let mut board = sample();
    let total = board.total();
    let (n_clips, n_scenes) = (board.clips.len(), board.scenes.len());
    place(&mut board, AssetId("a".into()), Some(ClipId(2))).unwrap();
    place(&mut board, AssetId("bg".into()), Some(ClipId(1))).unwrap();
    // Clips 1–3 selected, two have assets: the first press empties them and keeps the timing.
    let range = [ClipId(1), ClipId(2), ClipId(3)];
    assert_eq!(delete(&mut board, &range), Ok(Deleted::Shots(2)));
    assert_eq!(board.assets_assigned(), 0);
    assert_eq!((board.clips.len(), board.total()), (n_clips, total));
    // The second press, with nothing left in them, removes the clips.
    assert_eq!(delete(&mut board, &range), Ok(Deleted::Clips(3)));
    assert_eq!(board.clips.len(), n_clips - 3);
    assert!(board.total() < total);
    assert!(board.clip(ClipId(2)).is_none());
    assert!(Storyboard::from_json(board.to_json().unwrap().as_bytes()).is_ok());
    // Removing a whole scene drops the scene too.
    let first_scene = board.clips[0].scene_id;
    let all: Vec<ClipId> = board.clips.iter().filter(|c| c.scene_id == first_scene).map(|c| c.id).collect();
    assert_eq!(delete(&mut board, &all), Ok(Deleted::Clips(all.len())));
    assert_eq!(board.scenes.len(), n_scenes - 1);
    assert!(board.scenes.iter().all(|s| s.id != first_scene));
    // Unknown ids change nothing.
    let before = board.clone();
    assert_eq!(delete(&mut board, &[ClipId(999)]), Err(FillError::NoSuchClip(ClipId(999))));
    assert_eq!(board, before);
}

#[test]
fn recut_fills_the_track_in_whole_beats() {
    let board = sample();
    for (tempo, secs) in [(92.0, 60), (128.0, 30), (70.0, 95), (140.0, 61)] {
        let b = bpm(tempo);
        let len = Ticks::from_secs(secs).unwrap();
        let r = recut(&board, b, len, RecutOptions::default());
        assert!(r.fits(), "{tempo} BPM {secs}s: {} of {}", r.total_beats, r.available_beats);
        assert_eq!(r.available_beats, len.0 / b.beat().0);
        assert!(r.beats.iter().all(|&n| n >= 1));
        // Title cards keep 4 beats' worth (re-expressed at the new tempo).
        let ttl_secs = bpm(BPM).beats_to_ticks(4);
        for (c, n) in board.clips.iter().zip(&r.beats) {
            if c.shot_type == ShotType::Ttl {
                assert_eq!(*n, b.ticks_to_beats(ttl_secs).max(1));
            }
        }
        let sum: u64 = r.durations.iter().map(|d| d.0).sum();
        assert!(sum <= len.0);
    }
}

#[test]
fn recut_keeps_proportions() {
    let board = sample();
    let r = recut(&board, bpm(BPM), Ticks::from_secs(120).unwrap(), RecutOptions { keep_title_cards: false });
    assert!(r.fits());
    // A 6-beat clip stays longer than a 2-beat clip.
    let (two, six) = (1, 3);
    assert_eq!(board.clips[two].duration, bpm(BPM).beats_to_ticks(2));
    assert_eq!(board.clips[six].duration, bpm(BPM).beats_to_ticks(6));
    assert!(r.beats[six] > r.beats[two]);
}

#[test]
fn recut_reports_when_clips_cannot_fit() {
    let board = sample();
    // 5 seconds at 60 BPM is 5 beats for 24 clips.
    let r = recut(&board, bpm(60.0), Ticks::from_secs(5).unwrap(), RecutOptions::default());
    assert!(!r.fits());
    assert!(r.beats.iter().all(|&n| n >= 1));
    // Empty board and zero-length track don't panic.
    let empty = Storyboard::new("e");
    let r = recut(&empty, bpm(120.0), Ticks::ZERO, RecutOptions::default());
    assert!(r.fits() && r.beats.is_empty());
}

#[test]
fn json_round_trip() {
    let mut board = sample();
    place(&mut board, AssetId("F:/art/reaper.png".into()), None).unwrap();
    board.text_clips.push(TextClip {
        start: Ticks::ZERO,
        duration: Ticks::from_secs(2).unwrap(),
        text: "This fall".into(),
        placement: "Center".into(),
    });
    let json = board.to_json().unwrap();
    assert!(json.contains("\"shotType\": \"EST\""));
    let back = Storyboard::from_json(json.as_bytes()).unwrap();
    assert_eq!(back, board);
}

#[test]
fn hostile_documents_are_refused() {
    let good = sample().to_json().unwrap();
    let edit = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v: serde_json::Value = serde_json::from_str(&good).unwrap();
        f(&mut v);
        Storyboard::from_json(v.to_string().as_bytes())
    };
    assert!(matches!(Storyboard::from_json(b"not json"), Err(BoardError::Parse(_))));
    assert!(matches!(Storyboard::from_json(b""), Err(BoardError::Parse(_))));
    assert!(matches!(edit(&|v| v["version"] = 99.into()), Err(BoardError::NewerVersion(99))));
    assert!(matches!(edit(&|v| v["version"] = 0.into()), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|v| v["clips"][1]["id"] = 1.into()), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|v| v["clips"][1]["sceneId"] = 77.into()), Err(BoardError::Invalid(_))));
    assert!(matches!(
        edit(&|v| v["clips"][0]["assetIds"] = serde_json::json!(["x", "y"])),
        Err(BoardError::Invalid(_))
    ));
    assert!(matches!(edit(&|v| v["clips"][1]["duration"] = u64::MAX.into()), Err(BoardError::Limit(_))));
    assert!(matches!(edit(&|v| v["clips"][1]["duration"] = (-1).into()), Err(BoardError::Parse(_))));
    assert!(matches!(edit(&|v| v["track"]["bpm"] = 0.into()), Err(BoardError::Parse(_))));
    assert!(matches!(edit(&|v| v["track"]["beatGrid"] = serde_json::json!([5, 3])), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|v| v["clips"][1]["shotType"] = "XL".into()), Err(BoardError::Parse(_))));
    assert!(matches!(edit(&|v| v["clips"][1]["surprise"] = 1.into()), Err(BoardError::Parse(_))));
    // A title card can't also carry an overlay; styles must be known ids.
    assert!(matches!(edit(&|v| v["clips"][0]["overlay"] = "x".into()), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|v| v["clips"][1]["overlay"] = "x".repeat(9000).into()), Err(BoardError::Limit(_))));
    assert!(matches!(
        edit(&|v| v["clips"][0]["textStyle"] = serde_json::json!({ "theme": "neon" })),
        Err(BoardError::Parse(_))
    ));
    assert!(matches!(edit(&|v| v["name"] = "x".repeat(9000).into()), Err(BoardError::Limit(_))));
    let tiny = Limits { max_bytes: 10, ..Limits::default() };
    assert!(matches!(Storyboard::from_json_with(good.as_bytes(), &tiny), Err(BoardError::TooLarge(_))));
    // Many clip durations that each pass but overflow the Cut.
    let big = Limits::default().max_duration.0;
    assert!(matches!(
        edit(&|v| {
            v["clips"][1]["duration"] = big.into();
            v["clips"][2]["duration"] = big.into();
        }),
        Err(BoardError::Limit(_))
    ));
}

#[test]
fn the_sample_board_is_valid_and_matches_the_handoff() {
    let board = crate::sample();
    assert_eq!(board.clips.len(), 24);
    assert_eq!(board.scenes.len(), 5);
    assert_eq!(board.fill_progress(), (0, 19));
    assert!(board.validate(&Limits::default()).is_ok());
    let back = Storyboard::from_json(board.to_json().unwrap().as_bytes()).unwrap();
    assert_eq!(back, board);
    assert_eq!(board.scene_title(SceneId(4)), Some("Hollow Saint"));
}

#[test]
fn auto_fill_prefers_unused_assets_then_repeats() {
    let mut board = sample();
    place(&mut board, AssetId("a".into()), None).unwrap(); // clip 2
    let assets: Vec<AssetId> = ["a", "b", "c"].iter().map(|s| AssetId((*s).into())).collect();
    let filled = auto_fill(&mut board, &assets);
    assert_eq!(filled.len(), 18);
    assert_eq!(board.fill_progress(), (19, 19));
    let order: Vec<&str> = filled.iter().map(|id| board.clip(*id).unwrap().asset_ids[0].0.as_str()).collect();
    assert_eq!(&order[..5], &["b", "c", "a", "b", "c"], "unused first, then the list repeats");
    assert!(board.clips.iter().filter(|c| c.shot_type == ShotType::Ttl).all(|c| c.asset_ids.is_empty()));
    assert!(auto_fill(&mut board, &assets).is_empty(), "nothing left to fill");
    assert!(auto_fill(&mut sample(), &[]).is_empty());
    assert_eq!(clear(&mut board), 19);
    assert_eq!(board.fill_progress(), (0, 19));
    assert_eq!(clear(&mut board), 0);
}

// ---- cue maps and generation -------------------------------------------------------------

mod cue {
    use crate::cuemap::{CueHit, CueSection, CueTrack, FadeOut, FitChoice};
    use crate::*;
    use std::collections::BTreeMap;

    pub const SR: u64 = 48_000;
    pub const BEAT: u64 = SR / 2; // 120 BPM
    pub const BAR: u64 = 4 * BEAT;

    /// 64 bars at 120 BPM (128 s): Intro 8, Verse 16, Chorus 16 (drop on its first bar), Verse 16, Outro 8.
    pub fn map() -> CueMap {
        let len = 64 * BAR;
        let sec = |a: u64, b: u64, label: &str, energy: f32| CueSection {
            start: a * BAR,
            end: b * BAR,
            label: label.into(),
            energy,
        };
        let mut fits = BTreeMap::new();
        fits.insert(
            "60".into(),
            FitChoice {
                chosen: Some(FitMode::Edit),
                fade: None,
                edit: Some(Fit {
                    mode: FitMode::Edit,
                    length_frames: 30 * BAR,
                    segments: vec![[0, 22 * BAR], [56 * BAR, 64 * BAR]],
                    crossfade_frames: BEAT,
                    fade_out: None,
                    similarity: Some(0.8),
                    reasons: vec![],
                    changes: "Edited to 1:00".into(),
                }),
            },
        );
        fits.insert(
            "30".into(),
            FitChoice {
                chosen: Some(FitMode::Fade),
                fade: Some(Fit {
                    mode: FitMode::Fade,
                    length_frames: 15 * BAR,
                    segments: vec![[0, 15 * BAR]],
                    crossfade_frames: 0,
                    fade_out: Some(FadeOut { start: 13 * BAR, frames: 2 * BAR }),
                    similarity: None,
                    reasons: vec![],
                    changes: "Cut to 0:30 with a fade out".into(),
                }),
                edit: None,
            },
        );
        CueMap {
            version: 1,
            analyzer: "test".into(),
            track: CueTrack {
                file: "song.wav".into(),
                title: "Song".into(),
                artist: "Me".into(),
                license: "own".into(),
                credit: String::new(),
            },
            sample_rate: SR as u32,
            duration_frames: len,
            bpm: 120.0,
            beats_per_bar: 4,
            pulse_clarity: 0.8,
            beats: (0..64 * 4).map(|i| i * BEAT).collect(),
            downbeats: (0..64).map(|i| i * BAR).collect(),
            sections: vec![
                sec(0, 8, "Intro", 0.1),
                sec(8, 24, "Verse", 0.5),
                sec(24, 40, "Chorus", 1.0),
                sec(40, 56, "Verse", 0.5),
                sec(56, 64, "Outro", 0.3),
            ],
            hits: vec![
                CueHit { frame: 24 * BAR, kind: "drop".into(), strength: 0.9 },
                CueHit { frame: 30 * BAR + 2 * BEAT, kind: "hit".into(), strength: 0.8 },
            ],
            fits,
        }
    }

    pub fn opts(pacing: Pacing, seconds: Option<u32>) -> GenerateOptions {
        GenerateOptions {
            name: "Test".into(),
            track_path: "/music/song.wav".into(),
            pacing,
            seconds,
            fit_mode: None,
            density: 1.0,
            shape: Shape::default(),
        }
    }
}

#[test]
fn cue_map_round_trips_and_refuses_hostile_input() {
    let m = cue::map();
    let json = serde_json::to_string(&m).unwrap();
    assert_eq!(CueMap::from_json(json.as_bytes()).unwrap(), m);
    let edit = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        f(&mut v);
        CueMap::from_json(v.to_string().as_bytes())
    };
    assert!(matches!(CueMap::from_json(b"[]"), Err(CueMapError::Parse(_))));
    assert!(matches!(edit(&|v| v["version"] = 2.into()), Err(CueMapError::NewerVersion(2))));
    assert!(matches!(edit(&|v| v["sampleRate"] = 0.into()), Err(CueMapError::Invalid(_))));
    assert!(matches!(edit(&|v| v["bpm"] = 1000.into()), Err(CueMapError::Invalid(_))));
    assert!(matches!(edit(&|v| v["durationFrames"] = (48_000u64 * 3600).into()), Err(CueMapError::Invalid(_))));
    assert!(matches!(edit(&|v| v["beats"][1] = 0.into()), Err(CueMapError::Invalid(_))));
    assert!(matches!(edit(&|v| v["downbeats"][63] = u64::MAX.into()), Err(CueMapError::Invalid(_))));
    assert!(matches!(edit(&|v| v["sections"][1]["start"] = 5.into()), Err(CueMapError::Invalid(_))));
    assert!(matches!(edit(&|v| v["sections"][4]["end"] = 1.into()), Err(CueMapError::Invalid(_))));
    assert!(matches!(edit(&|v| v["sections"][0]["energy"] = 2.into()), Err(CueMapError::Invalid(_))));
    assert!(matches!(edit(&|v| v["hits"][0]["strength"] = (-1).into()), Err(CueMapError::Invalid(_))));
    assert!(matches!(edit(&|v| v["track"]["file"] = "../x.wav".into()), Err(CueMapError::Invalid(_))));
    assert!(matches!(
        edit(&|v| v["fits"]["60"]["edit"]["segments"][1][1] = u64::MAX.into()),
        Err(CueMapError::Invalid(_))
    ));
    assert!(matches!(
        edit(&|v| v["fits"]["60"]["edit"]["lengthFrames"] = u64::MAX.into()),
        Err(CueMapError::Invalid(_))
    ));
    assert!(matches!(edit(&|v| v["fits"]["sixty"] = v["fits"]["60"].clone()), Err(CueMapError::Invalid(_))));
    assert!(matches!(edit(&|v| v["surprise"] = 1.into()), Err(CueMapError::Parse(_))));
    assert!(matches!(CueMap::from_json(&vec![b' '; 9 << 20]), Err(CueMapError::TooLarge(_))));
}

#[test]
fn timeline_follows_the_splice() {
    use cue::*;
    let m = map();
    let fit = m.fits["60"].get(None).unwrap();
    let tl = m.timeline(Some(fit));
    assert_eq!(tl.length, 30 * BAR);
    assert_eq!(tl.downbeats.len(), 30);
    assert_eq!(tl.downbeats[22], 22 * BAR, "the ending's first bar follows the splice");
    let labels: Vec<&str> = tl.sections.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, ["Intro", "Verse", "Outro"]);
    assert_eq!((tl.sections[2].start, tl.sections[2].end), (22 * BAR, 30 * BAR));
    assert!(tl.hits.is_empty(), "the drop at bar 25 was cut out");
    let whole = m.timeline(None);
    assert_eq!((whole.length, whole.sections.len(), whole.hits.len()), (64 * BAR, 5, 2));
}

#[test]
fn every_profile_and_length_generates_a_valid_board_that_fills_the_track() {
    use cue::*;
    let m = map();
    for pacing in Pacing::ALL {
        for seconds in [None, Some(30), Some(60), Some(300)] {
            let (board, r) =
                generate(&m, &opts(pacing, seconds)).unwrap_or_else(|e| panic!("{pacing:?} {seconds:?}: {e}"));
            let what = format!("{pacing:?} {seconds:?}");
            assert!(board.validate(&Limits::default()).is_ok(), "{what}");
            assert_eq!(board.total(), r.length, "{what}");
            assert_eq!(board.clips[0].shot_type, ShotType::Ttl, "{what}: opens on a title");
            assert_eq!(board.clips[0].transition_in, Transition::FadeIn, "{what}");
            let min = m.ticks(BEAT);
            assert!(board.clips.iter().all(|c| c.duration >= min), "{what}: no clip under a beat");
            assert!(
                r.cuts_on_downbeats * 10 >= (r.clips - 1) * 7,
                "{what}: most cuts on bars ({}/{})",
                r.cuts_on_downbeats,
                r.clips - 1
            );
            let track = board.track.as_ref().unwrap();
            assert_eq!((track.duration, track.id.0.as_str()), (r.length, "/music/song.wav"));
            assert_eq!(board.format.as_deref(), Some(pacing.name()));
        }
    }
}

#[test]
fn profiles_differ_the_way_they_say() {
    use cue::*;
    let m = map();
    let n = |p| generate(&m, &opts(p, None)).unwrap().1;
    let (calm, cine, energetic, trailer) =
        (n(Pacing::Calm), n(Pacing::Cinematic), n(Pacing::Energetic), n(Pacing::Trailer));
    assert!(
        calm.clips < cine.clips && cine.clips < energetic.clips,
        "{} {} {}",
        calm.clips,
        cine.clips,
        energetic.clips
    );
    assert_eq!(calm.title_cards, 2, "Calm: opening title + end card");
    assert_eq!(cine.title_cards, 3, "Cinematic: opening, the chorus with the drop (key moment), end card");
    // Title cards take the profile's look.
    let (board, _) = generate(&m, &opts(Pacing::Trailer, None)).unwrap();
    for c in board.clips.iter().filter(|c| c.shot_type == ShotType::Ttl) {
        assert_eq!(c.text_style, Pacing::Trailer.text_style());
    }
    assert!(trailer.title_cards >= 6, "Trailer: a title per scene + end card, got {}", trailer.title_cards);
    // The drop at bar 25: a clip starts exactly there.
    let (board, _) = generate(&m, &opts(Pacing::Energetic, None)).unwrap();
    let starts = board.starts();
    assert!(starts.contains(&m.ticks(24 * BAR)));
    // The hit between beats in bar 31 moves a cut onto it.
    assert!(starts.contains(&m.ticks(30 * BAR + 2 * BEAT)), "cut moved onto the hit");
    // Density: more, shorter shots up; fewer, longer shots down; the length never changes.
    let at = |d: f32| generate(&m, &GenerateOptions { density: d, ..opts(Pacing::Cinematic, None) }).unwrap().1;
    let (low, mid, high) = (at(0.5), at(1.0), at(2.0));
    assert!(low.clips < mid.clips && mid.clips < high.clips, "{} {} {}", low.clips, mid.clips, high.clips);
    assert_eq!((low.length, high.length), (mid.length, mid.length));
    assert_eq!(at(f32::NAN).clips, mid.clips, "nonsense density is the default");
    assert_eq!(at(9.0).clips, high.clips, "clamped to 2");
    // Deterministic.
    assert_eq!(
        generate(&m, &opts(Pacing::Trailer, Some(60))).unwrap(),
        generate(&m, &opts(Pacing::Trailer, Some(60))).unwrap()
    );
}

#[test]
fn a_fade_fit_marks_the_last_clip_and_a_missing_fit_is_reported() {
    use cue::*;
    let m = map();
    let (board, r) = generate(&m, &opts(Pacing::Cinematic, Some(30))).unwrap();
    assert_eq!(r.length, m.ticks(15 * BAR));
    assert_eq!(board.clips.last().unwrap().transition_out, Transition::FadeOut);
    assert_eq!(r.fit_changes.as_deref(), Some("Cut to 0:30 with a fade out"));
    // The fit travels with the board, so playback can follow it.
    let edit = board.track.as_ref().and_then(|t| t.edit.clone()).unwrap();
    assert_eq!((edit.sample_rate, edit.length_frames), (48_000, 15 * BAR));
    assert!(edit.fade_out.is_some());
    // The 60 s edit splices: two segments and a crossfade.
    let (board, _) = generate(&m, &opts(Pacing::Cinematic, Some(60))).unwrap();
    let edit = board.track.as_ref().and_then(|t| t.edit.clone()).unwrap();
    assert_eq!((edit.segments.len(), edit.length_frames), (2, 30 * BAR));
    assert!(edit.crossfade_frames > 0);
    // Whole track: no edit.
    let (board, _) = generate(&m, &opts(Pacing::Cinematic, None)).unwrap();
    assert_eq!(board.track.as_ref().map(|t| t.edit.is_none()), Some(true));
    // Hostile edits are refused on load.
    let (good, _) = generate(&m, &opts(Pacing::Cinematic, Some(60))).unwrap();
    let json: serde_json::Value = serde_json::from_str(&good.to_json().unwrap()).unwrap();
    let edit = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v = json.clone();
        f(&mut v["track"]["edit"]);
        Storyboard::from_json(v.to_string().as_bytes())
    };
    assert!(edit(&|_| ()).is_ok());
    assert!(matches!(edit(&|e| e["segments"] = serde_json::json!([])), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|e| e["segments"] = serde_json::json!([[5, 5]])), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|e| e["lengthFrames"] = u64::MAX.into()), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|e| e["sampleRate"] = 0.into()), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|e| e["fadeOut"] = serde_json::json!([u64::MAX, 2])), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|e| e["segments"] = serde_json::json!(vec![[0, 1]; 17])), Err(BoardError::Invalid(_))));
    match generate(&m, &opts(Pacing::Calm, Some(90))) {
        Err(GenerateError::NoFit { seconds: 90, available }) => assert_eq!(available, ["30", "60"]),
        other => panic!("{other:?}"),
    }
    // A track with no beat grid still works (even grid at the tempo).
    let mut free = m.clone();
    free.beats.clear();
    free.downbeats.clear();
    let (board, r) = generate(&free, &opts(Pacing::Trailer, None)).unwrap();
    assert_eq!(board.total(), r.length);
}

// ---- Text clips ----------------------------------------------------------------------------

#[test]
fn a_title_card_switches_to_an_overlay_and_back_without_moving_the_cut() {
    let mut board = sample();
    let starts = board.starts();
    let id = board.clips[0].id;
    assert_eq!(board.clips[0].text_kind(), Some(TextKind::Card));
    assert_eq!(board.scene_title(SceneId(1)), Some("Title 1"));
    assert_eq!(board.fill_progress(), (0, 19));

    let c = set_text_kind(&mut board, id, TextKind::Overlay).unwrap();
    assert_eq!((c.from, c.to, c.removed_assets, c.changed()), (TextKind::Card, TextKind::Overlay, 0, true));
    let clip = &board.clips[0];
    assert_eq!(
        (clip.shot_type, clip.overlay.as_deref(), clip.text()),
        (ShotType::Ws, Some("Title 1"), Some("Title 1"))
    );
    assert_eq!(board.scene_title(SceneId(1)), Some("Title 1"), "the overlay still names the scene");
    assert_eq!(board.fill_progress(), (0, 20), "the shot under the title now wants an image");
    assert_eq!(board.starts(), starts);
    assert!(Storyboard::from_json(board.to_json().unwrap().as_bytes()).is_ok());

    // Quick fill puts an image under it; switching back keeps it as the card's background (B5).
    place(&mut board, AssetId("F:/art/a.png".into()), Some(id)).unwrap();
    let c = set_text_kind(&mut board, id, TextKind::Card).unwrap();
    assert_eq!(c.removed_assets, 0);
    let clip = &board.clips[0];
    assert_eq!((clip.shot_type, clip.description.as_str(), clip.overlay.as_deref()), (ShotType::Ttl, "Title 1", None));
    assert_eq!(clip.background().map(|a| a.0.as_str()), Some("F:/art/a.png"));
    assert_eq!(board.starts(), starts);
    assert!(Storyboard::from_json(board.to_json().unwrap().as_bytes()).is_ok(), "a card with a background is valid");
    board.clips[0].asset_ids.clear();
    assert_eq!(board, sample(), "round trip is exact once the background is removed");

    // Same kind: nothing changes.
    assert!(!set_text_kind(&mut board, id, TextKind::Card).unwrap().changed());
    assert_eq!(board, sample());
}

#[test]
fn text_edits_refuse_clips_without_text() {
    let mut board = sample();
    let shot = board.clips[1].id;
    assert_eq!(board.clips[1].text_kind(), None);
    assert_eq!(set_text_kind(&mut board, shot, TextKind::Card), Err(TextError::NoText(shot)));
    assert_eq!(set_text_kind(&mut board, ClipId(999), TextKind::Card), Err(TextError::NoClip(ClipId(999))));
    assert_eq!(set_text_style(&mut board, shot, Some(TextTheme::Paper), None, None), Err(TextError::NoText(shot)));
    assert_eq!(board, sample());
}

#[test]
fn text_style_edits_and_round_trips() {
    let mut board = sample();
    let id = board.clips[0].id;
    let json = board.to_json().unwrap();
    assert!(!json.contains("textStyle"), "the default style is left out");
    assert!(set_text_style(&mut board, id, Some(TextTheme::Cinematic), Some(TextAnimation::Typewriter), None).unwrap());
    assert!(!set_text_style(&mut board, id, Some(TextTheme::Cinematic), None, None).unwrap(), "no change");
    set_text_kind(&mut board, id, TextKind::Overlay).unwrap();
    assert!(set_text_style(&mut board, id, None, None, Some(TextPosition::Top)).unwrap());
    let style = board.clips[0].text_style;
    assert_eq!(
        (style.theme, style.animation, style.position),
        (TextTheme::Cinematic, TextAnimation::Typewriter, TextPosition::Top)
    );
    let json = board.to_json().unwrap();
    assert!(json.contains("\"theme\": \"cinematic\"") && json.contains("\"position\": \"top\""), "{json}");
    assert_eq!(Storyboard::from_json(json.as_bytes()).unwrap(), board);
    // Ids parse back.
    for t in TextTheme::ALL {
        assert_eq!(TextTheme::from_id(t.id()), Some(t));
    }
    for a in TextAnimation::ALL {
        assert_eq!(TextAnimation::from_id(a.id()), Some(a));
    }
    for p in TextPosition::ALL {
        assert_eq!(TextPosition::from_id(p.id()), Some(p));
    }
    assert_eq!(TextKind::from_id("card").map(TextKind::other), Some(TextKind::Overlay));
    assert_eq!(TextTheme::from_id("neon"), None);
}

#[test]
fn animations_start_hidden_end_at_rest_and_never_misbehave() {
    for a in TextAnimation::ALL {
        let end = a.frame(a.seconds() + 0.01);
        assert_eq!(end, AnimFrame::REST, "{a:?} ends at rest");
        assert_eq!(a.frame(f64::NAN), AnimFrame::REST);
        assert_eq!(a.frame(f64::INFINITY), AnimFrame::REST);
        let mut last = a.frame(-5.0);
        for i in 0..=20 {
            let f = a.frame(a.seconds() * f64::from(i) / 20.0);
            for v in [f.alpha, f.reveal] {
                assert!((0.0..=1.0).contains(&v), "{a:?} {f:?}");
            }
            assert!(f.alpha >= last.alpha && f.reveal >= last.reveal, "{a:?} only moves forward");
            assert!(f.offset_y >= 0.0 && f.scale >= 1.0, "{a:?} {f:?}");
            last = f;
        }
    }
    let fade = TextAnimation::Fade.frame(0.0);
    assert_eq!(fade.alpha, 0.0);
    assert!(TextAnimation::Rise.frame(0.0).offset_y > 0.0);
    assert!(TextAnimation::Zoom.frame(0.0).scale > 1.0);
    let type_on = TextAnimation::Typewriter.frame(0.6);
    assert_eq!(type_on.visible_chars(10), 5);
    assert_eq!(AnimFrame::REST.visible_chars(10), 10);
    assert_eq!(TextAnimation::Typewriter.frame(0.0).visible_chars(10), 0);
}

// ---- Shape (S2b) -----------------------------------------------------------------------------

#[test]
fn shape_overrides_titles_end_card_transitions_and_overlays() {
    use cue::*;
    let m = map();
    let gen_with =
        |pacing: Pacing, shape: Shape| generate(&m, &GenerateOptions { shape, ..opts(pacing, None) }).unwrap();
    // Titles.
    let titles = |t: Titles, end: bool| {
        gen_with(Pacing::Trailer, Shape { titles: Some(t), end_card: Some(end), ..Shape::default() }).1.title_cards
    };
    assert_eq!(titles(Titles::None, false), 0);
    assert_eq!(titles(Titles::OpeningOnly, false), 1);
    assert_eq!(titles(Titles::OpeningOnly, true), 2);
    assert!(titles(Titles::KeyMoments, false) < titles(Titles::EveryScene, false));
    // Transitions at scene boundaries.
    let (board, _) = gen_with(Pacing::Calm, Shape::default());
    let starts: Vec<usize> =
        (1..board.clips.len()).filter(|&i| board.clips[i].scene_id != board.clips[i - 1].scene_id).collect();
    assert!(!starts.is_empty());
    for &i in &starts {
        assert_eq!(board.clips[i].transition_in, Transition::FadeIn, "Calm dips to black");
        assert_eq!(board.clips[i - 1].transition_out, Transition::FadeOut);
    }
    let (board, _) = gen_with(Pacing::Calm, Shape { transition: Some(SceneTransition::Flash), ..Shape::default() });
    assert!(
        starts.iter().all(|&i| board.clips[i].transition_in == Transition::Flash
            || board.clips[i].transition_in == Transition::SmashCut)
    );
    // Overlays: scene titles after the opening become text over a shot; the opening and end card stay cards.
    let (board, _) = gen_with(Pacing::Trailer, Shape { overlays: Some(true), ..Shape::default() });
    let kinds: Vec<_> = board.clips.iter().filter_map(|c| c.text_kind()).collect();
    assert_eq!(kinds.first(), Some(&TextKind::Card), "opening title");
    assert_eq!(kinds.last(), Some(&TextKind::Card), "end card");
    assert!(kinds.contains(&TextKind::Overlay));
    assert!(Storyboard::from_json(board.to_json().unwrap().as_bytes()).is_ok());
    // Ids parse back; the shape travels as JSON.
    for t in Titles::ALL {
        assert_eq!(Titles::from_id(t.id()), Some(t));
    }
    for t in SceneTransition::ALL {
        assert_eq!(SceneTransition::from_id(t.id()), Some(t));
    }
    let shape = Shape { titles: Some(Titles::KeyMoments), transition: Some(SceneTransition::Dip), ..Shape::default() };
    let json = serde_json::to_value(shape).unwrap();
    assert_eq!(json, serde_json::json!({ "titles": "key", "transition": "dip" }));
    assert_eq!(serde_json::from_value::<Shape>(json).unwrap(), shape);
    assert!(serde_json::from_value::<Shape>(serde_json::json!({ "titles": "sometimes" })).is_err());
}

#[test]
fn family_and_hero_have_their_own_character() {
    use cue::*;
    let m = map();
    let (family, fr) = generate(&m, &opts(Pacing::Family, None)).unwrap();
    let (hero, hr) = generate(&m, &opts(Pacing::Hero, None)).unwrap();
    assert!(fr.title_cards >= 5 && hr.title_cards >= 5, "a title on every scene");
    let share =
        |b: &Storyboard, t: ShotType| b.clips.iter().filter(|c| c.shot_type == t).count() as f32 / b.clips.len() as f32;
    assert!(share(&family, ShotType::Cu) > share(&family, ShotType::Ws), "Family leans on close-ups");
    // Hero: the shot after the drop is a close-up.
    let starts = hero.starts();
    let drop_at = m.ticks(24 * BAR);
    let i = (0..hero.clips.len()).find(|&i| starts[i] >= drop_at && hero.clips[i].shot_type != ShotType::Ttl).unwrap();
    assert_eq!(hero.clips[i].shot_type, ShotType::Cu, "the first shot of the drop");
    assert!(hero.clips.len() > family.clips.len(), "Hero cuts faster");
    assert!(Pacing::Hero.suits(130.0) && !Pacing::Family.suits(130.0));
    assert_eq!(Pacing::from_id("hero"), Some(Pacing::Hero));
}

// ---- Motion (SB-3D1 step 1) ----------------------------------------------------------------

/// The sample board with a model in clip 2 and a still in clip 3 (both plain shots).
fn board_with_shots() -> (Storyboard, ClipId, ClipId, ClipId) {
    let mut board = sample();
    let card = board.clips[0].id;
    let model = board.clips[1].id;
    let still = board.clips[2].id;
    assert!(board.clips[1].shot_type.accepts_assets() && board.clips[2].shot_type.accepts_assets());
    board.clips[1].asset_ids = vec![AssetId("/art/orc-warrior.glb".into())];
    board.clips[2].asset_ids = vec![AssetId("/art/concept.png".into())];
    (board, card, model, still)
}

#[test]
fn motion_kind_comes_from_the_hero() {
    let (board, card, model, still) = board_with_shots();
    assert_eq!(MotionKind::of_clip(board.clip(card).unwrap()), None, "a title card has nothing to move");
    assert_eq!(MotionKind::of_clip(board.clip(model).unwrap()), Some(MotionKind::Orbit));
    assert_eq!(MotionKind::of_clip(board.clip(still).unwrap()), Some(MotionKind::PanZoom));
    assert_eq!(MotionKind::of_clip(&Clip::new(ClipId(9), SceneId(1), ShotType::Ws, Ticks(1))), None, "empty shot");
    assert_eq!(MotionKind::of_path("F:\\models\\Horse.GLTF"), MotionKind::Orbit);
    assert_eq!(MotionKind::of_path("/a/b.obj"), MotionKind::Orbit);
    assert_eq!(MotionKind::of_path("/a/b.mp4"), MotionKind::PanZoom);
}

#[test]
fn presets_fill_amount_and_ease_and_edits_make_custom() {
    let (mut board, _, model, still) = board_with_shots();
    let turn = MotionEdit { preset: Some(MotionPreset::Turntable), ..Default::default() };
    assert!(set_motion(&mut board, model, turn).unwrap());
    let m = board.clip(model).unwrap().motion.unwrap();
    assert_eq!((m.preset, m.ease), (MotionPreset::Turntable, Ease::Linear));
    assert!((m.amount.yaw - std::f32::consts::TAU).abs() < 1e-6 && m.amount.zoom == 1.0);
    assert_eq!(m.glyph(), Some("↻"));
    assert!(!set_motion(&mut board, model, turn).unwrap(), "same preset again: no change");
    // The preset's own amount sent back (as a slider does, rounded to its step) keeps the chip.
    let same = MotionEdit { amount: Some(Camera { yaw: 360f32.to_radians(), ..Camera::NONE }), ..Default::default() };
    assert!(!set_motion(&mut board, model, same).unwrap(), "within tolerance: no change");
    assert_eq!(board.clip(model).unwrap().motion.unwrap().preset, MotionPreset::Turntable);
    let rise = MotionEdit { preset: Some(MotionPreset::Rise), ..Default::default() };
    assert!(set_motion(&mut board, model, rise).unwrap());
    let rounded =
        MotionEdit { amount: Some(Camera { pitch: 20f32.to_radians(), ..Camera::NONE }), ..Default::default() };
    assert!(!set_motion(&mut board, model, rounded).unwrap(), "20.000002° is still Rise");
    assert_eq!(board.clip(model).unwrap().motion.unwrap().preset, MotionPreset::Rise);
    assert!(set_motion(&mut board, model, turn).unwrap());

    // Turntable is orbit only; a still pans instead.
    assert_eq!(
        set_motion(&mut board, still, turn),
        Err(MotionError::NotForKind(MotionPreset::Turntable, MotionKind::PanZoom))
    );
    assert_eq!(board.clip(still).unwrap().motion, None, "a refusal changes nothing");
    let pan = MotionEdit { preset: Some(MotionPreset::PanRight), ..Default::default() };
    assert!(set_motion(&mut board, still, pan).unwrap());
    let m = board.clip(still).unwrap().motion.unwrap();
    assert_eq!((m.amount.pan, m.ease, m.glyph()), ([STILL_PAN_TRAVEL, 0.0], Ease::Out, Some("⇆")));
    // A still's pan starts zoomed in on the far side, so the window has room to travel.
    assert_eq!((m.start.zoom, m.start.pan), (STILL_PAN_ZOOM, [-STILL_PAN_TRAVEL / 2.0, 0.0]));
    assert!((m.end().pan[0] - STILL_PAN_TRAVEL / 2.0).abs() < 1e-6);
    let pull = MotionEdit { preset: Some(MotionPreset::PullOut), ..Default::default() };
    assert!(set_motion(&mut board, still, pull).unwrap());
    let m = board.clip(still).unwrap().motion.unwrap();
    assert!(
        (m.start.zoom - 1.6).abs() < 1e-6 && (m.end().zoom - 1.0).abs() < 1e-6,
        "pull out: from 1.6× to the whole picture"
    );
    assert!(set_motion(&mut board, still, pan).unwrap());

    // Editing an amount flips the chip to Custom and keeps the start.
    let zoom = MotionEdit { amount: Some(Camera { zoom: 1.6, ..Camera::NONE }), ..Default::default() };
    assert!(set_motion(&mut board, still, zoom).unwrap());
    let m = board.clip(still).unwrap().motion.unwrap();
    assert_eq!((m.preset, m.glyph(), m.ease), (MotionPreset::Custom, Some("⤢"), Ease::Out));
    assert_eq!(m.start.zoom, 1.6, "the start stays (Pull out had left it at 1.6×, which a pan keeps)");
    // An ease change alone also makes it Custom; a preset on top resets both.
    assert!(set_motion(&mut board, model, MotionEdit { ease: Some(Ease::Out), ..Default::default() }).unwrap());
    assert_eq!(board.clip(model).unwrap().motion.unwrap().preset, MotionPreset::Custom);
    let push = MotionEdit { preset: Some(MotionPreset::PushIn), ..Default::default() };
    assert!(set_motion(&mut board, model, push).unwrap());
    let m = board.clip(model).unwrap().motion.unwrap();
    assert_eq!((m.preset, m.ease, m.amount.zoom, m.amount.yaw), (MotionPreset::PushIn, Ease::InOut, 1.6, 0.0));
    for p in MotionPreset::ALL {
        assert_eq!(MotionPreset::from_id(p.id()), Some(p));
    }
    for e in Ease::ALL {
        assert_eq!(Ease::from_id(e.id()), Some(e));
    }
}

#[test]
fn motion_at_is_eased_from_start_to_end() {
    let start = Camera { yaw: 0.5, pitch: 0.1, zoom: 2.0, pan: [0.1, 0.0] };
    let m = Motion::preset(MotionPreset::Turntable, MotionKind::Orbit, start);
    assert_eq!(m.at(0.0), start);
    assert_eq!(m.at(-3.0), start, "clamped");
    let end = m.at(1.0);
    assert!((end.yaw - (0.5 + std::f32::consts::TAU)).abs() < 1e-5 && end.zoom == 2.0 && end.pan == [0.1, 0.0]);
    assert_eq!(m.at(f32::NAN), start, "not finite: the start");
    assert!((m.at(0.5).yaw - (0.5 + std::f32::consts::PI)).abs() < 1e-5, "linear: halfway at t = 0.5");
    // Ease in-out is symmetric and slow at the ends; ease out is ahead of linear.
    assert_eq!(Ease::InOut.apply(0.5), 0.5);
    assert!(Ease::InOut.apply(0.1) < 0.1 && Ease::InOut.apply(0.9) > 0.9);
    assert!(Ease::Out.apply(0.25) > 0.25 && Ease::Out.apply(1.0) == 1.0 && Ease::Out.apply(0.0) == 0.0);
    // Zoom interpolates geometrically: a 4× push is 2× halfway.
    let push = Motion { amount: Camera { zoom: 4.0, ..Camera::NONE }, ease: Ease::Linear, ..Motion::default() };
    assert!((push.at(0.5).zoom - 2.0).abs() < 1e-6);
    // Swap runs it backwards and lands on the old start.
    let back = push.swapped();
    assert_eq!(back.start, push.end());
    let there = back.end();
    assert!((there.zoom - 1.0).abs() < 1e-6 && there.yaw == 0.0);
    assert_eq!(back.preset, MotionPreset::Custom);
}

#[test]
fn motion_refuses_out_of_range_and_round_trips() {
    let (mut board, card, model, still) = board_with_shots();
    let e = |amount| MotionEdit { amount: Some(amount), ..Default::default() };
    assert_eq!(set_motion(&mut board, card, e(Camera::NONE)), Err(MotionError::NoShot(card)));
    assert_eq!(set_motion(&mut board, ClipId(999), e(Camera::NONE)), Err(MotionError::NoClip(ClipId(999))));
    for bad in [
        Camera { yaw: 7.0, ..Camera::NONE },
        Camera { pitch: 1.5, ..Camera::NONE },
        Camera { zoom: 0.1, ..Camera::NONE },
        Camera { zoom: f32::NAN, ..Camera::NONE },
        Camera { yaw: f32::INFINITY, ..Camera::NONE },
    ] {
        assert_eq!(set_motion(&mut board, model, e(bad)), Err(MotionError::OutOfRange("amount")), "{bad:?}");
    }
    for bad in [Camera { pan: [0.6, 0.0], ..Camera::NONE }, Camera { zoom: 5.0, ..Camera::NONE }] {
        assert_eq!(set_motion(&mut board, still, e(bad)), Err(MotionError::OutOfRange("amount")), "{bad:?}");
    }
    let s = |start| MotionEdit { start: Some(start), ..Default::default() };
    assert_eq!(
        set_motion(&mut board, still, s(Camera { zoom: 0.5, ..Camera::NONE })),
        Err(MotionError::OutOfRange("start"))
    );
    assert_eq!(
        set_motion(&mut board, model, s(Camera { pitch: 2.0, ..Camera::NONE })),
        Err(MotionError::OutOfRange("start"))
    );
    assert!(board.clips.iter().all(|c| c.motion.is_none()), "refusals change nothing");

    // A saved board carries the motion, and one without it loads as `None`.
    let json = board.to_json().unwrap();
    assert!(!json.contains("motion"), "no motion: left out");
    assert!(
        set_motion(&mut board, model, MotionEdit { preset: Some(MotionPreset::Rise), ..Default::default() }).unwrap()
    );
    assert!(set_motion(&mut board, model, s(Camera { yaw: 0.6, pitch: 0.42, zoom: 1.0, pan: [0.0, 0.0] })).unwrap());
    let json = board.to_json().unwrap();
    assert!(json.contains("\"preset\": \"rise\"") && json.contains("\"ease\": \"inOut\""), "{json}");
    assert_eq!(Storyboard::from_json(json.as_bytes()).unwrap(), board);
    assert!(swap_motion(&mut board, model).unwrap());
    assert_eq!(board.clip(model).unwrap().motion.unwrap().preset, MotionPreset::Custom);
    assert!(clear_motion(&mut board, model).unwrap());
    assert!(!clear_motion(&mut board, model).unwrap());
    assert!(!swap_motion(&mut board, model).unwrap());
    assert_eq!(board.clip(model).unwrap().motion, None);
}

#[test]
fn the_bundled_templates_are_valid_boards_with_no_assets_and_the_promised_length() {
    for (info, secs) in BUNDLED.iter().zip([40.0, 60.0, 300.0]) {
        let t = bundled_template(info.id).unwrap_or_else(|| panic!("{}", info.id));
        assert!(t.template && t.track.is_none() && t.asset_refs().next().is_none(), "{}", info.id);
        assert_eq!(t.name, info.name);
        let total = t.total().as_secs_f64();
        assert!((total - secs).abs() < secs * 0.2, "{}: {total:.0} s, promised about {secs:.0}", info.id);
        assert!(t.clips.iter().any(|c| c.shot_type == ShotType::Ttl), "{}: has a card", info.id);
        assert!(t.clips.iter().all(|c| !c.description.is_empty()));
        assert!(t.scenes.len() >= 4);
        // Round-trips through the validating parser with the flag kept.
        let json = t.to_json().unwrap_or_else(|e| panic!("{e}"));
        assert!(json.contains("\"template\": true") || json.contains("\"template\":true"));
        let back = Storyboard::from_json(json.as_bytes()).unwrap_or_else(|e| panic!("{}: {e}", info.id));
        assert_eq!(back, t);
    }
    assert!(bundled_template("nope").is_none());
    // An older board without the field loads with it off.
    let json = sample().to_json().unwrap();
    assert!(!json.contains("template"));
    assert!(!Storyboard::from_json(json.as_bytes()).unwrap().template);
}

#[test]
fn as_template_strips_assets_and_from_template_makes_a_fresh_board() {
    let mut board = sample();
    board.clips[1].asset_ids.push(AssetId("/art/a.png".into()));
    board.clips[2].motion = Some(Motion::preset(MotionPreset::PushIn, MotionKind::PanZoom, Camera::NONE));
    let t = board.as_template("My shape");
    assert!(t.template && t.track.is_none() && t.asset_refs().next().is_none());
    assert_eq!(t.name, "My shape");
    assert_eq!(t.clips.len(), board.clips.len());
    assert_eq!(t.clips[2].motion, board.clips[2].motion, "motion is part of the shape");
    assert_eq!(t.clips[1].description, board.clips[1].description);
    let fresh = Storyboard::from_template(&t, "Episode 2");
    assert!(!fresh.template);
    assert_eq!(fresh.name, "Episode 2");
    assert_eq!(fresh.clips, t.clips);
    assert_eq!(fresh.scenes, t.scenes);
}
