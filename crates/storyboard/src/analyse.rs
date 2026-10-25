//! A cue map from the analysis curves (`docs/track-analysis.md` §4, §5, §6.5; phase A2c).
//!
//! [`cue_map`] turns what `assetcraft-analysis` already measures ([`Curves`], [`Tempo`]) into
//! the [`CueMap`] v1 that Storyboard mode consumes, in the same shape `tools/cuemap.py` wrote:
//!
//! | Part       | Method (the Python bridge's rule, on the Rust curves)                                                            |
//! |------------|---------------------------------------------------------------------------------------------------------------------|
//! | Beats, BPM | [`Tempo`] (A2b): seconds → source frames                                                                             |
//! | Downbeats  | The bar phase (0–3) whose beats carry the most bass onset + onset strength; 4 beats to a bar                         |
//! | Sections   | Novelty between the 4 bars before and after each phrase start, on per-bar timbre (band energies, brightness, noisiness, onset density); peaks ≥ 8 bars apart, about one per 16 bars; energy = mean RMS normalised 0..1; labels by position and energy (Intro, Verse, Build, Chorus, Outro) |
//! | Hits       | **drop**: bass rises ≥ 6 dB over 2 bars at a bar line, local peak, ≥ 8 bars apart; **hit**: a downbeat in the top 5 % of onset strength |
//! | Fits       | **fade** for 30/60/90/120 s: the best bar line within −6…+1.5 s of the target (phrase, section change, hit, closeness, quietness), fade over 2 bars (≤ 5 s) |
//!
//! Deviations from the bridge, until A4 lands chroma and MFCC: section novelty runs on timbre
//! only, so section boundaries are coarser; the **edit** fit (splice to the song's own ending) needs
//! the same features to judge a splice and is left out, so `fits[*].edit` is `None`. A length with
//! no usable fade gets no `fits` entry at all (the bridge wrote `chosen: null`, which the generator
//! refuses anyway).
//!
//! Pure: no I/O. Every input is hostile: non-finite values count as 0, and anything that would make
//! an invalid cue map (no pulse, times past the end) is an [`AnalyseError`], never a panic. The
//! result passes [`CueMap::validate`] before it is returned.

use crate::cuemap::{
    CUEMAP_VERSION, CueHit, CueMap, CueMapError, CueSection, CueTrack, FadeOut, Fit, FitChoice, FitMode,
};
use assetcraft_analysis::{Curves, Tempo};
use std::collections::BTreeMap;
use std::fmt;

/// What wrote the cue map (`analyzer` field).
pub const ANALYZER: &str = concat!("assetcraft-analysis ", env!("CARGO_PKG_VERSION"));

/// Beats per bar (meter detection is A4).
pub const BEATS_PER_BAR: u8 = 4;
/// Fit to length targets, seconds.
pub const FIT_TARGETS: [u32; 4] = [30, 60, 90, 120];
/// A track must be this much longer than a target to be fitted to it, seconds.
const FIT_MARGIN: f64 = 5.0;
/// Bass rise that makes a drop, dB.
const DROP_DB: f32 = 6.0;
/// Bass rise that makes a full-strength drop, dB.
const DROP_FULL_DB: f32 = 12.0;
/// Bars between drops, and between section boundaries.
const MIN_GAP_BARS: usize = 8;
/// Bars either side of a phrase start the novelty compares.
const NOVELTY_BARS: usize = 4;
/// Licence written when nothing is known (the user fills it in; credits policy C1).
pub const UNKNOWN_LICENSE: &str = "unknown";

/// The audio file a cue map is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackFacts<'a> {
    /// File name (no folders), as `CueTrack::file`.
    pub file: &'a str,
    /// Source sample rate.
    pub sample_rate: u32,
    /// Track length in source frames.
    pub duration_frames: u64,
}

/// Why no cue map could be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyseError {
    /// Too little audio, or no onsets to find a pulse in.
    NoPulse,
    /// The file name has a folder in it, or the facts are out of range.
    BadFacts(String),
    /// The result failed the cue map's own checks (a bug or hostile input).
    Invalid(CueMapError),
}

impl fmt::Display for AnalyseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalyseError::NoPulse => write!(f, "no pulse found (too short, or silent)"),
            AnalyseError::BadFacts(e) => write!(f, "{e}"),
            AnalyseError::Invalid(e) => write!(f, "analysis produced a bad cue map: {e}"),
        }
    }
}

