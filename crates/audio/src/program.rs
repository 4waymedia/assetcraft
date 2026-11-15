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
    /// Voices over the music (SR4): each ducks the music under its window and plays over it.
    layers: Vec<Layer>,
    /// The music's level (the storyboard's song level; 1 = as decoded).
    music_gain: f32,
    /// Stretches where the music dips (title cards, B-R3).
    dips: Vec<Dip>,
    /// The song's volume line (V3), on top of `music_gain`.
    envelope: Option<Envelope>,
}

/// The quietest level a volume point can hold: at or below it the song is silent.
pub const ENVELOPE_FLOOR_DB: f32 = -60.0;

/// The song's volume line (V3; Paul, 2026-11-15: "control the volume at any point in the
/// timeline using a simple timeline mark"): points of `(program seconds, dB)`, a straight line in
/// dB between them, level before the first and after the last.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Envelope {
    points: Vec<(f64, f32)>,
}

impl Envelope {
    /// A line through `points` (any order; points that aren't finite are dropped).
    pub fn new(points: impl IntoIterator<Item = (f64, f32)>) -> Envelope {
        let mut points: Vec<(f64, f32)> = points.into_iter().filter(|(t, d)| t.is_finite() && d.is_finite()).collect();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        Envelope { points }
    }

    /// The level in dB at program second `t` (0 dB with no points).
    pub fn db_at(&self, t: f64) -> f32 {
        let p = &self.points;
        let (Some(first), Some(last)) = (p.first(), p.last()) else { return 0.0 };
        if t <= first.0 {
            return first.1;
        }
        if t >= last.0 {
            return last.1;
        }
        let i = p.partition_point(|q| q.0 <= t);
        let (a, b) = (p[i - 1], p[i]);
        let x = if b.0 > a.0 { ((t - a.0) / (b.0 - a.0)) as f32 } else { 1.0 };
        a.1 + (b.1 - a.1) * x
    }

    /// The linear gain at program second `t`: silent at or below [`ENVELOPE_FLOOR_DB`].
    pub fn gain_at(&self, t: f64) -> f32 {
        let db = self.db_at(t);
        if db <= ENVELOPE_FLOOR_DB { 0.0 } else { 10f32.powf(db / 20.0) }
    }
}

/// How long a [`Dip`] takes to go down after its start and to come back up by its end.
pub const DIP_RAMP_SECS: f64 = 0.25;

/// A stretch of program time where the music plays at `gain` (B-R3; Paul, 2026-11-14: "Titles
/// should be softer and lead into the content"): it eases down over [`DIP_RAMP_SECS`] after
/// `start` and is back at full by `end`, so the hit after a title card lands at full level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dip {
    /// Program seconds it starts at.
    pub start: f64,
    /// Program seconds it is back at full level by.
    pub end: f64,
    /// Linear gain at the bottom.
    pub gain: f32,
}

impl Dip {
    /// The gain this dip puts on program second `t` (1 outside it).
    pub fn at(&self, t: f64) -> f32 {
        if !(t >= self.start && t < self.end) || !self.gain.is_finite() {
            return 1.0;
        }
        let ramp = DIP_RAMP_SECS.min((self.end - self.start) / 2.0).max(1e-6);
        let down = ((t - self.start) / ramp).min(1.0);
        let up = ((self.end - t) / ramp).min(1.0);
        let x = down.min(up) as f32;
        1.0 + (self.gain.clamp(0.0, 4.0) - 1.0) * x
    }
}

