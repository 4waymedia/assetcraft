//! Analysis curves (`docs/track-analysis.md` §4.1, 4.3–4.5; phase A2a).
//!
//! One short-time Fourier transform (Hann window, [`FRAME`] samples, every [`HOP`]) gives every
//! curve. Frames are **centred**: frame `i` covers the samples around `i × HOP` (the signal is
//! padded with half a frame of silence at each end), so frame `i` is at time `i × HOP / RATE`, the
//! same convention as librosa, which the oracle compares against (§11).
//!
//! | Curve        | Unit                  | What                                                                  |
//! |--------------|-----------------------|-----------------------------------------------------------------------|
//! | `rms_db`     | dBFS                  | RMS of the frame (a full-scale sine reads −3 dB); [`FLOOR_DB`] for silence |
//! | `bands_db`   | dBFS                  | Mean-square energy in each of [`BANDS`], on the same scale as `rms_db` |
//! | `onset`      | flux (≥ 0, unscaled)  | Vibrato-suppressed spectral flux on log-magnitude bands: how much the spectrum rises against the frame before (SoundCraft's `detect_transients` method, kept as a curve) |
//! | `centroid_hz`| Hz                    | Spectral centroid (brightness); 0 for silence                          |
//! | `flatness`   | 0–1                   | Geometric over arithmetic mean of the power spectrum (1 = noise, 0 = pure tone); 0 for silence |

use crate::{FRAME, HOP, RATE};
use rustfft::{FftPlanner, num_complex::Complex};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Lowest level reported, in dB (silence).
pub const FLOOR_DB: f32 = -120.0;
/// Below this mean-square power a frame is silence for centroid and flatness (−100 dBFS).
const SILENT_MS: f32 = 1e-10;

/// A frequency band for `bands_db`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Band {
    /// Id used in documents.
    pub id: &'static str,
    /// Lowest frequency, Hz.
    pub lo: f32,
    /// Highest frequency, Hz.
    pub hi: f32,
}

/// The default bands (§4.3): sub/bass, low-mid, mid, high.
pub const BANDS: [Band; 4] = [
    Band { id: "bass", lo: 20.0, hi: 150.0 },
    Band { id: "lowMid", lo: 150.0, hi: 500.0 },
    Band { id: "mid", lo: 500.0, hi: 2_000.0 },
    Band { id: "high", lo: 2_000.0, hi: 8_000.0 },
];

/// The analysis curves, one value per frame.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Curves {
    /// Sample rate the curves were computed at.
    pub rate: u32,
    /// Samples between frames.
    pub hop: usize,
    /// Frame RMS, dBFS.
    pub rms_db: Vec<f32>,
    /// Energy per band of [`BANDS`], dBFS: `bands_db[b][frame]`.
    pub bands_db: Vec<Vec<f32>>,
    /// Onset strength (spectral flux).
    pub onset: Vec<f32>,
    /// Spectral centroid, Hz.
    pub centroid_hz: Vec<f32>,
    /// Spectral flatness, 0–1.
    pub flatness: Vec<f32>,
}

impl Curves {
    /// Number of frames.
    pub fn frames(&self) -> usize {
        self.rms_db.len()
    }

    /// Time of frame `i`, seconds.
    pub fn time_of(&self, i: usize) -> f64 {
        if self.rate == 0 { 0.0 } else { (i * self.hop) as f64 / f64::from(self.rate) }
    }

    /// Frames per second.
    pub fn frame_rate(&self) -> f64 {
        if self.hop == 0 { 0.0 } else { f64::from(self.rate) / self.hop as f64 }
    }
}

fn db(ms: f32) -> f32 {
    if ms > 0.0 && ms.is_finite() { (10.0 * ms.log10()).max(FLOOR_DB) } else { FLOOR_DB }
}

/// Triangular filters, `per_octave` to the octave from 30 Hz to 10 kHz over the bins of a
/// [`FRAME`]-point FFT, without duplicates where bins are coarser; each filter's weights sum to 1.
fn log_bands(per_octave: f32) -> Vec<Vec<(usize, f32)>> {
    let bin_hz = RATE as f32 / FRAME as f32;
    let mut centres: Vec<usize> = Vec::new();
    let mut f = 30.0f32;
    while f <= 10_000.0 {
        let k = (f / bin_hz).round() as usize;
        if k >= 1 && centres.last() != Some(&k) {
            centres.push(k);
        }
        f *= 2f32.powf(1.0 / per_octave);
    }
    (0..centres.len())
        .map(|i| {
            let c = centres[i];
            let lo = if i == 0 { c.saturating_sub(1).max(1) } else { centres[i - 1] };
            let hi = centres.get(i + 1).copied().unwrap_or(c + 1);
            let mut w: Vec<(usize, f32)> = (lo..=hi)
                .map(|k| {
                    let v = if k <= c {
                        if c == lo { 1.0 } else { (k - lo) as f32 / (c - lo) as f32 }
                    } else if hi == c {
                        1.0
                    } else {
                        (hi - k) as f32 / (hi - c) as f32
                    };
                    (k, v)
                })
                .filter(|(_, v)| *v > 0.0)
                .collect();
            let sum: f32 = w.iter().map(|(_, v)| v).sum();
            w.iter_mut().for_each(|(_, v)| *v /= sum.max(1e-9));
            w
        })
        .collect()
}

