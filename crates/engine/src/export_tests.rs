//! SF5-4: `storyboard.exportVideo` — the plan, the refusals, a real export of a small Cut to
//! `.mp4` and `.mov`, cancel, and the status.

use super::*;
use crate::launch::RecordingLauncher;
use assetcraft_fs::RealFs;
use assetcraft_storyboard::{AssetId, Clip, ClipId, SceneId, ShotType, Ticks};
use serde_json::json;
use std::path::PathBuf;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetcraft-sf5-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn library(rel: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library").join(rel).to_string_lossy().into_owned()
}

/// A session with a three-clip Cut: a title card, a still with a push-in, a model turntable.
fn session_with_cut(secs: u64) -> Session {
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut s, "storyboard.new", json!({ "name": "Export me" }));
    let board = s.boards.board.as_mut().unwrap();
    board.scenes.push(assetcraft_storyboard::Scene::new(SceneId(1), None));
    let mut card = Clip::new(ClipId(1), SceneId(1), ShotType::Ttl, Ticks::from_secs(secs).unwrap());
    card.description = "Reapers".into();
    let mut still = Clip::new(ClipId(2), SceneId(1), ShotType::Est, Ticks::from_secs(secs).unwrap());
    still.asset_ids.push(AssetId(library("environments/soul_light_forest.jpg")));
    let mut model = Clip::new(ClipId(3), SceneId(1), ShotType::Ms, Ticks::from_secs(secs).unwrap());
    model.asset_ids.push(AssetId(library("props/models/soul_orb_icosphere.glb")));
    board.clips = vec![card, still, model];
    crate::document_cmd::mark_saved(&mut s, None);
    s
}

