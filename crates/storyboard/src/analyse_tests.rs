use crate::analyse::*;
use crate::cuemap::{CueMap, FitMode};
use assetcraft_analysis::{RATE, curves, tempo};
use std::f32::consts::PI;

const SR: u32 = 44_100;

/// Clicks at `bpm` (5 ms 2 kHz bursts from 0.25 s), plus a 60 Hz bass tone from `bass_from`
/// seconds on, for `secs`.
fn song(bpm: f32, secs: f32, bass_from: f32) -> Vec<f32> {
    let n = (secs * RATE as f32) as usize;
    let mut s = vec![0.0f32; n];
    let period = 60.0 / bpm;
    let mut t = 0.25f32;
    while t < secs - 0.1 {
        let at = (t * RATE as f32) as usize;
        for i in 0..(0.005 * RATE as f32) as usize {
            if let Some(v) = s.get_mut(at + i) {
                *v = 0.8 * (-(i as f32) / 30.0).exp() * (2.0 * PI * 2000.0 * i as f32 / RATE as f32).sin();
            }
        }
        t += period;
    }
    let from = (bass_from * RATE as f32) as usize;
    for (i, v) in s.iter_mut().enumerate().skip(from) {
        *v += 0.4 * (2.0 * PI * 60.0 * i as f32 / RATE as f32).sin();
    }
    s
}

fn facts(file: &str, secs: f32) -> (String, u64) {
    (file.to_string(), (secs * SR as f32) as u64)
}

fn analyse(file: &str, secs: f32, bass_from: f32) -> Result<CueMap, AnalyseError> {
    let signal = song(120.0, secs, bass_from);
    let c = curves(&signal);
    let t = tempo(&c);
    let (file, duration_frames) = facts(file, secs);
    cue_map(&TrackFacts { file: &file, sample_rate: SR, duration_frames }, &c, t.as_ref())
}

#[test]
fn a_click_track_with_a_bass_drop_gives_a_valid_cue_map() {
    let map = analyse("song.wav", 70.0, 35.0).unwrap();
    assert!(map.validate().is_ok());
    assert_eq!(map.analyzer, ANALYZER);
    assert_eq!(map.track.file, "song.wav");
    assert_eq!(map.track.title, "song");
    assert_eq!(map.track.license, "unknown");
    assert_eq!((map.sample_rate, map.duration_frames), (SR, 70 * u64::from(SR)));
    assert!((map.bpm - 120.0).abs() < 1.2, "{}", map.bpm);
    assert_eq!(map.beats_per_bar, 4);
    assert!(map.pulse_clarity > 0.25, "{}", map.pulse_clarity);
    // About 139 beats, every fourth a downbeat, all in source frames inside the track.
    assert!((130..=142).contains(&map.beats.len()), "{} beats", map.beats.len());
    assert!(map.downbeats.len().abs_diff(map.beats.len().div_ceil(4)) <= 1, "{} bars", map.downbeats.len());
    assert!(map.downbeats.iter().all(|d| map.beats.contains(d)));
    let second = map.beats[1] as f64 / f64::from(SR);
    assert!((0.7..0.8).contains(&second), "second beat at {second} s (0.25 + 0.5)");
    // Sections cover the track, the first one is the quiet Intro.
    assert!(map.sections.len() >= 2, "{:?}", map.sections);
    assert_eq!(map.sections[0].start, 0);
    assert_eq!(map.sections.last().unwrap().end, map.duration_frames);
    assert!(map.sections.windows(2).all(|w| w[0].end == w[1].start));
    assert_eq!(map.sections[0].label, "Intro");
    assert!(map.sections.last().unwrap().energy > map.sections[0].energy);
    // The bass coming in at 35 s is a drop at the bar line near it.
    let drops: Vec<f64> =
        map.hits.iter().filter(|h| h.kind == "drop").map(|h| h.frame as f64 / f64::from(SR)).collect();
    assert_eq!(drops.len(), 1, "{drops:?}");
    assert!((33.0..38.0).contains(&drops[0]), "{drops:?}");
    assert!(map.hits.windows(2).all(|w| w[0].frame <= w[1].frame));
    // Fits at 30 and 60 s (not 90: the track is 70 s), each a fade ending on a bar near the target.
    assert_eq!(map.fits.keys().cloned().collect::<Vec<_>>(), ["30", "60"]);
    for (key, target) in [("30", 30.0), ("60", 60.0)] {
        let choice = &map.fits[key];
        assert_eq!(choice.chosen, Some(FitMode::Fade));
        assert!(choice.edit.is_none(), "the edit candidate waits for A4");
        let fade = choice.fade.as_ref().unwrap();
        let end = fade.length_frames as f64 / f64::from(SR);
        assert!((target - 6.0..=target + 1.5).contains(&end), "{key}: ends at {end}");
        assert!(map.downbeats.contains(&fade.length_frames), "ends on a bar line");
        let out = fade.fade_out.unwrap();
        assert_eq!(out.start + out.frames, fade.length_frames);
        assert!((out.frames as f64 / f64::from(SR) - 4.0).abs() < 0.05, "two bars at 120 BPM");
        assert!(fade.changes.starts_with("Cut to "));
    }
    // What we write, the loader accepts.
    let bytes = serde_json::to_vec(&map).unwrap();
    assert_eq!(CueMap::from_json(&bytes).unwrap(), map);
}