impl std::error::Error for AnalyseError {}

/// Frame conversions and curve lookups for one track.
struct Clock<'a> {
    c: &'a Curves,
    sample_rate: f64,
    end: u64,
}

impl Clock<'_> {
    /// Seconds → source frames, clamped to the track.
    fn frames(&self, t: f64) -> u64 {
        if !t.is_finite() || t <= 0.0 {
            return 0;
        }
        // `as` saturates; the clamp keeps every time inside the track.
        ((t * self.sample_rate).round() as u64).min(self.end)
    }

    /// Seconds → analysis frame index, clamped to the curves.
    fn index(&self, t: f64) -> usize {
        let fps = self.c.frame_rate();
        let n = self.c.frames();
        if !t.is_finite() || t <= 0.0 || fps <= 0.0 || n == 0 {
            return 0;
        }
        ((t * fps).floor() as usize).min(n - 1)
    }

    /// Track length, seconds.
    fn seconds(&self) -> f64 {
        self.end as f64 / self.sample_rate
    }
}

fn finite(x: f32) -> f32 {
    if x.is_finite() { x } else { 0.0 }
}

/// Mean of `v[a..b]` (at least one element, clamped to the slice; 0 for an empty slice).
fn mean_range(v: &[f32], a: usize, b: usize) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let a = a.min(v.len() - 1);
    let b = b.clamp(a + 1, v.len());
    v[a..b].iter().map(|x| finite(*x)).sum::<f32>() / (b - a) as f32
}

fn max(v: &[f32]) -> f32 {
    v.iter().map(|x| finite(*x)).fold(f32::MIN, f32::max)
}

/// Linear-interpolated percentile (numpy's default), `p` in 0..=100.
fn percentile(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s: Vec<f32> = v.iter().map(|x| finite(*x)).collect();
    s.sort_by(f32::total_cmp);
    let rank = (p / 100.0).clamp(0.0, 1.0) * (s.len() - 1) as f32;
    let (lo, hi) = (rank.floor() as usize, rank.ceil() as usize);
    let frac = rank - lo as f32;
    s[lo] + (s[hi.min(s.len() - 1)] - s[lo]) * frac
}

fn median(v: &[f32]) -> f32 {
    percentile(v, 50.0)
}

fn round3(x: f32) -> f32 {
    (x * 1000.0).round() / 1000.0
}

fn mmss(secs: f64) -> String {
    let r = secs.round().max(0.0) as u64;
    format!("{}:{:02}", r / 60, r % 60)
}

/// The bar phase: which of the first four beats starts a bar. The phase whose beats carry the
/// most bass onset plus onset strength wins (ties go to the earliest).
fn bar_phase(clock: &Clock, beats: &[f64]) -> usize {
    let bass_db = clock.c.bands_db.first().map(Vec::as_slice).unwrap_or(&[]);
    // Positive rises of linear bass amplitude, frame to frame.
    let bass_on: Vec<f32> = bass_db
        .iter()
        .enumerate()
        .map(|(i, db)| {
            let amp = |d: f32| 10f32.powf(finite(d) / 20.0);
            let prev = if i == 0 { amp(*db) } else { amp(bass_db[i - 1]) };
            (amp(*db) - prev).max(0.0)
        })
        .collect();
    let score = |p: usize| -> f32 {
        let idx: Vec<usize> = beats.iter().skip(p).step_by(4).map(|t| clock.index(*t)).collect();
        if idx.is_empty() {
            return f32::MIN;
        }
        let at = |v: &[f32]| idx.iter().map(|i| v.get(*i).copied().map_or(0.0, finite)).sum::<f32>() / idx.len() as f32;
        at(&bass_on) + at(&clock.c.onset)
    };
    (0..4)
        .fold((0usize, f32::MIN), |best, p| {
            let s = score(p);
            if s > best.1 { (p, s) } else { best }
        })
        .0
}

