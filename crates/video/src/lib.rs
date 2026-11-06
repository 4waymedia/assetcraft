//! Video export in pure Rust (SF5; decision D-SF6, 2026-11-06).
//!
//! Layer 1, bytes in and bytes out: no file system, no threads of its own beyond the encoder's
//! per-GOP workers, no UI. The engine renders frames, decodes the song and writes the file.
//!
//! - [`VideoEncoder`]: RGBA frames in display order → H.264 access units in AVCC form
//!   ([`VideoTrack`]). Constant QP, one IDR per second, `Fast` preset: the spike showed the
//!   rate-controlled modes starve frames and `Balanced` costs 7× the time for no gain on
//!   animatic content. Frames are gathered into batches of whole GOPs and encoded with
//!   `encode_all`, which runs the GOPs of a batch in parallel across cores and is byte-identical
//!   to frame-by-frame encoding; the batch size bounds memory (a 1080p frame is 3 MiB as I420).
//! - [`encode_audio`]: interleaved `f32` PCM → AAC-LC access units ([`AudioTrack`]).
//! - [`mux`]: one video track and an optional audio track → a progressive MP4 or QuickTime MOV
//!   (`ftyp` · `mdat` · `moov`), written by this crate (no container dependency).
//!
//! Limits are checked before anything is allocated ([`Settings::check`]); a file that would need
//! 64-bit chunk offsets (over 4 GiB) is refused rather than written wrong.

#![forbid(unsafe_code)]

mod mux;
mod yuv;

pub use mux::{AudioTrack, Container, Sample, VideoTrack, mux, mux_size};
pub use yuv::rgba_to_i420;

use rusty_h264_common::YuvFrame;
use rusty_h264_encoder::{Encoder, EncoderConfig, Preset};
use std::fmt;

/// Widest and tallest frame encoded (8K).
pub const MAX_SIDE: u32 = 8192;
/// Most frames one export may hold (24 fps × 4 hours).
pub const MAX_FRAMES: usize = 24 * 60 * 60 * 4;
/// Default quantizer (0 best … 51 worst): 24 is transparent for stills and slow moves.
pub const DEFAULT_QP: u8 = 24;
/// Default AAC-LC bitrate.
pub const DEFAULT_AUDIO_BITRATE: u32 = 160_000;
/// GOPs encoded together (in parallel) before their bytes are taken; bounds memory to
/// `BATCH_GOPS × fps × frame bytes` (≈ 300 MiB at 1080p24).
pub const BATCH_GOPS: usize = 4;
/// AAC frame length in samples.
pub const AAC_FRAME: u32 = 1024;

/// Why an export could not be encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoError {
    /// Width, height, frame rate or quantizer out of range.
    BadSettings(String),
    /// A frame's byte length does not match the settings.
    FrameSize {
        /// `width × height × 4`.
        expected: usize,
        /// What was pushed.
        got: usize,
    },
    /// More frames than [`MAX_FRAMES`].
    TooManyFrames,
    /// The encoder refused.
    Encode(String),
    /// The audio could not be encoded.
    Audio(String),
    /// The file would exceed 4 GiB (32-bit chunk offsets).
    TooLarge,
}

impl fmt::Display for VideoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSettings(m) => write!(f, "bad export settings: {m}"),
            Self::FrameSize { expected, got } => write!(f, "frame has {got} bytes, expected {expected}"),
            Self::TooManyFrames => write!(f, "more than {MAX_FRAMES} frames"),
            Self::Encode(m) => write!(f, "video encoder: {m}"),
            Self::Audio(m) => write!(f, "audio encoder: {m}"),
            Self::TooLarge => write!(f, "the file would exceed 4 GiB"),
        }
    }
}

impl std::error::Error for VideoError {}

/// What to encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Frame width in pixels (even, ≤ [`MAX_SIDE`]).
    pub width: u32,
    /// Frame height in pixels (even, ≤ [`MAX_SIDE`]).
    pub height: u32,
    /// Frames per second (1..=120).
    pub fps: u32,
    /// Quantizer, 0..=51 (lower is better and bigger).
    pub qp: u8,
}

impl Settings {
    /// 1080p, 24 fps, [`DEFAULT_QP`].
    pub const HD: Settings = Settings { width: 1920, height: 1080, fps: 24, qp: DEFAULT_QP };

    /// Refuse what the encoder can't take, before any allocation.
    pub fn check(&self) -> Result<(), VideoError> {
        let bad = |m: &str| Err(VideoError::BadSettings(m.into()));
        if self.width < 16 || self.height < 16 || self.width > MAX_SIDE || self.height > MAX_SIDE {
            return bad("width and height must be 16..=8192");
        }
        if !self.width.is_multiple_of(2) || !self.height.is_multiple_of(2) {
            return bad("width and height must be even");
        }
        if self.fps == 0 || self.fps > 120 {
            return bad("fps must be 1..=120");
        }
        if self.qp > 51 {
            return bad("qp must be 0..=51");
        }
        Ok(())
    }

    /// Bytes of one RGBA frame.
    pub fn rgba_len(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }

    fn config(&self) -> EncoderConfig {
        let mut cfg = EncoderConfig::new(self.width as usize, self.height as usize);
        cfg.preset = Preset::Fast;
        cfg.gop_size = self.fps;
        cfg.min_keyint = self.fps;
        cfg.bitrate = 0;
        cfg.qp = self.qp;
        cfg.framerate = self.fps as f32;
        cfg.level_idc = level_for(self.width, self.height, self.fps);
        cfg
    }
}

