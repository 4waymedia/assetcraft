use super::*;
use assetcraft_fs::FakeFs;
use sio::{AudioBuffer, BitDepth, EncodeOptions, FileFormat};

/// `secs` of a 440 Hz sine at half scale, `channels` channels (the second inverted).
fn tone(rate: u32, channels: usize, secs: f64) -> AudioBuffer {
    let n = (f64::from(rate) * secs) as usize;
    let one: Vec<f32> =
        (0..n).map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin()).collect();
    let chans = (0..channels).map(|c| if c % 2 == 1 { one.iter().map(|s| -s).collect() } else { one.clone() });
    AudioBuffer { sample_rate: rate, channels: chans.collect() }
}

fn wav(buf: &AudioBuffer) -> Vec<u8> {
    sio::encode(buf, &EncodeOptions { bit_depth: BitDepth::Float32, ..EncodeOptions::default() }).unwrap()
}

#[test]
fn decodes_wav_and_flac_and_summarises() {
    let buf = tone(44_100, 2, 1.5);
    let a = Audio::decode(&wav(&buf), Some("wav")).unwrap();
    assert_eq!((a.rate(), a.channel_count(), a.frames()), (44_100, 2, 66_150));
    assert!((a.seconds() - 1.5).abs() < 1e-9);
    let s = a.summary();
    assert_eq!(s.format, "wav");
    assert!((s.peak_dbfs - dbfs(0.5)).abs() < 0.01, "{s:?}");
    // The second channel is inverted, so the mono mix cancels to silence.
    assert_eq!(s.rms_dbfs, -120.0);
    assert_eq!(a.stereo_at(10), (buf.channels[0][10], buf.channels[1][10]));
    assert_eq!(a.stereo_at(usize::MAX), (0.0, 0.0));

    let flac =
        sio::encode(&tone(48_000, 1, 0.5), &EncodeOptions { format: FileFormat::Flac, ..EncodeOptions::default() })
            .unwrap();
    let a = Audio::decode(&flac, Some("flac")).unwrap();
    assert_eq!((a.summary().format, a.rate(), a.channel_count(), a.frames()), ("flac", 48_000, 1, 24_000));
    let left = a.stereo_at(100).0;
    assert_eq!(a.stereo_at(100), (left, left), "mono plays on both sides");
    // RMS of a half-scale sine is 0.5 / √2.
    assert!((a.summary().rms_dbfs - dbfs(0.5 / 2f64.sqrt())).abs() < 0.05);
    assert_eq!(a.peaks().len_samples(), 24_000);
}

#[test]
fn loads_through_the_file_services() {
    let fs = FakeFs::new().file_with("/music/a.wav", wav(&tone(22_050, 1, 0.25)));
    let a = load(&fs, Path::new("/music/a.wav")).unwrap();
    assert_eq!(a.frames(), 5_512);
    assert!(matches!(load(&fs, Path::new("/music/missing.wav")), Err(AudioError::Read(_))));
}

#[test]
fn hostile_and_out_of_range_input_is_refused() {
    for junk in [&b""[..], b"RIFF", b"not audio at all, just text", &[0xff; 4096]] {
        assert!(Audio::decode(junk, Some("mp3")).is_err(), "{junk:?}");
    }
    // A WAV cut short still decodes what is there, or fails cleanly; never panics.
    let full = wav(&tone(44_100, 2, 0.5));
    for cut in [12, 44, 45, 100, full.len() / 2] {
        let _ = Audio::decode(&full[..cut], Some("wav"));
    }
    // Limits: channels, rate, length.
    assert!(matches!(Audio::decode(&wav(&tone(44_100, 9, 0.01)), None), Err(AudioError::Limit(_))));
    assert!(matches!(Audio::decode(&wav(&tone(4_000, 1, 0.1)), None), Err(AudioError::Limit(_))));
    assert!(matches!(check(48_000, 2, 48_000 * 1_201), Err(AudioError::Limit(_))));
    assert!(check(48_000, 2, 48_000 * 1_200).is_ok());
    assert!(matches!(Audio::from_planar(44_100, Vec::new(), "x"), Err(AudioError::Limit(_))));
    // Non-finite samples become silence; ragged channels are cut to the shortest.
    let a = Audio::from_planar(8_000, vec![vec![f32::NAN, 0.5, 0.25], vec![f32::INFINITY, -0.5]], "x").unwrap();
    assert_eq!((a.frames(), a.channel(0), a.channel(1)), (2, &[0.0, 0.5][..], &[0.0, -0.5][..]));
    assert_eq!(a.mono(), vec![0.0, 0.0]);
    assert_eq!(a.channel(5), &[] as &[f32]);
}

