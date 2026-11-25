//! SF1b-2: `storyboard.writeOtio` — the Cut as OTIO, read back by FilmCraft's own importer.

use super::*;
use crate::launch::RecordingLauncher;
use assetcraft_fs::RealFs;
use assetcraft_storyboard::{AssetId, Clip, ClipId, SceneId, ShotType, TICKS_PER_SECOND, Ticks, TrackEdit};
use serde_json::json;
use std::path::{Path, PathBuf};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetcraft-otio-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn library(rel: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library").join(rel).to_string_lossy().into_owned()
}

/// A saved 160 × 96 board at 24 fps in `dir` ("Hollow Saint"): a 1.3 s title card, a 1.25 s
/// video from 0.5 s in, a still with an overlay, an empty clip — lengths off the frame grid.
fn saved(dir: &Path) -> Session {
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut s, "storyboard.new", json!({ "name": "Hollow Saint" }));
    let board = s.boards.board.as_mut().unwrap();
    board.frame = Some(assetcraft_storyboard::FrameSpec {
        width: 160,
        height: 96,
        fps: 24,
        ..assetcraft_storyboard::FrameSpec::default()
    });
    board.scenes = vec![assetcraft_storyboard::Scene::new(SceneId(1), None)];
    let mut card = Clip::new(ClipId(1), SceneId(1), ShotType::Ttl, Ticks::from_secs_f64(1.3).unwrap());
    card.description = "Some saints were never saved".into();
    let mut video = Clip::new(ClipId(2), SceneId(1), ShotType::Est, Ticks::from_secs_f64(1.25).unwrap());
    video.asset_ids.push(AssetId(library("environments/snow_clearing_flythrough.mp4")));
    video.source_in = Ticks::from_secs_f64(0.5).unwrap();
    let mut over = Clip::new(ClipId(3), SceneId(1), ShotType::Ws, Ticks::from_secs_f64(1.11).unwrap());
    over.asset_ids.push(AssetId(library("environments/soul_light_forest.jpg")));
    over.overlay = Some("Over the shot".into());
    let empty = Clip::new(ClipId(4), SceneId(1), ShotType::Cu, Ticks::from_secs_f64(0.7).unwrap());
    board.clips = vec![card, video, over, empty];
    let path = dir.join("Hollow Saint.storyboard.json");
    run(&mut s, "storyboard.saveAs", json!({ "path": path.to_string_lossy() }));
    s
}

#[test]
fn a_file_url_is_what_filmcraft_decodes() {
    use crate::otio_out::file_url;
    assert_eq!(
        file_url(Path::new(r"C:\Users\Paul\Documents\AssetCraft\Hollow Saint.media\titles\c1-9f2a.mp4")),
        "file:///C:/Users/Paul/Documents/AssetCraft/Hollow%20Saint.media/titles/c1-9f2a.mp4"
    );
    assert_eq!(file_url(Path::new("/home/p/a b#c.wav")), "file:///home/p/a%20b%23c.wav");
    assert_eq!(file_url(Path::new("/tmp/é.mp4")), "file:///tmp/%C3%A9.mp4");
}

