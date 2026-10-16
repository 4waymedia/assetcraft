//! Track audio for Storyboard mode (`docs/track-analysis.md` §3, phase A1).
//!
//! Layer 1; depends on `assetcraft-fs` (reading) and `soundcraft-audio-io` (decoding), nothing
//! else of ours. Decoding is SoundCraft's: WAV and AIFF with native readers, MP3, AAC, FLAC, Ogg
//! and the rest through symphonia, with symphonia's panics turned into errors. This crate adds
//! the Storyboard limits and the views later phases need:
//! - [`load`] / [`Audio::decode`]: whole file → planar `f32`, refusing files over
//!   [`MAX_FILE_BYTES`], longer than [`MAX_SECONDS`] (SoundCraft's Beat Detective limit), with
//!   more than [`MAX_CHANNELS`] channels or a rate outside [`MIN_RATE`]..=[`MAX_RATE`]. The
//!   limits are checked from the header before decoding and again after. Non-finite samples
//!   become silence.
//! - [`Audio::mono`]: the channel mean, the analysis signal.
//! - [`Audio::peaks`]: SoundCraft's multi-resolution min/max overview, for the waveform.
//! - [`Audio::summary`]: duration, rate, channels, peak and RMS (`assetcraft-cli analyze`).
//! - [`Audio::stereo_at`]: one frame as left/right for playback (mono doubles; extra channels
//!   are dropped, not mixed, so a 5.1 stem plays its front pair).
//!
//! - [`program`]: the track through its audio edit (segments, crossfades, fade-out), rendered
//!   at the output device's rate, and the [`Player`] trait the engine drives.
//!
//! Every time is a sample frame at the source rate, so nothing here drifts.

use assetcraft_fs::Services;
use serde::Serialize;
use soundcraft_audio_io as sio;
use std::path::Path;

pub mod program;

pub use program::{Edit, Player, Program, RecordingPlayer, SilentPlayer};
pub use sio::peaks::Peaks;

/// Longest track, in seconds (20 minutes).
pub const MAX_SECONDS: f64 = 20.0 * 60.0;
/// Most channels.
pub const MAX_CHANNELS: usize = 8;
/// Lowest sample rate.
pub const MIN_RATE: u32 = 8_000;
/// Highest sample rate.
pub const MAX_RATE: u32 = 384_000;
/// Largest file read (512 MiB: a 20-minute 96 kHz stereo 32-bit WAV is about 920 MiB, but no
/// track a storyboard uses comes close; compressed tracks are a few MiB).
pub const MAX_FILE_BYTES: usize = 512 << 20;

/// Crate name, for `--version` lines.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// Why a track couldn't be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// The file couldn't be read.
    Read(String),
    /// Over a limit (size, length, channels, rate).
    Limit(String),
    /// A format or codec the decoder doesn't support.
    Unsupported(String),
    /// Corrupt, or not audio at all.
    Malformed(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::Read(m) => write!(f, "can't read the track: {m}"),
            AudioError::Limit(m) => write!(f, "track refused: {m}"),
            AudioError::Unsupported(m) => write!(f, "unsupported audio: {m}"),
            AudioError::Malformed(m) => write!(f, "not readable audio: {m}"),
        }
    }
}

impl std::error::Error for AudioError {}

impl From<sio::AudioError> for AudioError {
    fn from(e: sio::AudioError) -> AudioError {
        match e {
            sio::AudioError::Unsupported(m) => AudioError::Unsupported(m),
            sio::AudioError::TooLarge(m) => AudioError::Limit(m),
            sio::AudioError::Malformed(m) => AudioError::Malformed(m),
            sio::AudioError::Encode(m) => AudioError::Malformed(m),
            sio::AudioError::Io(e) => AudioError::Read(e.to_string()),
        }
    }
}

/// A decoded track: planar `f32` at its own rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    rate: u32,
    format: &'static str,
    channels: Vec<Vec<f32>>,
    frames: usize,
}

/// What `assetcraft-cli analyze --summary` prints.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Container / codec family (`mp3`, `wav`, …).
    pub format: &'static str,
    /// Sample rate in Hz.
    pub rate: u32,
    /// Channel count.
    pub channels: usize,
    /// Length in frames.
    pub frames: u64,
    /// Length in seconds.
    pub seconds: f64,
    /// Highest absolute sample, in dBFS (−120 for silence).
    pub peak_dbfs: f64,
    /// RMS of the mono mix, in dBFS (−120 for silence).
    pub rms_dbfs: f64,
}

/// Read and decode a track through the file services (the UI's and CLI's disk, or a fake).
pub fn load(services: &dyn Services, path: &Path) -> Result<Audio, AudioError> {
    let bytes = services.read_head(path, MAX_FILE_BYTES + 1).map_err(|e| AudioError::Read(e.to_string()))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AudioError::Limit(format!("the file is over {} MiB", MAX_FILE_BYTES >> 20)));
    }
    let ext = path.extension().and_then(|e| e.to_str());
    Audio::decode(&bytes, ext)
}

