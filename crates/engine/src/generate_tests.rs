use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::Pacing;
use serde_json::json;

const SR: u64 = 48_000;
const BAR: u64 = 2 * SR; // 120 BPM, 4/4

/// A 32-bar cue map (64 s) with a 30 s fade fit, as `tools/cuemap.py` writes it.
fn cue_json() -> String {
    let sec = |a: u64, b: u64, l: &str, e: f32| json!({"start": a * BAR, "end": b * BAR, "label": l, "energy": e});
    json!({
        "version": 1,
        "analyzer": "test",
        "track": {"file": "song.mp3", "title": "Song", "artist": "Me", "license": "CC-BY-4.0", "credit": "Music: Song by Me"},
        "sampleRate": SR, "durationFrames": 32 * BAR, "bpm": 120.0, "beatsPerBar": 4, "pulseClarity": 0.7,
        "beats": (0..128).map(|i| i * SR / 2).collect::<Vec<_>>(),
        "downbeats": (0..32).map(|i| i * BAR).collect::<Vec<_>>(),
        "sections": [sec(0, 4, "Intro", 0.1), sec(4, 20, "Chorus", 1.0), sec(20, 32, "Outro", 0.3)],
        "hits": [{"frame": 4 * BAR, "kind": "drop", "strength": 0.9}],
        "fits": {"30": {"chosen": "fade", "fade": {
            "mode": "fade", "lengthFrames": 15 * BAR, "segments": [[0, 15 * BAR]], "crossfadeFrames": 0,
            "fadeOut": {"start": 13 * BAR, "frames": 2 * BAR}, "reasons": [], "changes": "Cut to 0:30 with a fade out"}}}
    })
    .to_string()
}

fn session() -> Session {
    let fs = FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .known(KnownFolder::Pictures, "/music")
        .file_with("/music/song.cuemap.json", cue_json().into_bytes())
        .file_with("/music/bad.cuemap.json", b"{\"version\": 1}".to_vec())
        .file("/music/song.mp3", 1000)
        .file("/music/notes.txt", 10);
    Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default())
}

/// The audio path the engine builds for `/music/song.cuemap.json` (Windows joins with `\`).
fn song_mp3() -> String {
    std::path::Path::new("/music").join("song.mp3").to_string_lossy().into_owned()
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn lists_cue_maps_in_the_open_folder_without_reading_them() {
    let mut s = session();
    let r = run(&mut s, "storyboard.cueMaps", json!({}));
    let labels: Vec<&str> = r["cueMaps"].as_array().unwrap().iter().filter_map(|m| m["label"].as_str()).collect();
    assert_eq!(labels, ["bad", "song"]);
}

#[test]
fn generates_a_storyboard_from_a_cue_map_and_regenerates_with_undo() {
    let mut s = session();
    let r = run(&mut s, "storyboard.generate", json!({"cueMap": "/music/song.cuemap.json"}));
    assert_eq!(s.mode(), Mode::Storyboard);
    assert_eq!(r["track"], song_mp3());
    assert_eq!(r["credit"], "Music: Song by Me");
    assert!(r["message"].as_str().unwrap().starts_with("Song · Trailer:"));
    let board = s.storyboard().unwrap().clone();
    assert_eq!(board.total(), board.track.as_ref().unwrap().duration);
    assert_eq!(board.track.as_ref().unwrap().id.0, song_mp3());
    let st = run(&mut s, "storyboard.state", json!({}));
    assert_eq!(st["generatedFrom"]["pacing"], "trailer");
    assert_eq!(st["generatedFrom"]["lengths"], json!([null, 30]));
    let trailer_clips = board.clips.len();

    run(&mut s, "storyboard.generate", json!({"cueMap": "/music/song.cuemap.json", "pacing": "calm", "seconds": 30}));
    let calm = s.storyboard().unwrap();
    assert!(calm.clips.len() < trailer_clips);
    assert_eq!(calm.total().timecode(), "0:30.0");
    assert_eq!(s.generated_from().map(|g| (g.pacing, g.seconds)), Some((Pacing::Calm, Some(30))));
    // Pacing defaults to the last one used.
    run(&mut s, "storyboard.generate", json!({"cueMap": "/music/song.cuemap.json"}));
    assert_eq!(s.generated_from().map(|g| g.pacing), Some(Pacing::Calm));
    // Undo restores the board and what it was generated from.
    run(&mut s, "storyboard.undo", json!({}));
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(s.generated_from().map(|g| (g.pacing, g.seconds)), Some((Pacing::Trailer, None)));
    assert_eq!(s.storyboard().unwrap().clips.len(), trailer_clips);
    // A manual storyboard isn't generated.
    run(&mut s, "storyboard.new", json!({}));
    assert!(s.generated_from().is_none());
}

#[test]
fn refuses_bad_params_and_bad_files_without_an_undo_step() {
    let mut s = session();
    for bad in [
        json!({}),
        json!({"cueMap": "song.cuemap.json"}),
        json!({"cueMap": "/music/song.cuemap.json", "pacing": "frantic"}),
        json!({"cueMap": "/music/song.cuemap.json", "seconds": 1}),
        json!({"cueMap": "/music/song.cuemap.json", "fit": "loop"}),
        json!({"cueMap": "/music/song.cuemap.json", "name": ""}),
    ] {
        assert!(matches!(s.execute("storyboard.generate", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert!(matches!(
        s.execute("storyboard.generate", &json!({"cueMap": "/music/bad.cuemap.json"})),
        Err(CmdError::Unavailable(_))
    ));
    assert!(matches!(
        s.execute("storyboard.generate", &json!({"cueMap": "/music/missing.cuemap.json"})),
        Err(CmdError::Fs(_))
    ));
    assert!(matches!(
        s.execute("storyboard.generate", &json!({"cueMap": "/music/song.cuemap.json", "seconds": 45})),
        Err(CmdError::Unavailable(m)) if m.contains("no 45 s fit")
    ));
    assert!(!s.can_undo_storyboard());
    // Longer than the track: the whole track.
    run(&mut s, "storyboard.generate", json!({"cueMap": "/music/song.cuemap.json", "seconds": 90}));
    assert_eq!(s.storyboard().unwrap().total().timecode(), "1:04.0");
}
