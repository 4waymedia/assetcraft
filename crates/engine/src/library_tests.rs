use super::*;
use assetcraft_fs::FakeFs;
use serde_json::json;

/// A cue map for `<stem>.mp3`: 120 BPM at 8 kHz, 32 bars (64 s), a 30 s fade fit.
fn cue_map(stem: &str, title: &str) -> Vec<u8> {
    let rate = 8_000u64;
    let beat = rate / 2;
    json!({
        "version": 1,
        "analyzer": "test",
        "track": { "file": format!("{stem}.mp3"), "title": title, "artist": "Someone", "license": "CC-BY-4.0", "credit": "" },
        "sampleRate": rate,
        "durationFrames": 64 * rate,
        "bpm": 120.0,
        "beatsPerBar": 4,
        "pulseClarity": 0.9,
        "beats": (0..128).map(|i| i * beat).collect::<Vec<u64>>(),
        "downbeats": (0..32).map(|i| i * 4 * beat).collect::<Vec<u64>>(),
        "sections": [
            { "start": 0, "end": 32 * rate, "label": "Intro", "energy": 0.3 },
            { "start": 32 * rate, "end": 64 * rate, "label": "Chorus", "energy": 0.8 }
        ],
        "hits": [],
        "fits": { "30": { "chosen": "fade", "fade": {
            "mode": "fade", "lengthFrames": 30 * rate, "segments": [[0, 30 * rate]], "crossfadeFrames": 0,
            "fadeOut": { "start": 26 * rate, "frames": 4 * rate }, "reasons": [], "changes": "Cut to 0:30 with a fade out"
        }, "edit": null } }
    })
    .to_string()
    .into_bytes()
}

fn p(dir: &str, name: &str) -> String {
    std::path::Path::new(dir).join(name).to_string_lossy().into_owned()
}

fn session() -> Session {
    let fs = FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .dir(p("/", "music"))
        .file_with(p("/music", "vanguard.mp3"), vec![0; 10])
        .file_with(p("/music", "vanguard.cuemap.json"), cue_map("vanguard", "Vanguard"))
        .file_with(p("/music", "amber.mp3"), vec![0; 10])
        .file_with(p("/music", "broken.cuemap.json"), b"{ not json".to_vec())
        .file_with(p("/music", "notes.txt"), b"hi".to_vec());
    Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default())
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn the_music_folder_lists_analysed_tracks_first_and_is_saved() {
    let mut s = session();
    assert_eq!(run(&mut s, "storyboard.tracks", json!({}))["tracks"], json!([]));
    let r = run(&mut s, "storyboard.musicFolder", json!({ "path": p("/", "music") }));
    let tracks = r["tracks"].as_array().cloned().unwrap_or_default();
    assert_eq!(tracks.len(), 2, "{r}");
    assert_eq!((tracks[0]["title"].as_str(), tracks[0]["analysed"].as_bool()), (Some("Vanguard"), Some(true)));
    assert_eq!(tracks[0]["lengths"], json!([null, 30]));
    assert_eq!((tracks[0]["seconds"].as_f64(), tracks[0]["bpm"].as_f64()), (Some(64.0), Some(120.0)));
    assert_eq!(tracks[0]["audio"], p("/music", "vanguard.mp3"));
    assert_eq!((tracks[1]["title"].as_str(), tracks[1]["analysed"].as_bool()), (Some("amber"), Some(false)));
    assert!(r["message"].as_str().unwrap_or_default().contains("2 tracks"));
    assert_eq!(s.prefs().music_folder, Some(std::path::PathBuf::from(p("/", "music"))));
    // Cleared.
    run(&mut s, "storyboard.musicFolder", json!({ "path": null }));
    assert_eq!(s.music_folder(), None);
}