/// A voice over the program (SR4): a section's inset video sound, from `start` for `length`
/// seconds of program time, at `voice_gain`, with the music at `music_gain` under it.
#[derive(Debug, Clone)]
pub struct Layer {
    /// The decoded sound.
    pub audio: Arc<Audio>,
    /// Program seconds it starts at.
    pub start: f64,
    /// Program seconds it runs for (cut there).
    pub length: f64,
    /// Linear gain on the voice.
    pub voice_gain: f32,
    /// Linear gain on the music under it.
    pub music_gain: f32,
    /// Where to duck, in the layer's own seconds (`(start, length)`); empty = all of it. A
    /// voiceover ducks only while it speaks ([`crate::mix::speech_spans`]).
    pub duck: Vec<(f64, f64)>,
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
            return Program {
                audio,
                segments: vec![(0, total, 0)],
                half_crossfade: 0,
                fade_out: None,
                length: total,
                layers: Vec::new(),
                music_gain: 1.0,
                dips: Vec::new(),
                envelope: None,
            };
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
        Program {
            audio,
            segments,
            half_crossfade,
            fade_out,
            length,
            layers: Vec::new(),
            music_gain: 1.0,
            dips: Vec::new(),
            envelope: None,
        }
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
        let mut g = self.music_gain;
        if !self.dips.is_empty() || self.envelope.is_some() {
            let secs = t as f64 / f64::from(self.rate().max(1));
            g *= self.dips.iter().map(|d| d.at(secs)).product::<f32>();
            if let Some(e) = &self.envelope {
                g *= e.gain_at(secs);
            }
        }
        (out.0 * g, out.1 * g)
    }

    /// This program with the music following a volume line (V3).
    pub fn with_envelope(mut self, envelope: Option<Envelope>) -> Program {
        self.envelope = envelope;
        self
    }

    /// This program with the music dipping over `dips` (title cards, B-R3).
    pub fn with_dips(mut self, dips: Vec<Dip>) -> Program {
        self.dips = dips;
        self
    }

    /// This program with its music at `gain` (linear; see [`crate::mix::db`]). A gain that isn't
    /// a finite number from 0 up is taken as 1.
    pub fn with_music_gain(mut self, gain: f32) -> Program {
        self.music_gain = if gain.is_finite() && gain >= 0.0 { gain } else { 1.0 };
        self
    }

    /// This program with voices over it (SR4).
    pub fn with_layers(mut self, layers: Vec<Layer>) -> Program {
        self.layers = layers;
        self
    }

    /// The voices over it.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Fill `out` (interleaved, `channels` per frame) starting `from_secs` into the program, at
    /// `out_rate`; the music ducked under each layer and the layer over it. Channels past the
    /// first two get silence; a mono device gets the mean.
    pub fn render(&self, from_secs: f64, out_rate: u32, out: &mut [f32], channels: usize) {
        self.render_music(from_secs, out_rate, out, channels);
        if self.layers.is_empty() || channels == 0 || out_rate == 0 || !from_secs.is_finite() {
            return;
        }
        let from = from_secs.max(0.0);
        let window = (out.len() / channels) as f64 / f64::from(out_rate);
        for l in &self.layers {
            let (a, b) = (l.start, l.start + l.length);
            if b + crate::mix::DUCK_RAMP_SECS < from || a - crate::mix::DUCK_RAMP_SECS > from + window {
                continue;
            }
            if l.duck.is_empty() {
                crate::mix::duck(out, channels, out_rate, &[(a - from, l.length)], l.music_gain);
            } else {
                let spans: Vec<(f64, f64)> = l
                    .duck
                    .iter()
                    .filter(|(s, _)| *s < l.length)
                    .map(|(s, n)| (a + s - from, n.min(l.length - s)))
                    .collect();
                crate::mix::duck(out, channels, out_rate, &spans, l.music_gain);
            }
            // The part of the voice in this window: from `max(a, from)`, `skip` seconds in.
            let at = (a - from).max(0.0);
            let skip = (from - a).max(0.0);
            let length = (b - from).min(window) - at;
            if length > 0.0 {
                crate::mix::add_voice_from(out, channels, out_rate, &l.audio, at, length, skip, l.voice_gain);
            }
        }
    }

    fn render_music(&self, from_secs: f64, out_rate: u32, out: &mut [f32], channels: usize) {
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
    /// Output gain, 0 (silent) to 1 (as recorded); applies at once, also while playing. Players
    /// without a level control ignore it.
    fn set_volume(&self, _gain: f32) {}
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
    gain: Mutex<Option<f32>>,
}

impl RecordingPlayer {
    /// The last gain set (`None` until `set_volume` is called).
    pub fn gain(&self) -> Option<f32> {
        self.gain.lock().ok().and_then(|g| *g)
    }

    /// What was asked so far: `play <seconds> from <from>` and `stop`.
    pub fn log(&self) -> Vec<String> {
        self.log.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

impl Player for RecordingPlayer {
    fn play(&self, program: Arc<Program>, from_secs: f64) -> Result<(), String> {
        if let Ok(mut l) = self.log.lock() {
            let voices = match program.layers().len() {
                0 => String::new(),
                1 => " + 1 voice".into(),
                n => format!(" + {n} voices"),
            };
            l.push(format!("play {:.1}s from {from_secs:.1}{voices}", program.seconds()));
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
    fn set_volume(&self, gain: f32) {
        if let Ok(mut g) = self.gain.lock() {
            *g = Some(gain);
        }
    }
}
