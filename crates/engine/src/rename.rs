//! Batch rename (Tools → Batch Rename…): a file-name template applied to the selected files.
//!
//! T1 (this file today) plans only: [`plan`] and the `file.renamePreview` command say what each
//! file would be called, and change nothing. T2 adds `file.rename` (the moves, their sidecars and
//! undo).
//!
//! Template tokens, adapted from LightCraft `crates/engine/src/rename.rs` (storytold/lightcraft,
//! rev 16dbb90, MIT OR Apache-2.0), keeping its spelling so templates work in both apps:
//! `{name}` (current name without extension), `{num}` (the digits at the end of the name:
//! `IMG_0042` → `0042`), `{seq}` / `{seq:3}` (sequence number, zero-padded to 3 digits), `{folder}`
//! (the file's folder name), `{ext}` (extension, without the dot). Unknown tokens stay as typed
//! and are reported. Characters not allowed in file names become `-`. The original extension is
//! always kept.
//!
//! Safety, as in LightCraft: a target that exists on disk, or that an earlier file in the batch
//! gets, receives a `-1`, `-2`… suffix, so no file is ever overwritten. A target that differs only
//! in letter case from the file's own name is the file itself on Windows and macOS (case-insensitive
//! volumes), and is allowed.

use crate::{CmdError, CmdResult, Session};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Most files one rename may cover (a folder listing is capped at `MAX_ITEMS` anyway).
pub const MAX_RENAME_FILES: usize = 10_000;
/// Longest template accepted (characters).
pub const MAX_TEMPLATE_CHARS: usize = 512;
/// Longest file name produced (characters, extension included); most file systems allow 255 bytes.
pub const MAX_NAME_CHARS: usize = 200;
/// `-1` … `-N` suffixes tried before a name counts as unavailable.
const MAX_SUFFIX: usize = 9_999;

/// One template token, for help texts and the dialog's token buttons. A test checks that
/// [`expand_tokens`] knows each of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenHelp {
    /// The token as typed, e.g. `{seq:3}`.
    pub tag: &'static str,
    /// What it stands for.
    pub meaning: &'static str,
}

/// Every template token, in the order the help shows them.
pub const TOKENS: &[TokenHelp] = &[
    TokenHelp { tag: "{name}", meaning: "Current file name without its extension" },
    TokenHelp { tag: "{num}", meaning: "The digits at the end of the current name (IMG_0042 → 0042)" },
    TokenHelp { tag: "{seq}", meaning: "Sequence number, counting from the start number" },
    TokenHelp { tag: "{seq:3}", meaning: "Sequence number zero-padded to N digits (1–9)" },
    TokenHelp { tag: "{folder}", meaning: "Name of the folder the file is in" },
    TokenHelp { tag: "{ext}", meaning: "Current extension without the dot" },
];

/// One planned rename.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenamePlan {
    /// The file now.
    pub from: PathBuf,
    /// Its new path (same folder).
    pub to: PathBuf,
    /// A `-N` suffix was added because the name was taken.
    pub suffixed: bool,
}

impl RenamePlan {
    /// `true` when the name actually changes (letter case counts).
    pub fn changes(&self) -> bool {
        self.from != self.to
    }
}

/// `stem`, `ext` (without the dot). A leading dot alone (`.hidden`) is a stem, not an extension.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, ext),
        _ => (name, ""),
    }
}

/// Characters no file system takes (Windows' set, the strictest) become `-`; spaces at either
/// end and trailing dots go (Windows drops those silently). A leading dot stays (`.hidden`).
fn sanitize(s: &str) -> String {
    let t: String =
        s.chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                    '-'
                } else {
                    c
                }
            })
            .collect();
    t.trim().trim_end_matches(['.', ' ']).to_string()
}

/// Windows reserves these device names, with any extension.
fn reserved_on_windows(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.len() == 4
            && upper.as_bytes().get(3).is_some_and(|b| (b'1'..=b'9').contains(b)))
}

