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

/// Paul's Windows run, 2026-11-16: playback showed frame 36 of a 24-frame video, then stepped
/// back to 0. Read-ahead stops at the video's end, a time past it is the last frame, and a
/// stand-in is never later than the frame asked while an earlier one is at hand.
#[test]
fn nothing_past_the_end_and_stand_ins_never_run_ahead() {
    for workers in [1, 4] {
        let v = VideoFrames::with_workers(workers);
        let path = library("snow_clearing_flythrough.mp4");
        // Four seconds asked of a two-second video.
        v.ahead(&path, 0.0, 4.0, 64);
        let until = Instant::now() + Duration::from_secs(10);
        while !matches!(v.frame(&path, 1.95, 64), Lookup::Ready(_)) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        let indexes: Vec<u64> = {
            let s = v.shared.lock();
            s.cache.iter().map(|(_, i, ..)| *i).collect()
        };
        assert!(indexes.iter().all(|i| *i < 24), "{workers} worker(s): only the video's frames: {indexes:?}");
        // Past the end: its last frame, ready.
        match v.frame(&path, 3.5, 64) {
            Lookup::Ready(f) => assert_eq!(f.index, 23),
            other => panic!("past the end: {other:?}"),
        }
        // A size not decoded yet: the stand-in is the nearest frame at or before the one asked.
        v.stop_ahead();
        if let Lookup::Pending(Some(f)) = v.frame(&path, 1.0, 32) {
            assert!(f.index <= 12, "{workers} worker(s): stand-in {} for frame 12", f.index);
        }
    }
}

/// P3: the export reads frames on its own thread, in order, through `VideoReader`.
#[test]
fn a_reader_gives_frames_in_order_and_holds_the_end() {
    let mut r = VideoReader::open(&library("snow_clearing_flythrough.mp4")).unwrap();
    let a = r.frame(0.0, 64).unwrap();
    let b = r.frame(1.0, 64).unwrap();
    assert_eq!((a.index, b.index), (0, 12));
    assert!(a.width.max(a.height) <= 64 && a.rgba.len() == (a.width * a.height * 4) as usize);
    assert_ne!(a.rgba, b.rgba, "different pictures");
    // The same frame again is the one held, not decoded again.
    assert!(Arc::ptr_eq(&b, &r.frame(1.04, 64).unwrap()));
    assert_eq!(r.frame(9.0, 64).unwrap().index, 23, "past the end: the last frame");
    assert_eq!(r.frame(f64::NAN, 64).unwrap().index, 0);
    assert!(VideoReader::open(&library("no_such_clip.mp4")).is_err());
}
