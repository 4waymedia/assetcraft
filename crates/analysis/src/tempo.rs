//! Tempo and beats (`docs/track-analysis.md` §4.1; phase A2b).
//!
//! 1. **Candidates:** autocorrelation of the onset curve (mean removed) over 30–300 BPM, weighted
//!    by a log-normal prior centred on 120 BPM (one octave wide), local peaks refined to a
//!    fraction of a frame. The best three are kept, and the best one's ×½ and ×2 are always
//!    reported with their scores, so the UI can offer them with one click.
//! 2. **Twice as fast?** After tracking, the onset strength halfway between beats is compared
//!    with the strength on them (`offbeat`). When the off-beats are about as strong as the beats
//!    (≥ [`DOUBLE_OFFBEAT`]), every half-beat is as much a beat, so the tempo doubles and the
//!    beats are tracked again (equal clicks at 174 BPM are 174, not 87; a track read at 63 whose
//!    snare lands halfway is 126). Kick-and-snare music with quieter hats in between stays.
//! 3. **Beats:** dynamic-programming beat tracker (Ellis 2007, *Beat Tracking by Dynamic
//!    Programming*): maximise onset strength at beats minus `TIGHTNESS × log(interval / period)²`.
//!    Weak beats before the music starts and after it ends are trimmed.
//! 4. **BPM:** the slope of a straight line through the beat times (precise to a small fraction of
//!    a BPM over a track, where one frame of lag is ±2–5 %).
//! 5. **Confidence:** pulse clarity (the normalised autocorrelation at the beat period, 0–1) and
//!    the coefficient of variation of the beat intervals. Clarity < 0.25 or variation > 0.08 marks
//!    the track **free time**: no beat grid by default.
//!
//! Constant tempo only in v1 (decision AD3); step changes at bar lines come with the tempo map.

use crate::Curves;
use serde::{Deserialize, Serialize};

/// Slowest tempo considered, BPM.
pub const MIN_BPM: f64 = 30.0;
/// Fastest tempo considered, BPM.
pub const MAX_BPM: f64 = 300.0;
/// Centre of the tempo prior, BPM.
const PRIOR_BPM: f64 = 120.0;
/// Width of the tempo prior, octaves.
const PRIOR_OCTAVES: f64 = 1.0;
/// Fastest tempo the "twice as fast" rule may pick, BPM.
const DOUBLE_LIMIT: f64 = 200.0;
/// Off-beat strength, relative to the beats, at which the tempo doubles.
pub const DOUBLE_OFFBEAT: f32 = 0.8;
/// How strictly beats keep to the period (librosa's default).
const TIGHTNESS: f64 = 100.0;
/// Below this pulse clarity a track is free time.
pub const FREE_CLARITY: f32 = 0.25;
/// Above this beat-interval variation a track is free time.
pub const FREE_CV: f32 = 0.08;

/// A tempo the analysis considered.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Tempo, BPM.
    pub bpm: f64,
    /// Weighted autocorrelation, relative to the best candidate (best = 1).
    pub score: f32,
}

/// Whether the track has a steady pulse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rhythm {
    /// A beat grid fits.
    Steady,
    /// No reliable pulse: cuts follow sections, swells and phrases instead.
    Free,
}

/// Tempo and beats of a track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tempo {
    /// Tempo, BPM.
    pub bpm: f64,
    /// The best candidates (up to 3), best first; then the chosen tempo's ×½ and ×2.
    pub candidates: Vec<Candidate>,
    /// Half and double of [`Tempo::bpm`], with their scores (the one-click alternatives).
    pub half: Candidate,
    /// See [`Tempo::half`].
    pub double: Candidate,
    /// Beat times, seconds.
    pub beats: Vec<f64>,
    /// Normalised autocorrelation at the beat period, 0–1.
    pub pulse_clarity: f32,
    /// Coefficient of variation of the beat intervals.
    pub stability_cv: f32,
    /// Mean onset strength halfway between beats, relative to on them (≈ 1 when the half-beats
    /// are as strong as the beats).
    pub offbeat: f32,
    /// Steady or free time.
    pub rhythm: Rhythm,
}

/// Onset curve with its mean removed (non-finite values count as 0).
fn centred(onset: &[f32]) -> Vec<f64> {
    let v: Vec<f64> = onset.iter().map(|x| if x.is_finite() { f64::from(*x) } else { 0.0 }).collect();
    let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
    v.iter().map(|x| x - mean).collect()
}

/// Autocorrelation of `x` for lags `0..=max_lag`, divided by the number of terms.
fn autocorrelation(x: &[f64], max_lag: usize) -> Vec<f64> {
    (0..=max_lag.min(x.len().saturating_sub(1)))
        .map(|l| {
            let n = x.len() - l;
            x[..n].iter().zip(&x[l..]).map(|(a, b)| a * b).sum::<f64>() / n.max(1) as f64
        })
        .collect()
}