#[test]
fn silence_short_audio_and_bad_facts_are_errors_not_panics() {
    let quiet = vec![0.0f32; 10 * RATE as usize];
    let c = curves(&quiet);
    let f = TrackFacts { file: "quiet.wav", sample_rate: SR, duration_frames: 10 * u64::from(SR) };
    assert_eq!(cue_map(&f, &c, tempo(&c).as_ref()), Err(AnalyseError::NoPulse));
    assert_eq!(cue_map(&f, &c, None), Err(AnalyseError::NoPulse));
    assert!(matches!(analyse("short.wav", 2.0, 1.0), Err(AnalyseError::NoPulse)));
    assert!(matches!(analyse("a/b.wav", 40.0, 20.0), Err(AnalyseError::BadFacts(_))));
    assert!(matches!(analyse("", 40.0, 20.0), Err(AnalyseError::BadFacts(_))));
    let signal = song(120.0, 40.0, 20.0);
    let c = curves(&signal);
    let t = tempo(&c);
    for (rate, frames) in [(0u32, 1u64), (SR, 0)] {
        let f = TrackFacts { file: "x.wav", sample_rate: rate, duration_frames: frames };
        assert!(matches!(cue_map(&f, &c, t.as_ref()), Err(AnalyseError::BadFacts(_))));
    }
}

#[test]
fn hostile_curves_still_give_a_valid_map_or_an_error() {
    let signal = song(120.0, 40.0, 20.0);
    let mut c = curves(&signal);
    let t = tempo(&c);
    // Poison every curve with NaN and infinities.
    for (i, v) in c.rms_db.iter_mut().enumerate() {
        if i % 7 == 0 {
            *v = f32::NAN;
        }
    }
    for band in &mut c.bands_db {
        for (i, v) in band.iter_mut().enumerate() {
            if i % 5 == 0 {
                *v = f32::INFINITY;
            }
        }
    }
    c.onset[3] = f32::NEG_INFINITY;
    c.centroid_hz.truncate(10);
    c.flatness.clear();
    let f = TrackFacts { file: "x.wav", sample_rate: SR, duration_frames: 40 * u64::from(SR) };
    if let Ok(map) = cue_map(&f, &c, t.as_ref()) {
        assert!(map.validate().is_ok());
    }
    // A tempo whose beats run past the track, or carry NaN, is clamped away.
    let mut bad = t.clone().unwrap();
    bad.beats.push(f64::NAN);
    bad.beats.push(1e9);
    bad.beats.insert(0, -5.0);
    let map = cue_map(&f, &curves(&signal), Some(&bad)).unwrap();
    assert!(map.validate().is_ok());
    assert!(map.beats.iter().all(|b| *b <= map.duration_frames));
}
