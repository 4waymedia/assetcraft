//! Audio output for Watch your Cut (`docs/track-analysis.md` §9.2, PR3).
//!
//! Layer 2; depends on `assetcraft-audio` and `cpal` (Apache-2.0). Only the desktop app links
//! this crate: the engine drives any [`assetcraft_audio::Player`], so tests and the CLI never
//! open a device.
//!
//! [`DevicePlayer`] opens the default output device on first use, on a thread of its own that
//! owns the `cpal::Stream` (streams aren't `Send` everywhere) and keeps it running until the
//! player drops. The device callback renders the current [`Program`] at the device's rate and
//! counts the frames it has handed over; that count, less the output latency the device
//! reports, is the audio clock ([`Player::position`]) the playhead follows. Starting, stopping and seeking swap the voice; the stream never
//! restarts, so play is immediate.
//!
//! The device-opening shape (default device, default config, one stream per sample format,
//! errors logged not raised) follows SoundCraft's `crates/playback/src/lib.rs` (`open_device`,
//! `build_output`, storytold/soundcraft `3a28a65`); see NOTICE.
//!
//! Never panics: a missing device, an unsupported sample format or a stream error makes
//! [`Player::play`] return an error (the Cut plays silent) or the callback write silence.

use assetcraft_audio::{Player, Program};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// Crate name, for `--version` lines.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// How long [`DevicePlayer::play`] waits for the device to open the first time.
const OPEN_TIMEOUT: Duration = Duration::from_secs(5);
/// Frames rendered per pass inside the callback (the scratch buffer holds this many frames).
const CHUNK_FRAMES: usize = 4096;

/// What plays: a program, and where in it the first rendered frame was.
struct Voice {
    program: Arc<Program>,
    from: f64,
}

/// State shared with the device callback.
#[derive(Default)]
struct Shared {
    voice: Mutex<Option<Voice>>,
    /// Device frames handed over since the voice started.
    rendered: AtomicU64,
    /// The device's rate, once open.
    rate: AtomicU64,
    /// The stream reported the device gone.
    gone: AtomicBool,
    /// Output latency the device last reported (callback → speaker), in microseconds.
    latency_us: AtomicU64,
    /// Output gain as `f32` bits (`Player::set_volume`); `0` bits = never set = full level.
    gain_bits: AtomicU32,
}

impl Shared {
    fn gain(&self) -> f32 {
        match self.gain_bits.load(Ordering::Relaxed) {
            0 => 1.0,
            bits => f32::from_bits(bits),
        }
    }
}

/// The device thread: dropping `_keep` ends it, which drops the stream.
struct Device {
    _keep: Sender<()>,
    name: String,
    rate: u32,
    channels: u16,
}

/// Plays on the system's default output device.
#[derive(Default)]
pub struct DevicePlayer {
    shared: Arc<Shared>,
    device: Mutex<Option<Device>>,
}

impl DevicePlayer {
    /// A player; the device opens on the first [`Player::play`].
    pub fn new() -> DevicePlayer {
        DevicePlayer::default()
    }

    /// `name @ rate Hz, n ch` of the open device, if one is open.
    pub fn device(&self) -> Option<String> {
        let d = self.device.lock().unwrap_or_else(PoisonError::into_inner);
        d.as_ref().map(|d| format!("{} @ {} Hz, {} ch", d.name, d.rate, d.channels))
    }

