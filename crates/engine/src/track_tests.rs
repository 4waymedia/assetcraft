use super::*;
use assetcraft_fs::FakeFs;
use serde_json::json;

/// A 16-bit PCM mono WAV of `frames` frames of a square wave at quarter scale.
fn wav(rate: u32, frames: u32) -> Vec<u8> {
    let data = frames * 2;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        let s: i16 = if (i / 50) % 2 == 0 { 8192 } else { -8192 };
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

fn session(fs: FakeFs) -> Session {
    Session::new(
        Arc::new(fs.known(KnownFolder::Home, "/home/paul")),
        Arc::new(RecordingLauncher::default()),
        Prefs::default(),
    )
}

fn music(name: &str) -> String {
    std::path::Path::new("/music").join(name).to_string_lossy().into_owned()
}

#[test]
fn summarises_a_track() {
    let fs = FakeFs::new().file_with(music("a.wav"), wav(22_050, 44_100));
    let mut s = session(fs);
    let r = s.execute("track.summary", &json!({ "path": music("a.wav") })).unwrap();
    assert_eq!(
        (r["format"].as_str(), r["rate"].as_u64(), r["channels"].as_u64()),
        (Some("wav"), Some(22_050), Some(1))
    );
    assert_eq!(r["frames"], 44_100);
    assert_eq!(r["seconds"], 2.0);
    assert!((r["peakDbfs"].as_f64().unwrap() + 12.04).abs() < 0.01, "{r}");
    assert!((r["rmsDbfs"].as_f64().unwrap() + 12.04).abs() < 0.01, "a square wave's RMS is its peak");
    assert_eq!(r["path"], music("a.wav"));
}

#[test]
fn refuses_bad_paths_and_bad_audio() {
    let fs =
        FakeFs::new().file_with(music("junk.mp3"), b"not audio".to_vec()).file_with(music("slow.wav"), wav(4_000, 400));
    let mut s = session(fs);
    assert!(matches!(s.execute("track.summary", &json!({})), Err(CmdError::BadParams(_))));
    assert!(matches!(s.execute("track.summary", &json!({ "path": "relative.wav" })), Err(CmdError::BadParams(_))));
    for name in ["junk.mp3", "slow.wav", "missing.wav"] {
        assert!(
            matches!(s.execute("track.summary", &json!({ "path": music(name) })), Err(CmdError::Unavailable(_))),
            "{name}"
        );
    }
}
