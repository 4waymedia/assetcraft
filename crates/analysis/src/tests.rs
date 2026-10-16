use super::*;
use std::f32::consts::PI;

fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
    (0..(secs * RATE as f32) as usize).map(|i| amp * (2.0 * PI * freq * i as f32 / RATE as f32).sin()).collect()
}

/// Clicks (5 ms decaying 2 kHz bursts) at `bpm` for `secs`; returns the signal and the click times.
pub(crate) fn clicks(bpm: f32, secs: f32) -> (Vec<f32>, Vec<f64>) {
    let n = (secs * RATE as f32) as usize;
    let mut s = vec![0.0f32; n];
    let period = 60.0 / bpm;
    let mut times = Vec::new();
    let mut t = 0.25f32;
    while t < secs - 0.1 {
        let at = (t * RATE as f32) as usize;
        for i in 0..(0.005 * RATE as f32) as usize {
            if let Some(v) = s.get_mut(at + i) {
                *v = 0.8 * (-(i as f32) / 30.0).exp() * (2.0 * PI * 2000.0 * i as f32 / RATE as f32).sin();
            }
        }
        times.push(f64::from(t));
        t += period;
    }
    (s, times)
}

/// Deterministic white noise at `amp`.
fn noise(amp: f32, secs: f32) -> Vec<f32> {
    let mut x: u32 = 0x1234_5678;
    (0..(secs * RATE as f32) as usize)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            amp * (x as f32 / u32::MAX as f32 * 2.0 - 1.0)
        })
        .collect()
}

fn mean(v: &[f32]) -> f32 {
    v.iter().sum::<f32>() / v.len().max(1) as f32
}

/// The middle half of a curve (away from the padded edges).
fn mid(v: &[f32]) -> &[f32] {
    &v[v.len() / 4..v.len() * 3 / 4]
}

#[test]
fn a_full_scale_sine_reads_minus_3_db_in_its_band_with_its_pitch_as_centroid() {
    let c = curves(&sine(1000.0, 1.0, 2.0));
    assert_eq!(c.frames(), 2 * RATE as usize / HOP + 1);
    assert!((mean(mid(&c.rms_db)) + 3.01).abs() < 0.1, "{}", mean(mid(&c.rms_db)));
    let mid_band = mean(mid(&c.bands_db[2]));
    assert!((mid_band + 3.0).abs() < 0.3, "the 500–2,000 Hz band carries it: {mid_band}");
    assert!(mean(mid(&c.bands_db[0])) < mid_band - 40.0, "nothing in the bass");
    assert!((mean(mid(&c.centroid_hz)) - 1000.0).abs() < 60.0, "{}", mean(mid(&c.centroid_hz)));
    assert!(mean(mid(&c.flatness)) < 0.01, "a tone is not flat: {}", mean(mid(&c.flatness)));
    assert!(mean(mid(&c.onset)) < 0.2, "a steady tone has no real onsets (clicks read > 1): {}", mean(mid(&c.onset)));
    // Bass lands in the bass band.
    let b = curves(&sine(60.0, 0.5, 2.0));
    assert!(mean(mid(&b.bands_db[0])) > mean(mid(&b.bands_db[2])) + 40.0);
}

#[test]
fn noise_is_flat_and_bright() {
    let c = curves(&noise(0.5, 2.0));
    assert!(mean(mid(&c.flatness)) > 0.4, "{}", mean(mid(&c.flatness)));
    assert!(mean(mid(&c.centroid_hz)) > 3000.0, "{}", mean(mid(&c.centroid_hz)));
}

#[test]
fn onset_strength_peaks_on_each_click() {
    let (sig, times) = clicks(120.0, 6.0);
    let c = curves(&sig);
    let fps = c.frame_rate();
    // Every click is a local peak of the curve within two frames.
    for t in &times {
        let f = (t * fps).round() as usize;
        let near = c.onset[f.saturating_sub(2)..(f + 3).min(c.frames())].iter().copied().fold(0.0, f32::max);
        let between =
            c.onset[(f + 6).min(c.frames() - 1)..(f + 15).min(c.frames())].iter().copied().fold(0.0, f32::max);
        assert!(near > 1.0 && near > 10.0 * between.max(0.01), "click at {t}: {near} vs {between}");
    }
    assert!((c.time_of(43) - 43.0 * 512.0 / 22_050.0).abs() < 1e-12);
}

#[test]
fn hostile_input_gives_curves_or_nothing_never_a_panic() {
    let empty = curves(&[]);
    assert_eq!((empty.frames(), empty.bands_db.len()), (0, 4));
    let one = curves(&[0.5]);
    assert_eq!(one.frames(), 1);
    let silent = curves(&vec![0.0; 10_000]);
    assert!(silent.rms_db.iter().all(|v| *v == curves::FLOOR_DB));
    assert!(silent.flatness.iter().chain(&silent.centroid_hz).chain(&silent.onset).all(|v| *v == 0.0));
    let mut nan = sine(440.0, 0.5, 0.5);
    nan.iter_mut().step_by(7).for_each(|v| *v = f32::NAN);
    nan[100] = f32::INFINITY;
    let c = curves(&nan);
    assert!(
        c.rms_db
            .iter()
            .chain(&c.onset)
            .chain(&c.centroid_hz)
            .chain(&c.flatness)
            .chain(c.bands_db.iter().flatten())
            .all(|v| v.is_finite())
    );
    // Serialises with stable names.
    let v = serde_json::to_value(curves(&sine(440.0, 0.5, 0.1))).unwrap();
    assert!(v.get("rmsDb").is_some() && v.get("bandsDb").is_some() && v.get("centroidHz").is_some());
}
