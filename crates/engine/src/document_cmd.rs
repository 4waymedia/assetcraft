//! The storyboard as a file: Save and Save As (SF1; plan step 7, decision D3: AssetCraft's own
//! files only, written atomically, never overwriting a different file without being told).
//!
//! - `storyboard.save {path?, overwrite?}`: write the open storyboard to its own document (no
//!   `path`), or to `path` (Save As). A storyboard that has never been saved needs a `path`.
//! - `storyboard.saveAs {path, overwrite?}`: the same with `path` required.
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
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

/// What a storyboard document's name ends with.
pub const SUFFIX: &str = ".storyboard.json";
/// Where the UI offers to save, inside the working folder.
pub const STORYBOARDS_DIR: &str = "Storyboards";
/// Longest path accepted.
const MAX_PATH: usize = 4096;

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
    let mut out = crate::storyboard::state(s);
    if let Value::Object(map) = &mut out {
        map.insert("path".into(), json!(path.to_string_lossy()));
        map.insert("bytes".into(), json!(json.len()));
        map.insert("message".into(), json!(format!("Saved {}", path.display())));
    }
    Ok(out)
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
