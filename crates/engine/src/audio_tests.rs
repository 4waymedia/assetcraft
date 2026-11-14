use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::Ticks;
use serde_json::json;
use std::time::Duration;

/// A 16-bit mono WAV at 8 kHz: `secs` seconds of a quiet square wave.
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

/// A cue map for `track.wav`: 120 BPM at 8 kHz, 32 bars (64 s), one section, a 10 s fade fit.
fn cue_map() -> String {
    let rate = 8_000u64;
    let beat = rate / 2;
    let beats: Vec<u64> = (0..128).map(|i| i * beat).collect();
    let bars: Vec<u64> = (0..32).map(|i| i * 4 * beat).collect();
    json!({
        "version": 1,
        "analyzer": "test",
        "track": { "file": "track.wav", "title": "Test", "artist": "Me", "license": "CC0-1.0", "credit": "" },
        "sampleRate": rate,
        "durationFrames": 64 * rate,
        "bpm": 120.0,
        "beatsPerBar": 4,
        "pulseClarity": 0.9,
        "beats": beats,
        "downbeats": bars,
        "sections": [{ "start": 0, "end": 64 * rate, "label": "Song", "energy": 0.5 }],
        "hits": [],
        "fits": { "30": { "chosen": "fade", "fade": {
            "mode": "fade", "lengthFrames": 10 * rate, "segments": [[0, 10 * rate]], "crossfadeFrames": 0,
            "fadeOut": { "start": 8 * rate, "frames": 2 * rate }, "reasons": [], "changes": "Cut to 0:10 with a fade out"
        }, "edit": null } }
    })
    .to_string()
}

fn music(name: &str) -> String {
    std::path::Path::new("/music").join(name).to_string_lossy().into_owned()
}

