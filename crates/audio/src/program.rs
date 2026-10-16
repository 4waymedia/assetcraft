//! What plays during Watch your Cut: a decoded track through its audio edit (Fit to length,
//! `docs/track-analysis.md` §6.5), rendered as stereo at whatever rate the output device runs.
//!
//! - Segments play back to back; each splice has an equal-power crossfade centred on it (the
//!   outgoing segment runs on past its end and the incoming one starts before its start, so the
//!   edit keeps its length).
//! - The fade-out is a quarter-sine (equal-power) curve to silence; after the edit's length,
//!   silence.
//! - The device rate rarely matches the file (48 kHz devices, 44.1 kHz MP3s): rendering steps
//!   through the program at `rate / device rate` and interpolates linearly. Good enough for a
//!   preview; analysis never goes through here.
//!
//! [`Player`] is how the engine drives an output device without depending on one: the app
//! plugs in the real device (`assetcraft-playback`), tests use [`RecordingPlayer`].

use crate::Audio;
use std::f64::consts::FRAC_PI_2;
use std::sync::{Arc, Mutex};

/// An audio edit, in frames at `rate` (the rate the cue map was measured at).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Rate the frames are counted at.
    pub rate: u32,
    /// Source ranges `(start, end)` played back to back.
    pub segments: Vec<(u64, u64)>,
    /// Crossfade centred on each splice, in frames.
    pub crossfade: u64,
    /// Fade to silence `(start, frames)` on the output timeline.
    pub fade_out: Option<(u64, u64)>,
    /// Length of the edit in frames.
    pub length: u64,
}

/// A track ready to play: the decoded audio and its edit, both at the audio's rate.
#[derive(Debug, Clone)]
pub struct Program {
    audio: Arc<Audio>,
    /// `(source start, source end, output offset)`.
    segments: Vec<(u64, u64, u64)>,
    half_crossfade: u64,
    fade_out: Option<(u64, u64)>,
    length: u64,
}

fn rescale(frames: u64, from: u32, to: u32) -> u64 {
    if from == to || from == 0 {
        return frames;
    }
    let v = u128::from(frames) * u128::from(to) / u128::from(from);
    u64::try_from(v).unwrap_or(u64::MAX)
}

impl Program {
    /// The whole track, or the track through `edit`. Edit frames are rescaled from the edit's
    /// rate to the audio's; segments are clipped to the file, and empty ones dropped.
    pub fn new(audio: Arc<Audio>, edit: Option<&Edit>) -> Program {
        let total = audio.frames() as u64;
        let rate = audio.rate();
        let Some(edit) = edit else {
            return Program { audio, segments: vec![(0, total, 0)], half_crossfade: 0, fade_out: None, length: total };
        };
        let r = |f: u64| rescale(f, edit.rate, rate);
        let mut segments = Vec::with_capacity(edit.segments.len());
        let mut at = 0u64;
        for &(a, b) in &edit.segments {
            let (a, b) = (r(a).min(total), r(b).min(total));
            if a < b {
                segments.push((a, b, at));
                at = at.saturating_add(b - a);
            }
        }
        let length = r(edit.length).min(at);
        // A crossfade can't be longer than either side of a splice.
        let shortest = segments.iter().map(|(a, b, _)| b - a).min().unwrap_or(0);
        let half_crossfade = (r(edit.crossfade) / 2).min(shortest / 2);
        let fade_out = edit.fade_out.map(|(s, n)| (r(s), r(n).max(1)));
        Program { audio, segments, half_crossfade, fade_out, length }
    }

    /// Sample rate of the program (the audio's).
    pub fn rate(&self) -> u32 {
        self.audio.rate()
    }

    /// Length in frames.
    pub fn length_frames(&self) -> u64 {
        self.length
    }

    /// Length in seconds.
    pub fn seconds(&self) -> f64 {
        self.length as f64 / f64::from(self.rate())
    }

    fn source(&self, src: i128) -> (f32, f32) {
        match usize::try_from(src) {
            Ok(i) => self.audio.stereo_at(i),
            Err(_) => (0.0, 0.0),
        }
    }