/// An H.264 level that admits the frame size and rate (table A-1, by macroblocks per second).
fn level_for(width: u32, height: u32, fps: u32) -> u8 {
    let mbs = u64::from(width.div_ceil(16)) * u64::from(height.div_ceil(16));
    let mbps = mbs * u64::from(fps);
    match (mbs, mbps) {
        (m, r) if m <= 1620 && r <= 40_500 => 31,
        (m, r) if m <= 3600 && r <= 108_000 => 32,
        (m, r) if m <= 8192 && r <= 245_760 => 40,
        (m, r) if m <= 8192 && r <= 983_040 => 42,
        (m, r) if m <= 22_080 && r <= 589_824 => 50,
        (m, r) if m <= 36_864 && r <= 983_040 => 51,
        (m, r) if m <= 139_264 && r <= 4_177_920 => 60,
        _ => 62,
    }
}

/// RGBA frames in → an H.264 track out, in batches of whole GOPs.
pub struct VideoEncoder {
    settings: Settings,
    cfg: EncoderConfig,
    batch: Vec<YuvFrame>,
    batch_frames: usize,
    track: VideoTrack,
    frames: usize,
}

impl fmt::Debug for VideoEncoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VideoEncoder").field("settings", &self.settings).field("frames", &self.frames).finish()
    }
}

impl VideoEncoder {
    /// An encoder for `settings` (checked first).
    pub fn new(settings: Settings) -> Result<Self, VideoError> {
        settings.check()?;
        let cfg = settings.config();
        // The config is validated by building one encoder up front, so a bad level or size is
        // reported here and never from inside a batch.
        Encoder::new(cfg.clone()).map_err(|e| VideoError::Encode(format!("{e:?}")))?;
        Ok(Self {
            settings,
            cfg,
            batch: Vec::new(),
            batch_frames: settings.fps as usize * BATCH_GOPS,
            track: VideoTrack::new(settings.width, settings.height, settings.fps),
            frames: 0,
        })
    }

    /// The settings this encoder was made with.
    pub fn settings(&self) -> Settings {
        self.settings
    }

    /// Frames pushed so far.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Add the next frame (`width × height × 4` bytes, RGBA, rows top to bottom). Encoding
    /// happens when a batch of GOPs is complete, so most calls return at once.
    pub fn push_rgba(&mut self, rgba: &[u8]) -> Result<(), VideoError> {
        let expected = self.settings.rgba_len();
        if rgba.len() != expected {
            return Err(VideoError::FrameSize { expected, got: rgba.len() });
        }
        if self.frames >= MAX_FRAMES {
            return Err(VideoError::TooManyFrames);
        }
        self.batch.push(rgba_to_i420(rgba, self.settings.width, self.settings.height));
        self.frames += 1;
        if self.batch.len() >= self.batch_frames {
            self.encode_batch()?;
        }
        Ok(())
    }

    /// Encode what is buffered and return the track. Nothing pushed gives an empty track.
    pub fn finish(mut self) -> Result<VideoTrack, VideoError> {
        self.encode_batch()?;
        Ok(self.track)
    }

    fn encode_batch(&mut self) -> Result<(), VideoError> {
        if self.batch.is_empty() {
            return Ok(());
        }
        let frames = std::mem::take(&mut self.batch);
        let enc = Encoder::new(self.cfg.clone()).map_err(|e| VideoError::Encode(format!("{e:?}")))?;
        let units = enc.encode_all(&frames).map_err(|e| VideoError::Encode(format!("{e:?}")))?;
        if units.len() != frames.len() {
            return Err(VideoError::Encode(format!("{} frames in, {} access units out", frames.len(), units.len())));
        }
        drop(frames);
        for unit in &units {
            self.track.push_annex_b(unit);
        }
        Ok(())
    }
}

/// Interleaved `f32` PCM (−1..=1) → an AAC-LC track at `bitrate` bits per second.
/// `channels` 1..=2 (an animatic's song); other layouts are refused.
pub fn encode_audio(pcm: &[f32], channels: u16, sample_rate: u32, bitrate: u32) -> Result<AudioTrack, VideoError> {
    if !(1..=2).contains(&channels) {
        return Err(VideoError::Audio("1 or 2 channels".into()));
    }
    if rusty_aac::sf_index_for_rate(sample_rate).is_none() {
        return Err(VideoError::Audio(format!("unsupported sample rate {sample_rate}")));
    }
    if !pcm.len().is_multiple_of(channels as usize) {
        return Err(VideoError::Audio("pcm length is not a whole number of frames".into()));
    }
    let bitrate = bitrate.clamp(32_000, 320_000);
    let mut enc =
        rusty_aac::AacEncoder::new(rusty_aac::AacEncoderConfig { bitrate_bps: bitrate, ..Default::default() });
    enc.push_pcm(pcm, channels, sample_rate).map_err(|e| VideoError::Audio(e.to_string()))?;
    enc.finish();
    let mut track =
        AudioTrack::new(sample_rate, channels, rusty_aac::audio_specific_config_bytes(sample_rate, channels));
    while let Ok(packet) = enc.next_packet() {
        track.push(packet.data);
    }
    Ok(track)
}

#[cfg(test)]
mod tests;