/// A 16-bit mono WAV at 8 kHz: `secs` seconds of a quiet square wave (as `audio_tests`).
fn wav(secs: u32) -> Vec<u8> {
    let rate = 8_000u32;
    let frames = rate * secs;
    let data = frames * 2;
    let mut b = Vec::new();
    for part in [&b"RIFF"[..], &(36 + data).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes()] {
        b.extend_from_slice(part);
    }
    for v in [1u16, 1] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    for v in [2u16, 16] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        let s: i16 = if (i / 40) % 2 == 0 { 2000 } else { -2000 };
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

fn leftovers(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect()
}

#[test]
fn the_plan_is_checked_before_anything_runs() {
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    let abs = |name: &str| std::env::temp_dir().join(name).to_string_lossy().into_owned();
    assert!(
        matches!(s.execute("storyboard.exportVideo", &json!({ "path": abs("x.mp4") })), Err(CmdError::Unavailable(_))),
        "no board"
    );
    run(&mut s, "storyboard.new", json!({}));
    s.boards.board.as_mut().unwrap().clips.clear();
    assert!(s.enabled("storyboard.exportVideo").is_err(), "no clips");
    assert!(
        matches!(s.execute("storyboard.exportVideo", &json!({ "path": abs("x.mp4") })), Err(CmdError::Unavailable(_))),
        "no clips"
    );
    let mut s = session_with_cut(1);
    assert!(s.enabled("storyboard.exportVideo").is_ok());
    assert!(s.enabled("storyboard.exportCancel").is_err(), "nothing running");
    for (bad, why) in [
        (json!({}), "no path"),
        (json!({ "path": "relative.mp4" }), "relative"),
        (json!({ "path": abs("x.avi") }), "extension"),
        (json!({ "path": abs("x.mp4"), "size": "8k" }), "size name"),
        (json!({ "path": abs("x.mp4"), "size": { "width": 33, "height": 20 } }), "odd size"),
        (json!({ "path": abs("x.mp4"), "fps": 23 }), "fps"),
        (json!({ "path": abs("x.mp4"), "quality": 50 }), "quality"),
    ] {
        assert!(matches!(s.execute("storyboard.exportVideo", &bad), Err(CmdError::BadParams(_))), "{why}: {bad}");
    }
    // A dry run plans and writes nothing.
    let path = abs("plan.mov");
    let plan = run(
        &mut s,
        "storyboard.exportVideo",
        json!({ "path": path, "dryRun": true, "size": "720p", "fps": 30, "quality": 28 }),
    );
    assert_eq!(plan["dryRun"], true);
    assert_eq!(
        (plan["width"].as_u64(), plan["height"].as_u64(), plan["fps"].as_u64()),
        (Some(1280), Some(720), Some(30))
    );
    assert_eq!(plan["container"], "mov");
    assert_eq!(plan["frames"], 90, "3 s at 30 fps");
    assert_eq!(plan["audio"], false);
    assert!(plan["estimatedBytes"].as_u64().unwrap() > 50_000, "{plan}");
    assert!(!Path::new(&path).exists());
    assert_eq!(s.export_progress().stage, ExportStage::Idle);
    // Defaults.
    let plan = run(&mut s, "storyboard.exportVideo", json!({ "path": abs("plan.mp4"), "dryRun": true }));
    assert_eq!(
        (plan["width"].as_u64(), plan["fps"].as_u64(), plan["quality"].as_u64()),
        (Some(1920), Some(24), Some(24))
    );
}

#[test]
fn a_small_cut_exports_to_a_playable_file_and_reports() {
    let dir = temp("export");
    let mut s = session_with_cut(1);
    let path = dir.join("Cut.mp4");
    let out = run(
        &mut s,
        "storyboard.exportVideo",
        json!({ "path": path.to_string_lossy(), "size": { "width": 320, "height": 176 }, "fps": 24, "quality": 30, "wait": true }),
    );
    assert_eq!(out["started"], true);
    assert_eq!(out["status"]["stage"], "done", "{out}");
    assert_eq!(out["status"]["framesDone"], 72);
    assert!(out["message"].as_str().unwrap().starts_with("Exported "), "{out}");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[4..8], b"ftyp");
    assert_eq!(bytes.len() as u64, out["status"]["bytes"].as_u64().unwrap());
    assert!(bytes.windows(4).any(|w| w == b"avc1") && !bytes.windows(4).any(|w| w == b"mp4a"), "video only: no track");
    assert!(leftovers(&dir).is_empty());
    let status = run(&mut s, "storyboard.exportStatus", json!({}));
    assert_eq!(status["stage"], "done");
    assert_eq!(status["problems"].as_array().unwrap().len(), 0);
    // The next estimate for the same settings is what this export measured.
    let again = run(
        &mut s,
        "storyboard.exportVideo",
        json!({ "path": path.to_string_lossy(), "size": { "width": 320, "height": 176 }, "fps": 24, "quality": 30, "dryRun": true }),
    );
    let est = again["estimatedBytes"].as_u64().unwrap();
    assert!((est as i64 - bytes.len() as i64).abs() <= 2, "learned: {est} vs {}", bytes.len());
    assert_eq!(run(&mut s, "storyboard.state", json!({}))["export"]["stage"], "done");
    // Exists now: refused without overwrite, replaced with it (as .mov).
    assert!(matches!(
        s.execute(
            "storyboard.exportVideo",
            &json!({ "path": path.to_string_lossy(), "size": { "width": 320, "height": 176 } })
        ),
        Err(CmdError::Unavailable(_))
    ));
    // With a song: the track plays under the Cut as AAC, as long as the Cut.
    let song = dir.join("song.wav");
    std::fs::write(&song, wav(10)).unwrap();
    s.boards.board.as_mut().unwrap().track = Some(assetcraft_storyboard::Track {
        id: AssetId(song.to_string_lossy().into_owned()),
        name: "song".into(),
        bpm: assetcraft_storyboard::Bpm::DEFAULT,
        duration: Ticks::from_secs(10).unwrap(),
        beat_grid: Vec::new(),
        song_sections: Vec::new(),
        edit: None,
        level_db: 0,
        credit: None,
    });
    let mov = dir.join("Cut.mov");
    std::fs::write(&mov, b"old").unwrap();
    let out = run(
        &mut s,
        "storyboard.exportVideo",
        json!({ "path": mov.to_string_lossy(), "size": { "width": 160, "height": 96 }, "quality": 36, "overwrite": true, "wait": true }),
    );
    assert_eq!(out["audio"], true);
    assert_eq!(out["status"]["stage"], "done", "{out}");
    let bytes = std::fs::read(&mov).unwrap();
    assert_eq!(&bytes[8..12], b"qt  ");
    assert!(bytes.windows(4).any(|w| w == b"mp4a") && bytes.windows(4).any(|w| w == b"esds"), "an audio track");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_broken_asset_is_reported_and_a_cancel_leaves_no_file() {
    let dir = temp("cancel");
    let mut s = session_with_cut(1);
    s.boards.board.as_mut().unwrap().clips[1].asset_ids = vec![AssetId(library("edge-cases/broken_not_an_image.jpg"))];
    let path = dir.join("Broken.mp4");
    let out = run(
        &mut s,
        "storyboard.exportVideo",
        json!({ "path": path.to_string_lossy(), "size": { "width": 160, "height": 96 }, "quality": 36, "wait": true }),
    );
    assert_eq!(out["status"]["stage"], "done");
    let problems = out["status"]["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].as_str().unwrap().contains("broken_not_an_image.jpg"));
    assert!(out["message"].as_str().unwrap().contains("could not be drawn"));

    // A long export, cancelled right away: no file, no temp, the status says so.
    let mut s = session_with_cut(20);
    let path = dir.join("Long.mp4");
    let out = run(
        &mut s,
        "storyboard.exportVideo",
        json!({ "path": path.to_string_lossy(), "size": { "width": 160, "height": 96 } }),
    );
    assert_eq!(out["started"], true);
    assert!(s.export_progress().running());
    assert!(
        matches!(
            s.execute("storyboard.exportVideo", &json!({ "path": path.to_string_lossy() })),
            Err(CmdError::Unavailable(_))
        ),
        "one at a time"
    );
    assert!(s.enabled("storyboard.exportCancel").is_ok());
    run(&mut s, "storyboard.exportCancel", json!({}));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while s.export_progress().running() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let status = run(&mut s, "storyboard.exportStatus", json!({}));
    assert_eq!(status["stage"], "cancelled", "{status}");
    assert!(!path.exists());
    assert!(leftovers(&dir).is_empty());
    assert!(s.enabled("storyboard.exportVideo").is_ok(), "free again");
    let _ = std::fs::remove_dir_all(&dir);
}

/// An MP4 with a sound track, made with our own encoder: `secs` of grey video and a loud tone.
fn talking_mp4(path: &Path, secs: u32) {
    use assetcraft_video::{Container, Settings, VideoEncoder, encode_audio, mux};
    let s = Settings { width: 64, height: 48, fps: 12, qp: 36 };
    let mut enc = VideoEncoder::new(s).unwrap();
    for _ in 0..(secs * 12) {
        enc.push_rgba(&vec![128u8; 64 * 48 * 4]).unwrap();
    }
    let video = enc.finish().unwrap();
    let rate = 48_000u32;
    let pcm: Vec<f32> =
        (0..rate * secs).map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 300.0 / rate as f32).sin() * 0.8).collect();
    let audio = encode_audio(&pcm, 1, rate, 96_000).unwrap();
    let mut out = Vec::new();
    mux(&video, Some(&audio), Container::Mp4, &mut out).unwrap();
    std::fs::write(path, out).unwrap();
}

