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
