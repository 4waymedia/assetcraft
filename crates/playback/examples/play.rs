//! Hear a track on the default output device (dev check for Watch your Cut's audio path).
//!
//! `cargo run -p assetcraft-playback --release --example play -- <audio file> [from seconds] [play seconds]`

use assetcraft_audio::{Player, Program};
use assetcraft_fs::RealFs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(file) = args.next().map(PathBuf::from) else {
        eprintln!("usage: play <audio file> [from seconds] [play seconds]");
        return;
    };
    let from: f64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let secs: f64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(10.0);
    let audio = match assetcraft_audio::load(&RealFs::new(), &file) {
        Ok(a) => Arc::new(a),
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    println!("{:?}", audio.summary());
    let player = assetcraft_playback::DevicePlayer::new();
    if let Err(e) = player.play(Arc::new(Program::new(audio, None)), from) {
        eprintln!("can't play: {e}");
        return;
    }
    println!("playing on {}", player.device().unwrap_or_default());
    let steps = (secs * 4.0).max(1.0) as u32;
    for _ in 0..steps {
        std::thread::sleep(Duration::from_millis(250));
        println!("clock {:.2}s", player.position().unwrap_or(0.0));
    }
    player.stop();
}