/// One timbre vector per bar (bar `i` spans `bars[i]..bars[i+1]`, the last to the end), each
/// feature scaled 0..1 across the track and each bar scaled to unit length, so the dot product
/// of two bars is how alike they sound.
fn bar_features(clock: &Clock, bars: &[f64]) -> Vec<Vec<f32>> {
    let c = clock.c;
    let n = bars.len();
    if n == 0 {
        return Vec::new();
    }
    let span = |i: usize| -> (usize, usize) {
        let a = clock.index(bars[i]);
        let b = if i + 1 < n { clock.index(bars[i + 1]) } else { c.frames() };
        (a, b.max(a + 1))
    };
    let mut raw: Vec<Vec<f32>> = (0..n)
        .map(|i| {
            let (a, b) = span(i);
            let mut f: Vec<f32> = c.bands_db.iter().map(|band| mean_range(band, a, b)).collect();
            f.push(mean_range(&c.centroid_hz, a, b).max(1.0).log2());
            f.push(mean_range(&c.flatness, a, b));
            f.push(mean_range(&c.onset, a, b));
            f
        })
        .collect();
    let dims = raw.first().map_or(0, Vec::len);
    for d in 0..dims {
        let lo = raw.iter().map(|f| f[d]).fold(f32::MAX, f32::min);
        let hi = raw.iter().map(|f| f[d]).fold(f32::MIN, f32::max);
        for f in &mut raw {
            f[d] = if hi > lo { (f[d] - lo) / (hi - lo) } else { 0.5 };
        }
    }
    for f in &mut raw {
        let norm = f.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
        f.iter_mut().for_each(|x| *x /= norm);
    }
    raw
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Mean of the vectors `feat[a..b]`.
fn centroid(feat: &[Vec<f32>], a: usize, b: usize) -> Vec<f32> {
    let dims = feat.first().map_or(0, Vec::len);
    let mut m = vec![0.0f32; dims];
    let rows = &feat[a.min(feat.len())..b.min(feat.len())];
    for f in rows {
        for (x, y) in m.iter_mut().zip(f) {
            *x += y;
        }
    }
    let n = rows.len().max(1) as f32;
    m.iter_mut().for_each(|x| *x /= n);
    m
}

/// Novelty per bar: how unlike the 4 bars before are to the 4 bars after (0 at the edges).
fn novelty(feat: &[Vec<f32>]) -> Vec<f32> {
    let nb = feat.len();
    let mut nov = vec![0.0f32; nb];
    for (i, slot) in nov.iter_mut().enumerate().take(nb.saturating_sub(NOVELTY_BARS)).skip(NOVELTY_BARS) {
        let a = centroid(feat, i - NOVELTY_BARS, i);
        let b = centroid(feat, i, i + NOVELTY_BARS);
        let na = dot(&a, &a).sqrt();
        let nb_ = dot(&b, &b).sqrt();
        *slot = 1.0 - dot(&a, &b) / (na * nb_ + 1e-9);
    }
    nov
}

/// Section boundaries as bar indices: novelty peaks on 4-bar phrase starts, at least 8 bars
/// apart, about one per 16 bars.
fn section_bounds(bars: &[f64], nov: &[f32]) -> Vec<usize> {
    let nb = nov.len().min(bars.len());
    let want = (bars.len() as f32 / 16.0).round().clamp(2.0, 10.0) as usize;
    let mut cand: Vec<usize> = (NOVELTY_BARS..nb.saturating_sub(NOVELTY_BARS)).step_by(4).collect();
    cand.sort_by(|a, b| nov[*b].total_cmp(&nov[*a]));
    let mut bounds: Vec<usize> = Vec::new();
    for i in cand {
        if bounds.iter().all(|j| i.abs_diff(*j) >= MIN_GAP_BARS) && nov[i] > 0.01 {
            bounds.push(i);
        }
        if bounds.len() + 1 >= want {
            break;
        }
    }
    bounds.sort_unstable();
    bounds
}

/// Sections covering the whole track, with energies and labels.
fn sections(clock: &Clock, bars: &[f64], bounds: &[usize]) -> Vec<CueSection> {
    let end = clock.end;
    let mut edges: Vec<u64> = vec![0];
    for i in bounds {
        if let Some(t) = bars.get(*i) {
            let f = clock.frames(*t);
            if f > edges[edges.len() - 1] && f < end {
                edges.push(f);
            }
        }
    }
    if end > edges[edges.len() - 1] {
        edges.push(end);
    } else {
        return Vec::new();
    }
    let rms = &clock.c.rms_db;
    let secs = |f: u64| f as f64 / clock.sample_rate;
    let energies: Vec<f32> =
        edges.windows(2).map(|w| mean_range(rms, clock.index(secs(w[0])), clock.index(secs(w[1])))).collect();
    let lo = energies.iter().copied().fold(f32::MAX, f32::min);
    let hi = energies.iter().copied().fold(f32::MIN, f32::max);
    let norm: Vec<f32> = energies.iter().map(|e| if hi > lo { (e - lo) / (hi - lo) } else { 0.5 }).collect();
    let n = norm.len();
    edges
        .windows(2)
        .zip(norm.iter().enumerate())
        .map(|(w, (k, e))| {
            let label = if k == 0 && *e < 0.6 {
                "Intro"
            } else if k + 1 == n && n > 1 {
                "Outro"
            } else if *e >= 0.75 {
                "Chorus"
            } else if norm.get(k + 1).is_some_and(|next| *next > e + 0.2) {
                "Build"
            } else {
                "Verse"
            };
            CueSection { start: w[0], end: w[1], label: label.into(), energy: round3(e.clamp(0.0, 1.0)) }
        })
        .collect()
}

/// Drops (bass rises at bar lines) and hits (the strongest downbeats), ascending.
fn hits(clock: &Clock, bars: &[f64]) -> Vec<CueHit> {
    let c = clock.c;
    let bass_db = c.bands_db.first().map(Vec::as_slice).unwrap_or(&[]);
    let n = bars.len();
    let level = |a: f64, b: f64| mean_range(bass_db, clock.index(a), clock.index(b));
    let rise: Vec<f32> = (0..n)
        .map(|i| if i >= 2 && i + 2 < n { level(bars[i], bars[i + 2]) - level(bars[i - 2], bars[i]) } else { -99.0 })
        .collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| rise[*b].total_cmp(&rise[*a]));
    let mut drops: Vec<(usize, CueHit)> = Vec::new();
    for i in order {
        if rise[i] < DROP_DB {
            break;
        }
        let local_max = rise[i.saturating_sub(2)..(i + 3).min(n)].iter().copied().fold(f32::MIN, f32::max);
        if rise[i] >= local_max && drops.iter().all(|(j, _)| i.abs_diff(*j) >= MIN_GAP_BARS) {
            drops.push((
                i,
                CueHit {
                    frame: clock.frames(bars[i]),
                    kind: "drop".into(),
                    strength: round3((rise[i] / DROP_FULL_DB).clamp(0.0, 1.0)),
                },
            ));
        }
    }
    let mut out: Vec<CueHit> = drops.into_iter().map(|(_, h)| h).collect();
    let strengths: Vec<f32> = bars.iter().map(|t| c.onset.get(clock.index(*t)).copied().map_or(0.0, finite)).collect();
    if !strengths.is_empty() {
        let thr = percentile(&strengths, 95.0);
        let top = max(&strengths) + 1e-9;
        let quarter = clock.frames(0.25);
        for (t, s) in bars.iter().zip(&strengths) {
            let frame = clock.frames(*t);
            if *s >= thr && out.iter().all(|h| frame.abs_diff(h.frame) > quarter) {
                out.push(CueHit { frame, kind: "hit".into(), strength: round3((s / top).clamp(0.0, 1.0)) });
            }
        }
    }
    out.sort_by_key(|h| h.frame);
    out
}