#[test]
fn dbfs_floors_silence() {
    assert_eq!(dbfs(0.0), -120.0);
    assert_eq!(dbfs(f64::NAN), -120.0);
    assert!((dbfs(1.0)).abs() < 1e-12);
}

// ---- Program -------------------------------------------------------------------------------

/// Mono audio whose sample `i` is `i / 1000` (a ramp: every frame says where it came from).
fn ramp(rate: u32, frames: usize) -> Arc<Audio> {
    Arc::new(Audio::from_planar(rate, vec![(0..frames).map(|i| i as f32 / 1000.0).collect()], "x").unwrap())
}

use std::sync::Arc;

#[test]
fn a_program_plays_the_whole_track_or_its_edit() {
    let audio = ramp(8_000, 1_000);
    let whole = Program::new(audio.clone(), None);
    assert_eq!((whole.length_frames(), whole.seconds()), (1_000, 0.125));
    assert_eq!(whole.frame(250), (0.25, 0.25));
    assert_eq!(whole.frame(1_000), (0.0, 0.0));

    // Butt splice: 0..100 then 600..700, fade-out over the last 50 frames.
    let edit = Edit {
        rate: 8_000,
        segments: vec![(0, 100), (600, 700)],
        crossfade: 0,
        fade_out: Some((150, 50)),
        length: 200,
    };
    let p = Program::new(audio.clone(), Some(&edit));
    assert_eq!(p.length_frames(), 200);
    assert_eq!(p.frame(99).0, 0.099);
    assert_eq!(p.frame(100).0, 0.6, "the second segment starts at the splice");
    assert_eq!(p.frame(149).0, 0.649, "before the fade");
    assert!((p.frame(175).0 - 0.675 * std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4, "half-way: equal power");
    assert_eq!(p.frame(200), (0.0, 0.0), "silence after the edit");

    // Crossfade of 20 frames centred on the splice at output frame 100.
    let xf = Edit { crossfade: 20, fade_out: None, ..edit.clone() };
    let p = Program::new(audio.clone(), Some(&xf));
    assert_eq!(p.frame(89).0, 0.089, "before the crossfade");
    let g = std::f32::consts::FRAC_1_SQRT_2;
    let mid = p.frame(100).0;
    assert!((mid - (0.100 * g + 0.600 * g)).abs() < 1e-4, "at the splice both sides at √½: {mid}");
    let before = p.frame(95).0; // outgoing 0.095, incoming from 595
    let x = (10.0 - 5.0) / 20.0 * std::f64::consts::FRAC_PI_2;
    let want = 0.095 * x.cos() + 0.595 * x.sin();
    assert!((f64::from(before) - want).abs() < 1e-4, "{before} vs {want}");
    assert_eq!(p.frame(110).0, 0.61, "after the crossfade");
    assert_eq!(p.length_frames(), 200, "a crossfade keeps the edit's length");

    // Edit frames at 16 kHz on 8 kHz audio are halved; segments past the file are clipped.
    let hi =
        Edit { rate: 16_000, segments: vec![(0, 400), (1_800, 4_000)], crossfade: 0, fade_out: None, length: 2_600 };
    let p = Program::new(audio, Some(&hi));
    assert_eq!(p.length_frames(), 200 + 100);
    assert_eq!(p.frame(200).0, 0.9);
}

#[test]
fn rendering_resamples_to_the_device_and_fills_its_channels() {
    let p = Program::new(ramp(8_000, 1_000), None);
    // A device at twice the rate: every other output frame is half-way between two source frames.
    let mut out = vec![9.0; 8];
    p.render(500.0 / 8_000.0, 16_000, &mut out, 2);
    let want = [0.5, 0.5, 0.5005, 0.5005, 0.501, 0.501, 0.5015, 0.5015];
    for (a, b) in out.iter().zip(want) {
        assert!((a - b).abs() < 1e-6, "{out:?}");
    }
    // Mono device: the mean. Six channels: the rest silent.
    let mut mono = vec![9.0; 2];
    p.render(0.0, 8_000, &mut mono, 1);
    assert_eq!(mono, vec![0.0, 0.001]);
    let mut six = vec![9.0; 6];
    p.render(100.0 / 8_000.0, 8_000, &mut six, 6);
    assert_eq!(six, vec![0.1, 0.1, 0.0, 0.0, 0.0, 0.0]);
    // Past the end, before the start and nonsense times are silent or clamped, never a panic.
    let mut tail = vec![9.0; 4];
    p.render(5.0, 8_000, &mut tail, 2);
    assert_eq!(tail, vec![0.0; 4]);
    p.render(f64::NAN, 8_000, &mut tail, 2);
    assert_eq!(tail, vec![0.0; 4]);
    p.render(-3.0, 8_000, &mut tail, 2);
    assert_eq!(tail[0], 0.0);
    p.render(0.0, 0, &mut tail, 2);
    p.render(0.0, 8_000, &mut tail, 0);
}

#[test]
fn players_report_what_they_do() {
    let p = Arc::new(Program::new(ramp(8_000, 2_000), None));
    let rec = RecordingPlayer::default();
    assert_eq!(rec.position(), None);
    rec.play(p.clone(), 0.5).unwrap();
    assert_eq!(rec.position(), Some(0.5));
    rec.stop();
    assert_eq!(rec.log(), ["play 0.2s from 0.5", "stop"]);
    assert_eq!(rec.position(), None);
    assert!(SilentPlayer.play(p, 0.0).is_err());
    assert_eq!(SilentPlayer.position(), None);
}

#[test]
fn the_analysis_signal_is_mono_at_22050_and_keeps_the_pitch() {
    let n = 44_100;
    let sine: Vec<f32> =
        (0..n).map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44_100.0).sin()).collect();
    let inverted: Vec<f32> = sine.iter().map(|s| s * 0.0).collect();
    let audio = Audio::from_planar(44_100, vec![sine, inverted], "wav").unwrap();
    let sig = audio.analysis_signal();
    assert!((sig.len() as i64 - 22_050).abs() <= 1, "{}", sig.len());
    // 440 Hz for one second: about 880 zero crossings.
    let crossings = sig.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    assert!((crossings as i64 - 880).abs() <= 4, "{crossings}");
    // The mono mean of a half-scale sine and silence peaks near 0.25.
    let peak = sig.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!((peak - 0.25).abs() < 0.01, "{peak}");
    // Already at the analysis rate: unchanged.
    let same = Audio::from_planar(ANALYSIS_RATE, vec![vec![0.1; 1000]], "wav").unwrap();
    assert_eq!(same.analysis_signal(), vec![0.1; 1000]);
}

