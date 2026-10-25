//! `storyboard.analyse` end to end on real files: a song in a music folder, analysed in the
//! background into the analysis folder, then listed as analysed and usable for a storyboard.

use super::*;
use crate::launch::RecordingLauncher;
use assetcraft_fs::RealFs;
use serde_json::json;
use std::f32::consts::PI;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A 16-bit mono 22,050 Hz WAV: clicks at 120 BPM from 0.25 s, a 60 Hz bass from `bass_from` s.
fn song_wav(secs: f32, bass_from: f32) -> Vec<u8> {
    let rate = 22_050u32;
    let n = (secs * rate as f32) as usize;
    let mut x = vec![0.0f32; n];
    let mut t = 0.25f32;
    while t < secs - 0.1 {
        let at = (t * rate as f32) as usize;
        for i in 0..(0.005 * rate as f32) as usize {
            if let Some(v) = x.get_mut(at + i) {
                *v = 0.8 * (-(i as f32) / 30.0).exp() * (2.0 * PI * 2000.0 * i as f32 / rate as f32).sin();
            }
        }
        t += 0.5;
    }
    for (i, v) in x.iter_mut().enumerate().skip((bass_from * rate as f32) as usize) {
        *v += 0.4 * (2.0 * PI * 60.0 * i as f32 / rate as f32).sin();
    }
    let data = (n * 2) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for v in x {
        b.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32_000.0) as i16).to_le_bytes());
    }
    b
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetcraft-analyse-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn wait_idle(s: &Session) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while s.analyse_progress().pending > 0 {
        assert!(Instant::now() < deadline, "analyser never finished");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn analyse_stores_cue_maps_outside_the_music_folder_and_the_tracks_become_usable() {
    let music = temp("music");
    let store = temp("store").join("analysis");
    std::fs::write(music.join("Song One.wav"), song_wav(70.0, 35.0)).unwrap();
    std::fs::write(music.join("broken.mp3"), b"not audio at all").unwrap();
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default())
        .with_analysis_dir(store.clone());
    run(&mut s, "storyboard.musicFolder", json!({ "path": music.to_string_lossy() }));
    let before = run(&mut s, "storyboard.tracks", json!({}));
    assert_eq!(before["tracks"].as_array().unwrap().len(), 2);
    assert!(before["tracks"].as_array().unwrap().iter().all(|t| t["analysed"] == false && t["status"] == "new"));
    assert_eq!(before["analysis"]["available"], true);

    // Analyse all: both queued, the bad one fails with a reason, the good one is stored.
    let r = run(&mut s, "storyboard.analyse", json!({}));
    assert_eq!(r["queued"], 2, "{r}");
    let again = run(&mut s, "storyboard.analyse", json!({}));
    assert!(again["queued"] == 0 || s.analyse_progress().pending == 0, "already queued tracks aren't queued twice");
    wait_idle(&s);
    let p = s.analyse_progress();
    assert_eq!((p.pending, p.done, p.failed), (0, 1, 1));
    assert!(p.batches >= 1);

    // Nothing was written beside the music; the store has the cue map and its source.
    let mut in_music: Vec<String> =
        std::fs::read_dir(&music).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    in_music.sort();
    assert_eq!(in_music, ["Song One.wav", "broken.mp3"]);
    let stored: Vec<String> =
        std::fs::read_dir(&store).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(stored.iter().filter(|n| n.ends_with(".cuemap.json")).count(), 1, "{stored:?}");
    assert_eq!(stored.iter().filter(|n| n.ends_with(".source")).count(), 1, "{stored:?}");
    assert!(stored.iter().all(|n| !n.ends_with(".tmp")));

    let after = run(&mut s, "storyboard.tracks", json!({}));
    let tracks = after["tracks"].as_array().unwrap();
    let good = tracks.iter().find(|t| t["title"] == "Song One").unwrap();
    assert_eq!(good["analysed"], true, "{good}");
    assert_eq!(good["audio"], json!(music.join("Song One.wav").to_string_lossy()));
    assert!(good["cueMap"].as_str().unwrap().starts_with(&*store.to_string_lossy()));
    assert!((good["bpm"].as_f64().unwrap() - 120.0).abs() < 1.5, "{good}");
    let bad = tracks.iter().find(|t| t["title"] == "broken").unwrap();
    assert_eq!((&bad["analysed"], &bad["status"]), (&json!(false), &json!("failed")));
    assert!(!bad["error"].as_str().unwrap_or("").is_empty());

    // The stored cue map drives a storyboard, with the real song as its track.
    let cue = good["cueMap"].as_str().unwrap().to_owned();
    let preview = run(&mut s, "storyboard.preview", json!({ "cueMap": cue, "seconds": 60 }));
    assert!(preview["count"].as_u64().unwrap() > 4, "{preview}");
    run(&mut s, "storyboard.generate", json!({ "cueMap": cue, "seconds": 30 }));
    let state = run(&mut s, "storyboard.state", json!({}));
    assert!(state.to_string().contains("Song One.wav"), "the board's track is the song, not the store");

    // A tampered .source (pointing at another file) is not trusted.
    let source = store.join(stored.iter().find(|n| n.ends_with(".source")).unwrap());
    std::fs::write(&source, "/etc/passwd").unwrap();
    let tampered = run(&mut s, "storyboard.tracks", json!({}));
    let row = tampered["tracks"].as_array().unwrap().iter().find(|t| t["title"] == "Song One").cloned().unwrap();
    assert_eq!(row["analysed"], false, "falls back to not analysed: {row}");

    let _ = std::fs::remove_dir_all(&music);
    let _ = std::fs::remove_dir_all(store.parent().unwrap());
}

#[test]
fn analyse_needs_a_store_and_checks_its_params() {
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    assert!(matches!(s.execute("storyboard.analyse", &json!({})), Err(CmdError::Unavailable(_))));
    assert_eq!(run(&mut s, "storyboard.tracks", json!({}))["analysis"]["available"], false);
    let mut s = s.with_analysis_dir(temp("params"));
    for bad in [json!({ "paths": "x.wav" }), json!({ "paths": ["relative.wav"] }), json!({ "paths": [3] })] {
        assert!(matches!(s.execute("storyboard.analyse", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert_eq!(run(&mut s, "storyboard.analyse", json!({ "paths": [] }))["queued"], 0);
}