/// RMS of an exported file's sound between `a` and `b` seconds.
fn rms_between(path: &Path, a: f64, b: f64) -> f64 {
    let audio = assetcraft_audio::load(&RealFs, path).unwrap();
    let (r, ch) = (f64::from(audio.rate()), audio.channel(0));
    let (i, j) = (((a * r) as usize).min(ch.len()), ((b * r) as usize).min(ch.len()));
    (ch[i..j].iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / (j - i).max(1) as f64).sqrt()
}

#[test]
fn an_inset_video_with_mix_audio_is_heard_and_the_music_ducks_under_it() {
    use assetcraft_storyboard::{Inset, InsetKind};
    let dir = temp("voice");
    let song = dir.join("song.wav");
    std::fs::write(&song, wav(10)).unwrap();
    let host = dir.join("host.mp4");
    talking_mp4(&host, 3);
    // The host's own sound decodes (symphonia's MP4 reader).
    assert!(assetcraft_audio::load(&RealFs, &host).unwrap().seconds() > 2.9);
    // Two scenes: scene 1 a 2 s card + 2 s shot (the inset's window: 2–4 s), scene 2 a 2 s shot.
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut s, "storyboard.new", json!({ "name": "Voice" }));
    let board = s.boards.board.as_mut().unwrap();
    board.scenes =
        vec![assetcraft_storyboard::Scene::new(SceneId(1), None), assetcraft_storyboard::Scene::new(SceneId(2), None)];
    let mut card = Clip::new(ClipId(1), SceneId(1), ShotType::Ttl, Ticks::from_secs(2).unwrap());
    card.description = "Step 1".into();
    board.clips = vec![
        card,
        Clip::new(ClipId(2), SceneId(1), ShotType::Ms, Ticks::from_secs(2).unwrap()),
        Clip::new(ClipId(3), SceneId(2), ShotType::Ms, Ticks::from_secs(2).unwrap()),
    ];
    board.track = Some(assetcraft_storyboard::Track {
        id: AssetId(song.to_string_lossy().into_owned()),
        name: "song".into(),
        bpm: assetcraft_storyboard::Bpm::DEFAULT,
        duration: Ticks::from_secs(10).unwrap(),
        beat_grid: Vec::new(),
        song_sections: Vec::new(),
        edit: None,
        level_db: 0,
        credit: None,
    });
    let export = |s: &mut Session, name: &str| -> PathBuf {
        let path = dir.join(name);
        let out = run(
            s,
            "storyboard.exportVideo",
            json!({ "path": path.to_string_lossy(), "size": { "width": 160, "height": 96 }, "quality": 36, "wait": true }),
        );
        assert_eq!(out["status"]["stage"], "done", "{out}");
        assert_eq!(out["audio"], true);
        path
    };
    let plain = export(&mut s, "plain.mp4");
    // The host over scene 1, silent: the music is untouched.
    s.boards.board.as_mut().unwrap().scenes[0].inset =
        Some(Inset { asset: Some(AssetId(host.to_string_lossy().into_owned())), ..Inset::slot(InsetKind::Video) });
    let silent = export(&mut s, "silent.mp4");
    assert!((rms_between(&silent, 2.5, 3.5) - rms_between(&plain, 2.5, 3.5)).abs() < 0.01, "no mix: same sound");
    // Mixed in, music at -40 dB, voice at 0: the window carries the host's loud tone, not the
    // quiet square wave; outside it the music is unchanged.
    run(&mut s, "storyboard.inset", json!({ "scene": 1, "mixAudio": true, "musicDb": -40, "fit": false }));
    let mixed = export(&mut s, "mixed.mp4");
    let (before, under, after) =
        (rms_between(&mixed, 0.3, 1.5), rms_between(&mixed, 2.5, 3.5), rms_between(&mixed, 4.6, 5.8));
    let music = rms_between(&plain, 2.5, 3.5);
    assert!(under > music * 3.0, "the voice is heard: {under} vs music {music}");
    assert!((before - rms_between(&plain, 0.3, 1.5)).abs() < 0.02, "the card keeps its music: {before}");
    assert!((after - rms_between(&plain, 4.6, 5.8)).abs() < 0.02, "scene 2 keeps its music: {after}");
    // Voice down to -24 dB and music back to 0: the window is close to the plain music again.
    run(&mut s, "storyboard.inset", json!({ "scene": 1, "musicDb": 0, "voiceDb": -24 }));
    let quiet = export(&mut s, "quiet.mp4");
    assert!(rms_between(&quiet, 2.5, 3.5) < under * 0.5, "the voice slider works");
    // A level out of range is refused.
    assert!(matches!(s.execute("storyboard.inset", &json!({ "scene": 1, "musicDb": 6 })), Err(CmdError::BadParams(_))));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A 16-bit mono WAV at 8 kHz from samples in −1..1.
