//! The AssetCraft working folder (W1; decision D-WF1, Paul 2026-10-25: "the users assetcraft
//! working folder... where the users songs, templates, and unique config files are saved
//! (projects, settings, edits etc.)"; "default to copy, and give the user a notification").
//!
//! - `app.workingFolder {path?: folder | null}`: read it (no `path`), set it, or go back to the
//!   default (`null`). The default is `Documents/AssetCraft` (the home folder when there is no
//!   Documents). It is created on first use, with `Music/` inside; `Presets/`, `Templates/` and
//!   `Projects/` follow when something is saved there.
//! - `storyboard.musicImport {paths?: [audio], dryRun?}`: **copy** songs into `<working>/Music/`
//!   (default: every song in the music folder that isn't already there). The originals are never
//!   touched; nothing in `Music/` is ever overwritten: a song already there (same name and size) is
//!   skipped, and a different file with the same name gets a free `name (2).ext`. Each copy is
//!   written to a temp file and published with a hard link, so it appears whole or not at all. With
//!   `dryRun` it reports the plan and writes nothing. Afterwards the music folder is `Music/`, so
//!   the Start screen lists the copies; the UI then analyses them (`storyboard.analyse`).
//!
//! `prefs.json` and the caches stay in the OS app folders (D-WF1 (c), recommended default), so a
//! broken or missing working folder can't stop the app from starting.

use crate::library_cmd::AUDIO_EXTENSIONS;
use crate::{CmdError, CmdResult, Session};
use assetcraft_fs::{EntryKind, KnownFolder, ListOptions};
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The working folder's name inside Documents.
pub const WORKING_NAME: &str = "AssetCraft";
/// Where imported songs go, inside the working folder.
pub const MUSIC_DIR: &str = "Music";
/// Largest song copied (the decoder's limit).
pub const MAX_IMPORT_BYTES: u64 = 512 << 20;
/// Most songs one import copies.
pub const MAX_IMPORT: usize = 500;

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

impl Session {
    /// The default working folder: `Documents/AssetCraft`, else `<home>/AssetCraft`.
    pub fn default_working_folder(&self) -> Option<PathBuf> {
        self.services
            .known_folder(KnownFolder::Documents)
            .or_else(|| self.services.home_dir())
            .map(|d| d.join(WORKING_NAME))
    }

    /// The working folder in use (as set, else the default).
    pub fn working_folder(&self) -> Option<PathBuf> {
        self.working_folder.clone().or_else(|| self.default_working_folder())
    }

    /// Where imported songs go: `<working folder>/Music`.
    pub fn working_music(&self) -> Option<PathBuf> {
        self.working_folder().map(|w| w.join(MUSIC_DIR))
    }
}

/// `app.workingFolder {path?: folder | null}` → `{workingFolder, default, music, custom}`.
pub(crate) fn working_folder(s: &mut Session, p: &Value) -> CmdResult {
    match p.get("path") {
        None => {}
        Some(Value::Null) => s.working_folder = None,
        Some(v) => {
            let path =
                v.as_str().map(str::trim).filter(|x| !x.is_empty() && x.len() <= 4096).map(PathBuf::from).ok_or_else(
                    || CmdError::BadParams("`path` must be a folder path, or null for the default".into()),
                )?;
            if !path.has_root() {
                return Err(CmdError::BadParams("`path` must be a full path".into()));
            }
            // It may not exist yet (created on first use), but it can't be a file.
            if s.services.entry(&path).is_ok_and(|e| e.kind != EntryKind::Dir) {
                return Err(CmdError::BadParams("`path` is a file, not a folder".into()));
            }
            s.working_folder = Some(path);
        }
    }
    let shown = |p: Option<PathBuf>| p.map(|p| p.to_string_lossy().into_owned());
    Ok(json!({
        "workingFolder": shown(s.working_folder()),
        "default": shown(s.default_working_folder()),
        "music": shown(s.working_music()),
        "custom": s.working_folder.is_some(),
    }))
}

