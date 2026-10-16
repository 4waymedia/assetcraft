//! The Start screen's sources (design 3b / 3c): the music library and clip-structure previews.
//!
//! - `storyboard.musicFolder {path | null}`: where to look for music (saved in prefs).
//! - `storyboard.tracks`: the music the Start screen offers, from the music folder and the folder
//!   open in the Organizer. A track with a `<name>.cuemap.json` beside it is analysed (title,
//!   artist, licence, tempo, the lengths it fits); other audio files are listed as not analysed
//!   yet (the Rust analyser, A2–A4, will cover them). Cue maps are read size-capped and validated
//!   as hostile input; nothing else is opened.
//! - `storyboard.preview {cueMap, pacing?, seconds?, density?}`: what `storyboard.generate` would build,
//!   as a clip-structure strip, without touching the open storyboard.

use crate::generate_cmd::{
    CUEMAP_SUFFIX, cue_map_param, density_param, fit_lengths, pacing_param, read_cue, seconds_param, shape_param,
    track_path,
};
use crate::{CmdError, CmdResult, Session};
use assetcraft_fs::{EntryKind, ListOptions};
use assetcraft_storyboard::{GenerateOptions, Pacing, generate};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Audio file extensions the library lists (what the decoder reads).
pub const AUDIO_EXTENSIONS: [&str; 9] = ["mp3", "wav", "flac", "ogg", "m4a", "aac", "aif", "aiff", "opus"];

/// Most tracks listed.
pub const MAX_TRACKS: usize = 500;

/// Bars in a track's energy graph.
pub const ENERGY_BARS: usize = 28;

/// Tempo band for the Start screen's filter: Slow < 100, Mid 100–118, Upbeat 118–135, Fast ≥ 135.
pub fn tempo_band(bpm: f64) -> &'static str {
    match bpm {
        b if b < 100.0 => "slow",
        b if b < 118.0 => "mid",
        b if b < 135.0 => "upbeat",
        _ => "fast",
    }
}

/// Seconds as `m:ss`.
fn mmss(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

fn is_cue_map(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_lowercase().ends_with(CUEMAP_SUFFIX))
}

impl Session {
    /// The music folder the Start screen reads.
    pub fn music_folder(&self) -> Option<&Path> {
        self.music_folder.as_deref()
    }

    /// Files in the music folder and the Organizer's open folder (music folder first, deduped).
    fn library_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = Vec::new();
        if let Some(dir) = &self.music_folder {
            let opts = ListOptions { include_hidden: false, max_entries: 4 * MAX_TRACKS };
            if let Ok(listing) = self.services.list_entries(dir, &opts) {
                files.extend(listing.entries.into_iter().filter(|e| e.kind == EntryKind::File).map(|e| e.path));
            }
        }
        let open = self.items.iter().filter(|e| e.kind == EntryKind::File).map(|e| e.path.clone());
        for p in open {
            if !files.contains(&p) {
                files.push(p);
            }
        }
        files
    }
}

/// `storyboard.musicFolder {path | null}`.
pub(crate) fn sb_music_folder(s: &mut Session, p: &Value) -> CmdResult {
    let folder = match p.get("path") {
        None => return Err(CmdError::BadParams("need `path` (a folder, or null to clear)".into())),
        Some(Value::Null) => None,
        Some(v) => {
            let path = v
                .as_str()
                .map(str::trim)
                .filter(|x| !x.is_empty() && x.len() <= 4096)
                .map(PathBuf::from)
                .ok_or_else(|| CmdError::BadParams("`path` must be a folder path".into()))?;
            if !path.has_root() {
                return Err(CmdError::BadParams("`path` must be a full path".into()));
            }
            let entry = s.services.entry(&path).map_err(CmdError::Fs)?;
            if entry.kind != EntryKind::Dir {
                return Err(CmdError::BadParams("`path` is not a folder".into()));
            }
            Some(path)
        }
    };
    s.music_folder = folder;
    let tracks = sb_tracks(s, &json!({}))?;
    let n = tracks["tracks"].as_array().map_or(0, Vec::len);
    let message = match &s.music_folder {
        Some(f) => format!("Music folder: {} ({n} tracks)", f.display()),
        None => "Music folder cleared".into(),
    };
    Ok(
        json!({ "musicFolder": s.music_folder.as_ref().map(|f| f.to_string_lossy()), "tracks": tracks["tracks"], "message": message }),
    )
}

