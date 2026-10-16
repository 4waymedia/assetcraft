//! Track commands (`docs/track-analysis.md` §9.1, phase A1): read a track's audio facts.
//!
//! Decoding is `assetcraft-audio` (SoundCraft's decoder, with Storyboard's limits). A1 answers
//! `track.summary`; analysis (`track.analyze`) and playback build on the same decode.

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

/// `track.preview {path | null, from?}`: play a track from the Start screen (null stops).
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
    let status = s.preview_start(path.clone(), from);
    Ok(json!({ "preview": status, "path": path.to_string_lossy(), "from": from }))
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
