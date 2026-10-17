//! Waveform overviews for the Start screen's full-width timeline (S2d-1; Paul, 2026-10-16: "the
//! full audio is placed in the bottom track"): `track.peaks`.
//!
//! The overview comes from the shared track decode ([`crate::decode`]), which computes it on its
//! background thread, so drawing a song's waveform also gets it ready to preview and play. Overviews
//! are kept for the last [`KEEP`] tracks, longer than the decoded audio itself. The first call
//! answers `loading`; the window asks again on its next frames.

use crate::decode::{Load, OVERVIEW_BUCKETS};
use crate::{CmdError, CmdResult, Session};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

/// Overviews kept.
const KEEP: usize = 8;
/// What `buckets` may ask for.
pub(crate) const BUCKETS_RANGE: std::ops::RangeInclusive<u64> = 16..=OVERVIEW_BUCKETS as u64;

/// A finished overview: the track's length and its peaks, or why it couldn't be read.
#[derive(Debug, Clone)]
enum Overview {
    Ready { seconds: f64, peaks: Arc<Vec<(f32, f32)>> },
    Failed(String),
}

/// Finished overviews by track, the most recently asked last.
#[derive(Debug, Default)]
pub(crate) struct Waveforms {
    kept: Vec<(PathBuf, Overview)>,
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
    /// `path`'s overview, if finished; otherwise starts (or keeps waiting on) the shared decode.
    fn overview(&mut self, path: &PathBuf) -> Option<Overview> {
        let w = &mut self.waveforms;
        if let Some(i) = w.kept.iter().position(|(p, _)| p == path) {
            let kept = w.kept.remove(i);
            let o = kept.1.clone();
            w.kept.push(kept);
            return Some(o);
        }
        let state = match self.load_state(path) {
            Some(state) => state,
            None => self.load_slot(path).lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone(),
        };
        let done = match state {
            Load::Loading => return None,
            Load::Ready(d) => Overview::Ready { seconds: d.audio.seconds(), peaks: Arc::clone(&d.overview) },
            Load::Failed(e) => Overview::Failed(e),
        };
        let w = &mut self.waveforms;
        if w.kept.len() >= KEEP {
            w.kept.remove(0);
        }
        w.kept.push((path.clone(), done.clone()));
        Some(done)
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
    let shown = path.to_string_lossy().into_owned();
    Ok(match s.overview(&path) {
        None => json!({ "path": shown, "status": "loading" }),
        Some(Overview::Failed(e)) => json!({ "path": shown, "status": "failed", "error": e }),
        Some(Overview::Ready { seconds, peaks }) => {
            let peaks: Vec<[f64; 2]> =
                rebucket(&peaks, buckets).into_iter().map(|(a, b)| [round3(a), round3(b)]).collect();
            json!({ "path": shown, "status": "ready", "seconds": (seconds * 1000.0).round() / 1000.0, "peaks": peaks })
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
