//! Waveform overviews for the Start screen's full-width timeline (S2d-1; Paul, 2026-10-16: "the
//! full audio is placed in the bottom track"): `track.peaks`.
//!
//! The overview is decoded on a background thread (an MP3 takes about a second) and kept for the
//! last few tracks, apart from the playback decode slot, so a Cut playing behind the Start screen
//! and the timeline never evict each other. The first call answers `loading`; the window asks
//! again on its next frames.

use crate::{CmdError, CmdResult, Session};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

/// Buckets kept per track; requests are merged down from these.
pub(crate) const OVERVIEW_BUCKETS: usize = 4096;
/// Tracks kept.
const KEEP: usize = 8;
/// What `buckets` may ask for.
pub(crate) const BUCKETS_RANGE: std::ops::RangeInclusive<u64> = 16..=4096;

#[derive(Debug, Clone)]
enum WaveLoad {
    Loading,
    Ready(Arc<Overview>),
    Failed(String),
}

#[derive(Debug)]
struct Overview {
    seconds: f64,
    peaks: Vec<(f32, f32)>,
}

type Slot = Arc<Mutex<WaveLoad>>;

/// Overviews by track, the most recently asked last.
#[derive(Debug, Default)]
pub(crate) struct Waveforms {
    slots: Vec<(PathBuf, Slot)>,
}

/// `peaks` merged down to `buckets` (`(min, max)` of each run).
fn rebucket(peaks: &[(f32, f32)], buckets: usize) -> Vec<(f32, f32)> {
    let n = peaks.len();
    if n <= buckets || buckets == 0 {
        return peaks.to_vec();
    }
    (0..buckets)
        .map(|j| {
            let (a, b) = (j * n / buckets, ((j + 1) * n / buckets).max(j * n / buckets + 1));
            peaks[a..b.min(n)].iter().fold((0.0f32, 0.0f32), |(lo, hi), (mn, mx)| (lo.min(*mn), hi.max(*mx)))
        })
        .collect()
}

fn round3(v: f32) -> f64 {
    (f64::from(v) * 1000.0).round() / 1000.0
}

impl Session {
    /// The overview slot for `path`, starting its decode if it isn't kept.
    fn waveform_slot(&mut self, path: &PathBuf) -> Slot {
        let w = &mut self.waveforms;
        if let Some(i) = w.slots.iter().position(|(p, _)| p == path) {
            let kept = w.slots.remove(i);
            let slot = Arc::clone(&kept.1);
            w.slots.push(kept);
            return slot;
        }
        let slot: Slot = Arc::new(Mutex::new(WaveLoad::Loading));
        let (services, file, out) = (Arc::clone(&self.services), path.clone(), Arc::clone(&slot));
        let spawned = std::thread::Builder::new().name("assetcraft-waveform".into()).spawn(move || {
            let result = match assetcraft_audio::load(services.as_ref(), &file) {
                Ok(a) => {
                    WaveLoad::Ready(Arc::new(Overview { seconds: a.seconds(), peaks: a.overview(OVERVIEW_BUCKETS) }))
                }
                Err(e) => WaveLoad::Failed(e.to_string()),
            };
            *out.lock().unwrap_or_else(PoisonError::into_inner) = result;
        });
        if let Err(e) = spawned {
            *slot.lock().unwrap_or_else(PoisonError::into_inner) =
                WaveLoad::Failed(format!("can't start decoding: {e}"));
        }
        let w = &mut self.waveforms;
        if w.slots.len() >= KEEP {
            w.slots.remove(0);
        }
        w.slots.push((path.clone(), Arc::clone(&slot)));
        slot
    }
}

/// `track.peaks {path, buckets?}` → `{path, status: loading}` | `{path, status: ready, seconds,
/// peaks: [[min, max]]}` | `{path, status: failed, error}`. Buckets 16–4096 (default 1024) span the
/// whole file.
pub(crate) fn track_peaks(s: &mut Session, p: &Value) -> CmdResult {
    let path = crate::track_cmd::audio_path(p)?;
    let buckets = match p.get("buckets") {
        None | Some(Value::Null) => 1024,
        Some(v) => v
            .as_u64()
            .filter(|b| BUCKETS_RANGE.contains(b))
            .ok_or_else(|| CmdError::BadParams("`buckets` must be a whole number from 16 to 4096".into()))?,
    } as usize;
    let slot = s.waveform_slot(&path);
    let state = slot.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let shown = path.to_string_lossy();
    Ok(match state {
        WaveLoad::Loading => json!({ "path": shown, "status": "loading" }),
        WaveLoad::Failed(e) => json!({ "path": shown, "status": "failed", "error": e }),
        WaveLoad::Ready(o) => {
            let peaks: Vec<[f64; 2]> =
                rebucket(&o.peaks, buckets).into_iter().map(|(a, b)| [round3(a), round3(b)]).collect();
            json!({ "path": shown, "status": "ready", "seconds": (o.seconds * 1000.0).round() / 1000.0, "peaks": peaks })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::rebucket;

    #[test]
    fn rebucket_keeps_the_extremes_of_each_run() {
        let peaks = [(-0.1, 0.2), (-0.5, 0.1), (0.0, 0.9), (-0.3, 0.3)];
        assert_eq!(rebucket(&peaks, 2), vec![(-0.5, 0.2), (-0.3, 0.9)]);
        assert_eq!(rebucket(&peaks, 8), peaks.to_vec(), "never invents buckets");
        assert_eq!(rebucket(&peaks, 3).len(), 3);
    }
}