#[test]
fn the_cuts_clips_are_whole_frames_back_to_back() {
    let dir = temp("spans");
    let s = saved(&dir);
    let board = s.boards.board.as_ref().unwrap();
    let spans = crate::title_render::cut_spans(board, 24);
    let firsts: Vec<u64> = spans.iter().map(|x| x.first).collect();
    let frames: Vec<u64> = spans.iter().map(|x| x.frames).collect();
    // 0, 1.3, 2.55, 3.66, 4.36 s at 24 fps: frames 0, 31, 61, 88, 105.
    assert_eq!(firsts, [0, 31, 61, 88]);
    assert_eq!(frames, [31, 30, 27, 17]);
    assert_eq!(spans.windows(2).filter(|w| w[0].first + w[0].frames != w[1].first).count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dry_run_shows_the_timeline_and_writing_waits_for_the_titles() {
    let dir = temp("dry");
    let mut s = saved(&dir);
    let p = run(&mut s, "storyboard.writeOtio", json!({ "dryRun": true }));
    assert_eq!(p["path"], json!(dir.join("Hollow Saint.otio").to_string_lossy()));
    assert_eq!((p["video"].clone(), p["audio"].clone(), p["markers"].clone()), (json!(4), json!(0), json!(3)));
    assert_eq!(p["unrendered"], json!([1, 3]));
    let v1 = &p["otio"]["tracks"]["children"][0]["children"];
    assert_eq!(v1[0]["name"], json!("TITLE · Some saints were never saved"));
    assert!(
        v1[0]["media_references"]["DEFAULT_MEDIA"]["target_url"]
            .as_str()
            .unwrap()
            .contains("Hollow%20Saint.media/titles/c1-")
    );
    assert_eq!(v1[1]["source_range"]["start_time"]["value"], json!(12), "0.5 s in, at 24 fps");
    assert_eq!(v1[1]["metadata"]["filmcraft"]["source_in_ticks"], json!(Ticks::from_secs_f64(0.5).unwrap().0));
    assert_eq!(v1[2]["name"], json!("#03 WS · Over the shot"));
    assert!(
        v1[2]["media_references"]["DEFAULT_MEDIA"]["target_url"].as_str().unwrap().contains("/titles/c3-"),
        "the overlay merged"
    );
    assert_eq!(v1[3]["OTIO_SCHEMA"], json!("Gap.1"));
    let mark = &v1[0]["markers"][0];
    assert_eq!((mark["color"].clone(), mark["name"].clone()), (json!("PURPLE"), json!("Some saints were never saved")));
    assert!(mark["comment"].as_str().unwrap().starts_with("assetcraft clip 1 · "), "{mark}");
    assert!(!dir.join("Hollow Saint.otio").exists(), "a dry run writes nothing");
    let err = s.execute("storyboard.writeOtio", &json!({})).unwrap_err().to_string();
    assert!(err.contains("renderTitles"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The file AssetCraft writes, read by FilmCraft's own importer (as SF1b-0 read the fixture): no
/// warnings, the clips where the Cut has them to the tick, the video from its in-point, the song
/// through its edit, the markers and their keys.
#[test]
fn filmcraft_reads_the_file_as_written() {
    let dir = temp("read");
    let mut s = saved(&dir);
    // A song cut down to two stretches: 0–2 s and 5–8 s of the file.
    let song = dir.join("song.wav");
    std::fs::write(&song, crate::export_tests::wav(10)).unwrap();
    s.boards.board.as_mut().unwrap().track = Some(assetcraft_storyboard::Track {
        id: AssetId(song.to_string_lossy().into_owned()),
        name: "song".into(),
        bpm: assetcraft_storyboard::Bpm::DEFAULT,
        duration: Ticks::from_secs(10).unwrap(),
        beat_grid: Vec::new(),
        song_sections: Vec::new(),
        edit: Some(TrackEdit {
            sample_rate: 44_100,
            segments: vec![[0, 2 * 44_100], [5 * 44_100, 8 * 44_100]],
            crossfade_frames: 441,
            fade_out: None,
            length_frames: 5 * 44_100,
        }),
        downbeats: Vec::new(),
        hits: Vec::new(),
        volume: Vec::new(),
        level_db: -3,
        title_db: assetcraft_storyboard::TITLE_DB_DEFAULT,
        credit: None,
    });
    run(&mut s, "storyboard.renderTitles", json!({ "wait": true }));
    let out = run(&mut s, "storyboard.writeOtio", json!({}));
    assert_eq!(out["written"], json!(true), "{out}");
    assert!(out["notes"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("crossfades")), "{out}");
    let path = dir.join("Hollow Saint.otio");
    let bytes = std::fs::read(&path).unwrap();
    let (imported, report) =
        filmcraft_interchange::import(&bytes, filmcraft_interchange::Format::Otio, None).expect("FilmCraft imports it");
    assert!(report.entries.is_empty(), "{report:?}");
    assert_eq!(imported.sequences.len(), 1);
    let seq = imported.project.sequence(imported.sequences[0]).expect("a sequence");
    let v1 = &seq.video_tracks[0].items;
    assert_eq!(v1.len(), 3, "three clips and a gap: {:?}", v1.iter().map(|i| &i.name).collect::<Vec<_>>());
    let board = s.boards.board.as_ref().unwrap();
    let starts = board.starts();
    let tick = TICKS_PER_SECOND / 24;
    // Each starts on its whole frame and runs to the next one's: the Cut's frames.
    let spans = crate::title_render::cut_spans(board, 24);
    for (item, i) in v1.iter().zip(0..) {
        assert_eq!(item.start.0 as u64, spans[i].first * tick, "{}", item.name);
        assert_eq!(item.duration.0 as u64, spans[i].frames * tick, "{}", item.name);
        assert!((starts[i].0 as i64 - item.start.0).unsigned_abs() < tick, "{} within a frame of the Cut", item.name);
    }
    assert_eq!(v1[1].source_in.0 as u64, Ticks::from_secs_f64(0.5).unwrap().0, "the video from its in-point");
    let keys: Vec<&str> = v1.iter().flat_map(|i| i.markers.iter().map(|m| m.comment.as_str())).collect();
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert!(keys[0].starts_with("assetcraft clip 1 · ") && keys[1].starts_with("assetcraft clip 3 · "), "{keys:?}");
    assert_eq!(seq.markers.len(), 1, "the scene");
    let a1 = &seq.audio_tracks[0].items;
    assert_eq!(a1.len(), 2, "the song's two stretches");
    assert_eq!(a1[1].source_in.0 as u64, 5 * TICKS_PER_SECOND, "the second from 5 s into the file");
    assert!((a1[0].gain_db + 3.0).abs() < 1e-9, "the song's level");
    // Again: our own file is replaced; a path of the user's needs `overwrite`.
    run(&mut s, "storyboard.writeOtio", json!({}));
    let other = dir.join("mine.otio");
    std::fs::write(&other, b"{}").unwrap();
    assert!(s.execute("storyboard.writeOtio", &json!({ "path": other.to_string_lossy() })).is_err());
    run(&mut s, "storyboard.writeOtio", json!({ "path": other.to_string_lossy(), "overwrite": true }));
    let _ = std::fs::remove_dir_all(&dir);
}
