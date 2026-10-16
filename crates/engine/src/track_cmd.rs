//! Track commands (`docs/track-analysis.md` §9.1, phase A1): read a track's audio facts.
//!
//! Decoding is `assetcraft-audio` (SoundCraft's decoder, with Storyboard's limits). A1 answers
//! `track.summary`; analysis (`track.analyze`) and playback build on the same decode.

use crate::{CmdError, CmdResult, Session};
use serde_json::{Value, json};
use std::path::PathBuf;

/// `track.summary {path}` → `{path, format, rate, channels, frames, seconds, peakDbfs, rmsDbfs}`.
pub(crate) fn track_summary(s: &mut Session, p: &Value) -> CmdResult {
    let path = p
        .get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| CmdError::BadParams("need `path` (an audio file)".into()))?;
    if !path.has_root() {
        return Err(CmdError::BadParams("`path` must be a full path".into()));
    }
    let audio = assetcraft_audio::load(s.services.as_ref(), &path).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    let mut v = serde_json::to_value(audio.summary()).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut v {
        map.insert("path".into(), json!(path.to_string_lossy()));
    }
    Ok(v)
}
