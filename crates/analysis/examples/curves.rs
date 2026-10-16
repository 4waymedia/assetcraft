//! Print a track's analysis summary: curves (A2a) and tempo (A2b); the oracle comparison in A2d.
//!
//! `cargo run -p assetcraft-analysis --release --example curves -- <audio file>...`

use std::time::Instant;

fn main() {
    let fs = assetcraft_fs::RealFs::new();
    for arg in std::env::args().skip(1) {
        let t0 = Instant::now();
        let audio = match assetcraft_audio::load(&fs, std::path::Path::new(&arg)) {
            Ok(a) => a,
            Err(e) => {
                println!("{arg}: {e}");
                continue;
            }
        };
        let t1 = Instant::now();
        let signal = audio.analysis_signal();
        let t2 = Instant::now();
        let c = assetcraft_analysis::curves(&signal);
        let t3 = Instant::now();
        let tempo = assetcraft_analysis::tempo(&c);
        let t4 = Instant::now();
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
        let name = std::path::Path::new(&arg).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        println!(
            "{name}: {:.1}s, {} frames · rms {:.1} dB · bass {:.1} mid {:.1} high {:.1} dB · centroid {:.0} Hz · flatness {:.3} · onset {:.2} · decode {:.2}s resample {:.2}s curves {:.2}s",
            audio.seconds(),
            c.frames(),
            mean(&c.rms_db),
            mean(&c.bands_db[0]),
            mean(&c.bands_db[2]),
            mean(&c.bands_db[3]),
            mean(&c.centroid_hz),
            mean(&c.flatness),
            mean(&c.onset),
            (t1 - t0).as_secs_f64(),
            (t2 - t1).as_secs_f64(),
            (t3 - t2).as_secs_f64(),
        );
        match tempo {
            Some(t) => println!(
                "    tempo {:.1} BPM ({:?}, clarity {:.2}, cv {:.3}, offbeat {:.2}, {} beats) · ×½ {:.1} ({:.2}) ×2 {:.1} ({:.2}) · candidates {} · {:.2}s",
                t.bpm,
                t.rhythm,
                t.pulse_clarity,
                t.stability_cv,
                t.offbeat,
                t.beats.len(),
                t.half.bpm,
                t.half.score,
                t.double.bpm,
                t.double.score,
                t.candidates.iter().map(|c| format!("{:.1} ({:.2})", c.bpm, c.score)).collect::<Vec<_>>().join(", "),
                (t4 - t3).as_secs_f64(),
            ),
            None => println!("    no tempo"),
        }
    }
}
