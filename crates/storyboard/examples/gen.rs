//! Dev aid: `cargo run -p assetcraft-storyboard --example gen -- <cuemap.json> <pacing> [seconds]`
//! prints the generated clip list.
use assetcraft_storyboard::{CueMap, GenerateOptions, Pacing, generate};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else { return eprintln!("usage: gen <cuemap.json> <pacing> [seconds]") };
    let bytes = std::fs::read(path).unwrap_or_default();
    let cue = match CueMap::from_json(&bytes) {
        Ok(c) => c,
        Err(e) => return eprintln!("{e}"),
    };
    let pacing = args.get(2).and_then(|p| Pacing::from_id(p)).unwrap_or(Pacing::Trailer);
    let seconds = args.get(3).and_then(|s| s.parse().ok());
    let opts = GenerateOptions {
        name: cue.track.title.clone(),
        track_path: cue.track.file.clone(),
        pacing,
        seconds,
        fit_mode: None,
        density: args.get(4).and_then(|d| d.parse().ok()).unwrap_or(1.0),
        shape: Default::default(),
    };
    match generate(&cue, &opts) {
        Ok((board, r)) => {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(
                out,
                "{} · {} · {} clips, {} scenes, {} titles, {} cuts on bars, {} on hits, {} · {}",
                board.name,
                pacing.name(),
                r.clips,
                r.scenes,
                r.title_cards,
                r.cuts_on_downbeats,
                r.cuts_on_hits,
                r.length.timecode(),
                r.fit_changes.unwrap_or_default()
            );
            let starts = board.starts();
            for (c, s) in board.clips.iter().zip(starts) {
                let _ = writeln!(
                    out,
                    "  #{:02} {:>6} {:>4} {:5.2}s  {:<28} {}",
                    c.id.0,
                    s.timecode(),
                    c.shot_type.code(),
                    c.duration.as_secs_f64(),
                    c.description,
                    c.notes
                );
            }
        }
        Err(e) => eprintln!("{e}"),
    }
}