fn format_name(f: sio::FileFormat) -> &'static str {
    match f {
        sio::FileFormat::Wav => "wav",
        sio::FileFormat::Aiff => "aiff",
        sio::FileFormat::Flac => "flac",
        sio::FileFormat::Mp3 => "mp3",
        sio::FileFormat::Ogg => "ogg",
        sio::FileFormat::Aac => "aac",
        sio::FileFormat::Alac => "alac",
        sio::FileFormat::Caf => "caf",
        sio::FileFormat::Other => "other",
    }
}

fn check(rate: u32, channels: usize, frames: u64) -> Result<(), AudioError> {
    if !(MIN_RATE..=MAX_RATE).contains(&rate) {
        return Err(AudioError::Limit(format!("sample rate {rate} Hz is outside {MIN_RATE}–{MAX_RATE} Hz")));
    }
    if channels == 0 || channels > MAX_CHANNELS {
        return Err(AudioError::Limit(format!("{channels} channels (1–{MAX_CHANNELS} supported)")));
    }
    if frames as f64 / f64::from(rate) > MAX_SECONDS {
        return Err(AudioError::Limit(format!("longer than {} minutes", MAX_SECONDS / 60.0)));
    }
    Ok(())
}

impl Audio {
    /// Decode a whole file from its bytes; `ext` (`"mp3"`) helps formats without magic bytes.
    pub fn decode(bytes: &[u8], ext: Option<&str>) -> Result<Audio, AudioError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(AudioError::Limit(format!("the file is over {} MiB", MAX_FILE_BYTES >> 20)));
        }
        // The header first, so a file that claims to be huge is refused before decoding.
        let info = sio::probe(bytes, ext)?;
        check(info.sample_rate, usize::from(info.channels), info.frames)?;
        let (info, buffer) = sio::decode(bytes, ext)?;
        Audio::from_planar(info.sample_rate, buffer.channels, format_name(info.format))
    }

    /// A track from planar samples (tests, generated audio). Checks the same limits as decoding;
    /// ragged channels are cut to the shortest, non-finite samples become silence.
    pub fn from_planar(rate: u32, mut channels: Vec<Vec<f32>>, format: &'static str) -> Result<Audio, AudioError> {
        let frames = channels.iter().map(Vec::len).min().unwrap_or(0);
        check(rate, channels.len(), frames as u64)?;
        for c in &mut channels {
            c.truncate(frames);
            for s in c.iter_mut() {
                if !s.is_finite() {
                    *s = 0.0;
                }
            }
        }
        Ok(Audio { rate, format, channels, frames })
    }

    /// Sample rate in Hz.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Channel count (1–8).
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Length in frames.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Length in seconds.
    pub fn seconds(&self) -> f64 {
        self.frames as f64 / f64::from(self.rate)
    }

    /// One channel's samples (empty past the last channel).
    pub fn channel(&self, i: usize) -> &[f32] {
        self.channels.get(i).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Frame `i` as left/right; silence past the end.
    pub fn stereo_at(&self, i: usize) -> (f32, f32) {
        let left = self.channel(0).get(i).copied().unwrap_or(0.0);
        match self.channels.len() {
            1 => (left, left),
            _ => (left, self.channel(1).get(i).copied().unwrap_or(0.0)),
        }
    }

    /// The channel mean: the analysis signal.
    pub fn mono(&self) -> Vec<f32> {
        let n = self.channels.len().max(1) as f32;
        (0..self.frames).map(|i| self.channels.iter().map(|c| c[i]).sum::<f32>() / n).collect()
    }

    /// Waveform overview of the mono mix.
    pub fn peaks(&self) -> Peaks {
        Peaks::build(&self.mono())
    }

    /// Duration, rate, channels, peak and RMS.
    pub fn summary(&self) -> Summary {
        let peak = self.channels.iter().flatten().fold(0.0f32, |m, s| m.max(s.abs()));
        let mono = self.mono();
        let squares: f64 = mono.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
        let rms = if mono.is_empty() { 0.0 } else { (squares / mono.len() as f64).sqrt() };
        Summary {
            format: self.format,
            rate: self.rate,
            channels: self.channels.len(),
            frames: self.frames as u64,
            seconds: self.seconds(),
            peak_dbfs: dbfs(f64::from(peak)),
            rms_dbfs: dbfs(rms),
        }
    }
}

/// Amplitude in dBFS, floored at −120.
pub fn dbfs(amplitude: f64) -> f64 {
    if amplitude <= 1e-6 { -120.0 } else { (20.0 * amplitude.log10()).max(-120.0) }
}

#[cfg(test)]
mod tests;
