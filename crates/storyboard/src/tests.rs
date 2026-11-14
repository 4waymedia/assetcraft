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
            inset: None,
            part: ScenePart::Section,
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
        level_db: 0,
        credit: None,
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
    assert!(
        set_text_style(&mut board, id, Some(TextTheme::Cinematic), Some(Some(TextAnimation::Typewriter)), None)
            .unwrap()
    );
    assert!(!set_text_style(&mut board, id, Some(TextTheme::Cinematic), None, None).unwrap(), "no change");
    set_text_kind(&mut board, id, TextKind::Overlay).unwrap();
    assert!(set_text_style(&mut board, id, None, None, Some(TextPosition::Top)).unwrap());
    let style = board.clips[0].text_style;
    assert_eq!(
        (style.theme, style.animation, style.position),
        (TextTheme::Cinematic, Some(TextAnimation::Typewriter), TextPosition::Top)
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
    for (info, secs) in BUNDLED.iter().zip([120.0, 120.0, 300.0]) {
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
    // Step sections (SR2): a title card every `STEP_SECS`, then the pattern's three shots.
    let t = bundled_template_with("tutorial", 10).unwrap();
    let steps: Vec<&Scene> =
        t.scenes.iter().filter(|sc| sc.song_section.as_deref().is_some_and(|n| n.starts_with("Step "))).collect();
    assert_eq!(steps.len(), 10);
    for (i, sc) in steps.iter().enumerate() {
        let clips: Vec<&Clip> = t.clips.iter().filter(|c| c.scene_id == sc.id).collect();
        assert_eq!(clips[0].shot_type, ShotType::Ttl, "a title card opens step {}", i + 1);
        assert_eq!(clips[0].description, format!("Step {}", i + 1));
        let shots: Vec<ShotType> = clips[1..].iter().map(|c| c.shot_type).collect();
        assert_eq!(shots, STEP_PATTERN[i % 2], "step {} follows the pattern", i + 1);
        let total: f64 = clips.iter().map(|c| c.duration.as_secs_f64()).sum();
        assert!((total - STEP_SECS).abs() < 0.01, "step {} is {total} s", i + 1);
    }
    assert_eq!(t.clips.iter().filter(|c| c.shot_type == ShotType::Ttl).count(), 12, "10 step titles + intro + outro");
    // Another step length, clamped to the range.
    let t = bundled_template_sized("instructions", 2, 45.0).unwrap();
    let step: f64 = t.clips.iter().filter(|c| c.scene_id == t.scenes[1].id).map(|c| c.duration.as_secs_f64()).sum();
    assert!((step - 45.0).abs() < 0.01);
    let t = bundled_template_sized("review", 1, 1.0).unwrap();
    let step: f64 = t.clips.iter().filter(|c| c.scene_id == t.scenes[2].id).map(|c| c.duration.as_secs_f64()).sum();
    assert!((step - f64::from(STEP_SECS_RANGE.0)).abs() < 0.01);
    // The count of the repeating section is the user's: two points, eight steps, clamped.
    assert_eq!(bundled_template_with("review", 2).unwrap().scenes.len(), 2 + 2 + 1);
    assert_eq!(bundled_template_with("tutorial", 8).unwrap().scenes.len(), 2 + 8 + 1);
    assert_eq!(bundled_template_with("instructions", 99).unwrap().scenes.len(), 1 + STEPS_RANGE.1 + 1);
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

#[test]
fn the_credits_clip_is_appended_last_with_its_placeholders_filled() {
    let credits = credits_template();
    assert!(credits.template && credits.clips.len() == 1 && credits.clips[0].shot_type == ShotType::Ttl);
    // With a song: the credit line comes from the track.
    let mut board = sample();
    board.name = "Hollow Saint".into();
    if let Some(t) = board.track.as_mut() {
        t.credit = Some("Music: Sample track by Nobody · CC0".into());
    }
    let before = board.clips.len();
    let last_scene = board.scenes.len();
    assert_eq!(board.append_credits(&credits), 1);
    assert_eq!(board.clips.len(), before + 1);
    assert_eq!(board.scenes.len(), last_scene + 1);
    let card = board.clips.last().unwrap();
    assert_eq!(card.description, "Hollow Saint\nMusic: Sample track by Nobody · CC0\nMade with AssetCraft");
    assert_eq!(card.shot_type, ShotType::Ttl);
    assert_eq!(card.transition_out, Transition::FadeOut);
    assert_eq!(board.clips[before - 1].transition_out, Transition::Cut, "the old last clip no longer fades out");
    assert_eq!(card.scene_id, board.scenes.last().unwrap().id);
    assert!(
        board.clips.iter().map(|c| c.id).collect::<std::collections::BTreeSet<_>>().len() == board.clips.len(),
        "ids unique"
    );
    board.validate(&Limits::default()).unwrap_or_else(|e| panic!("{e}"));
    // No song: the music line is dropped; no credit text: "Music: <name>".
    let mut plain = Storyboard::new("Plain");
    plain.scenes.push(Scene { id: SceneId(1), song_section: None, inset: None, part: ScenePart::Section });
    plain.append_credits(&credits);
    assert_eq!(plain.clips.last().unwrap().description, "Plain\nMade with AssetCraft");
    let mut named = sample();
    named.append_credits(&credits);
    assert!(
        named.clips.last().unwrap().description.contains("Music: Hollow"),
        "{}",
        named.clips.last().unwrap().description
    );
}

#[test]
fn a_section_inset_spans_its_shots_not_its_title_card_and_older_boards_load() {
    let mut board = sample();
    let scene = board.scenes[0].id;
    let shots: Vec<ClipId> = board.inset_span(scene).map(|c| c.id).collect();
    assert!(!shots.is_empty());
    assert!(board.clips.iter().filter(|c| c.scene_id == scene).count() > shots.len(), "the title card is left out");
    assert!(board.inset_span(scene).all(|c| c.shot_type != ShotType::Ttl));
    assert_eq!(board.inset_scenes().count(), board.scenes.len(), "every sample scene has shots");
    // A title-only scene can't carry one.
    board.scenes.push(Scene::new(SceneId(99), None));
    let mut card = Clip::new(ClipId(999), SceneId(99), ShotType::Ttl, Ticks::from_secs(2).unwrap());
    card.description = "Only a card".into();
    board.clips.push(card);
    assert_eq!(board.inset_scenes().count(), board.scenes.len() - 1);
    // The file: `inset` is left out when none, read back when set, and a file without the key
    // (every board saved before SR2) loads with no inset.
    let text = board.to_json().unwrap();
    assert!(!text.contains("\"inset\""));
    board.scenes[0].inset =
        Some(Inset { asset: Some(AssetId("/clips/host.mp4".into())), ..Inset::slot(InsetKind::Video) });
    let text = board.to_json().unwrap();
    assert!(text.contains("\"inset\""));
    let back = Storyboard::from_json(text.as_bytes()).unwrap();
    assert_eq!(back, board);
    let slot = Inset::slot(InsetKind::Graphic);
    assert!(slot.is_empty());
    assert_eq!((slot.corner, slot.size), (Corner::BottomRight, InsetSize::Medium));
    assert_eq!(Corner::ALL.map(Corner::code), ["TL", "TR", "BL", "BR"]);
    assert!(
        InsetSize::Small.share() < InsetSize::Medium.share() && InsetSize::Medium.share() < InsetSize::Large.share()
    );
}

#[test]
fn the_chorus_loops_under_a_cut_longer_than_the_song_and_the_track_carries_it() {
    use cue::{BAR, BEAT, SR};
    let m = cue::map();
    // The chorus is bars 24–40 (32 s); a 100 s Cut needs it three times, the last piece cut.
    let total = Ticks::from_secs(100).unwrap();
    let fit = chorus_loop(&m, total, false).expect("a chorus to loop");
    assert_eq!(fit.length_frames, 100 * SR);
    assert_eq!(fit.segments.len(), 4, "32 + 32 + 32 + 4 s: {:?}", fit.segments);
    assert_eq!(fit.segments[0], [24 * BAR, 40 * BAR]);
    assert_eq!(fit.segments[1], [24 * BAR, 40 * BAR]);
    assert_eq!(fit.segments[2], [24 * BAR, 40 * BAR]);
    assert_eq!(fit.segments[3], [24 * BAR, 24 * BAR + 4 * SR]);
    assert_eq!(fit.crossfade_frames, BEAT, "a beat at each splice");
    let fade = fit.fade_out.unwrap();
    assert_eq!((fade.start, fade.frames), (98 * SR, 2 * SR));
    assert_eq!(fit.mode, FitMode::Fade);
    // The track under a 100 s Cut: as long as the Cut, every piece labelled Chorus, beats in
    // every repeat.
    // A board as long as `total` with one plain section.
    let board_of = |secs: u64| {
        let mut b = Storyboard::new("Loop");
        b.scenes.push(Scene::new(SceneId(1), None));
        b.clips.push(Clip::new(ClipId(1), SceneId(1), ShotType::Ms, Ticks::from_secs(secs).unwrap()));
        b
    };
    let (track, what) = track_under(&m, "/music/song.wav", &board_of(100), MusicMode::ChorusLoop);
    assert_eq!(what, "the chorus looped");
    assert_eq!(track.duration, total);
    assert!(track.song_sections.iter().all(|s| s.name == "Chorus"), "{:?}", track.song_sections);
    assert_eq!(track.song_sections.len(), 1, "same-labelled neighbours merge into one Chorus band");
    assert_eq!((track.song_sections[0].start, track.song_sections[0].end), (Ticks::ZERO, total));
    assert_eq!(track.beat_grid.len(), 200, "two beats a second for 100 s");
    let edit = track.edit.as_ref().unwrap();
    assert_eq!(edit.segments, fit.segments);
    assert_eq!(edit.length_frames, 100 * SR);
    // Shorter than one chorus: one piece, cut.
    let short = chorus_loop(&m, Ticks::from_secs(10).unwrap(), false).unwrap();
    assert_eq!(short.segments, vec![[24 * BAR, 24 * BAR + 10 * SR]]);
    // A tail shorter than a beat goes to the previous piece.
    let odd = chorus_loop(&m, Ticks::from_secs_f64(32.25).unwrap(), false).unwrap();
    assert_eq!(odd.segments, vec![[24 * BAR, 40 * BAR + SR / 4]]);
    assert_eq!(odd.length_frames, 32 * SR + SR / 4);
    // Whole: the song from the top, no edit, its own length.
    let (whole, what) = track_under(&m, "/music/song.wav", &board_of(100), MusicMode::Whole);
    assert_eq!(what, "the whole song");
    assert_eq!(whole.duration, Ticks::from_secs(128).unwrap());
    assert!(whole.edit.is_none());
    // Two choruses: both in order, then again; first only takes the first.
    let mut two = m.clone();
    two.sections[3].label = "Chorus".into(); // bars 40–56 too
    let fit = chorus_loop(&two, Ticks::from_secs(70).unwrap(), false).unwrap();
    assert_eq!(fit.segments, vec![[24 * BAR, 40 * BAR], [40 * BAR, 56 * BAR], [24 * BAR, 24 * BAR + 6 * SR]]);
    let fit = chorus_loop(&two, Ticks::from_secs(70).unwrap(), true).unwrap();
    assert_eq!(fit.segments, vec![[24 * BAR, 40 * BAR], [24 * BAR, 40 * BAR], [24 * BAR, 24 * BAR + 6 * SR]]);
    // No chorus: the highest-energy section; no sections: nothing to loop → the whole song.
    let mut none = m.clone();
    for s in &mut none.sections {
        s.label = s.label.replace("Chorus", "Build");
    }
    let fit = chorus_loop(&none, Ticks::from_secs(10).unwrap(), false).unwrap();
    assert_eq!(fit.segments[0][0], 24 * BAR, "the Build at energy 1.0");
    none.sections.clear();
    assert!(chorus_loop(&none, Ticks::from_secs(10).unwrap(), false).is_none());
    let (_, what) = track_under(&none, "/music/song.wav", &board_of(100), MusicMode::ChorusLoop);
    assert_eq!(what, "the whole song (nothing to loop)");
    assert_eq!(MusicMode::parse("firstChorus"), Some(MusicMode::FirstChorus));
    assert_eq!(MusicMode::default().id(), "structured");
}

#[test]
fn structured_music_puts_the_intro_under_the_opening_a_chorus_on_every_section_title_and_the_outro_under_the_close() {
    use cue::{BAR, BEAT, SR};
    let m = cue::map();
    // A Tutorial with two 20 s steps: Goal + Setup (opening) · Step 1 · Step 2 · Result (close).
    let t = bundled_template_sized("tutorial", 2, 20.0).unwrap();
    let spans = t.music_spans();
    assert_eq!(
        spans.iter().map(|s| s.part).collect::<Vec<_>>(),
        [ScenePart::Opening, ScenePart::Section, ScenePart::Section, ScenePart::Close]
    );
    let opening = spans[0].length.as_secs_f64();
    assert!(
        opening > 0.0 && (spans[1].start.as_secs_f64() - opening).abs() < 1e-9,
        "the first section starts where the opening ends"
    );
    assert!((spans[1].length.as_secs_f64() - 20.0).abs() < 1e-9 && (spans[2].length.as_secs_f64() - 20.0).abs() < 1e-9);
    let fit = structured_fit(&m, &spans).expect("a song with a chorus");
    // Intro (bars 0–8 = 16 s) under the opening, cut to its length; a chorus from bar 24 at
    // each step's start; the outro (bars 56–64) under the close.
    let intro_len = fit.segments[0][1] - fit.segments[0][0];
    assert_eq!(fit.segments[0][0], 0);
    assert!(intro_len <= 8 * BAR);
    let frames = |secs: f64| (secs * SR as f64).round() as u64;
    // Where each span starts in the output = the sum of the pieces before it.
    let mut off = 0u64;
    let mut starts = Vec::new();
    for s in &fit.segments {
        starts.push((off, *s));
        off += s[1] - s[0];
    }
    let at = |t: u64| {
        starts.iter().find(|(o, s)| *o <= t && t < *o + (s[1] - s[0])).map(|(o, s)| (s[0] + (t - o), s)).unwrap()
    };
    let (src1, piece1) = at(frames(opening));
    assert_eq!(src1, 24 * BAR, "step 1's title lands on the chorus start: {piece1:?}");
    let (src2, piece2) = at(frames(opening + 20.0));
    assert_eq!(src2, 24 * BAR, "step 2's title lands on the chorus start again (one chorus rotates): {piece2:?}");
    let close_at = frames(opening + 40.0);
    let (src3, _) = at(close_at);
    assert_eq!(src3, 56 * BAR, "the outro under the close");
    assert_eq!(fit.length_frames, m.frames(t.total()));
    assert_eq!(fit.crossfade_frames, BEAT);
    assert!(fit.reasons[0].contains("1 chorus over 2 sections"), "{:?}", fit.reasons);
    // Through the track: as long as the Cut, with the edit.
    let (track, what) = track_under(&m, "/music/song.wav", &t, MusicMode::Structured);
    assert_eq!(what, "intro · a chorus per section · outro");
    assert_eq!(track.duration, t.total());
    assert_eq!(track.edit.as_ref().unwrap().segments, fit.segments);
    // Two choruses rotate: section 1 gets the first, section 2 the second, section 3 the first.
    let mut two = m.clone();
    two.sections[3].label = "Chorus".into();
    let t3 = bundled_template_sized("tutorial", 3, 20.0).unwrap();
    let fit = structured_fit(&two, &t3.music_spans()).unwrap();
    let mut off = 0u64;
    let mut starts = Vec::new();
    for s in &fit.segments {
        starts.push((off, *s));
        off += s[1] - s[0];
    }
    let src =
        |t: u64| starts.iter().find(|(o, s)| *o <= t && t < *o + (s[1] - s[0])).map(|(o, s)| s[0] + (t - o)).unwrap();
    let o = t3.music_spans()[0].length.as_secs_f64();
    assert_eq!(src(frames(o)), 24 * BAR);
    assert_eq!(src(frames(o + 20.0)), 40 * BAR);
    assert_eq!(src(frames(o + 40.0)), 24 * BAR);
    // A song without intro / outro labels: the stretch before the first chorus and after the
    // last stand in; no chorus at all → the loudest section; no sections → nothing.
    let mut bare = m.clone();
    bare.sections[0].label = "Part A".into();
    bare.sections[4].label = "Part E".into();
    let fit = structured_fit(&bare, &spans).unwrap();
    assert_eq!(fit.segments[0][0], 0, "the stretch before the chorus");
    bare.sections.clear();
    assert!(structured_fit(&bare, &spans).is_none());
    assert!(structured_fit(&m, &[]).is_none());
    // A board with only sections (an old board, a preset): choruses throughout.
    let mut plain = Storyboard::new("Plain");
    plain.scenes.push(Scene::new(SceneId(1), None));
    plain.clips.push(Clip::new(ClipId(1), SceneId(1), ShotType::Ms, Ticks::from_secs(10).unwrap()));
    let fit = structured_fit(&m, &plain.music_spans()).unwrap();
    assert_eq!(fit.segments, vec![[24 * BAR, 24 * BAR + 10 * SR]]);
}

#[test]
fn built_in_themes_paint_exactly_todays_looks_and_round_trip_as_files() {
    for t in TextTheme::ALL {
        let theme = Theme::builtin(t);
        assert_eq!(theme.look(), t.look(), "{}", t.id());
        let (back, warnings) =
            Theme::parse(theme.to_json().as_bytes(), true).unwrap_or_else(|e| panic!("{}: {e}", t.id()));
        assert_eq!(back, theme);
        assert!(warnings.is_empty(), "{}: {warnings:?}", t.id());
        assert!(back.title_in().is_some());
    }
    assert_eq!(Theme::builtins().len(), 8);
}

#[test]
fn theme_files_are_checked_as_hostile_input() {
    let good = r##"{
        "version": 1, "id": "noir-gold", "name": "Noir Gold", "tags": ["dark", "1920s"],
        "credit": {"author": "Paul", "license": "own"},
        "color": {"background": "#0B0B0D", "surface": "#17171B", "text": "#F2EFE8", "muted": "#8A857A", "accent": "#C9A227"},
        "type": {"display": {"family": "Cinzel", "weight": 700, "tracking": 0.12, "uppercase": true},
                 "body": {"family": "Inter"}, "scale": {"card": 0.14, "overlay": 0.06}},
        "background": {"kind": "gradient", "stops": ["#0B0B0D", "#1C1408"], "angle": 90, "grain": 0.08, "vignette": 0.35},
        "plate": {"color": "#0B0B0D", "alpha": 180},
        "space": {"margin": 0.07},
        "motion": {"titleIn": "fade"}
    }"##;
    let (theme, warnings) = Theme::parse(good.as_bytes(), false).unwrap();
    assert_eq!(theme.id, "noir-gold");
    assert_eq!(warnings.len(), 1, "Cinzel isn't bundled: {warnings:?}");
    assert!(warnings[0].contains("Cinzel"));
    let look = theme.look();
    assert_eq!((look.background, look.text, look.card_size, look.uppercase), (0x0B0B0D, 0xF2EFE8, 0.14, true));
    assert_eq!(look.plate, Some((0x0B0B0D, 180)));
    assert_eq!(theme.space.padding, 0.02, "defaults fill in");
    // Each broken field is refused with a reason.
    let swap = |from: &str, to: &str| good.replacen(from, to, 1);
    for (bad, why) in [
        (swap("\"version\": 1", "\"version\": 2"), "version"),
        (swap("noir-gold", "Noir Gold"), "id"),
        (swap("\"#0B0B0D\", \"surface\"", "\"black\", \"surface\""), "colour"),
        (swap("\"card\": 0.14", "\"card\": 0.9"), "size"),
        (swap("\"tracking\": 0.12", "\"tracking\": 2"), "tracking"),
        (swap("\"weight\": 700", "\"weight\": 1200"), "weight"),
        (swap("\"grain\": 0.08", "\"grain\": 3"), "grain"),
        (swap("\"stops\": [\"#0B0B0D\", \"#1C1408\"]", "\"stops\": [\"#0B0B0D\"]"), "gradient stops"),
        (swap("\"titleIn\": \"fade\"", "\"titleIn\": \"explode\""), "motion"),
        (swap("\"motion\"", "\"imagery\": {}, \"motion\""), "unknown field"),
        (swap("\"name\": \"Noir Gold\"", "\"name\": \"  \""), "empty name"),
        ("{".to_owned(), "not JSON"),
    ] {
        assert!(Theme::parse(bad.as_bytes(), false).is_err(), "{why}");
    }
    assert!(Theme::parse(&vec![b' '; MAX_THEME_BYTES + 1], false).is_err(), "too large");
    // Low contrast: a warning for the user's own, refused for a shipped theme.
    let grey = swap("\"text\": \"#F2EFE8\"", "\"text\": \"#202022\"");
    let (_, w) = Theme::parse(grey.as_bytes(), false).unwrap();
    assert!(w.iter().any(|w| w.contains("contrast")), "{w:?}");
    assert!(Theme::parse(grey.as_bytes(), true).is_err());
    assert!((contrast(0xffffff, 0x000000) - 21.0).abs() < 0.01);
    assert_eq!(parse_color("#c9a227"), Some(0xC9A227));
    assert_eq!(parse_color("c9a227"), None);
}

/// Paul, 2026-11-08: "the step/instruction might want to set a display time for the text… a
/// variable for duration and then the effect type for end."
#[test]
fn text_timing_runs_in_effect_out_then_hides() {
    let style = TextStyle {
        animation: Some(TextAnimation::Fade),
        duration_ms: Some(4000),
        effect: Some(TextEffect::None),
        exit: Some(TextExit::Fade),
        ..TextStyle::default()
    };
    let clip = 10.0;
    assert_eq!(style.shown_secs(clip), 4.0);
    assert!(style.frame_at(0.1, clip).alpha < 0.6, "fading in");
    assert_eq!(style.frame_at(2.0, clip), AnimFrame::REST, "up");
    let out = style.frame_at(3.75, clip);
    assert!(out.alpha > 0.0 && out.alpha < 1.0, "fading out: {out:?}");
    assert!(style.frame_at(4.0, clip).is_hidden() && style.frame_at(9.0, clip).is_hidden(), "gone after");
    assert_eq!(style.frame_at(f64::INFINITY, clip), AnimFrame::REST, "stills show it at rest");
    // A duration past the clip counts as the clip.
    assert_eq!(style.shown_secs(2.5), 2.5);
    // No duration, no out: the text stays to the end (the old behaviour).
    let plain = TextStyle {
        animation: Some(TextAnimation::Fade),
        duration_ms: Some(0),
        effect: Some(TextEffect::None),
        exit: Some(TextExit::None),
        ..TextStyle::default()
    };
    assert_eq!(plain.frame_at(9.99, clip), AnimFrame::REST);
    assert_eq!(plain.frame_at(clip, clip), AnimFrame::REST);
    // No duration but an out: it leaves as the clip ends.
    let ends = TextStyle { exit: Some(TextExit::Erase), ..TextStyle::default() };
    assert!(ends.frame_at(9.6, clip).reveal < 1.0 && ends.frame_at(9.6, clip).reveal > 0.0);
    // A duration shorter than in + out: the in finishes, the out gets what is left.
    let tight = TextStyle {
        animation: Some(TextAnimation::Typewriter),
        duration_ms: Some(1500),
        exit: Some(TextExit::Erase),
        ..TextStyle::default()
    };
    assert!(tight.frame_at(1.19, clip).reveal > 0.9, "typing on");
    assert!(tight.frame_at(1.4, clip).reveal < 0.5, "erasing");
    // Effects move the text while it is up.
    let push = TextStyle { effect: Some(TextEffect::Push), duration_ms: Some(5000), ..TextStyle::default() };
    assert!(push.frame_at(4.0, clip).scale > push.frame_at(1.0, clip).scale);
    let drift = TextStyle { effect: Some(TextEffect::Drift), ..TextStyle::default() };
    assert!(drift.frame_at(8.0, clip).offset_y < 0.0);
    for e in TextEffect::ALL {
        assert_eq!(TextEffect::from_id(e.id()), Some(e));
    }
    for e in TextExit::ALL {
        assert_eq!(TextExit::from_id(e.id()), Some(e));
    }
}

#[test]
fn text_timing_edits_round_trip_and_old_boards_load() {
    let mut board = sample();
    let id = board.clips[0].id;
    assert!(
        set_text_timing(&mut board, id, Some(Some(3000)), Some(Some(TextEffect::Pulse)), Some(Some(TextExit::Sink)))
            .unwrap()
    );
    assert!(!set_text_timing(&mut board, id, Some(Some(3000)), None, None).unwrap(), "no change");
    // Clamped to the allowed range.
    set_text_timing(&mut board, id, Some(Some(10)), None, None).unwrap();
    assert_eq!(board.clips[0].text_style.duration_ms, Some(MIN_TEXT_MS));
    set_text_timing(&mut board, id, Some(Some(3000)), None, None).unwrap();
    let json = board.to_json().unwrap();
    assert!(json.contains("\"durationMs\": 3000") && json.contains("\"exit\": \"sink\""), "{json}");
    assert_eq!(Storyboard::from_json(json.as_bytes()).unwrap(), board);
    // "Until the clip ends", chosen on the clip: saved as 0.
    assert!(set_text_timing(&mut board, id, Some(Some(0)), None, None).unwrap());
    assert!(board.to_json().unwrap().contains("\"durationMs\": 0"));
    assert_eq!(board.clips[0].text_style.timing().duration_ms, None);
    // Back to the theme's for all three: nothing saved.
    assert!(set_text_timing(&mut board, id, Some(None), Some(None), Some(None)).unwrap());
    let json = board.to_json().unwrap();
    assert!(!json.contains("durationMs") && !json.contains("\"exit\""), "{json}");
    let shot = board.clips[1].id;
    assert_eq!(set_text_timing(&mut board, shot, Some(Some(2000)), None, None), Err(TextError::NoText(shot)));
}

/// Themes step 1b-i (Paul, 2026-11-10: "build it" — the spec's §4 #5, #6, #8, #13).
#[test]
fn the_four_new_themes_save_by_their_spec_ids_and_read_on_their_background() {
    let mut board = sample();
    let id = board.clips[0].id;
    for (t, want) in [
        (TextTheme::NoirGold, "noir-gold"),
        (TextTheme::NeonNight, "neon-night"),
        (TextTheme::FieldNotes, "field-notes"),
        (TextTheme::Blueprint, "blueprint"),
    ] {
        assert_eq!(t.id(), want);
        assert_eq!(TextTheme::from_id(want), Some(t));
        set_text_style(&mut board, id, Some(t), None, None).unwrap();
        let json = board.to_json().unwrap();
        assert!(json.contains(&format!("\"theme\": \"{want}\"")), "{json}");
        assert_eq!(Storyboard::from_json(json.as_bytes()).unwrap(), board);
        let look = t.look();
        assert!(contrast(look.text, look.background) >= 4.5, "{want}: text must read on its card");
        assert!(contrast(t.accent(), look.background) >= 3.0, "{want}: the accent shows on the card");
    }
    // Ids are unique, and a board from before still loads its old ids.
    let ids: std::collections::HashSet<&str> = TextTheme::ALL.iter().map(|t| t.id()).collect();
    assert_eq!(ids.len(), TextTheme::ALL.len());
    assert_eq!(TextTheme::from_id("paper"), Some(TextTheme::Paper));
}

/// Themes 1d (Paul, 2026-11-10: "themes that are more than just a solid color and font size…
/// lets get the base working").
#[test]
fn a_backdrop_paints_gradient_vignette_and_steady_grain() {
    let flat = Backdrop::solid(0x204060);
    assert_eq!(flat.color_at(0.1, 0.9), [32.0, 64.0, 96.0]);
    assert_eq!(flat.grain_at(3, 4), 0.0);
    // 90° runs top to bottom: the first stop at the top, the second at the bottom.
    let g = Backdrop::gradient(0xffffff, 0x000000, 90.0, 0.0, 0.0);
    assert_eq!(g.color_at(0.5, 0.0), [255.0, 255.0, 255.0]);
    assert_eq!(g.color_at(0.5, 1.0), [0.0, 0.0, 0.0]);
    assert!((g.color_at(0.2, 0.5)[0] - 127.5).abs() < 0.01, "even across");
    // 0° runs left to right.
    let h = Backdrop::gradient(0x000000, 0xff0000, 0.0, 0.0, 0.0);
    assert!(h.color_at(0.9, 0.5)[0] > h.color_at(0.1, 0.5)[0]);
    // Vignette darkens the corners, not the middle.
    let v = Backdrop { vignette: 0.8, ..Backdrop::solid(0xc0c0c0) };
    assert_eq!(v.color_at(0.5, 0.5), [192.0, 192.0, 192.0]);
    assert!(v.color_at(0.0, 0.0)[0] < 100.0);
    // Grain: the same at the same pixel, different nearby, bounded.
    let n = Backdrop { grain: 1.0, ..Backdrop::solid(0x808080) };
    assert_eq!(n.grain_at(10, 20), n.grain_at(10, 20));
    assert!((0..64).any(|x| n.grain_at(x, 0) != n.grain_at(0, 0)));
    assert!((0..256).all(|x| n.grain_at(x, 7).abs() <= 28.0));
    // Every built-in has depth now: a gradient, grain or vignette.
    for t in TextTheme::ALL {
        let b = t.look().backdrop;
        assert!(b.count > 1 || b.grain > 0.0 || b.vignette > 0.0, "{}", t.id());
    }
}

#[test]
fn theme_files_carry_the_gradient_and_the_accent_bar() {
    let mut theme = Theme::builtin(TextTheme::Blueprint);
    assert_eq!(theme.background.kind, BackgroundKind::Gradient);
    assert!(theme.decor.accent_bar);
    theme.background.stops = vec!["#102030".into(), "#203040".into(), "#304050".into()];
    theme.background.angle = 45.0;
    let json = theme.to_json();
    assert!(json.contains("\"accentBar\": true"), "{json}");
    let back = Theme::parse(json.as_bytes(), false).unwrap().0;
    let look = back.look();
    assert_eq!((look.backdrop.count, look.backdrop.stops), (3, [0x102030, 0x203040, 0x304050, 0x304050]));
    assert_eq!(look.accent, Some(0xffd23f));
    // No bar unless the file asks; an old file without `decor` loads with none.
    theme.decor = Decor::default();
    assert!(!theme.to_json().contains("decor"));
    assert_eq!(Theme::parse(theme.to_json().as_bytes(), false).unwrap().0.look().accent, None);
    // A stop the text can't be read on is a warning, not a refusal.
    theme.background.stops = vec!["#1E4F8F".into(), "#FFFFFF".into()];
    let (_, warnings) = Theme::parse(theme.to_json().as_bytes(), false).unwrap();
    assert!(warnings.iter().any(|w| w.contains("#FFFFFF")), "{warnings:?}");
}

/// Themes 1e: rules, corners, frame and patterns, as plain rectangles both renderers draw.
#[test]
fn decor_draws_rules_corners_frame_and_patterns_inside_the_card() {
    let (w, h) = (1920.0, 1080.0);
    let inside = |r: &DecorRect| r.x >= -1.0 && r.y >= -1.0 && r.x + r.w <= w + 1.0 && r.y + r.h <= h + 1.0;
    // Blueprint: grid, corners, frame — all inside the card, corners and frame in yellow.
    let blue = TextTheme::Blueprint.look();
    let shapes = card_decor(&blue, w, h);
    assert!(shapes.iter().all(inside));
    assert!(shapes.iter().filter(|r| r.color == 0xffd23f).count() == 4 + 8, "frame sides + corner arms");
    let grid = shapes.iter().filter(|r| r.color == blue.text).count();
    assert!(grid > 20 && grid < 60, "about 10 rows and 18 columns: {grid}");
    // Neon Night: scanlines; Field Notes: dots — bounded on a huge frame.
    assert!(card_decor(&TextTheme::NeonNight.look(), w, h).len() > 150);
    assert!(card_decor(&TextTheme::FieldNotes.look(), 20_000.0, 20_000.0).len() <= MAX_PATTERN_SHAPES + 8);
    // Nonsense sizes draw nothing rather than misbehave.
    assert!(card_decor(&blue, f32::NAN, h).is_empty() && card_decor(&blue, 2.0, 2.0).is_empty());
    // Bold has no decor on the card.
    assert!(card_decor(&TextTheme::Bold.look(), w, h).is_empty());
    // Noir Gold: a rule above and one below the title, wider than it; no accent bar then.
    let noir = TextTheme::NoirGold.look();
    let block = [760.0, 480.0, 400.0, 120.0];
    let rules = text_decor(&noir, TextKind::Card, block, h, [0.0, 0.0]);
    assert_eq!(rules.len(), 2);
    assert!(rules[0].y < block[1] && rules[1].y > block[1] + block[3] && rules[0].w > block[2]);
    // An overlay keeps its accent bar even when the theme's card has rules.
    let ov = text_decor(&noir, TextKind::Overlay, block, h, [20.0, 10.0]);
    assert_eq!((ov.len(), ov[0].color), (1, noir.accent.unwrap_or(0)));
    // Radial: the middle is the first stop, the corners the last.
    let r = Backdrop::radial(0xffffff, 0x000000, 0.0, 0.0);
    assert_eq!(r.color_at(0.5, 0.5), [255.0, 255.0, 255.0]);
    assert_eq!(r.color_at(0.0, 0.0), [0.0, 0.0, 0.0]);
}

#[test]
fn theme_files_name_their_decor() {
    let mut theme = Theme::builtin(TextTheme::Paper);
    theme.decor =
        Decor { accent_bar: false, rules: Rules::Double, corners: true, frame: false, pattern: Pattern::Dots };
    theme.background.kind = BackgroundKind::Radial;
    theme.background.stops = vec!["#FFFFFF".into(), "#EEE8DC".into()];
    let json = theme.to_json();
    for want in ["\"rules\": \"double\"", "\"pattern\": \"dots\"", "\"corners\": true", "\"kind\": \"radial\""] {
        assert!(json.contains(want), "{want} in {json}");
    }
    let look = Theme::parse(json.as_bytes(), false).unwrap().0.look();
    assert_eq!((look.decor.rules, look.decor.pattern, look.decor.corners), (Rules::Double, Pattern::Dots, true));
    assert!(look.backdrop.radial);
    // A name it doesn't know is refused, with the field named.
    let bad = json.replace("\"dots\"", "\"plaid\"");
    assert!(Theme::parse(bad.as_bytes(), false).is_err());
    // Every built-in round-trips through its file.
    for t in TextTheme::ALL {
        assert_eq!(
            Theme::parse(Theme::builtin(t).to_json().as_bytes(), true).unwrap().0.look(),
            t.look(),
            "{}",
            t.id()
        );
    }
}

/// Themes 1f: shadow, outline and glow are extra passes of the words, under them.
#[test]
fn text_finish_makes_passes_under_the_words() {
    let mut look = TextTheme::Bold.look();
    assert_eq!(look.finish, TextFinish::Shadow);
    let shadow = finish_passes(&look, 100.0);
    assert!(shadow.iter().all(|p| p.dx > 0.0 && p.dy > 0.0 && p.color == 0x000000), "down and right, black");
    // Outline: a ring all round, in the tone opposite the text.
    look.finish = TextFinish::Outline;
    let ring = finish_passes(&look, 100.0);
    assert_eq!(ring.len(), 12);
    assert!(ring.iter().all(|p| p.color == 0x000000), "white text gets a dark edge");
    assert!(ring.iter().any(|p| p.dx < 0.0) && ring.iter().any(|p| p.dy < 0.0));
    let mut paper = TextTheme::Paper.look();
    paper.finish = TextFinish::Outline;
    assert!(finish_passes(&paper, 100.0).iter().all(|p| p.color == 0xffffff), "dark text gets a light edge");
    // Glow: rings in the accent, faint, wider than the outline.
    let neon = TextTheme::NeonNight.look();
    let glow = finish_passes(&neon, 100.0);
    assert!(glow.iter().all(|p| p.color == 0xff2bd6 && p.alpha < 0.3));
    assert!(glow.iter().map(|p| p.dx.hypot(p.dy)).fold(0.0, f32::max) > 10.0);
    // None, or a nonsense height: nothing.
    assert!(finish_passes(&TextTheme::Paper.look(), 100.0).is_empty());
    assert!(finish_passes(&neon, f32::NAN).is_empty() && finish_passes(&neon, 0.0).is_empty());
    // The theme file names it on the display face; old files have none.
    let mut theme = Theme::builtin(TextTheme::NeonNight);
    let json = theme.to_json();
    assert!(json.contains("\"finish\": \"glow\""), "{json}");
    theme.type_.display.finish = TextFinish::None;
    assert!(!theme.to_json().contains("finish"));
    let bad = json.replace("\"glow\"", "\"sparkle\"");
    assert!(Theme::parse(bad.as_bytes(), false).is_err(), "an unknown finish is refused");
}

/// Themes 1g (Paul, 2026-11-08: "theme, transition in, duration, effect, transition out,
/// position"): the theme picks the moves; a clip's own choice wins, and "theme" goes back.
#[test]
fn a_theme_sets_the_moves_and_a_clip_can_override_each() {
    let neon = TextStyle { theme: TextTheme::NeonNight, ..TextStyle::default() };
    let t = neon.timing();
    assert_eq!(
        (t.animation, t.effect, t.exit, t.duration_ms),
        (TextAnimation::Zoom, TextEffect::Pulse, TextExit::Zoom, None)
    );
    // The clip's own choices, one at a time.
    let own = TextStyle { animation: Some(TextAnimation::Fade), exit: Some(TextExit::None), ..neon };
    let t = own.timing();
    assert_eq!((t.animation, t.effect, t.exit), (TextAnimation::Fade, TextEffect::Pulse, TextExit::None));
    // Switching the theme re-times what the clip didn't choose.
    let blue = TextStyle { theme: TextTheme::Blueprint, ..own };
    assert_eq!((blue.timing().animation, blue.timing().effect), (TextAnimation::Fade, TextEffect::None));
    // A theme with an out hides the text at the clip's end.
    assert!(TextStyle { theme: TextTheme::Blueprint, ..TextStyle::default() }.frame_at(9.99, 10.0).reveal < 0.1);
    // Every built-in's moves survive its theme file.
    for th in TextTheme::ALL {
        let file = Theme::parse(Theme::builtin(th).to_json().as_bytes(), true).unwrap().0;
        assert_eq!(file.look().motion, th.look().motion, "{}", th.id());
    }
    // A file's moves are checked like the rest.
    let mut theme = Theme::builtin(TextTheme::Paper);
    theme.motion.title_out = Some("explode".into());
    assert!(Theme::parse(theme.to_json().as_bytes(), false).is_err());
    theme.motion.title_out = Some("sink".into());
    theme.motion.title_seconds = Some(0.1);
    assert!(Theme::parse(theme.to_json().as_bytes(), false).is_err(), "too short");
    theme.motion.title_seconds = Some(4.0);
    let look = Theme::parse(theme.to_json().as_bytes(), false).unwrap().0.look();
    assert_eq!((look.motion.exit, look.motion.duration_ms), (TextExit::Sink, Some(4000)));
}