/// `ac` at a fractional lag (linear interpolation; 0 outside).
fn ac_at(ac: &[f64], lag: f64) -> f64 {
    if lag.is_nan() || lag < 0.0 {
        return 0.0;
    }
    let i = lag.floor() as usize;
    match (ac.get(i), ac.get(i + 1)) {
        (Some(a), Some(b)) => a + (b - a) * (lag - i as f64),
        (Some(a), None) => *a,
        _ => 0.0,
    }
}

fn prior(bpm: f64) -> f64 {
    let o = (bpm / PRIOR_BPM).log2() / PRIOR_OCTAVES;
    (-0.5 * o * o).exp()
}

/// Ellis's DP beat tracker over `score` (one value per frame) at `period` frames. Returns beat
/// frames, ascending.
fn track(score: &[f64], period: f64) -> Vec<usize> {
    let n = score.len();
    if n == 0 || period < 1.0 {
        return Vec::new();
    }
    let mut cum = vec![0.0f64; n];
    let mut back: Vec<Option<usize>> = vec![None; n];
    let (lo, hi) = ((period / 2.0).round() as usize, (2.0 * period).round() as usize);
    for i in 0..n {
        let mut best: Option<(f64, usize)> = None;
        for d in lo.max(1)..=hi {
            let Some(j) = i.checked_sub(d) else { break };
            let pen = (d as f64 / period).ln();
            let v = cum[j] - TIGHTNESS * pen * pen;
            if best.is_none_or(|(b, _)| v > b) {
                best = Some((v, j));
            }
        }
        match best {
            Some((v, j)) if v > 0.0 => {
                cum[i] = score[i] + v;
                back[i] = Some(j);
            }
            _ => cum[i] = score[i],
        }
    }
    // End on the best cumulative score among the local maxima of the last period.
    let tail = n.saturating_sub(period.ceil() as usize + 1);
    let maxima: Vec<usize> =
        (tail..n).filter(|&i| (i == 0 || cum[i] >= cum[i - 1]) && (i + 1 == n || cum[i] >= cum[i + 1])).collect();
    let median = {
        let mut m: Vec<f64> = maxima.iter().map(|&i| cum[i]).collect();
        m.sort_by(f64::total_cmp);
        m.get(m.len() / 2).copied().unwrap_or(0.0)
    };
    let Some(mut at) = maxima.iter().copied().rev().find(|&i| cum[i] >= 0.5 * median).or(maxima.last().copied()) else {
        return Vec::new();
    };
    let mut beats = vec![at];
    while let Some(j) = back[at] {
        beats.push(j);
        at = j;
    }
    beats.reverse();
    beats
}

/// Smooth the onset curve into the tracker's local score (a Gaussian ~1/32 of a beat wide),
/// normalised by its standard deviation.
fn local_score(x: &[f64], period: f64) -> Vec<f64> {
    let sd = (x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64).sqrt().max(1e-9);
    let sigma = (period / 32.0).max(0.5);
    let half = (3.0 * sigma).ceil() as i64;
    let kernel: Vec<f64> = (-half..=half).map(|k| (-0.5 * (k as f64 / sigma).powi(2)).exp()).collect();
    (0..x.len() as i64)
        .map(|i| {
            kernel
                .iter()
                .enumerate()
                .map(|(k, w)| usize::try_from(i + k as i64 - half).ok().and_then(|j| x.get(j)).map_or(0.0, |v| v * w))
                .sum::<f64>()
                / sd
        })
        .collect()
}

/// Mean `score` halfway between consecutive beats over the mean on the beats (0 when there are
/// too few beats or no strength on them).
fn offbeat_ratio(score: &[f64], beats: &[usize]) -> f32 {
    if beats.len() < 4 {
        return 0.0;
    }
    let on = beats.iter().map(|&b| score.get(b).copied().unwrap_or(0.0).max(0.0)).sum::<f64>() / beats.len() as f64;
    let mids: Vec<f64> = beats
        .windows(2)
        .map(|w| {
            // The strongest point within a frame of the midpoint (beats sit on frame peaks too).
            let m = (w[0] + w[1]) / 2;
            score
                .get(m.saturating_sub(1)..(m + 2).min(score.len()))
                .map_or(0.0, |s| s.iter().copied().fold(0.0, f64::max))
        })
        .collect();
    let off = mids.iter().sum::<f64>() / mids.len().max(1) as f64;
    if on > 1e-9 { (off / on).clamp(0.0, 2.0) as f32 } else { 0.0 }
}