/// What happens to one song.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Plan {
    /// Copy to this path.
    Copy(PathBuf),
    /// Already in `Music/` (same name and size) at this path.
    Present(PathBuf),
    /// Can't be imported, with why.
    Refuse(String),
}

/// A free name in `dir` for `name`: `name`, else `stem (2).ext`, `stem (3).ext`, … (`taken` holds
/// names already planned in this import).
fn free_name(s: &Session, dir: &Path, name: &str, taken: &[PathBuf]) -> Option<PathBuf> {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((a, b)) if !a.is_empty() => (a, format!(".{b}")),
        _ => (name, String::new()),
    };
    (1..1000)
        .map(|n| if n == 1 { name.to_string() } else { format!("{stem} ({n}){ext}") })
        .map(|n| dir.join(n))
        .find(|candidate| s.services.entry(candidate).is_err() && !taken.contains(candidate))
}

fn plan(s: &Session, music: &Path, sources: &[PathBuf]) -> Vec<(PathBuf, Plan)> {
    let mut taken: Vec<PathBuf> = Vec::new();
    sources
        .iter()
        .map(|src| {
            let step = (|| {
                let entry = s.services.entry(src).map_err(|e| e.to_string())?;
                if entry.kind != EntryKind::File {
                    return Err("not a file".to_string());
                }
                if !is_audio(src) {
                    return Err("not an audio file".to_string());
                }
                if entry.size > MAX_IMPORT_BYTES {
                    return Err(format!("over {} MiB", MAX_IMPORT_BYTES >> 20));
                }
                if src.parent() == Some(music) {
                    return Ok(Plan::Present(src.clone()));
                }
                let name = src.file_name().and_then(|n| n.to_str()).ok_or("the name isn't valid text")?;
                let same = music.join(name);
                if s.services.entry(&same).is_ok_and(|e| e.kind == EntryKind::File && e.size == entry.size) {
                    return Ok(Plan::Present(same));
                }
                let dest = free_name(s, music, name, &taken).ok_or("no free name in Music")?;
                taken.push(dest.clone());
                Ok(Plan::Copy(dest))
            })();
            (src.clone(), step.unwrap_or_else(Plan::Refuse))
        })
        .collect()
}

/// Copy `src` to `dest` without ever replacing a file: write a temp file beside `dest`, flush it,
/// then publish it with a hard link (fails if `dest` appeared meanwhile).
fn copy_new(src: &Path, dest: &Path) -> std::io::Result<()> {
    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dest.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut from = std::fs::File::open(src)?;
        let mut to = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        std::io::copy(&mut from, &mut to)?;
        to.flush()?;
        to.sync_all()?;
        drop(to);
        match std::fs::hard_link(&tmp, dest) {
            Ok(()) => std::fs::remove_file(&tmp),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "a file with that name appeared"))
            }
            // File systems without hard links: check, then rename.
            Err(_) if std::fs::symlink_metadata(dest).is_ok() => {
                Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "a file with that name appeared"))
            }
            Err(_) => std::fs::rename(&tmp, dest),
        }
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Short form of a folder for messages: `~/Documents/AssetCraft/Music` under home.
fn short(s: &Session, path: &Path) -> String {
    match s.services.home_dir().and_then(|h| path.strip_prefix(h).ok().map(Path::to_path_buf)) {
        Some(rel) => format!("~/{}", rel.display()),
        None => path.display().to_string(),
    }
}

