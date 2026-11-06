//! The storyboard as a file: Save, Save As, Open and Recent (SF1–SF2; plan step 7, decision
//! D3: AssetCraft's own files only, written atomically, never overwriting a different file
//! without being told; everything read from disk is hostile until `Storyboard::from_json_with`
//! has validated it).
//!
//! - `storyboard.save {path?, overwrite?}`: write the open storyboard to its own document (no
//!   `path`), or to `path` (Save As). A storyboard that has never been saved needs a `path`.
//! - `storyboard.saveAs {path, overwrite?}`: the same with `path` required.
//! - `storyboard.open {path}`: read a `.storyboard.json` (at most the document limit), validate
//!   it, and make it the open storyboard — one undo step, like New (D-SF5), so the board it
//!   replaces is not lost; a file from a newer AssetCraft is refused; assets that are no longer
//!   where the file says are reported per clip and the board keeps them (the Viewer shows them
//!   empty). The file goes to the top of the recent list.
//! - `storyboard.recent {}`: the last [`MAX_RECENT`] storyboards opened or saved, newest first,
//!   with whether each still exists.
//!
//! Rules (D-SF1): the file is `<name>.storyboard.json` ([`SUFFIX`]); the folder is the user's
//! choice (the UI offers `<working folder>/Storyboards/`, [`STORYBOARDS_DIR`], created on first
//! save). The bytes are `Storyboard::to_json` (the versioned envelope `storyboard.open` reads
//! back). The write is `<folder>/.<name>.<pid>.tmp`, flushed, then renamed over the target, so a
//! crash mid-write leaves the old file intact and never a half file under the real name. Writing
//! over a file that is *not* this document is refused unless `overwrite: true` (the UI's picker
//! has already asked). Nothing else on disk is touched.
//!
//! Dirty: the session keeps the board as it was last saved (or opened / created) and compares,
//! so undoing back to the saved state is clean again and a no-op edit never marks the document.

use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{BoardError, FORMAT_VERSION, Limits, Storyboard};
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

/// What a storyboard document's name ends with.
pub const SUFFIX: &str = ".storyboard.json";
/// Where the UI offers to save, inside the working folder.
pub const STORYBOARDS_DIR: &str = "Storyboards";
/// Longest path accepted.
const MAX_PATH: usize = 4096;
/// Storyboards remembered for Open Recent.
pub const MAX_RECENT: usize = 10;

impl Session {
    /// The open storyboard's file, once it has been saved (or opened).
    pub fn document(&self) -> Option<&Path> {
        self.boards.document.as_deref()
    }

    /// `true` when the open storyboard differs from what its document holds (or it was never
    /// saved and has been edited since it was created).
    pub fn storyboard_dirty(&self) -> bool {
        match &self.boards.board {
            None => false,
            Some(board) => self.boards.saved.as_ref() != Some(board),
        }
    }

    /// Where Save As starts: `<working folder>/Storyboards` (may not exist yet).
    pub fn storyboards_folder(&self) -> Option<PathBuf> {
        self.working_folder().map(|w| w.join(STORYBOARDS_DIR))
    }

    /// The storyboards opened or saved most recently, newest first (saved in prefs).
    pub fn recent_storyboards(&self) -> &[PathBuf] {
        &self.recent_storyboards
    }

    /// `<working folder>/Storyboards`, created when missing (AssetCraft's own folder, like
    /// `Music/`; D-SF1: "created on first save"), so a Save picker can start there. `None` when
    /// there is no working folder or it can't be made.
    pub fn prepare_storyboards_folder(&self) -> Option<PathBuf> {
        let dir = self.storyboards_folder()?;
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    /// The file name Save As suggests for the open storyboard: `<name>.storyboard.json`, with
    /// the characters no file system takes replaced.
    pub fn suggested_document_name(&self) -> String {
        let name = self.boards.board.as_ref().map(|b| b.name.trim()).filter(|n| !n.is_empty()).unwrap_or("Storyboard");
        let clean: String =
            name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '-' } else { c }).collect();
        let clean: String = clean.trim_matches(['.', ' ', '-']).chars().take(120).collect();
        let clean = clean.trim();
        let clean = if clean.is_empty() { "Storyboard" } else { clean };
        format!("{clean}{SUFFIX}")
    }
}

/// `storyboard.save {path?, overwrite?: bool}` → `{path, bytes, message}` + the state.
pub(crate) fn sb_save(s: &mut Session, p: &Value) -> CmdResult {
    let path = match p.get("path") {
        None | Some(Value::Null) => s.boards.document.clone().ok_or_else(|| {
            CmdError::BadParams("this storyboard has not been saved yet: give a `path` (Save As)".into())
        })?,
        Some(v) => path_param(v)?,
    };
    save_to(s, path, flag(p, "overwrite"))
}

/// `storyboard.saveAs {path, overwrite?: bool}`: Save with the path required.
pub(crate) fn sb_save_as(s: &mut Session, p: &Value) -> CmdResult {
    let path = p.get("path").map(path_param).unwrap_or_else(|| Err(CmdError::BadParams("need `path`".into())))?;
    save_to(s, path, flag(p, "overwrite"))
}

fn flag(p: &Value, name: &str) -> bool {
    p.get(name).and_then(Value::as_bool).unwrap_or(false)
}

/// An absolute `<name>.storyboard.json` path, else why not.
fn path_param(v: &Value) -> Result<PathBuf, CmdError> {
    let text = v
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty() && t.len() <= MAX_PATH)
        .ok_or_else(|| CmdError::BadParams("`path` must be a file path".into()))?;
    let path = PathBuf::from(text);
    if !path.is_absolute() {
        return Err(CmdError::BadParams("`path` must be absolute".into()));
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if !name.to_ascii_lowercase().ends_with(SUFFIX) || name.len() <= SUFFIX.len() {
        return Err(CmdError::BadParams(format!("`path` must end with `{SUFFIX}`")));
    }
    Ok(path)
}