/// Least-squares slope of `y` against its index.
fn slope(y: &[f64]) -> Option<f64> {
    let n = y.len() as f64;
    if y.len() < 2 {
        return None;
    }
    let mx = (n - 1.0) / 2.0;
    let my = y.iter().sum::<f64>() / n;
    let (num, den) = y.iter().enumerate().fold((0.0, 0.0), |(a, b), (i, v)| {
        let dx = i as f64 - mx;
        (a + dx * (v - my), b + dx * dx)
    });
    (den > 0.0).then(|| num / den)
}

/// Tempo and beats from the analysis curves. `None` when there is too little audio (under four
/// seconds) or no onsets at all.
pub fn tempo(c: &Curves) -> Option<Tempo> {
    let fps = c.frame_rate();
    if fps <= 0.0 || (c.frames() as f64) < 4.0 * fps {
        return None;
    }
    let x = centred(&c.onset);
    let max_lag = (60.0 * fps / MIN_BPM).ceil() as usize;
    let ac = autocorrelation(&x, max_lag);
    let zero = ac.first().copied().unwrap_or(0.0);
    if zero <= 1e-12 {
        return None;
    }
    let lag_of = |bpm: f64| 60.0 * fps / bpm;
    let bpm_of = |lag: f64| 60.0 * fps / lag;
    // Weighted peaks, refined by a parabola through the peak and its neighbours.
    let (lo, hi) =
        (lag_of(MAX_BPM).floor() as usize, (lag_of(MIN_BPM).ceil() as usize).min(ac.len().saturating_sub(2)));
    let mut peaks: Vec<(f64, f64)> = Vec::new();
    for l in lo.max(1)..=hi {
        let (a, b, d) = (ac[l - 1], ac[l], ac[l + 1]);
        if b > 0.0 && b >= a && b >= d {
            let den = a - 2.0 * b + d;
            let off = if den.abs() > 1e-12 { (0.5 * (a - d) / den).clamp(-0.5, 0.5) } else { 0.0 };
            let lag = l as f64 + off;
            peaks.push((lag, ac_at(&ac, lag) * prior(bpm_of(lag))));
        }
    }
    peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
    let &(mut lag, best) = peaks.first()?;
    let weighted = |l: f64| (ac_at(&ac, l) * prior(bpm_of(l)) / best.max(1e-12)) as f32;
    let candidates: Vec<Candidate> =
        peaks.iter().take(3).map(|&(l, s)| Candidate { bpm: bpm_of(l), score: (s / best.max(1e-12)) as f32 }).collect();
    // Beats at that period; twice as fast while the off-beats are as strong as the beats.
    let mut score = local_score(&x, lag);
    let mut frames = track(&score, lag);
    let mut offbeat = offbeat_ratio(&score, &frames);
    for _ in 0..2 {
        if offbeat < DOUBLE_OFFBEAT || bpm_of(lag / 2.0) > DOUBLE_LIMIT {
            break;
        }
        lag /= 2.0;
        score = local_score(&x, lag);
        frames = track(&score, lag);
        offbeat = offbeat_ratio(&score, &frames);
    }
    // Trim weak beats where the music hasn't started or has stopped.
    let strength: Vec<f64> = frames.iter().map(|&f| score[f]).collect();
    let rms = (strength.iter().map(|s| s * s).sum::<f64>() / strength.len().max(1) as f64).sqrt();
    while frames.first().is_some_and(|&f| score[f] < 0.5 * rms) {
        frames.remove(0);
    }
    while frames.last().is_some_and(|&f| score[f] < 0.5 * rms) {
        frames.pop();
    }
    let beats: Vec<f64> = frames.iter().map(|&f| c.time_of(f)).collect();
    let bpm = match slope(&beats) {
        Some(s) if s > 0.0 && beats.len() >= 8 => 60.0 / s,
        _ => bpm_of(lag),
    };
    let iois: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    let mean_ioi = iois.iter().sum::<f64>() / iois.len().max(1) as f64;
    let cv = if iois.len() >= 2 && mean_ioi > 0.0 {
        (iois.iter().map(|d| (d - mean_ioi).powi(2)).sum::<f64>() / iois.len() as f64).sqrt() / mean_ioi
    } else {
        1.0
    };
    let clarity = (ac_at(&ac, lag_of(bpm)) / zero).clamp(0.0, 1.0) as f32;
    let rhythm = if clarity < FREE_CLARITY || cv as f32 > FREE_CV { Rhythm::Free } else { Rhythm::Steady };
    let half = Candidate { bpm: bpm / 2.0, score: weighted(lag_of(bpm / 2.0)) };
    let double = Candidate { bpm: bpm * 2.0, score: weighted(lag_of(bpm * 2.0)) };
    Some(Tempo {
        bpm,
        candidates,
        half,
        double,
        beats,
        pulse_clarity: clarity,
        stability_cv: cv as f32,
        offbeat,
        rhythm,
    })
}