/// The fade fit for one target length (spec §6.5): the best bar line near the target.
fn fade_fit(clock: &Clock, bars: &[f64], nov: &[f32], bar_len: f64, target: u32) -> Option<Fit> {
    let c = clock.c;
    let target = f64::from(target);
    let onset_max = f64::from(max(&c.onset).max(0.0)) + 1e-9;
    let rms_median = f64::from(median(&c.rms_db));
    let mut best: Option<(f64, usize, f64)> = None;
    for (k, t) in bars.iter().enumerate() {
        if !(target - 6.0..=target + 1.5).contains(t) {
            continue;
        }
        let phrase = if k % 8 == 0 {
            1.2
        } else if k % 4 == 0 {
            0.7
        } else {
            0.0
        };
        let sect = nov.get(k).copied().map_or(0.0, |x| f64::from(finite(x)));
        let i = clock.index(*t);
        let hit = c.onset.get(i).copied().map_or(0.0, |x| f64::from(finite(x))) / onset_max;
        let loud = c.rms_db.get(i).copied().map_or(0.0, |x| f64::from(finite(x))) - rms_median;
        let score = phrase + 2.0 * sect + 0.6 * hit - 0.05 * (t - target).abs() - 0.02 * loud.max(0.0);
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, k, *t));
        }
    }
    let (_, k, end) = best?;
    let length = clock.frames(end);
    let fade = clock.frames((2.0 * bar_len).min(5.0)).min(length);
    if length == 0 || fade == 0 {
        return None;
    }
    let mut reasons = vec![format!("fade ends on bar {}", k + 1)];
    if k % 8 == 0 {
        reasons[0].push_str(" (8-bar phrase)");
    }
    Some(Fit {
        mode: FitMode::Fade,
        length_frames: length,
        segments: vec![[0, length]],
        crossfade_frames: 0,
        fade_out: Some(FadeOut { start: length - fade, frames: fade }),
        similarity: None,
        reasons,
        changes: format!("Cut to {} with a fade out", mmss(end)),
    })
}

