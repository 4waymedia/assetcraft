use super::*;
use std::time::{Duration, Instant};

fn library(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/environments").join(name)
}

/// Ask until the frame is ready (or give up after a few seconds).
fn ready(v: &VideoFrames, path: &Path, secs: f64, side: u32) -> Lookup {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let r = v.frame(path, secs, side);
        if !matches!(r, Lookup::Pending(_)) || Instant::now() > until {
            return r;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn an_mp4_and_a_mov_decode_to_preview_sized_rgba() {
    let v = VideoFrames::new();
    for name in ["snow_clearing_flythrough.mp4", "snow_clearing_flythrough.mov"] {
        let path = library(name);
        let Lookup::Ready(f) = ready(&v, &path, 1.0, 64) else { panic!("{name} didn't decode") };
        assert!(f.width <= 64 && f.height <= 64 && f.width > 0, "{name}: {}x{}", f.width, f.height);
        assert_eq!(f.rgba.len(), (f.width * f.height * 4) as usize);
        assert_eq!(f.index, 12, "1 s at 12 fps");
        assert!(matches!(v.frame(&path, 1.02, 64), Lookup::Ready(_)), "the same frame from the cache");
    }
}

#[test]
fn a_pending_answer_offers_the_nearest_frame_meanwhile() {
    let v = VideoFrames::new();
    let path = library("snow_clearing_flythrough.mp4");
    assert!(matches!(ready(&v, &path, 0.5, 64), Lookup::Ready(_)));
    match v.frame(&path, 1.5, 64) {
        Lookup::Pending(Some(f)) => assert_eq!(f.index, 6, "the 0.5 s frame meanwhile"),
        Lookup::Ready(_) => {} // decoded already on a fast machine
        other => panic!("{other:?}"),
    }
}

#[test]
fn files_that_cant_play_are_reported_not_panicked() {
    let v = VideoFrames::new();
    let missing = library("no_such_clip.mp4");
    assert!(matches!(ready(&v, &missing, 0.0, 64), Lookup::Failed(_)));
    let not_video = library("../../library/README.md");
    if not_video.exists() {
        assert!(matches!(ready(&v, &not_video, 0.0, 64), Lookup::Failed(_)));
    }
    assert!(is_video(Path::new("a/B.MOV")) && !is_video(Path::new("a/b.png")));
}

/// P2: read-ahead decodes the coming stretch without each frame being asked, so playback finds
/// them ready; it works with one worker as with many.
#[test]
fn reading_ahead_makes_the_coming_frames_ready() {
    for workers in [1, 3] {
        let v = VideoFrames::with_workers(workers);
        assert_eq!(v.workers(), workers);
        let path = library("snow_clearing_flythrough.mp4");
        v.ahead(&path, 0.0, 2.0, 64);
        let until = Instant::now() + Duration::from_secs(10);
        // 12 fps for 2 s: frames 0..24, none of them asked for one by one.
        let all_ready = loop {
            let ready = (0..24).all(|i| matches!(v.frame(&path, (f64::from(i) + 0.5) / 12.0, 64), Lookup::Ready(_)));
            if ready || Instant::now() > until {
                break ready;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(all_ready, "{workers} worker(s): two seconds ready ahead");
        v.stop_ahead();
        v.ahead(&path, 0.0, 2.0, 64);
        assert!(matches!(v.frame(&path, 1.0, 64), Lookup::Ready(_)), "a second call queues nothing new");
    }
    assert!(worker_count() >= 1, "every core but the UI's, at least one");
}

#[test]
fn a_video_s_length_comes_from_its_header() {
    for name in ["snow_clearing_flythrough.mp4", "snow_clearing_flythrough.mov"] {
        let secs = seconds(&library(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!((secs - 2.0).abs() < 0.05, "{name}: {secs}");
    }
    assert!(seconds(&library("no_such_clip.mp4")).is_err());
}
