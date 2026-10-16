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

#[test]
fn peaks_load_in_the_background_and_keep_the_shape_of_the_track() {
    let fs = FakeFs::new()
        .file_with(music("a.wav"), wav(22_050, 44_100))
        .file_with(music("junk.mp3"), b"not audio".to_vec());
    let mut s = session(fs);
    let ask = |s: &mut Session, path: &str, buckets: u64| {
        for _ in 0..2000 {
            let r = s.execute("track.peaks", &json!({ "path": path, "buckets": buckets })).unwrap();
            if r["status"] != "loading" {
                return r;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        json!(null)
    };
    let r = ask(&mut s, &music("a.wav"), 64);
    assert_eq!(r["status"], "ready", "{r}");
    assert_eq!(r["seconds"], 2.0);
    let peaks = r["peaks"].as_array().cloned().unwrap_or_default();
    assert_eq!(peaks.len(), 64);
    // A quarter-scale square wave: every bucket spans -0.25..0.25.
    assert!(peaks.iter().all(|p| p[0] == -0.25 && p[1] == 0.25), "{:?}", &peaks[..4]);
    // Kept: a second ask is ready at once, at another resolution.
    let r = s.execute("track.peaks", &json!({ "path": music("a.wav") })).unwrap();
    assert_eq!((r["status"].as_str(), r["peaks"].as_array().map(Vec::len)), (Some("ready"), Some(1024)));
    // Bad audio fails without an error; bad params are refused.
    assert_eq!(ask(&mut s, &music("junk.mp3"), 64)["status"], "failed");
    for bad in [json!({}), json!({ "path": "rel.wav" }), json!({ "path": music("a.wav"), "buckets": 8 })] {
        assert!(matches!(s.execute("track.peaks", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
}