/// A cue map for `facts` from its analysis `curves` and `tempo` (`assetcraft_analysis::tempo`
/// on the same curves). `None` for `tempo` means no pulse was found.
pub fn cue_map(facts: &TrackFacts, curves: &Curves, tempo: Option<&Tempo>) -> Result<CueMap, AnalyseError> {
    if facts.file.is_empty() || facts.file.contains(['/', '\\']) {
        return Err(AnalyseError::BadFacts("the track file must be a bare file name".into()));
    }
    if facts.sample_rate == 0 || facts.duration_frames == 0 {
        return Err(AnalyseError::BadFacts("the track has no sample rate or no length".into()));
    }
    let tempo = tempo.ok_or(AnalyseError::NoPulse)?;
    if !(tempo.bpm.is_finite() && tempo.bpm > 0.0) {
        return Err(AnalyseError::NoPulse);
    }
    let clock = Clock { c: curves, sample_rate: f64::from(facts.sample_rate), end: facts.duration_frames };

    // Beats and bars, in seconds while scoring, then frames (strictly ascending, inside the track).
    let beat_times: Vec<f64> = tempo.beats.iter().copied().filter(|t| t.is_finite() && *t >= 0.0).collect();
    let phase = bar_phase(&clock, &beat_times);
    let bar_times: Vec<f64> = beat_times.iter().skip(phase).step_by(usize::from(BEATS_PER_BAR)).copied().collect();
    let to_frames = |times: &[f64]| -> Vec<u64> {
        let mut out: Vec<u64> = Vec::with_capacity(times.len());
        for t in times {
            let f = clock.frames(*t);
            if out.last().is_none_or(|last| f > *last) && f < clock.end {
                out.push(f);
            }
        }
        out
    };
    let beats = to_frames(&beat_times);
    let downbeats = to_frames(&bar_times);

    let feat = bar_features(&clock, &bar_times);
    let nov = novelty(&feat);
    let bounds = section_bounds(&bar_times, &nov);
    let sections = sections(&clock, &bar_times, &bounds);
    let hits = hits(&clock, &bar_times);

    let bar_len = 60.0 / tempo.bpm * f64::from(BEATS_PER_BAR);
    let mut fits = BTreeMap::new();
    for target in FIT_TARGETS {
        if clock.seconds() < f64::from(target) + FIT_MARGIN {
            continue;
        }
        if let Some(fade) = fade_fit(&clock, &bar_times, &nov, bar_len, target) {
            fits.insert(target.to_string(), FitChoice { chosen: Some(FitMode::Fade), fade: Some(fade), edit: None });
        }
    }

    let title = facts.file.rsplit_once('.').map_or(facts.file, |(stem, _)| stem).to_string();
    let map = CueMap {
        version: CUEMAP_VERSION,
        analyzer: ANALYZER.into(),
        track: CueTrack {
            file: facts.file.to_string(),
            title,
            artist: String::new(),
            license: UNKNOWN_LICENSE.into(),
            credit: String::new(),
        },
        sample_rate: facts.sample_rate,
        duration_frames: facts.duration_frames,
        bpm: (tempo.bpm * 100.0).round() / 100.0,
        beats_per_bar: BEATS_PER_BAR,
        pulse_clarity: round3(finite(tempo.pulse_clarity).clamp(0.0, 1.0)),
        beats,
        downbeats,
        sections,
        hits,
        fits,
    };
    map.validate().map_err(AnalyseError::Invalid)?;
    Ok(map)
}