/// The template's tokens replaced for the file `name` in folder `folder` (no extension added,
/// nothing sanitized). A bare `{seq}` is not padded.
pub fn expand_tokens(template: &str, name: &str, folder: &str, seq: usize) -> String {
    let (stem, ext) = split_ext(name);
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(rest.get(..i).unwrap_or_default());
        let after = rest.get(i..).unwrap_or_default();
        let Some(j) = after.find('}') else {
            out.push_str(after);
            rest = "";
            break;
        };
        let tok = after.get(1..j).unwrap_or_default();
        let (key, arg) = tok.split_once(':').map_or((tok, None), |(a, b)| (a, Some(b)));
        let value = match key.trim().to_ascii_lowercase().as_str() {
            "name" | "filename" => Some(stem.to_string()),
            "num" => {
                let digits = stem.chars().rev().take_while(char::is_ascii_digit).count();
                // ASCII digits are one byte each, so this is a char boundary.
                Some(stem.get(stem.len().saturating_sub(digits)..).unwrap_or_default().to_string())
            }
            "seq" | "n" => match arg {
                None => Some(seq.to_string()),
                Some(a) => a.trim().parse::<usize>().ok().filter(|w| (1..=9).contains(w)).map(|w| format!("{seq:0w$}")),
            },
            "folder" => Some(folder.to_string()),
            "ext" => Some(ext.to_string()),
            _ => None,
        };
        match value {
            Some(v) => out.push_str(&v),
            None => out.push_str(after.get(..=j).unwrap_or_default()),
        }
        rest = after.get(j + 1..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

/// The `{…}` tags in `template` that aren't tokens (they stay as typed), for a warning.
pub fn unknown_tokens(template: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        let after = rest.get(i..).unwrap_or_default();
        let Some(j) = after.find('}') else { break };
        let tag = after.get(..=j).unwrap_or_default();
        if expand_tokens(tag, "IMG_0042.jpg", "folder", 1) == tag && !out.iter().any(|t| t == tag) {
            out.push(tag.to_string());
        }
        rest = after.get(j + 1..).unwrap_or_default();
    }
    out
}

/// The new file name for `name` (in `folder`, sequence number `seq`): the template expanded,
/// sanitized and capped, plus the original extension. An empty result keeps the original stem.
pub fn new_name(template: &str, name: &str, folder: &str, seq: usize) -> String {
    let (stem, ext) = split_ext(name);
    let mut out = sanitize(&expand_tokens(template, name, folder, seq));
    if out.is_empty() {
        out = stem.to_string();
    }
    if reserved_on_windows(&out) {
        out.push('_');
    }
    let room = MAX_NAME_CHARS.saturating_sub(ext.chars().count() + 1).max(1);
    if out.chars().count() > room {
        out = out.chars().take(room).collect::<String>().trim_end_matches(['.', ' ']).to_string();
    }
    if ext.is_empty() { out } else { format!("{out}.{ext}") }
}

/// A path folded for "is this name taken?" checks: Windows and macOS volumes are usually
/// case-insensitive, so `IMG.jpg` and `img.JPG` collide there. Folding everywhere is the safe side.
fn folded(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// Plan renaming `files` (full paths) with `template`, numbering from `start`. `exists` says
/// whether a path is taken on disk. Files keep their folder.
pub fn plan(
    files: &[PathBuf],
    template: &str,
    start: usize,
    exists: &dyn Fn(&Path) -> bool,
) -> Result<Vec<RenamePlan>, CmdError> {
    if files.len() > MAX_RENAME_FILES {
        return Err(CmdError::Unavailable(format!("rename at most {MAX_RENAME_FILES} files at once")));
    }
    // Claimed in this batch. A name another file in the batch has now still counts as taken (as in
    // LightCraft): simple and safe, at the cost of a `-1` when names swap.
    let mut taken: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(files.len());
    for (seq, from) in (start..).zip(files) {
        let name = from
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| CmdError::BadParams(format!("not a file path: {}", from.to_string_lossy())))?;
        let dir = from.parent().map(Path::to_path_buf).unwrap_or_default();
        let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let want = new_name(template, &name, &folder, seq);
        let (stem, ext) = split_ext(&want);
        let mut found = None;
        for k in 0..=MAX_SUFFIX {
            let candidate = match (k, ext.is_empty()) {
                (0, _) => want.clone(),
                (_, true) => format!("{stem}-{k}"),
                (_, false) => format!("{stem}-{k}.{ext}"),
            };
            let to = dir.join(&candidate);
            let key = folded(&to);
            // The file itself, maybe with its letter case changed.
            let itself = key == folded(from);
            if !taken.contains(&key) && (itself || !exists(&to)) {
                taken.insert(key);
                found = Some(RenamePlan { from: from.clone(), to, suffixed: k > 0 });
                break;
            }
        }
        let plan = found.ok_or_else(|| {
            CmdError::Unavailable(format!("no free name for {name} (tried {want}-1 … -{MAX_SUFFIX})"))
        })?;
        out.push(plan);
    }
    Ok(out)
}

/// `file.renamePreview {template, start?, paths?}`: what Batch Rename would do. Changes nothing.
pub(crate) fn rename_preview(s: &mut Session, p: &Value) -> CmdResult {
    let template = p
        .get("template")
        .and_then(Value::as_str)
        .ok_or_else(|| CmdError::BadParams("need `template` (string)".into()))?;
    if template.chars().count() > MAX_TEMPLATE_CHARS {
        return Err(CmdError::BadParams(format!("`template` is longer than {MAX_TEMPLATE_CHARS} characters")));
    }
    let start = match p.get("start") {
        None => 1,
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n <= 1_000_000_000)
            .ok_or_else(|| CmdError::BadParams("`start` must be a whole number 0..1000000000".into()))?,
    };
    let paths = crate::commands::paths_param(s, p)?;
    let mut files = Vec::with_capacity(paths.len());
    let mut skipped = Vec::new();
    for path in paths {
        if !path.has_root() {
            return Err(CmdError::BadParams(format!("not a full path: {}", path.to_string_lossy())));
        }
        match s.services.entry(&path) {
            Ok(e) if e.kind == assetcraft_fs::EntryKind::File => files.push(path),
            _ => skipped.push(path.to_string_lossy().into_owned()),
        }
    }
    if files.is_empty() {
        return Err(CmdError::Unavailable("no files selected to rename (folders aren't renamed)".into()));
    }
    let services = s.services.clone();
    let plans = plan(&files, template, start, &|path| services.entry(path).is_ok())?;
    let changed = plans.iter().filter(|r| r.changes()).count();
    let suffixed = plans.iter().filter(|r| r.suffixed).count();
    Ok(json!({
        "template": template,
        "start": start,
        "plans": plans.iter().map(|r| json!({
            "from": r.from.to_string_lossy(),
            "to": r.to.to_string_lossy(),
            "name": r.from.file_name().map(|n| n.to_string_lossy()),
            "newName": r.to.file_name().map(|n| n.to_string_lossy()),
            "changes": r.changes(),
            "suffixed": r.suffixed,
        })).collect::<Vec<_>>(),
        "count": plans.len(),
        "changed": changed,
        "suffixed": suffixed,
        "skipped": skipped,
        "unknownTokens": unknown_tokens(template),
    }))
}
