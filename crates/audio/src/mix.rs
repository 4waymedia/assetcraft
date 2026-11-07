//! Voice over music (SR4; Paul, 2026-11-06: "The ducking as a rule is good, but what about video
//! clips with no audio? As long as the user can easily change the volume"): a section's inset
//! video with "Mix audio in" plays its own sound over the section, and the music is lowered
//! under it. Pure functions over interleaved PCM, so the export and Watch your Cut share them.
//!
//! - [`duck`] lowers the music to `gain` over each span, with a short linear ramp in and out
//!   (`DUCK_RAMP_SECS`) so it never clicks.
//! - [`add_voice`] lays a decoded file over the buffer from `at` for at most `length` seconds,
//!   resampled linearly to the buffer's rate, at `gain`.
//! - [`db`] turns decibels into a linear gain.
//!
//! Every index is bounds-checked; non-finite input is skipped; the output is clamped to ±1.

use crate::Audio;

/// How long the music takes to dip and to come back, in seconds.
pub const DUCK_RAMP_SECS: f64 = 0.25;

/// Decibels → linear gain (`-inf`/NaN → 0).
pub fn db(decibels: f64) -> f32 {
    if decibels.is_finite() { 10f64.powf(decibels / 20.0) as f32 } else { 0.0 }
}

/// Lower `out` (interleaved, `channels` wide, at `rate`) to `gain` over each `(start, length)`
/// span in seconds, ramping over [`DUCK_RAMP_SECS`] on each side (the ramps sit outside the
/// span, so the voice is never under full music).
pub fn duck(out: &mut [f32], channels: usize, rate: u32, spans: &[(f64, f64)], gain: f32) {
    if channels == 0 || rate == 0 || !(0.0..=1.0).contains(&gain) {
        return;
    }
    let frames = out.len() / channels;
    let r = f64::from(rate);
    let ramp = (DUCK_RAMP_SECS * r).max(1.0);
    // The gain at each frame is the lowest any span asks for.
    for &(start, length) in spans {
        if !start.is_finite() || !length.is_finite() || length <= 0.0 {
            continue;
        }
        let a = start * r;
        let b = (start + length) * r;
        let lo = (a - ramp).max(0.0) as usize;
        let hi = ((b + ramp).ceil() as usize).min(frames);
        for f in lo..hi {
            let x = f as f64;
            let depth = if x < a {
                (x - (a - ramp)) / ramp
            } else if x > b {
                ((b + ramp) - x) / ramp
            } else {
                1.0
            }
            .clamp(0.0, 1.0) as f32;
            let g = 1.0 - depth * (1.0 - gain);
            for c in 0..channels {
                if let Some(s) = out.get_mut(f * channels + c) {
                    // Applying twice where spans overlap would dip deeper: keep the lowest.
                    *s *= g;
                }
            }
        }
    }
}

/// Mix `voice` into `out` (interleaved, `channels` wide, at `rate`) from `at` seconds for at
/// most `length` seconds, at `gain`; linear resampling; a mono voice goes to every channel.
pub fn add_voice(out: &mut [f32], channels: usize, rate: u32, voice: &Audio, at: f64, length: f64, gain: f32) {
    add_voice_from(out, channels, rate, voice, at, length, 0.0, gain);
}

/// [`add_voice`] starting `skip` seconds into the voice (a playback window that begins part
/// way through it).
#[allow(clippy::too_many_arguments)]
pub fn add_voice_from(
    out: &mut [f32],
    channels: usize,
    rate: u32,
    voice: &Audio,
    at: f64,
    length: f64,
    skip: f64,
    gain: f32,
) {
    if channels == 0
        || rate == 0
        || voice.rate() == 0
        || !at.is_finite()
        || !length.is_finite()
        || !skip.is_finite()
        || at < 0.0
        || skip < 0.0
    {
        return;
    }
    let frames = out.len() / channels;
    let r = f64::from(rate);
    let step = f64::from(voice.rate()) / r;
    let offset = skip * f64::from(voice.rate());
    let first = (at * r).round() as usize;
    let n = ((length.max(0.0) * r) as usize).min(frames.saturating_sub(first));
    let vc = voice.channel_count().max(1);
    for i in 0..n {
        let pos = offset + i as f64 * step;
        let k = pos.floor() as usize;
        if k + 1 >= voice.frames() {
            break;
        }
        let t = (pos - k as f64) as f32;
        for c in 0..channels {
            let src = voice.channel(c.min(vc - 1));
            let (Some(&s0), Some(&s1)) = (src.get(k), src.get(k + 1)) else { continue };
            let v = s0 + (s1 - s0) * t;
            if let Some(o) = out.get_mut((first + i) * channels + c)
                && v.is_finite()
            {
                *o = (*o + v * gain).clamp(-1.0, 1.0);
            }
        }
    }
}

