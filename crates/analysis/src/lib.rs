//! Track analysis (`docs/track-analysis.md` §4, phase A2): pure functions over the analysis signal,
//! the mono mix at [`RATE`] (`assetcraft_audio::Audio::analysis_signal`). No I/O, no threads; the
//! engine runs it in the background and caches the result.
//!
//! - [`curves`] (A2a): loudness, band energies, onset strength, brightness and noisiness every
//!   [`HOP`] samples (23.2 ms).
//! - [`tempo`] (A2b): tempo candidates with ×½ and ×2, beats, pulse clarity, free time.
//!
//! Every input is treated as hostile: non-finite samples count as silence, and an empty or tiny
//! signal gives empty curves, never a panic.

pub mod curves;
pub mod tempo;

pub use curves::{BANDS, Band, Curves, curves};
pub use tempo::{Candidate, Rhythm, Tempo, tempo};

/// Sample rate of the analysis signal.
pub const RATE: u32 = 22_050;
/// Hop between analysis frames, in samples (23.2 ms at [`RATE`]).
pub const HOP: usize = 512;
/// Analysis frame (FFT) length, in samples (92.9 ms at [`RATE`]).
pub const FRAME: usize = 2048;

#[cfg(test)]
mod tests;
