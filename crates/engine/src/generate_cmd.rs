//! `storyboard.generate` and `storyboard.cueMaps`: a storyboard from a track's cue map.
//!
//! The cue map (`<track>.cuemap.json`, docs/track-analysis.md §5) is read through `Services` with a
//! size cap and validated as hostile input; generation itself is pure (`assetcraft-storyboard`
//! `generate`). The audio file is expected next to the cue map (`track.file`); it isn't opened
//! here. The last generation's inputs are kept so the UI can regenerate with another pacing or
//! length (`generatedFrom` in `storyboard.state`).

use crate::storyboard::{Mode, Transport};
use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::cuemap::MAX_CUEMAP_BYTES;
use assetcraft_storyboard::{CueMap, FitMode, GenerateOptions, Pacing, generate, next_empty};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// File-name suffix of a cue map.
pub const CUEMAP_SUFFIX: &str = ".cuemap.json";

/// Lengths offered for generation, in seconds (`None` = the whole track).
pub const LENGTHS: [Option<u32>; 5] = [None, Some(30), Some(60), Some(90), Some(120)];

/// What the last generation was made from.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedFrom {
    /// The cue map file.
    pub cue_map: PathBuf,
    /// The pacing profile.
    pub pacing: Pacing,
    /// The target length (`None` = whole track).
    pub seconds: Option<u32>,
    /// Lengths the cue map can fit to (whole track included as `None`).
    pub lengths: Vec<Option<u32>>,
}

impl Session {
    /// Cue maps in the open folder (from the listing; nothing is read).
    pub fn cue_maps(&self) -> Vec<PathBuf> {
        self.items
            .iter()
            .filter(|e| e.kind == assetcraft_fs::EntryKind::File && e.name.to_lowercase().ends_with(CUEMAP_SUFFIX))
            .map(|e| e.path.clone())
            .collect()
    }

    /// What the open storyboard was generated from, if it was.
    pub fn generated_from(&self) -> Option<&GeneratedFrom> {
        self.boards.generated.as_ref()
    }
}

/// Display name for a cue map path: the file name without the suffix.
pub fn cue_map_label(path: &Path) -> String {
    let name = assetcraft_fs::display_name(path);
    match name.to_lowercase().rfind(CUEMAP_SUFFIX) {
        Some(i) => name[..i].to_string(),
        None => name.to_string(),
    }
}

pub(crate) fn sb_cue_maps(s: &mut Session, _: &Value) -> CmdResult {
    let maps: Vec<Value> =
        s.cue_maps().iter().map(|p| json!({ "path": p.to_string_lossy(), "label": cue_map_label(p) })).collect();
    Ok(json!({ "cueMaps": maps }))
}

pub(crate) fn sb_generate(s: &mut Session, p: &Value) -> CmdResult {
    let path = match p.get("cueMap").and_then(Value::as_str).map(str::trim) {
        Some(x) if !x.is_empty() && x.len() <= 4096 => PathBuf::from(x),
        _ => return Err(CmdError::BadParams("need `cueMap` (path to a .cuemap.json)".into())),
    };
    // `has_root`, not `is_absolute` (as `file.openIn`): listing paths always have one, and
    // Windows calls `\music\x` rooted but not absolute.
    if !path.has_root() {
        return Err(CmdError::BadParams("`cueMap` must be a full path".into()));
    }
    let pacing = match p.get("pacing") {
        None => s.boards.generated.as_ref().map_or(Pacing::Trailer, |g| g.pacing),
        Some(v) => v
            .as_str()
            .and_then(Pacing::from_id)
            .ok_or_else(|| CmdError::BadParams("`pacing`: calm|cinematic|energetic|trailer".into()))?,
    };
    let seconds = match p.get("seconds") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| (5..=3600).contains(n))
                .ok_or_else(|| CmdError::BadParams("`seconds`: 5..3600, or null for the whole track".into()))?,
        ),
    };
    let fit_mode = match p.get("fit").and_then(Value::as_str) {
        None => None,
        Some("fade") => Some(FitMode::Fade),
        Some("edit") => Some(FitMode::Edit),
        Some(_) => return Err(CmdError::BadParams("`fit`: fade|edit".into())),
    };
    let bytes = s.services.read_head(&path, MAX_CUEMAP_BYTES + 1).map_err(CmdError::Fs)?;
    let cue = CueMap::from_json(&bytes).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    let track_path = path.parent().map(|d| d.join(&cue.track.file)).unwrap_or_else(|| PathBuf::from(&cue.track.file));
    let name = match p.get("name").and_then(Value::as_str).map(str::trim) {
        Some(n) if !n.is_empty() && n.len() <= 256 => n.to_owned(),
        Some(_) => return Err(CmdError::BadParams("`name` must be 1–256 bytes".into())),
        None => format!("{} · {}", cue.track.title, pacing.name()),
    };
    let opts =
        GenerateOptions { name, track_path: track_path.to_string_lossy().into_owned(), pacing, seconds, fit_mode };
    let (board, report) = generate(&cue, &opts).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    let duration_secs = cue.duration_frames / u64::from(cue.sample_rate.max(1));
    let lengths = std::iter::once(None)
        .chain(
            cue.fits
                .keys()
                .filter_map(|k| k.parse::<u32>().ok())
                .filter(|k| u64::from(*k) + 2 < duration_secs)
                .map(Some),
        )
        .collect();
    s.boards.checkpoint();
    s.boards.selected = next_empty(&board);
    s.boards.board = Some(board);
    s.boards.mode = Mode::Storyboard;
    s.boards.transport = Transport::default();
    s.audio_stop();
    s.boards.generated = Some(GeneratedFrom { cue_map: path, pacing, seconds, lengths });
    let mut message = format!(
        "{} · {}: {} clips in {} scenes, {} on bars, {} on hits, {}",
        cue.track.title,
        pacing.name(),
        report.clips,
        report.scenes,
        report.cuts_on_downbeats,
        report.cuts_on_hits,
        report.length.timecode()
    );
    if let Some(c) = &report.fit_changes {
        message.push_str(&format!(" ({c})"));
    }
    Ok(json!({
        "clips": report.clips,
        "scenes": report.scenes,
        "titleCards": report.title_cards,
        "cutsOnDownbeats": report.cuts_on_downbeats,
        "cutsOnHits": report.cuts_on_hits,
        "length": report.length.timecode(),
        "fitChanges": report.fit_changes,
        "track": track_path.to_string_lossy(),
        "credit": cue.track.credit,
        "license": cue.track.license,
        "message": message,
    }))
}

/// `generatedFrom` for `storyboard.state`.
pub(crate) fn generated_json(s: &Session) -> Value {
    match &s.boards.generated {
        Some(g) => json!({
            "cueMap": g.cue_map.to_string_lossy(),
            "pacing": g.pacing.id(),
            "seconds": g.seconds,
            "lengths": g.lengths,
        }),
        None => Value::Null,
    }
}