fn session(player: Arc<RecordingPlayer>, with_track: bool) -> Session {
    let mut fs = FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .file_with(music("track.cuemap.json"), cue_map().into_bytes());
    if with_track {
        fs = fs.file_with(music("track.wav"), wav(64));
    }
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    s.set_player(player);
    s
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn play_starts_the_fitted_track_at_the_playhead_and_pause_stops_it() {
    let player = Arc::new(RecordingPlayer::default());
    let mut s = session(Arc::clone(&player), true);
    run(&mut s, "storyboard.generate", json!({ "cueMap": music("track.cuemap.json"), "seconds": 30 }));
    // The window's first frame starts the decode (a real player was given).
    s.poll_audio(Ticks::ZERO);
    assert!(s.wait_track_audio(Duration::from_secs(10)));
    assert_eq!(s.audio_status(), "ready");
    let r = run(&mut s, "storyboard.play", json!({ "from": 2.0 }));
    assert_eq!(r["audio"], "playing");
    assert!(r.get("message").is_none());
    // 10 s fade fit: the program is the edit, not the 32 s file.
    assert_eq!(player.log(), ["play 10.0s from 2.0"]);
    assert_eq!(s.audio_clock(), Ticks::from_secs_f64(2.0));
    // Frames keep the same program playing; no restarts.
    s.poll_audio(Ticks::from_secs_f64(2.5).unwrap_or(Ticks::ZERO));
    assert_eq!(player.log().len(), 1);
    run(&mut s, "storyboard.pause", json!({ "at": 3.0 }));
    assert_eq!(player.log(), ["play 10.0s from 2.0", "stop"]);
    assert_eq!(s.audio_clock(), None);
    assert_eq!(run(&mut s, "storyboard.state", json!({}))["transport"]["audio"], "ready");
}

#[test]
fn a_cut_that_plays_before_the_decode_finishes_brings_the_music_in_later() {
    let player = Arc::new(RecordingPlayer::default());
    let mut s = session(Arc::clone(&player), true);
    run(&mut s, "storyboard.generate", json!({ "cueMap": music("track.cuemap.json") }));
    // No poll yet, so play starts the decode itself.
    let r = run(&mut s, "storyboard.play", json!({}));
    let started_now = player.log().len() == 1;
    if !started_now {
        assert_eq!(r["message"], "Loading the track…");
        assert!(s.wait_track_audio(Duration::from_secs(10)));
        s.poll_audio(Ticks::from_secs_f64(1.5).unwrap_or(Ticks::ZERO));
        assert_eq!(player.log(), ["play 64.0s from 1.5"], "music joins at the playhead");
    }
    // Undo (back to no board) stops it; the poll after that has nothing to play.
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(player.log().last().map(String::as_str), Some("stop"));
    s.poll_audio(Ticks::ZERO);
    assert_eq!(s.audio_status(), "none");
}

#[test]
fn a_missing_track_plays_silent_and_says_why() {
    let player = Arc::new(RecordingPlayer::default());
    let mut s = session(Arc::clone(&player), false);
    run(&mut s, "storyboard.generate", json!({ "cueMap": music("track.cuemap.json") }));
    s.poll_audio(Ticks::ZERO);
    assert!(s.wait_track_audio(Duration::from_secs(10)));
    let r = run(&mut s, "storyboard.play", json!({}));
    assert!(r["message"].as_str().unwrap_or_default().starts_with("Playing without music"), "{r}");
    assert!(r["audio"].as_str().unwrap_or_default().starts_with("failed:"), "{r}");
    assert!(player.log().is_empty());
    assert!(s.transport().playing, "the Cut still plays");
}

#[test]
fn without_a_player_the_session_never_decodes() {
    let fs = FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .file_with(music("track.cuemap.json"), cue_map().into_bytes());
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    run(&mut s, "storyboard.generate", json!({ "cueMap": music("track.cuemap.json") }));
    s.poll_audio(Ticks::ZERO);
    assert_eq!(s.audio_status(), "idle");
    // The sample board has a stand-in track with no file: nothing to play.
    run(&mut s, "storyboard.new", json!({ "sample": true }));
    assert_eq!(s.audio_status(), "none");
    assert_eq!(run(&mut s, "storyboard.play", json!({}))["audio"], "none");
}

#[test]
fn preview_plays_the_whole_track_and_the_cut_takes_over() {
    let player = Arc::new(RecordingPlayer::default());
    let mut s = session(Arc::clone(&player), true);
    let r = run(&mut s, "track.preview", json!({ "path": music("track.wav"), "from": 4.0 }));
    if r["preview"] == "loading" {
        // Decode on its thread, then the window's next frame starts it.
        for _ in 0..2000 {
            s.poll_audio(Ticks::ZERO);
            if player.log().len() == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    assert_eq!(player.log(), ["play 64.0s from 4.0"], "the whole file, not a fit");
    let quiet = crate::audio_out::volume_gain(crate::audio_out::DEFAULT_PREVIEW_VOLUME);
    assert_eq!(player.gain(), Some(quiet), "previews start quiet");
    run(&mut s, "track.previewVolume", json!({ "volume": 0.5 }));
    assert_eq!(player.gain(), Some(0.25), "changes at once while playing");
    assert_eq!(s.prefs().preview_volume, Some(50), "saved, in percent");
    assert!(matches!(s.execute("track.previewVolume", &json!({ "volume": 2 })), Err(CmdError::BadParams(_))));
    assert_eq!(s.preview_state().map(|(_, loading)| loading), Some(false));
    assert_eq!(s.preview_position(), Some(4.0), "seconds into the track (the section strip's playhead)");
    // Jump to another section: a new `from` replaces the playing preview.
    run(&mut s, "track.preview", json!({ "path": music("track.wav"), "from": 32.0 }));
    assert_eq!(player.log().last().map(String::as_str), Some("play 64.0s from 32.0"), "decoded once, starts at once");
    assert_eq!(s.preview_position(), Some(32.0));
    // Stop with null.
    run(&mut s, "track.preview", json!({ "path": null }));
    assert_eq!(player.log().last().map(String::as_str), Some("stop"));
    assert!(s.preview_state().is_none());
    assert_eq!(s.preview_position(), None);
    // Preview again, then the Cut plays: the preview stops first.
    run(&mut s, "track.preview", json!({ "path": music("track.wav") }));
    run(&mut s, "storyboard.generate", json!({ "cueMap": music("track.cuemap.json"), "seconds": 30 }));
    s.poll_audio(Ticks::ZERO);
    assert!(s.wait_track_audio(Duration::from_secs(10)));
    run(&mut s, "storyboard.play", json!({}));
    assert!(s.preview_state().is_none());
    assert_eq!(player.log().last().map(String::as_str), Some("play 10.0s from 0.0"), "the fixture's 30 fit is 10 s");
    assert_eq!(player.gain(), Some(1.0), "the Cut plays at full level");
    // Bad params.
    for bad in [json!({}), json!({ "path": "rel.wav" }), json!({ "path": music("track.wav"), "from": -1 })] {
        assert!(matches!(s.execute("track.preview", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
}

#[test]
fn the_waveform_preview_and_cut_share_one_decode_per_track() {
    let player = Arc::new(RecordingPlayer::default());
    let mut s = session(Arc::clone(&player), true);
    let song = music("track.wav");
    // The timeline asks for the waveform first (picking a song): that starts the one decode.
    let mut peaks = json!(null);
    for _ in 0..2000 {
        peaks = run(&mut s, "track.peaks", json!({ "path": song, "buckets": 64 }));
        if peaks["status"] != "loading" {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(peaks["status"], "ready", "{peaks}");
    assert_eq!(s.decodes.len(), 1);
    // A section click on the same song plays at once: no second decode, no "loading".
    let r = run(&mut s, "track.preview", json!({ "path": song, "from": 8.0 }));
    assert_eq!(r["preview"], "playing", "{r}");
    assert_eq!(s.decodes.len(), 1, "reused, not decoded again");
    assert_eq!(s.preview_heard(&song, crate::PreviewMode::Song), Some(8.0));
    assert_eq!(s.preview_heard(&song, crate::PreviewMode::Cut), None, "the timeline's playhead stays put");
    assert_eq!(s.preview_heard("/music/other.wav", crate::PreviewMode::Song), None);
    // The Cut's own track stays decoded while other songs are previewed (up to decode::KEEP).
    run(&mut s, "track.preview", json!({ "path": null }));
    run(&mut s, "storyboard.generate", json!({ "cueMap": music("track.cuemap.json"), "seconds": 30 }));
    run(&mut s, "storyboard.play", json!({}));
    assert_eq!(s.decodes.len(), 1, "the board's track is the same file");
    assert_eq!(s.audio_status(), "playing", "and it starts at once");
}

#[test]
fn the_timeline_previews_the_cut_and_the_track_column_the_song() {
    use crate::PreviewMode::{Cut, Song};
    let player = Arc::new(RecordingPlayer::default());
    let mut s = session(Arc::clone(&player), true);
    let song = music("track.wav");
    let cut = json!({ "cueMap": music("track.cuemap.json"), "seconds": 30 });
    // Cut mode: the 30 s length's edit (a 10 s fade fit here), `from` in cut seconds.
    let mut r = json!(null);
    for _ in 0..2000 {
        r = run(&mut s, "track.preview", json!({ "path": song, "from": 4.0, "cut": cut }));
        if r["preview"] == "playing" {
            break;
        }
        s.poll_audio(Ticks::ZERO);
        if s.preview_heard(&song, Cut).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(r["mode"], "cut");
    assert_eq!(s.preview_mode(), Some(Cut));
    assert_eq!(player.log().last().map(String::as_str), Some("play 10.0s from 4.0"), "the edit, not the 64 s file");
    assert_eq!(s.preview_heard(&song, Cut), Some(4.0));
    assert_eq!(s.preview_heard(&song, Song), None, "the track column's playhead stays put");
    // A section click in the track column: Song mode replaces it (the cut stops first).
    let r = run(&mut s, "track.preview", json!({ "path": song, "from": 40.0 }));
    assert_eq!((r["mode"].as_str(), r["preview"].as_str()), (Some("song"), Some("playing")));
    assert_eq!(player.log()[player.log().len() - 2..], ["stop".to_owned(), "play 64.0s from 40.0".to_owned()]);
    assert_eq!(s.preview_heard(&song, Song), Some(40.0));
    assert_eq!(s.preview_heard(&song, Cut), None);
    // Full length is the whole track in cut mode too; bad cut params are refused.
    let r = run(&mut s, "track.preview", json!({ "path": song, "cut": { "cueMap": music("track.cuemap.json") } }));
    assert_eq!(r["mode"], "cut");
    assert_eq!(player.log().last().map(String::as_str), Some("play 64.0s from 0.0"));
    for bad in [
        json!({ "cueMap": music("track.cuemap.json"), "seconds": 45 }),
        json!("x"),
        json!({ "cueMap": music("track.cuemap.json"), "fit": "x" }),
    ] {
        assert!(s.execute("track.preview", &json!({ "path": song, "cut": bad })).is_err(), "{bad}");
    }
}

#[test]
fn recut_to_a_new_length_drops_the_old_audio_edit_and_undo_brings_it_back() {
    let player = Arc::new(RecordingPlayer::default());
    let mut s = session(Arc::clone(&player), true);
    run(&mut s, "storyboard.generate", json!({ "cueMap": music("track.cuemap.json"), "seconds": 30 }));
    let edit = |s: &mut Session| run(s, "storyboard.state", json!({}))["board"]["track"]["edit"].clone();
    assert!(!edit(&mut s).is_null(), "the 30 s fit gives an audio edit");
    // Same length: the edit stays.
    run(&mut s, "storyboard.recut", json!({ "bpm": 120 }));
    assert!(!edit(&mut s).is_null());
    // A new length: the edit no longer matches, so it goes (picture and music agree again).
    run(&mut s, "storyboard.recut", json!({ "bpm": 120, "seconds": 20 }));
    assert!(edit(&mut s).is_null(), "a 20 s recut must not keep the 10 s audio edit");
    run(&mut s, "storyboard.undo", json!({}));
    assert!(!edit(&mut s).is_null(), "undo restores it");
}

/// MP4 bytes with a sound track, made with our own encoder: `secs` of grey video and a tone.
fn talking_mp4(secs: u32) -> Vec<u8> {
    use assetcraft_video::{Container, Settings, VideoEncoder, encode_audio, mux};
    let st = Settings { width: 64, height: 48, fps: 12, qp: 36 };
    let mut enc = VideoEncoder::new(st).unwrap();
    for _ in 0..(secs * 12) {
        enc.push_rgba(&vec![128u8; 64 * 48 * 4]).unwrap();
    }
    let video = enc.finish().unwrap();
    let pcm: Vec<f32> = (0..48_000 * secs).map(|i| (i as f32 * 0.04).sin() * 0.5).collect();
    let audio = encode_audio(&pcm, 1, 48_000, 96_000).unwrap();
    let mut out = Vec::new();
    mux(&video, Some(&audio), Container::Mp4, &mut out).unwrap();
    out
}

#[test]
fn watch_your_cut_hears_an_inset_videos_sound_over_the_ducked_music() {
    let player = Arc::new(RecordingPlayer::default());
    let fs = FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .file_with(music("track.cuemap.json"), cue_map().into_bytes())
        .file_with(music("track.wav"), wav(64))
        .file_with("/clips/host.mp4", talking_mp4(3));
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    s.set_player(Arc::clone(&player) as Arc<dyn assetcraft_audio::Player>);
    run(&mut s, "storyboard.new", json!({ "template": "tutorial", "steps": 1, "cueMap": music("track.cuemap.json") }));
    s.poll_audio(Ticks::ZERO);
    assert!(s.wait_track_audio(Duration::from_secs(10)));
    let scene = s.storyboard().unwrap().inset_scenes().next().unwrap().id.0;
    run(&mut s, "storyboard.inset", json!({ "scene": scene, "asset": "/clips/host.mp4", "fit": false }));
    // Not mixed in: no voice.
    run(&mut s, "storyboard.play", json!({ "from": 0.0 }));
    assert!(!player.log().last().unwrap().contains("voice"), "{:?}", player.log());
    run(&mut s, "storyboard.pause", json!({ "at": 0.0 }));
    // Mixed in: the first ▶ starts its decode, the next one hears it.
    run(&mut s, "storyboard.inset", json!({ "scene": scene, "mixAudio": true }));
    run(&mut s, "storyboard.play", json!({ "from": 0.0 }));
    run(&mut s, "storyboard.pause", json!({ "at": 0.0 }));
    let host = std::path::Path::new("/clips/host.mp4");
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while !matches!(s.load_state(host), Some(crate::decode::Load::Ready(_))) && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    run(&mut s, "storyboard.play", json!({ "from": 0.0 }));
    assert!(player.log().last().unwrap().ends_with("+ 1 voice"), "{:?}", player.log());
}

/// V1 + V2 while the Cut plays: the listening volume changes the player's gain at once (and is
/// saved); a new song level rebuilds the program, so the next frame replays it from the playhead.
#[test]
fn the_listening_volume_and_song_level_change_a_playing_cut() {
    let player = Arc::new(RecordingPlayer::default());
    let mut s = session(Arc::clone(&player), true);
    run(&mut s, "storyboard.generate", json!({ "cueMap": music("track.cuemap.json"), "seconds": 30 }));
    s.poll_audio(Ticks::ZERO);
    assert!(s.wait_track_audio(Duration::from_secs(10)));
    run(&mut s, "storyboard.play", json!({ "from": 1.0 }));
    assert_eq!(player.gain(), Some(1.0), "full by default, as before");
    let r = run(&mut s, "storyboard.listenVolume", json!({ "volume": 0.5 }));
    assert_eq!(r["volume"], 0.5);
    assert_eq!(player.gain(), Some(0.25), "at once, on the slider's curve (squared)");
    assert_eq!(s.prefs().listen_volume, Some(50), "saved");
    for bad in [json!({}), json!({ "volume": 1.5 }), json!({ "volume": "half" })] {
        assert!(matches!(s.execute("storyboard.listenVolume", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    // The song level: the next frame plays the rebuilt program from where the playhead is.
    let before = player.log().len();
    run(&mut s, "storyboard.trackLevel", json!({ "db": -12 }));
    s.poll_audio(Ticks::from_secs_f64(1.5).unwrap_or(Ticks::ZERO));
    let log = player.log();
    assert_eq!(log[before..], ["stop", "play 10.0s from 1.5"], "replayed once, from the playhead");
    assert_eq!(player.gain(), Some(0.25), "and still at the listening volume");
    // A session restored from prefs keeps the listening volume.
    let s2 = Session::new(
        Arc::new(FakeFs::new().known(KnownFolder::Home, "/home/paul")),
        Arc::new(RecordingLauncher::default()),
        s.prefs(),
    );
    assert!((s2.listen_volume() - 0.5).abs() < 1e-6);
}
