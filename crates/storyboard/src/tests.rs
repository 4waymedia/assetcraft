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
    assert_eq!(place(&mut board, AssetId("x".into()), Some(ClipId(1))), Err(FillError::TitleCard(ClipId(1))));
    assert_eq!(place(&mut board, AssetId("x".into()), Some(ClipId(99))), Err(FillError::NoSuchClip(ClipId(99))));
    assert_eq!(board.assets_assigned(), 19);
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
    assert!(matches!(edit(&|v| v["clips"][0]["assetIds"] = serde_json::json!(["x"])), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|v| v["clips"][1]["duration"] = u64::MAX.into()), Err(BoardError::Limit(_))));
    assert!(matches!(edit(&|v| v["clips"][1]["duration"] = (-1).into()), Err(BoardError::Parse(_))));
    assert!(matches!(edit(&|v| v["track"]["bpm"] = 0.into()), Err(BoardError::Parse(_))));
    assert!(matches!(edit(&|v| v["track"]["beatGrid"] = serde_json::json!([5, 3])), Err(BoardError::Invalid(_))));
    assert!(matches!(edit(&|v| v["clips"][1]["shotType"] = "XL".into()), Err(BoardError::Parse(_))));
    assert!(matches!(edit(&|v| v["clips"][1]["surprise"] = 1.into()), Err(BoardError::Parse(_))));
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
