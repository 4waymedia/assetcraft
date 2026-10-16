//! Print a track's analysis curves summary (dev check for A2a; the oracle comparison in A2d).
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
    }
}
