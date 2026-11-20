use super::builtin::{self, LASER_TRACE};
use super::frames::{AHEAD, DesignFrames, DesignJob, DesignLookup, FPS, frame_index};
use super::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn job(key: u64) -> Arc<DesignJob> {
    let design = Arc::new(builtin::builtin(LASER_TRACE).expect("listed").expect("builds"));
    Arc::new(DesignJob { key, design, fill: Fill::default().text(Slot::Headline, "ISTANBUL") })
}

/// Ask until it's ready (or give up after a while).
fn ready(frames: &DesignFrames, job: &Arc<DesignJob>, secs: f64, width: u32) -> Arc<frames::DesignFrame> {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        match frames.frame(job, secs, width) {
            DesignLookup::Ready(f) => return f,
            DesignLookup::Failed(e) => panic!("{e}"),
            DesignLookup::Pending(_) => {
                assert!(Instant::now() < until, "never rendered");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

#[test]
fn frames_render_in_the_background_and_match_a_direct_render() {
    let frames = DesignFrames::with_workers(2);
    let j = job(1);
    assert!(matches!(frames.frame(&j, 3.0, 320), DesignLookup::Pending(None)), "never waits");
    let f = ready(&frames, &j, 3.0, 320);
    assert_eq!(f.index, (3.0 * FPS) as u32);
    let direct = j.design.render(&j.fill, f64::from(f.index) / FPS, 320.0 / 1920.0).expect("renders");
    assert_eq!((f.width, f.height, &f.rgba), (direct.width, direct.height, &direct.rgba));
    // The read-ahead lands too, and a later frame waits with the nearest one before it.
    let next = ready(&frames, &j, 3.0 + f64::from(AHEAD) / FPS, 320);
    assert_eq!(next.index, f.index + AHEAD);
    let far = frames.frame(&j, 5.5, 320);
    match far {
        DesignLookup::Ready(x) => assert_eq!(x.index, (5.5 * FPS) as u32),
        DesignLookup::Pending(Some(near)) => assert!(near.index <= (5.5 * FPS) as u32),
        other => panic!("{other:?}"),
    }
}

#[test]
fn indices_hold_at_the_end_and_another_key_is_another_entry() {
    let d = builtin::builtin(LASER_TRACE).expect("listed").expect("builds");
    assert_eq!(frame_index(&d, 0.0), 0);
    assert_eq!(frame_index(&d, 1.0), 30);
    let last = frame_index(&d, 600.0);
    assert_eq!(last, (6.0 * FPS) as u32 - 1, "held at the last frame");
    assert_eq!(frame_index(&d, f64::NAN), 0);
    let frames = DesignFrames::with_workers(1);
    let (a, b) = (job(7), job(8));
    let fa = ready(&frames, &a, 600.0, 160);
    assert_eq!(fa.index, last);
    // Same design, another key (another fill): rendered separately.
    assert!(matches!(frames.frame(&b, 600.0, 160), DesignLookup::Pending(None)));
    assert_eq!(frames.workers(), 1);
}