/// `storyboard.musicImport {paths?: [audio], dryRun?: bool}` → `{music, dryRun, copied, present,
/// failed, items: [{from, to?, action: copy|present|failed, error?}], tracks: [path], message}`.
pub(crate) fn music_import(s: &mut Session, p: &Value) -> CmdResult {
    let music = s.working_music().ok_or_else(|| CmdError::Unavailable("no working folder (no home folder)".into()))?;
    let dry_run = match p.get("dryRun") {
        None | Some(Value::Null) => false,
        Some(v) => v.as_bool().ok_or_else(|| CmdError::BadParams("`dryRun` must be true or false".into()))?,
    };
    let sources: Vec<PathBuf> = match p.get("paths") {
        None | Some(Value::Null) => {
            let dir =
                s.music_folder.clone().ok_or_else(|| CmdError::BadParams("no music folder to import from".into()))?;
            let opts = ListOptions { include_hidden: false, max_entries: 4 * MAX_IMPORT };
            let listing = s.services.list_entries(&dir, &opts).map_err(CmdError::Fs)?;
            let mut v: Vec<PathBuf> = listing
                .entries
                .into_iter()
                .filter(|e| e.kind == EntryKind::File && is_audio(&e.path))
                .map(|e| e.path)
                .collect();
            v.sort();
            v
        }
        Some(Value::Array(a)) => {
            let mut out = Vec::new();
            for v in a {
                let path = v
                    .as_str()
                    .map(str::trim)
                    .filter(|x| !x.is_empty() && x.len() <= 4096)
                    .map(PathBuf::from)
                    .ok_or_else(|| CmdError::BadParams("`paths` must be audio file paths".into()))?;
                if !path.has_root() {
                    return Err(CmdError::BadParams("each path must be a full path".into()));
                }
                out.push(path);
            }
            out
        }
        Some(_) => return Err(CmdError::BadParams("`paths` must be a list of audio file paths".into())),
    };
    if sources.len() > MAX_IMPORT {
        return Err(CmdError::BadParams(format!("at most {MAX_IMPORT} songs at a time")));
    }
    let steps = plan(s, &music, &sources);
    let needs_copy = steps.iter().any(|(_, st)| matches!(st, Plan::Copy(_)));
    if !dry_run && needs_copy {
        std::fs::create_dir_all(&music)
            .map_err(|e| CmdError::Unavailable(format!("can't make {}: {e}", short(s, &music))))?;
    }
    let (mut copied, mut present, mut failed) = (0usize, 0usize, 0usize);
    let mut items = Vec::new();
    let mut tracks = Vec::new();
    for (src, step) in steps {
        let from = src.to_string_lossy().into_owned();
        match step {
            Plan::Copy(dest) => {
                let result = if dry_run { Ok(()) } else { copy_new(&src, &dest) };
                match result {
                    Ok(()) => {
                        copied += 1;
                        tracks.push(dest.to_string_lossy().into_owned());
                        items.push(json!({ "from": from, "to": dest.to_string_lossy(), "action": "copy" }));
                    }
                    Err(e) => {
                        failed += 1;
                        items.push(json!({ "from": from, "action": "failed", "error": e.to_string() }));
                    }
                }
            }
            Plan::Present(at) => {
                present += 1;
                tracks.push(at.to_string_lossy().into_owned());
                items.push(json!({ "from": from, "to": at.to_string_lossy(), "action": "present" }));
            }
            Plan::Refuse(e) => {
                failed += 1;
                items.push(json!({ "from": from, "action": "failed", "error": e }));
            }
        }
    }
    let to = short(s, &music);
    let plural = |n: usize| if n == 1 { "song" } else { "songs" };
    let mut message = if dry_run {
        format!("Would copy {copied} {} to {to}", plural(copied))
    } else {
        format!("Copied {copied} {} to {to}", plural(copied))
    };
    if present > 0 {
        message.push_str(&format!(" ({present} already there)"));
    }
    if failed > 0 {
        message.push_str(&format!(", {failed} couldn't be copied"));
    }
    // The Start screen now lists the working folder's music.
    if !dry_run && (copied + present) > 0 && s.services.entry(&music).is_ok() {
        s.music_folder = Some(music.clone());
    }
    Ok(json!({
        "music": music.to_string_lossy(),
        "dryRun": dry_run,
        "copied": copied,
        "present": present,
        "failed": failed,
        "items": items,
        "tracks": tracks,
        "message": message,
    }))
}