/// Every curve of `signal` (mono at [`RATE`]). Non-finite samples count as silence.
pub fn curves(signal: &[f32]) -> Curves {
    let mut out = Curves { rate: RATE, hop: HOP, bands_db: vec![Vec::new(); BANDS.len()], ..Curves::default() };
    if signal.is_empty() {
        return out;
    }
    let half = FRAME / 2;
    let frames = signal.len() / HOP + 1;
    let sample = |i: i64| -> f32 {
        usize::try_from(i).ok().and_then(|i| signal.get(i)).copied().filter(|v| v.is_finite()).unwrap_or(0.0)
    };
    let window: Vec<f32> = (0..FRAME).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FRAME as f32).cos()).collect();
    let win_ss: f32 = window.iter().map(|w| w * w).sum();
    let bin_hz = RATE as f32 / FRAME as f32;
    let nbins = FRAME / 2 + 1;
    // One-sided power → mean square: Parseval with the window's energy (a full-scale sine → 0.5).
    let ms_scale = 2.0 / (FRAME as f32 * win_ss);
    let band_bins: Vec<(usize, usize)> = BANDS
        .iter()
        .map(|b| (((b.lo / bin_hz).ceil() as usize).max(1), ((b.hi / bin_hz).floor() as usize).min(nbins - 1)))
        .collect();
    let flux_bands = log_bands(24.0);
    // Magnitudes relative to a full-scale sine; below about −80 dB a band's changes barely count.
    let gain = 1.0e4 / (FRAME as f32 / 4.0);
    let fft = FftPlanner::<f32>::new().plan_fft_forward(FRAME);
    let mut buf = vec![Complex::new(0.0f32, 0.0); FRAME];
    let mut power = vec![0.0f32; nbins];
    let mut prev: Vec<f32> = Vec::new();
    for m in 0..frames {
        let start = (m * HOP) as i64 - half as i64;
        let mut sq = 0.0f32;
        for (i, b) in buf.iter_mut().enumerate() {
            let s = sample(start + i as i64);
            sq += s * s;
            *b = Complex::new(s * window[i], 0.0);
        }
        fft.process(&mut buf);
        for (p, b) in power.iter_mut().zip(&buf) {
            *p = b.norm_sqr();
        }
        out.rms_db.push(db(sq / FRAME as f32));
        for (b, &(lo, hi)) in band_bins.iter().enumerate() {
            let e: f32 = power.get(lo..=hi).map_or(0.0, |s| s.iter().sum());
            out.bands_db[b].push(db(e * ms_scale));
        }
        // Centroid and flatness over bins 1.. (DC carries no pitch).
        let body = &power[1..];
        let total: f32 = body.iter().sum();
        if total * ms_scale < SILENT_MS {
            out.centroid_hz.push(0.0);
            out.flatness.push(0.0);
        } else {
            let mags: Vec<f32> = body.iter().map(|p| p.sqrt()).collect();
            let msum: f32 = mags.iter().sum();
            let centroid = mags.iter().enumerate().map(|(k, m)| (k + 1) as f32 * bin_hz * m).sum::<f32>() / msum;
            out.centroid_hz.push(centroid);
            let floor = total / body.len() as f32 * 1e-10;
            let log_mean = body.iter().map(|p| (p + floor).ln()).sum::<f32>() / body.len() as f32;
            let flat = log_mean.exp() / (total / body.len() as f32);
            out.flatness.push(if flat.is_finite() { flat.clamp(0.0, 1.0) } else { 0.0 });
        }
        // Onset strength: each log band's rise against the loudest of its neighbours a frame ago.
        let cur: Vec<f32> = flux_bands
            .iter()
            .map(|band| {
                (1.0 + gain * band.iter().map(|&(k, w)| power.get(k).map_or(0.0, |p| p.sqrt()) * w).sum::<f32>())
                    .log10()
            })
            .collect();
        let flux = if prev.is_empty() {
            0.0
        } else {
            cur.iter()
                .enumerate()
                .map(|(b, v)| {
                    let was = prev
                        .get(b.saturating_sub(1)..(b + 2).min(prev.len()))
                        .map_or(0.0, |s| s.iter().copied().fold(0.0, f32::max));
                    (v - was).max(0.0)
                })
                .sum()
        };
        out.onset.push(flux);
        prev = cur;
    }
    out
}
