use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::Pacing;
use assetcraft_storyboard::Ticks;
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

#[test]
fn a_template_with_a_cue_map_takes_the_song_as_its_track_and_snaps_to_its_beat() {
    let mut s = session();
    // The whole song (64 s at 120 BPM) is the track; the template keeps its own length — three
    // 30 s points plus the hook, "what it is" and verdict — with every shot on the beat (SR2,
    // Paul: "a 'title' clip every 30 seconds… and then the chorus music between each title").
    let own = assetcraft_storyboard::bundled_template("review").unwrap().total();
    let out = run(&mut s, "storyboard.new", json!({ "template": "review", "cueMap": "/music/song.cuemap.json" }));
    let board = s.storyboard().unwrap();
    let track = board.track.as_ref().expect("the song is the track");
    assert_eq!(track.name, "Song");
    assert_eq!(track.id.0, song_mp3());
    assert_eq!(track.bpm.get(), 120.0);
    assert_eq!(board.total(), own, "the template's own length, not the song's");
    // SR3: the song's chorus loops under the whole Cut by default, so the track is as long as
    // the Cut (this song is 64 s, the Cut about two minutes).
    assert_eq!(track.duration, board.total(), "the loop runs the length of the Cut");
    assert_eq!(out["fitted"]["music"], "the chorus looped");
    let edit = track.edit.as_ref().expect("the loop is an edit");
    assert!(edit.segments.len() >= 3, "{:?}", edit.segments);
    assert!(track.song_sections.iter().all(|sc| sc.name == "Chorus"), "{:?}", track.song_sections);
    assert!(edit.fade_out.is_some());
    // The whole song instead: 64 s from the top, no edit.
    run(
        &mut s,
        "storyboard.new",
        json!({ "template": "review", "cueMap": "/music/song.cuemap.json", "music": "whole" }),
    );
    let board = s.storyboard().unwrap();
    let track = board.track.as_ref().unwrap();
    assert_eq!(track.duration, Ticks::from_secs(64).unwrap(), "the whole song");
    assert!(track.edit.is_none());
    assert_eq!(track.song_sections.len(), 3);
    assert!(matches!(
        s.execute(
            "storyboard.new",
            &json!({ "template": "review", "cueMap": "/music/song.cuemap.json", "music": "loud" })
        ),
        Err(CmdError::BadParams(_))
    ));
    let out = run(&mut s, "storyboard.new", json!({ "template": "review", "cueMap": "/music/song.cuemap.json" }));
    let board = s.storyboard().unwrap();
    let track = board.track.as_ref().unwrap();
    let beat = track.bpm.beat();
    for c in board.clips.iter().filter(|c| c.shot_type != assetcraft_storyboard::ShotType::Ttl) {
        assert_eq!(c.duration.0 % beat.0, 0, "shot #{} on the beat", c.id.0);
    }
    assert_eq!(board.name, "Song · Review");
    assert!(!board.template && board.assets_assigned() == 0);
    assert_eq!(out["fitted"]["fits"], true);
    assert_eq!(
        board.clips.iter().filter(|c| c.shot_type == assetcraft_storyboard::ShotType::Ttl).count(),
        5,
        "hook, three points, end card"
    );
    // Another step length: 20 s steps, named.
    run(
        &mut s,
        "storyboard.new",
        json!({ "template": "tutorial", "cueMap": "/music/song.cuemap.json", "stepSeconds": 20, "name": "Lamp" }),
    );
    let board = s.storyboard().unwrap();
    assert_eq!(board.name, "Lamp");
    let twenty = assetcraft_storyboard::bundled_template_sized("tutorial", 3, 20.0).unwrap().total();
    assert_eq!(board.total(), twenty);
    assert!(matches!(
        s.execute("storyboard.new", &json!({ "template": "tutorial", "stepSeconds": 5 })),
        Err(CmdError::BadParams(_))
    ));
    // Generate with credits: the credits card closes the Cut with the song's credit line.
    run(&mut s, "storyboard.generate", json!({ "cueMap": "/music/song.cuemap.json", "credits": true }));
    let board = s.storyboard().unwrap();
    let last = board.clips.last().unwrap();
    assert!(last.description.ends_with("Music: Song by Me\nMade with AssetCraft"), "{}", last.description);
    assert_eq!(board.total(), board.track.as_ref().unwrap().duration, "the credits sit inside the song");
    // A bad cue map is refused before anything changes.
    assert!(s.execute("storyboard.new", &json!({ "template": "review", "cueMap": "/music/bad.cuemap.json" })).is_err());
    assert_eq!(s.storyboard().unwrap().name, "Song · Trailer");
}