fn save_to(s: &mut Session, path: PathBuf, overwrite: bool) -> CmdResult {
    let board = s.boards.board.as_ref().ok_or_else(crate::storyboard::no_board)?;
    let is_own = s.boards.document.as_deref().is_some_and(|d| crate::paths::same_path(d, &path));
    if !is_own && !overwrite && std::fs::symlink_metadata(&path).is_ok() {
        return Err(CmdError::Unavailable(format!(
            "{} exists and is not this storyboard's file; pass `overwrite: true` to replace it",
            path.display()
        )));
    }
    let json = board.to_json().map_err(|e| CmdError::Unavailable(format!("could not encode the storyboard: {e}")))?;
    write_atomic(&path, json.as_bytes())
        .map_err(|e| CmdError::Unavailable(format!("could not save {}: {e}", path.display())))?;
    s.boards.saved = Some(board.clone());
    s.boards.document = Some(path.clone());
    remember(s, &path);
    let mut out = crate::storyboard::state(s);
    if let Value::Object(map) = &mut out {
        map.insert("path".into(), json!(path.to_string_lossy()));
        map.insert("bytes".into(), json!(json.len()));
        map.insert("message".into(), json!(format!("Saved {}", path.display())));
    }
    Ok(out)
}

/// Put `path` at the top of the recent list.
fn remember(s: &mut Session, path: &Path) {
    s.recent_storyboards.retain(|p| !crate::paths::same_path(p, path));
    s.recent_storyboards.insert(0, path.to_path_buf());
    s.recent_storyboards.truncate(MAX_RECENT);
}

/// `storyboard.open {path}` → the state + `{opened, missing: [path…], message}`.
pub(crate) fn sb_open(s: &mut Session, p: &Value) -> CmdResult {
    let path = p.get("path").map(path_param).unwrap_or_else(|| Err(CmdError::BadParams("need `path`".into())))?;
    let limits = Limits::default();
    let bytes = s
        .services
        .read_head(&path, limits.max_bytes + 1)
        .map_err(|e| CmdError::Unavailable(format!("could not open {}: {e}", path.display())))?;
    let board = Storyboard::from_json_with(&bytes, &limits).map_err(|e| {
        let why = match e {
            BoardError::NewerVersion(v) => {
                format!("it was saved by a newer AssetCraft (format {v}, this one reads {FORMAT_VERSION})")
            }
            BoardError::TooLarge(_) => format!("it is larger than {} MiB", limits.max_bytes >> 20),
            other => other.to_string(),
        };
        CmdError::Unavailable(format!("could not open {}: {why}", path.display()))
    })?;
    // Assets the file names that are not where it says (the board keeps them; the Viewer and
    // the export show those clips empty).
    let mut missing: Vec<String> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let names = board.clips.iter().flat_map(|c| c.asset_ids.iter()).chain(board.track.iter().map(|t| &t.id));
    for id in names {
        let asset = Path::new(id.0.as_str());
        if !asset.is_absolute() || !seen.insert(id.0.clone()) {
            continue;
        }
        if s.services.entry(asset).is_err() {
            missing.push(id.0.clone());
        }
    }
    s.boards.checkpoint();
    s.boards.selected = board.clips.first().map(|c| c.id);
    s.boards.generated = None;
    s.boards.board = Some(board);
    s.boards.mode = crate::Mode::Storyboard;
    s.boards.view = crate::storyboard::BoardView::Strip;
    s.boards.transport = crate::storyboard::Transport::default();
    mark_saved(s, Some(path.clone()));
    remember(s, &path);
    s.audio_stop();
    let message = match missing.len() {
        0 => format!("Opened {}", path.display()),
        1 => format!("Opened {} · 1 asset is missing", path.display()),
        n => format!("Opened {} · {n} assets are missing", path.display()),
    };
    let mut out = crate::storyboard::state(s);
    if let Value::Object(map) = &mut out {
        map.insert("opened".into(), json!(path.to_string_lossy()));
        map.insert("missing".into(), json!(missing));
        map.insert("message".into(), json!(message));
    }
    Ok(out)
}

/// `storyboard.recent {}` → `{recent: [{path, name, exists}]}`.
pub(crate) fn sb_recent(s: &mut Session, _p: &Value) -> CmdResult {
    let recent: Vec<Value> = s
        .recent_storyboards
        .iter()
        .map(|p| {
            json!({
                "path": p.to_string_lossy(),
                "name": document_name(p),
                "exists": s.services.entry(p).is_ok(),
            })
        })
        .collect();
    Ok(json!({ "recent": recent }))
}

/// A document's display name: the file name without [`SUFFIX`].
pub fn document_name(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match name.len().checked_sub(SUFFIX.len()) {
        Some(cut) if name[cut..].eq_ignore_ascii_case(SUFFIX) => name[..cut].to_owned(),
        _ => name,
    }
}

/// Write `bytes` to `path` whole or not at all: a temp file in the same folder, flushed to disk,
/// then renamed over `path`. The folder is created when missing. A failure leaves `path` as it
/// was and removes the temp file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).ok_or_else(|| std::io::Error::other("no folder"))?;
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let written = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// What `storyboard.new` and (later) `storyboard.open` record: the board as it is on disk.
pub(crate) fn mark_saved(s: &mut Session, document: Option<PathBuf>) {
    s.boards.saved = s.boards.board.clone();
    s.boards.document = document;
}