    fn ensure_open(&self) -> Result<(), String> {
        let mut device = self.device.lock().unwrap_or_else(PoisonError::into_inner);
        if self.shared.gone.load(Ordering::Relaxed) {
            // The device went away (unplugged): open the default again.
            *device = None;
            self.shared.gone.store(false, Ordering::Relaxed);
        }
        if device.is_some() {
            return Ok(());
        }
        let (ready_tx, ready_rx) = sync_channel::<Result<(String, u32, u16), String>>(1);
        let (keep_tx, keep_rx) = channel::<()>();
        let shared = Arc::clone(&self.shared);
        let spawned = std::thread::Builder::new().name("assetcraft-audio-out".into()).spawn(move || {
            match open_stream(&shared) {
                Ok((stream, name, rate, channels)) => {
                    let _ = ready_tx.send(Ok((name, rate, channels)));
                    // Hold the stream until the player drops its sender.
                    let _ = keep_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        });
        if let Err(e) = spawned {
            return Err(format!("can't start the audio thread: {e}"));
        }
        match ready_rx.recv_timeout(OPEN_TIMEOUT) {
            Ok(Ok((name, rate, channels))) => {
                *device = Some(Device { _keep: keep_tx, name, rate, channels });
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err("the audio device didn't open in time".into()),
        }
    }
}

impl Player for DevicePlayer {
    fn play(&self, program: Arc<Program>, from_secs: f64) -> Result<(), String> {
        self.ensure_open()?;
        let from = if from_secs.is_finite() { from_secs.max(0.0) } else { 0.0 };
        let mut voice = self.shared.voice.lock().unwrap_or_else(PoisonError::into_inner);
        *voice = Some(Voice { program, from });
        self.shared.rendered.store(0, Ordering::Relaxed);
        Ok(())
    }

    fn stop(&self) {
        let mut voice = self.shared.voice.lock().unwrap_or_else(PoisonError::into_inner);
        *voice = None;
    }

    fn set_volume(&self, gain: f32) {
        let g = if gain.is_finite() { gain.clamp(0.0, 1.0) } else { 1.0 };
        // Store silence as the smallest positive value so `0` keeps meaning "never set".
        let g = if g <= 0.0 { f32::MIN_POSITIVE } else { g };
        self.shared.gain_bits.store(g.to_bits(), Ordering::Relaxed);
    }

    fn position(&self) -> Option<f64> {
        let voice = self.shared.voice.lock().unwrap_or_else(PoisonError::into_inner);
        let from = voice.as_ref()?.from;
        let rate = self.shared.rate.load(Ordering::Relaxed);
        if rate == 0 {
            return Some(from);
        }
        // Frames handed over, less what is still in the device's buffer on its way out.
        let handed = self.shared.rendered.load(Ordering::Relaxed) as f64 / rate as f64;
        let latency = self.shared.latency_us.load(Ordering::Relaxed) as f64 / 1e6;
        Some(from + (handed - latency).max(0.0))
    }
}

/// Open the default output device with its default config and start a stream rendering the
/// shared voice. Returns the stream (kept alive by the caller), device name, rate and channels.
fn open_stream(shared: &Arc<Shared>) -> Result<(cpal::Stream, String, u32, u16), String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or_else(|| "no audio output device".to_string())?;
    #[allow(deprecated)] // `name` is the portable label in cpal 0.16
    let name = device.name().unwrap_or_else(|_| "default output".into());
    let supported = device.default_output_config().map_err(|e| format!("audio output: {e}"))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate.0;
    let channels = config.channels;
    shared.rate.store(u64::from(rate), Ordering::Relaxed);
    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, shared),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, shared),
        cpal::SampleFormat::I32 => build::<i32>(&device, &config, shared),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, shared),
        other => return Err(format!("audio output: unsupported sample format {other}")),
    }
    .map_err(|e| format!("audio output: {e}"))?;
    stream.play().map_err(|e| format!("audio output: {e}"))?;
    Ok((stream, name, rate, channels))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: &Arc<Shared>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = usize::from(config.channels).max(1);
    let rate = config.sample_rate.0;
    let mut scratch = vec![0.0f32; CHUNK_FRAMES * channels];
    let state = Arc::clone(shared);
    let errors = Arc::clone(shared);
    device.build_output_stream(
        config,
        move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
            let ts = info.timestamp();
            if let Some(d) = ts.playback.duration_since(&ts.callback) {
                let us = u64::try_from(d.as_micros()).unwrap_or(u64::MAX).min(2_000_000);
                state.latency_us.store(us, Ordering::Relaxed);
            }
            // Never wait in the audio callback: if the voice is being swapped, play silence.
            let voice = state.voice.try_lock();
            for out in data.chunks_mut(CHUNK_FRAMES * channels) {
                let Some(buf) = scratch.get_mut(..out.len()) else { return };
                match voice.as_ref().ok().and_then(|v| v.as_ref()) {
                    Some(v) => {
                        let done = state.rendered.load(Ordering::Relaxed);
                        let at = v.from + done as f64 / f64::from(rate);
                        v.program.render(at, rate, buf, channels);
                        let frames = (out.len() / channels) as u64;
                        state.rendered.store(done.saturating_add(frames), Ordering::Relaxed);
                    }
                    None => buf.iter_mut().for_each(|s| *s = 0.0),
                }
                let gain = state.gain();
                for (o, s) in out.iter_mut().zip(buf.iter()) {
                    *o = T::from_sample((s * gain).clamp(-1.0, 1.0));
                }
            }
        },
        move |e: cpal::StreamError| {
            if matches!(e, cpal::StreamError::DeviceNotAvailable) {
                errors.gone.store(true, Ordering::Relaxed);
            }
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_player_has_no_clock_and_stop_is_harmless() {
        let p = DevicePlayer::new();
        assert_eq!(p.position(), None);
        p.stop();
        assert_eq!(p.device(), None, "nothing opens until play");
        assert_eq!(p.shared.gain(), 1.0, "full level until set");
        p.set_volume(0.25);
        assert_eq!(p.shared.gain(), 0.25);
        p.set_volume(0.0);
        assert!(p.shared.gain() < 1e-30, "silent");
        p.set_volume(f32::NAN);
        assert_eq!(p.shared.gain(), 1.0);
    }
}
