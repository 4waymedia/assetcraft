//! Track commands (`docs/track-analysis.md` §9.1, phase A1): read a track's audio facts.
//!
//! Decoding is `assetcraft-audio` (SoundCraft's decoder, with Storyboard's limits). A1 answers
//! `track.summary`; A2c answers `track.analyze` (the cue map); playback builds on the same decode.

use crate::audio_out::PreviewMode;
use crate::{CmdError, CmdResult, Session};
use serde_json::{Value, json};
use std::path::PathBuf;

/// The `path` param: a full path to an audio file.
pub(crate) fn audio_path(p: &Value) -> Result<PathBuf, CmdError> {
    let path = p
        .get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| CmdError::BadParams("need `path` (an audio file)".into()))?;
    if !path.has_root() {
        return Err(CmdError::BadParams("`path` must be a full path".into()));
    }
    Ok(path)
}

/// `track.summary {path}` → `{path, format, rate, channels, frames, seconds, peakDbfs, rmsDbfs}`.
pub(crate) fn track_summary(s: &mut Session, p: &Value) -> CmdResult {
    let path = audio_path(p)?;
    let audio = assetcraft_audio::load(s.services.as_ref(), &path).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    let mut v = serde_json::to_value(audio.summary()).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut v {
        map.insert("path".into(), json!(path.to_string_lossy()));
    }
    Ok(v)
}

/// `track.analyze {path}` → `{path, cueMapPath, cueMap, seconds, tempo: {bpm, candidates, half,
/// double, pulseClarity, stabilityCv, rhythm}}`: decode, analyse (A2a curves, A2b tempo) and build
/// the cue map (`assetcraft_storyboard::cue_map`). Reads only; `cueMapPath` is where a writer
/// would put the sidecar (`<stem>.cuemap.json` beside the audio), which is what `storyboard.tracks`
/// looks for. The CLI's `analyze --cuemap` writes it; the app's background job comes with A2c-2.
pub(crate) fn track_analyze(s: &mut Session, p: &Value) -> CmdResult {
    let path = audio_path(p)?;
    let started = std::time::Instant::now();
    let audio = assetcraft_audio::load(s.services.as_ref(), &path).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| CmdError::BadParams("`path` has no file name".into()))?;
    let facts = assetcraft_storyboard::TrackFacts {
        file: &file,
        sample_rate: audio.rate(),
        duration_frames: audio.frames() as u64,
    };
    let curves = assetcraft_analysis::curves(&audio.analysis_signal());
    let tempo = assetcraft_analysis::tempo(&curves);
    let cue = assetcraft_storyboard::cue_map(&facts, &curves, tempo.as_ref())
        .map_err(|e| CmdError::Unavailable(format!("{}: {e}", path.display())))?;
    let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| file.clone());
    let cue_map_path = path.with_file_name(format!("{stem}{}", crate::generate_cmd::CUEMAP_SUFFIX));
    let tempo = tempo.map(|t| {
        json!({
            "bpm": t.bpm, "candidates": t.candidates, "half": t.half, "double": t.double,
            "pulseClarity": t.pulse_clarity, "stabilityCv": t.stability_cv, "rhythm": t.rhythm,
        })
    });
    Ok(json!({
        "path": path.to_string_lossy(),
        "cueMapPath": cue_map_path.to_string_lossy(),
        "cueMap": serde_json::to_value(&cue).unwrap_or(Value::Null),
        "seconds": started.elapsed().as_secs_f64(),
        "tempo": tempo,
    }))
}

/// `track.preview {path | null, from?, cut?: {cueMap, seconds?, fit?}}`: play a track from the
/// Start screen (null stops). Without `cut`: the whole song, `from` in song seconds (Song mode).
/// With `cut`: the song through the edit the cue map's fit for `seconds` gives (the same one
/// `storyboard.generate` puts on the board), `from` in cut seconds (Cut mode).
pub(crate) fn track_preview(s: &mut Session, p: &Value) -> CmdResult {
    let path = match p.get("path") {
        None => return Err(CmdError::BadParams("need `path` (an audio file, or null to stop)".into())),
        Some(Value::Null) => {
            s.preview_stop();
            return Ok(json!({ "preview": "stopped" }));
        }
        Some(v) => v
            .as_str()
            .map(str::trim)
            .filter(|x| !x.is_empty() && x.len() <= 4096)
            .map(PathBuf::from)
            .ok_or_else(|| CmdError::BadParams("`path` must be a file path".into()))?,
    };
    if !path.has_root() {
        return Err(CmdError::BadParams("`path` must be a full path".into()));
    }
    let from = match p.get("from") {
        None | Some(Value::Null) => 0.0,
        Some(v) => v
            .as_f64()
            .filter(|f| f.is_finite() && *f >= 0.0)
            .ok_or_else(|| CmdError::BadParams("`from` must be seconds, 0 or more".into()))?,
    };
    let (mode, edit) = match p.get("cut") {
        None | Some(Value::Null) => (PreviewMode::Song, None),
        Some(c @ Value::Object(_)) => {
            let cue_path = crate::generate_cmd::cue_map_param(c)?;
            let cue = crate::generate_cmd::read_cue(s, &cue_path)?;
            let seconds = crate::generate_cmd::seconds_param(c)?;
            let fit_mode = crate::generate_cmd::fit_mode_param(c)?;
            let fit = assetcraft_storyboard::fit_for(&cue, seconds, fit_mode)
                .map_err(|e| CmdError::Unavailable(e.to_string()))?;
            (PreviewMode::Cut, assetcraft_storyboard::track_edit(&cue, fit))
        }
        Some(_) => return Err(CmdError::BadParams("`cut` must be {cueMap, seconds?, fit?}".into())),
    };
    let status = s.preview_start(path.clone(), from, mode, edit);
    let mode = match mode {
        PreviewMode::Song => "song",
        PreviewMode::Cut => "cut",
    };
    Ok(json!({ "preview": status, "mode": mode, "path": path.to_string_lossy(), "from": from }))
}

/// `track.previewVolume {volume: 0..1}`: the Start screen preview's level (saved).
pub(crate) fn track_preview_volume(s: &mut Session, p: &Value) -> CmdResult {
    let v = p
        .get("volume")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
        .ok_or_else(|| CmdError::BadParams("`volume` must be 0..1".into()))?;
    s.set_preview_volume(v as f32);
    Ok(json!({ "volume": s.preview_volume() }))
}