    /// Output frame `t` as left/right.
    pub fn frame(&self, t: u64) -> (f32, f32) {
        if t >= self.length {
            return (0.0, 0.0);
        }
        let Some(k) = self.segments.iter().rposition(|&(_, _, off)| off <= t) else { return (0.0, 0.0) };
        let (start, end, off) = self.segments[k];
        let d = i128::from(t - off);
        let len = i128::from(end - start);
        let h = i128::from(self.half_crossfade);
        let mut out = self.source(i128::from(start) + d);
        let mix = |a: (f32, f32), ga: f64, b: (f32, f32), gb: f64| {
            ((f64::from(a.0) * ga + f64::from(b.0) * gb) as f32, (f64::from(a.1) * ga + f64::from(b.1) * gb) as f32)
        };
        if h > 0 {
            if k > 0 && d < h {
                // Just after a splice: the previous segment fades out past its end.
                let (_, prev_end, _) = self.segments[k - 1];
                let x = (h + d) as f64 / (2 * h) as f64;
                out = mix(self.source(i128::from(prev_end) + d), (x * FRAC_PI_2).cos(), out, (x * FRAC_PI_2).sin());
            } else if k + 1 < self.segments.len() && len - d <= h {
                // Just before a splice: the next segment fades in before its start.
                let (next_start, _, _) = self.segments[k + 1];
                let rel = d - len; // -h ..= 0
                let x = (h + rel) as f64 / (2 * h) as f64;
                out = mix(out, (x * FRAC_PI_2).cos(), self.source(i128::from(next_start) + rel), (x * FRAC_PI_2).sin());
            }
        }
        if let Some((fs, n)) = self.fade_out
            && t >= fs
        {
            let x = ((t - fs) as f64 / n as f64).min(1.0);
            let g = (x * FRAC_PI_2).cos() as f32;
            out = (out.0 * g, out.1 * g);
        }
        out
    }

    /// Fill `out` (interleaved, `channels` per frame) starting `from_secs` into the program, at
    /// `out_rate`. Channels past the first two get silence; a mono device gets the mean.
    pub fn render(&self, from_secs: f64, out_rate: u32, out: &mut [f32], channels: usize) {
        if channels == 0 || out_rate == 0 {
            return;
        }
        let rate = f64::from(self.rate());
        let step = rate / f64::from(out_rate);
        let start = if from_secs.is_finite() { from_secs.max(0.0) * rate } else { f64::MAX };
        for (i, frame) in out.chunks_mut(channels).enumerate() {
            let pos = start + i as f64 * step;
            let (l, r) = if pos >= self.length as f64 {
                (0.0, 0.0)
            } else {
                let t = pos.floor();
                let frac = (pos - t) as f32;
                let t = t as u64;
                let (a, b) = (self.frame(t), self.frame(t.saturating_add(1)));
                (a.0 + (b.0 - a.0) * frac, a.1 + (b.1 - a.1) * frac)
            };
            match frame {
                [m] => *m = (l + r) * 0.5,
                [a, b, rest @ ..] => {
                    *a = l;
                    *b = r;
                    rest.iter_mut().for_each(|s| *s = 0.0);
                }
                [] => {}
            }
        }
    }
}

/// An audio output the engine can drive. Implementations never block for long and never
/// panic; a failure to open a device is an `Err` the engine reports, and the Cut plays silent.
pub trait Player: Send + Sync {
    /// Start `program` at `from_secs`, replacing anything playing.
    fn play(&self, program: Arc<Program>, from_secs: f64) -> Result<(), String>;
    /// Stop playing (no-op when idle).
    fn stop(&self);
    /// Seconds into the program being heard now, while playing; `None` when idle. This is the
    /// audio clock the playhead follows.
    fn position(&self) -> Option<f64>;
}

/// No output: the Cut plays silent on the window's clock (headless sessions, the CLI).
#[derive(Debug, Default)]
pub struct SilentPlayer;

impl Player for SilentPlayer {
    fn play(&self, _: Arc<Program>, _: f64) -> Result<(), String> {
        Err("no audio output".into())
    }
    fn stop(&self) {}
    fn position(&self) -> Option<f64> {
        None
    }
}

/// Records calls instead of making sound (tests). `position` reports where the last `play`
/// started, until `stop`.
#[derive(Debug, Default)]
pub struct RecordingPlayer {
    log: Mutex<Vec<String>>,
    at: Mutex<Option<f64>>,
}

impl RecordingPlayer {
    /// What was asked so far: `play <seconds> from <from>` and `stop`.
    pub fn log(&self) -> Vec<String> {
        self.log.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

impl Player for RecordingPlayer {
    fn play(&self, program: Arc<Program>, from_secs: f64) -> Result<(), String> {
        if let Ok(mut l) = self.log.lock() {
            l.push(format!("play {:.1}s from {from_secs:.1}", program.seconds()));
        }
        if let Ok(mut a) = self.at.lock() {
            *a = Some(from_secs);
        }
        Ok(())
    }
    fn stop(&self) {
        if let Ok(mut l) = self.log.lock() {
            l.push("stop".into());
        }
        if let Ok(mut a) = self.at.lock() {
            *a = None;
        }
    }
    fn position(&self) -> Option<f64> {
        self.at.lock().ok().and_then(|a| *a)
    }
}
