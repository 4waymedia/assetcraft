//! Exact time and beat arithmetic.
//!
//! Durations are integer [`Ticks`] at FilmCraft's rate (`crates/core` `Tick`, 254,016,000,000 per
//! second, divisible by every common frame and sample rate), so summing clips never drifts and an
//! OTIO export can carry exact values. Seconds as `f64` appear only at the edges (UI, JSON
//! convenience) and are converted with checks.

use serde::{Deserialize, Serialize};

/// Ticks per second (FilmCraft's rate).
pub const TICKS_PER_SECOND: u64 = 254_016_000_000;

/// Slowest tempo accepted, in beats per minute.
pub const MIN_BPM: f64 = 20.0;
/// Fastest tempo accepted, in beats per minute.
pub const MAX_BPM: f64 = 400.0;

/// A duration or position in ticks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ticks(pub u64);

impl Ticks {
    /// Zero.
    pub const ZERO: Ticks = Ticks(0);

    /// Whole seconds.
    pub fn from_secs(secs: u64) -> Option<Ticks> {
        secs.checked_mul(TICKS_PER_SECOND).map(Ticks)
    }

    /// Fractional seconds, rounded to the nearest tick. `None` for negative, non-finite or
    /// out-of-range values.
    pub fn from_secs_f64(secs: f64) -> Option<Ticks> {
        if !secs.is_finite() || secs < 0.0 {
            return None;
        }
        let ticks = (secs * TICKS_PER_SECOND as f64).round();
        // u64::MAX as f64 rounds up to 2^64; stay strictly below it.
        if ticks >= u64::MAX as f64 {
            return None;
        }
        Some(Ticks(ticks as u64))
    }

    /// Seconds as `f64` (for display; not for arithmetic).
    pub fn as_secs_f64(self) -> f64 {
        self.0 as f64 / TICKS_PER_SECOND as f64
    }

    /// Checked addition.
    pub fn checked_add(self, other: Ticks) -> Option<Ticks> {
        self.0.checked_add(other.0).map(Ticks)
    }

    /// `m:ss.s`, the toolbar timecode format.
    pub fn timecode(self) -> String {
        let tenths = (self.0 as u128 * 10 + TICKS_PER_SECOND as u128 / 2) / TICKS_PER_SECOND as u128;
        let secs = tenths / 10;
        format!("{}:{:02}.{}", secs / 60, secs % 60, tenths % 10)
    }
}

/// A tempo in beats per minute, always finite and within [`MIN_BPM`]..=[`MAX_BPM`].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Bpm(f64);

impl Bpm {
    /// 120 BPM, the usual default tempo.
    pub const DEFAULT: Bpm = Bpm(120.0);

    /// A checked tempo.
    pub fn new(bpm: f64) -> Option<Bpm> {
        (bpm.is_finite() && (MIN_BPM..=MAX_BPM).contains(&bpm)).then_some(Bpm(bpm))
    }

    /// The tempo.
    pub fn get(self) -> f64 {
        self.0
    }

    /// Length of one beat, `60 / bpm` seconds, rounded to the nearest tick (never zero).
    pub fn beat(self) -> Ticks {
        let t = (60.0 * TICKS_PER_SECOND as f64 / self.0).round();
        Ticks((t as u64).max(1))
    }

    /// `beats` beats as ticks (saturating).
    pub fn beats_to_ticks(self, beats: u32) -> Ticks {
        Ticks(self.beat().0.saturating_mul(u64::from(beats)))
    }

    /// Nearest whole number of beats in `d` (the design's `round(duration / (60 / BPM))`).
    pub fn ticks_to_beats(self, d: Ticks) -> u32 {
        let beat = self.beat().0;
        let beats = (d.0 / beat).saturating_add(u64::from(d.0 % beat >= beat.div_ceil(2)));
        u32::try_from(beats).unwrap_or(u32::MAX)
    }
}

impl<'de> Deserialize<'de> for Bpm {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = f64::deserialize(d)?;
        Bpm::new(v).ok_or_else(|| {
            serde::de::Error::custom(format_args!("bpm must be between {MIN_BPM} and {MAX_BPM}, got {v}"))
        })
    }
}