#[test]
fn resampling_for_analysis_removes_what_would_alias() {
    // 15 kHz at 44.1 kHz is above the analysis Nyquist (11,025 Hz): it must not fold down to 7 kHz.
    let n = 44_100;
    let high: Vec<f32> =
        (0..n).map(|i| 0.5 * (2.0 * std::f32::consts::PI * 15_000.0 * i as f32 / 44_100.0).sin()).collect();
    let out = to_analysis_rate(&high, 44_100);
    let rms = (out[1000..out.len() - 1000].iter().map(|v| v * v).sum::<f32>() / (out.len() - 2000) as f32).sqrt();
    assert!(rms < 0.01, "aliased energy: {rms}");
    // 48 kHz (not a multiple) keeps a 1 kHz tone's level and length.
    let tone: Vec<f32> =
        (0..48_000).map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48_000.0).sin()).collect();
    let out = to_analysis_rate(&tone, 48_000);
    assert!((out.len() as i64 - 22_050).abs() <= 1);
    let peak = out[1000..out.len() - 1000].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!((peak - 0.5).abs() < 0.02, "{peak}");
    // Upsampling (8 kHz) works too; empty stays empty.
    assert!((to_analysis_rate(&vec![0.0; 8_000], 8_000).len() as i64 - 22_050).abs() <= 1);
    assert!(to_analysis_rate(&[], 44_100).is_empty());
}