#[test]
fn the_music_folder_refuses_files_relative_paths_and_missing_folders() {
    let mut s = session();
    for bad in [json!({}), json!({ "path": 3 }), json!({ "path": "music" })] {
        assert!(matches!(s.execute("storyboard.musicFolder", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert!(matches!(
        s.execute("storyboard.musicFolder", &json!({ "path": p("/music", "notes.txt") })),
        Err(CmdError::BadParams(_))
    ));
    assert!(s.execute("storyboard.musicFolder", &json!({ "path": p("/", "nowhere") })).is_err());
    assert_eq!(s.music_folder(), None);
}

#[test]
fn preview_shows_the_structure_without_touching_the_board() {
    let mut s = session();
    let cue = p("/music", "vanguard.cuemap.json");
    let full = run(&mut s, "storyboard.preview", json!({ "cueMap": cue, "pacing": "trailer" }));
    let short = run(&mut s, "storyboard.preview", json!({ "cueMap": cue, "pacing": "trailer", "seconds": 30 }));
    assert_eq!(full["length"], "1:04.0");
    assert_eq!(short["length"], "0:30.0");
    assert!(short["count"].as_u64() < full["count"].as_u64());
    assert_eq!(full["clips"][0]["shot"], "TTL");
    assert!(s.storyboard().is_none(), "nothing opened");
    assert!(matches!(s.execute("storyboard.preview", &json!({ "cueMap": "x.json" })), Err(CmdError::BadParams(_))));
    assert!(s.execute("storyboard.preview", &json!({ "cueMap": p("/music", "broken.cuemap.json") })).is_err());
}

#[test]
fn density_changes_the_clip_count_and_sticks_when_regenerating() {
    let mut s = session();
    let cue = p("/music", "vanguard.cuemap.json");
    let count = |s: &mut Session, d: f64| {
        run(s, "storyboard.preview", json!({ "cueMap": cue, "pacing": "cinematic", "density": d }))["count"]
            .as_u64()
            .unwrap_or(0)
    };
    let (low, mid, high) = (count(&mut s, 0.5), count(&mut s, 1.0), count(&mut s, 2.0));
    assert!(low < mid && mid < high, "{low} {mid} {high}");
    let r = run(&mut s, "storyboard.preview", json!({ "cueMap": cue, "density": 2.0 }));
    assert!(r["avgSeconds"].as_f64().unwrap_or(0.0) > 0.0);
    for bad in [json!(0.1), json!(3), json!("lots")] {
        assert!(
            matches!(
                s.execute("storyboard.preview", &json!({ "cueMap": cue, "density": bad })),
                Err(CmdError::BadParams(_))
            ),
            "{bad}"
        );
    }
    // Generate at 2, then change the length: the density stays.
    run(&mut s, "storyboard.generate", json!({ "cueMap": cue, "pacing": "cinematic", "density": 2.0 }));
    let st = run(&mut s, "storyboard.state", json!({}));
    assert_eq!(st["generatedFrom"]["density"], 2.0);
    let dense = s.storyboard().map(|b| b.clips.len()).unwrap_or(0);
    run(&mut s, "storyboard.generate", json!({ "cueMap": cue, "seconds": 30 }));
    assert_eq!(run(&mut s, "storyboard.state", json!({}))["generatedFrom"]["density"], 2.0);
    assert!(dense as u64 == high, "full length at 2 matches the preview");
}

#[test]
fn tracks_carry_band_energy_sections_and_suggestions_from_the_cue_map() {
    let mut s = session();
    let r = run(&mut s, "storyboard.musicFolder", json!({ "path": p("/", "music") }));
    let t = &r["tracks"][0];
    assert_eq!(t["band"], "upbeat", "120 BPM");
    assert_eq!(t["energy"].as_array().map(Vec::len), Some(crate::library_cmd::ENERGY_BARS));
    assert_eq!((t["energy"][0].as_f64(), t["energy"][27].as_f64()), (Some(0.3), Some(0.8)), "Intro then Chorus");
    assert_eq!(t["sections"][1]["label"], "Chorus");
    assert_eq!(t["sections"][1]["start"], 32.0);
    assert_eq!(t["suits"], json!(["Trailer", "Cinematic", "Family", "Hero"]), "card order");
    assert!(
        t["analysis"]
            .as_str()
            .unwrap_or_default()
            .starts_with("120.0 BPM, upbeat tempo. Suits Trailer, Cinematic, Family, Hero."),
        "{t}"
    );
    assert_eq!(crate::library_cmd::tempo_band(99.9), "slow");
    assert_eq!(crate::library_cmd::tempo_band(135.0), "fast");
}

#[test]
fn shape_overrides_reach_preview_and_generate_and_stick_when_regenerating() {
    let mut s = session();
    let cue = p("/music", "vanguard.cuemap.json");
    let titles = |s: &mut Session, shape: Value| {
        run(s, "storyboard.preview", json!({ "cueMap": cue, "pacing": "trailer", "shape": shape }))["titleCards"]
            .as_u64()
            .unwrap_or(99)
    };
    let every = titles(&mut s, json!({}));
    assert_eq!(titles(&mut s, json!({ "titles": "none", "endCard": false })), 0);
    assert_eq!(titles(&mut s, json!({ "titles": "opening", "endCard": true })), 2);
    assert!(every > 2);
    let r = run(&mut s, "storyboard.preview", json!({ "cueMap": cue }));
    assert!(
        r["avgBeats"].as_f64().unwrap_or(0.0) > r["avgSeconds"].as_f64().unwrap_or(0.0),
        "fast track: beats > seconds"
    );
    for bad in [json!({ "titles": "sometimes" }), json!({ "wipe": true }), json!("cut")] {
        assert!(
            matches!(
                s.execute("storyboard.preview", &json!({ "cueMap": cue, "shape": bad })),
                Err(CmdError::BadParams(_))
            ),
            "{bad}"
        );
    }
    // Generate with a shape, then change only the length: the shape stays.
    let shape = json!({ "titles": "opening", "transition": "dip" });
    run(&mut s, "storyboard.generate", json!({ "cueMap": cue, "pacing": "trailer", "shape": shape }));
    assert_eq!(run(&mut s, "storyboard.state", json!({}))["generatedFrom"]["shape"], shape);
    run(&mut s, "storyboard.generate", json!({ "cueMap": cue, "seconds": 30 }));
    assert_eq!(run(&mut s, "storyboard.state", json!({}))["generatedFrom"]["shape"], shape);
    // An explicit empty shape goes back to the style's own choices.
    run(&mut s, "storyboard.generate", json!({ "cueMap": cue, "shape": {} }));
    assert_eq!(run(&mut s, "storyboard.state", json!({}))["generatedFrom"]["shape"], json!({}));
    // Family and Hero are styles like the others.
    for style in ["family", "hero"] {
        assert!(titles_for(&mut s, &cue, style) > 0, "{style}");
    }
}

fn titles_for(s: &mut Session, cue: &str, pacing: &str) -> u64 {
    run(s, "storyboard.preview", json!({ "cueMap": cue, "pacing": pacing }))["count"].as_u64().unwrap_or(0)
}

#[test]
fn preview_clips_carry_times_text_and_transitions_and_the_edit_is_in_track_time() {
    let mut s = session();
    let cue = p("/music", "vanguard.cuemap.json");
    let full = run(&mut s, "storyboard.preview", json!({ "cueMap": cue, "pacing": "calm" }));
    let clips = full["clips"].as_array().cloned().unwrap_or_default();
    assert!(clips.len() > 2);
    assert_eq!(clips[0]["start"], 0.0);
    assert_eq!(clips[0]["text"], "card", "the opening title");
    // Starts run on: each clip starts where the last one ended.
    for w in clips.windows(2) {
        let end = w[0]["start"].as_f64().unwrap_or(0.0) + w[0]["seconds"].as_f64().unwrap_or(0.0);
        assert!((w[1]["start"].as_f64().unwrap_or(0.0) - end).abs() < 0.02, "{w:?}");
    }
    assert!(clips.iter().any(|c| c["in"] == "Fade in"), "Calm dips to black between scenes");
    assert_eq!(full["trackSeconds"], 64.0);
    assert_eq!(full["edit"]["segments"], json!([[0.0, 64.0]]), "the whole track, no edit");
    // A fit to 30 s: the edit is in track seconds.
    let cut = run(&mut s, "storyboard.preview", json!({ "cueMap": cue, "seconds": 30 }));
    let segs = cut["edit"]["segments"].as_array().cloned().unwrap_or_default();
    assert!(!segs.is_empty());
    let kept: f64 = segs.iter().map(|s| s[1].as_f64().unwrap_or(0.0) - s[0].as_f64().unwrap_or(0.0)).sum();
    assert!((kept - 30.0).abs() < 1.0, "{segs:?}");
}