/// Analysis window for [`speech_spans`], in seconds.
pub const SPEECH_WINDOW_SECS: f64 = 0.05;
/// Below this level (dB below the voice's loudest window) a window is a pause.
pub const SPEECH_GATE_DB: f64 = -30.0;
/// Pauses shorter than this don't let the music back up, in seconds (breaths between words).
pub const SPEECH_HOLD_SECS: f64 = 0.6;

/// Where a narration is speaking (SR4b): `(start, length)` spans in its own seconds. Windows of
/// [`SPEECH_WINDOW_SECS`] louder than [`SPEECH_GATE_DB`] under its loudest window count; pauses
/// shorter than [`SPEECH_HOLD_SECS`] are bridged. A silent file has none.
pub fn speech_spans(voice: &Audio) -> Vec<(f64, f64)> {
    let rate = voice.rate();
    let n = voice.frames();
    if rate == 0 || n == 0 {
        return Vec::new();
    }
    let w = ((SPEECH_WINDOW_SECS * f64::from(rate)) as usize).max(1);
    let channels = voice.channel_count().max(1);
    let rms: Vec<f64> = (0..n.div_ceil(w))
        .map(|k| {
            let (a, b) = (k * w, ((k + 1) * w).min(n));
            let mut sum = 0.0f64;
            for c in 0..channels {
                for s in voice.channel(c).get(a..b).unwrap_or(&[]) {
                    sum += f64::from(*s) * f64::from(*s);
                }
            }
            (sum / ((b - a) * channels).max(1) as f64).sqrt()
        })
        .collect();
    let loudest = rms.iter().copied().fold(0.0f64, f64::max);
    if loudest <= 1e-6 {
        return Vec::new();
    }
    let gate = loudest * 10f64.powf(SPEECH_GATE_DB / 20.0);
    let secs = |k: usize| (k * w) as f64 / f64::from(rate);
    let mut spans: Vec<(f64, f64)> = Vec::new();
    let mut open: Option<usize> = None;
    for (k, level) in rms.iter().enumerate() {
        match (open, *level >= gate) {
            (None, true) => open = Some(k),
            (Some(a), false) => {
                spans.push((secs(a), secs(k)));
                open = None;
            }
            _ => {}
        }
    }
    if let Some(a) = open {
        spans.push((secs(a), n as f64 / f64::from(rate)));
    }
    // Bridge short pauses.
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (a, b) in spans {
        match merged.last_mut() {
            Some(last) if a - last.1 < SPEECH_HOLD_SECS => last.1 = b,
            _ => merged.push((a, b)),
        }
    }
    merged.into_iter().map(|(a, b)| (a, b - a)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, secs: f64, level: f32) -> Audio {
        let n = (secs * f64::from(rate)) as usize;
        Audio::from_channels(rate, "test", vec![(0..n).map(|i| if i % 2 == 0 { level } else { -level }).collect()])
    }

    #[test]
    fn speech_spans_find_the_talking_and_bridge_short_pauses() {
        // 1 s talk, 0.3 s breath, 1 s talk, 2 s silence, 0.5 s talk.
        let rate = 8_000u32;
        let mut s = Vec::new();
        for (secs, loud) in [(1.0, true), (0.3, false), (1.0, true), (2.0, false), (0.5, true)] {
            let n = (secs * f64::from(rate)) as usize;
            s.extend((0..n).map(|i| if loud { ((i as f32) * 0.3).sin() * 0.5 } else { 0.0005 }));
        }
        let voice = Audio::from_channels(rate, "test", vec![s]);
        let spans = speech_spans(&voice);
        assert_eq!(spans.len(), 2, "{spans:?}");
        assert!(spans[0].0.abs() < 0.06 && (spans[0].1 - 2.3).abs() < 0.11, "{spans:?}");
        assert!((spans[1].0 - 4.3).abs() < 0.06 && (spans[1].1 - 0.5).abs() < 0.06, "{spans:?}");
        assert!(speech_spans(&Audio::from_channels(rate, "test", vec![vec![0.0; 8000]])).is_empty());
    }

    #[test]
    fn duck_lowers_the_span_with_ramps_outside_it() {
        let rate = 1000;
        let mut out = vec![1.0f32; 4 * rate as usize * 2];
        duck(&mut out, 2, rate, &[(1.0, 2.0)], 0.25);
        let at = |s: f64| out[(s * f64::from(rate)) as usize * 2];
        assert_eq!(at(0.5), 1.0, "before the ramp");
        assert!((at(0.875) - 0.625).abs() < 0.01, "half way down the ramp: {}", at(0.875));
        assert_eq!(at(1.0), 0.25);
        assert_eq!(at(2.5), 0.25);
        assert!(at(3.1) > 0.25 && at(3.1) < 1.0, "coming back");
        assert_eq!(at(3.5), 1.0);
        // Nonsense spans and gains change nothing.
        let mut same = vec![1.0f32; 100];
        duck(&mut same, 2, rate, &[(f64::NAN, 1.0), (0.0, -1.0)], 0.5);
        duck(&mut same, 2, rate, &[(0.0, 1.0)], 2.0);
        assert!(same.iter().all(|s| *s == 1.0));
        assert!((db(-12.0) - 0.251).abs() < 0.001 && db(0.0) == 1.0 && db(f64::NEG_INFINITY) == 0.0);
    }

    #[test]
    fn a_program_window_part_way_through_a_voice_hears_the_same_samples() {
        // Rendering the whole program once, or in two windows, gives the same sound.
        use crate::program::{Layer, Program};
        use std::sync::Arc;
        let music = Arc::new(tone(8_000, 6.0, 0.1));
        let voice = Arc::new(Audio::from_channels(
            8_000,
            "test",
            vec![(0..32_000).map(|i| (i as f32 / 4000.0).sin() * 0.5).collect()],
        ));
        let p = Program::new(music, None).with_layers(vec![Layer {
            audio: voice,
            start: 1.0,
            length: 3.0,
            voice_gain: 1.0,
            music_gain: 0.25,
            duck: Vec::new(),
        }]);
        let mut whole = vec![0.0f32; 8_000 * 6 * 2];
        p.render(0.0, 8_000, &mut whole, 2);
        let mut a = vec![0.0f32; 8_000 * 2 * 2];
        let mut b = vec![0.0f32; 8_000 * 4 * 2];
        p.render(0.0, 8_000, &mut a, 2);
        p.render(2.0, 8_000, &mut b, 2);
        let joined: Vec<f32> = a.into_iter().chain(b).collect();
        let worst = whole.iter().zip(&joined).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
        assert!(worst < 1e-4, "windows match the whole: {worst}");
        // Under the voice the music is ducked: at most voice (0.5) + music × 0.25.
        let i = (2.5 * 8_000.0) as usize * 2;
        assert!(whole[i].abs() <= 0.5 + 0.025 + 1e-3);
        assert_eq!(p.layers().len(), 1);
    }

    #[test]
    fn a_voice_is_laid_in_at_its_time_resampled_and_cut_to_its_length() {
        let mut out = vec![0.0f32; 48_000 * 3 * 2];
        let voice = tone(24_000, 5.0, 0.5);
        add_voice(&mut out, 2, 48_000, &voice, 1.0, 1.5, 0.5);
        let frame = |s: f64| out[(s * 48_000.0) as usize * 2];
        assert_eq!(frame(0.5), 0.0, "before");
        assert!(out[48_000 * 2..(48_000 + 2000) * 2].iter().any(|s| s.abs() > 0.1), "playing at 1.0 s");
        assert_eq!(frame(2.6), 0.0, "cut at 1.5 s long");
        assert!(out.iter().all(|s| s.abs() <= 0.25 + 1e-6), "at half gain");
        // Mono to both channels.
        let i = 48_000 * 2 + 200;
        assert_eq!(out[i], out[i + 1]);
        // Past the buffer and before zero: nothing breaks.
        add_voice(&mut out, 2, 48_000, &voice, 2.9, 10.0, 1.0);
        add_voice(&mut out, 2, 48_000, &voice, -1.0, 1.0, 1.0);
        add_voice(&mut out, 0, 48_000, &voice, 0.0, 1.0, 1.0);
    }
}