/// `storyboard.tracks` → `{musicFolder, tracks: [...], truncated}`, analysed tracks first, by title.
pub(crate) fn sb_tracks(s: &mut Session, _: &Value) -> CmdResult {
    let files = s.library_files();
    // Cue maps first: each names its audio file.
    let mut by_audio: BTreeMap<PathBuf, Value> = BTreeMap::new();
    for cue_path in files.iter().filter(|p| is_cue_map(p)) {
        let Ok(cue) = read_cue(s, cue_path) else { continue };
        let audio = track_path(cue_path, &cue);
        let sr = f64::from(cue.sample_rate.max(1));
        let seconds = cue.duration_frames as f64 / sr;
        let sections: Vec<Value> = cue
            .sections
            .iter()
            .map(|c| {
                json!({
                    "label": c.label,
                    "start": (c.start as f64 / sr * 10.0).round() / 10.0,
                    "end": (c.end as f64 / sr * 10.0).round() / 10.0,
                    "energy": (f64::from(c.energy) * 100.0).round() / 100.0,
                })
            })
            .collect();
        // Energy graph: the section energy at the middle of each bar slot.
        let energy: Vec<f64> = (0..ENERGY_BARS)
            .map(|i| {
                let f = ((i as f64 + 0.5) / ENERGY_BARS as f64 * cue.duration_frames as f64) as u64;
                let e = cue.sections.iter().find(|c| f >= c.start && f < c.end).map_or(0.0, |c| f64::from(c.energy));
                (e * 100.0).round() / 100.0
            })
            .collect();
        let drops: Vec<f64> =
            cue.hits.iter().filter(|h| h.kind == "drop").map(|h| (h.frame as f64 / sr * 10.0).round() / 10.0).collect();
        let suits: Vec<&str> = Pacing::ALL.iter().filter(|p| p.suits(cue.bpm)).map(|p| p.name()).collect();
        let band = tempo_band(cue.bpm);
        let mut analysis = format!("{:.1} BPM, {band} tempo", cue.bpm);
        if !drops.is_empty() {
            let at: Vec<String> = drops.iter().take(3).map(|d| mmss(*d)).collect();
            analysis.push_str(&format!(", drop{} at {}", if drops.len() > 1 { "s" } else { "" }, at.join(", ")));
        }
        analysis.push_str(". ");
        analysis.push_str(&if suits.is_empty() {
            "No style matches this tempo; adjust the cut pace.".to_string()
        } else {
            format!("Suits {}.", suits.join(", "))
        });
        by_audio.insert(
            audio.clone(),
            json!({
                "audio": audio.to_string_lossy(),
                "cueMap": cue_path.to_string_lossy(),
                "analysed": true,
                "title": cue.track.title,
                "artist": cue.track.artist,
                "license": cue.track.license,
                "seconds": (seconds * 10.0).round() / 10.0,
                "bpm": (cue.bpm * 10.0).round() / 10.0,
                "lengths": fit_lengths(&cue),
                "band": band,
                "energy": energy,
                "sections": sections,
                "drops": drops,
                "suits": suits,
                "analysis": analysis,
            }),
        );
    }
    // Audio without a cue map: listed, not analysed yet.
    for audio in files.iter().filter(|p| is_audio(p)) {
        by_audio.entry(audio.clone()).or_insert_with(|| {
            let title = audio.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            json!({ "audio": audio.to_string_lossy(), "cueMap": null, "analysed": false, "title": title })
        });
    }
    let mut tracks: Vec<Value> = by_audio.into_values().collect();
    tracks
        .sort_by_key(|t| (!t["analysed"].as_bool().unwrap_or(false), t["title"].as_str().unwrap_or("").to_lowercase()));
    let truncated = tracks.len() > MAX_TRACKS;
    tracks.truncate(MAX_TRACKS);
    Ok(json!({
        "musicFolder": s.music_folder.as_ref().map(|f| f.to_string_lossy()),
        "tracks": tracks,
        "truncated": truncated,
    }))
}