fn wav_of(samples: &[f32]) -> Vec<u8> {
    let rate = 8_000u32;
    let data = samples.len() as u32 * 2;
    let mut b = Vec::new();
    for part in [&b"RIFF"[..], &(36 + data).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes()] {
        b.extend_from_slice(part);
    }
    for v in [1u16, 1] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    for v in [2u16, 16] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32_000.0) as i16).to_le_bytes());
    }
    b
}

#[test]
fn a_voiceover_ducks_the_song_only_while_it_speaks() {
    let dir = temp("voiceover");
    let song = dir.join("song.wav");
    std::fs::write(&song, wav(10)).unwrap();
    // Talk 0–1.5 s, silence 1.5–4.5 s, talk 4.5–6 s.
    let talk = |n: usize| (0..n).map(|i| (i as f32 * 0.2).sin() * 0.6).collect::<Vec<f32>>();
    let mut narration = talk(12_000);
    narration.extend(std::iter::repeat_n(0.0, 24_000));
    narration.extend(talk(12_000));
    let vo = dir.join("narration.wav");
    std::fs::write(&vo, wav_of(&narration)).unwrap();
    let mut s = session_with_cut(2);
    s.boards.board.as_mut().unwrap().clips[0].shot_type = ShotType::Ms;
    s.boards.board.as_mut().unwrap().track = Some(assetcraft_storyboard::Track {
        id: AssetId(song.to_string_lossy().into_owned()),
        name: "song".into(),
        bpm: assetcraft_storyboard::Bpm::DEFAULT,
        duration: Ticks::from_secs(10).unwrap(),
        beat_grid: Vec::new(),
        song_sections: Vec::new(),
        edit: None,
        level_db: 0,
        credit: None,
    });
    let export = |s: &mut Session, name: &str| -> PathBuf {
        let path = dir.join(name);
        let out = run(
            s,
            "storyboard.exportVideo",
            json!({ "path": path.to_string_lossy(), "size": { "width": 160, "height": 96 }, "quality": 36, "wait": true }),
        );
        assert_eq!(out["status"]["stage"], "done", "{out}");
        path
    };
    let plain = export(&mut s, "plain.mp4");
    // The command: set, levels, refusals, one undo step.
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.voiceover", json!({ "asset": vo.to_string_lossy(), "musicDb": -40 }));
    assert_eq!(r["changed"], true);
    assert_eq!((r["voiceover"]["musicDb"].as_i64(), r["voiceover"]["voiceDb"].as_i64()), (Some(-40), Some(0)));
    assert!(r["message"].as_str().unwrap().contains("narration.wav"));
    assert_eq!(s.boards.undo_len(), undo + 1);
    assert_eq!(run(&mut s, "storyboard.voiceover", json!({ "musicDb": -40 }))["changed"], false);
    for bad in [json!({ "musicDb": 3 }), json!({ "voiceDb": 40 }), json!({ "asset": "" }), json!({ "asset": 7 })] {
        assert!(matches!(s.execute("storyboard.voiceover", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    let mixed = export(&mut s, "voiceover.mp4");
    let music = rms_between(&plain, 2.2, 3.8);
    assert!(rms_between(&mixed, 0.3, 1.2) > music * 3.0, "the narration is heard over a ducked song");
    assert!((rms_between(&mixed, 2.2, 3.8) - music).abs() < music * 0.15, "the song comes back in the pause");
    assert!(rms_between(&mixed, 4.8, 5.7) > music * 3.0);
    // Remove: the plain song again; levels need a voiceover.
    run(&mut s, "storyboard.voiceover", json!({ "asset": null }));
    assert!(s.storyboard().unwrap().voiceover.is_none());
    assert!(matches!(s.execute("storyboard.voiceover", &json!({ "voiceDb": -6 })), Err(CmdError::Unavailable(_))));
    run(&mut s, "storyboard.undo", json!({}));
    assert!(s.storyboard().unwrap().voiceover.is_some(), "undo brings it back");
    let _ = std::fs::remove_dir_all(&dir);
}