/// `storyboard.preview {cueMap, pacing?, seconds?, density?, shape?}` → `{clips: [{shot, seconds}],
/// count, titleCards, scenes, length, avgSeconds, avgBeats, fitChanges}`. The open storyboard is
/// untouched.
pub(crate) fn sb_preview(s: &mut Session, p: &Value) -> CmdResult {
    let path = cue_map_param(p)?;
    let pacing = pacing_param(p, Pacing::Trailer)?;
    let seconds = seconds_param(p)?;
    let density = density_param(p, 1.0)?;
    let shape = shape_param(p, assetcraft_storyboard::Shape::default())?;
    let cue = read_cue(s, &path)?;
    let opts = GenerateOptions {
        name: cue.track.title.clone(),
        track_path: track_path(&path, &cue).to_string_lossy().into_owned(),
        pacing,
        seconds,
        fit_mode: None,
        density,
        shape,
    };
    let (board, report) = generate(&cue, &opts).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    let avg = if report.clips > 0 { report.length.as_secs_f64() / report.clips as f64 } else { 0.0 };
    let r2 = |v: f64| (v * 100.0).round() / 100.0;
    let scene_index: std::collections::HashMap<_, usize> =
        board.scenes.iter().enumerate().map(|(i, sc)| (sc.id, i)).collect();
    let clips: Vec<Value> = board
        .clips
        .iter()
        .zip(board.starts())
        .map(|(c, at)| {
            let mut v = json!({
                "shot": c.shot_type.code(),
                "start": r2(at.as_secs_f64()),
                "seconds": r2(c.duration.as_secs_f64()),
                "scene": scene_index.get(&c.scene_id).copied().unwrap_or(0),
                "in": c.transition_in,
                "out": c.transition_out,
            });
            if let Some(kind) = c.text_kind() {
                v["text"] = json!(kind.id());
            }
            v
        })
        .collect();
    let track_seconds = cue.duration_frames as f64 / f64::from(cue.sample_rate.max(1));
    let edit = edit_json(board.track.as_ref().and_then(|t| t.edit.as_ref()), board_seconds(&board), track_seconds);
    Ok(json!({
        "clips": clips,
        "count": report.clips,
        "titleCards": report.title_cards,
        "scenes": report.scenes,
        "length": report.length.timecode(),
        "avgSeconds": (avg * 10.0).round() / 10.0,
        "avgBeats": (avg * cue.bpm / 60.0 * 10.0).round() / 10.0,
        "fitChanges": report.fit_changes,
        "trackSeconds": r2(track_seconds),
        "edit": edit,
    }))
}

fn board_seconds(board: &assetcraft_storyboard::Storyboard) -> f64 {
    board.clips.iter().map(|c| c.duration.as_secs_f64()).sum()
}

/// The audio edit in track seconds: `segments` played back to back (the whole track up to the
/// board's length when there is no edit), and the fade-out as `[start, seconds]` on the board's
/// timeline.
fn edit_json(edit: Option<&assetcraft_storyboard::TrackEdit>, board_secs: f64, track_secs: f64) -> Value {
    let r2 = |v: f64| (v * 100.0).round() / 100.0;
    let Some(e) = edit.filter(|e| e.sample_rate > 0) else {
        return json!({ "segments": [[0.0, r2(board_secs.min(track_secs))]] });
    };
    let sec = |f: u64| f as f64 / f64::from(e.sample_rate);
    let segments: Vec<[f64; 2]> = e.segments.iter().map(|[a, b]| [r2(sec(*a)), r2(sec(*b))]).collect();
    let mut v = json!({ "segments": segments });
    if let Some([s, n]) = e.fade_out {
        v["fadeOut"] = json!([r2(sec(s)), r2(sec(n))]);
    }
    v
}
