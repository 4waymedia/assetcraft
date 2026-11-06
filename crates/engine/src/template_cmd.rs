//! Storyboard templates (ST1–ST3): the bundled three, the user's own under
//! `<working>/Templates/`, Save as Template, and New from Template.
//!
//! - `storyboard.templates {}` → `{templates: [{id, name, description, source: bundled|user,
//!   path?}]}` — the bundled three from `assetcraft_storyboard::template`, then the user's
//!   `*.storyboard.json` files in the Templates folder by name (not parsed until used).
//! - `storyboard.saveTemplate {name, overwrite?}` → `{path, message}` + state: the open board
//!   as a template (`Storyboard::as_template`: assets and track removed, `template: true`)
//!   written atomically to `<working>/Templates/<name>.storyboard.json`; an existing file only
//!   with `overwrite`. The open board itself is unchanged.
//! - `storyboard.new {template: <id | path>, name?}`: a fresh board from a template
//!   (`Storyboard::from_template`), named after it unless `name` is given; a path is read and
//!   validated like `storyboard.open`.

use crate::document_cmd::{SUFFIX, document_name, write_atomic};
use crate::{CmdError, CmdResult, Session};
use assetcraft_fs::{EntryKind, ListOptions};
use assetcraft_storyboard::{BUNDLED, Limits, Storyboard, bundled_template};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Where the user's templates live, inside the working folder.
pub const TEMPLATES_DIR: &str = "Templates";
/// Most user templates listed.
const MAX_USER_TEMPLATES: usize = 200;

impl Session {
    /// `<working folder>/Templates` (may not exist yet).
    pub fn templates_folder(&self) -> Option<PathBuf> {
        self.working_folder().map(|w| w.join(TEMPLATES_DIR))
    }

    /// The user's templates: `*.storyboard.json` under the Templates folder, by name.
    pub fn user_templates(&self) -> Vec<(String, PathBuf)> {
        let Some(dir) = self.templates_folder() else { return Vec::new() };
        let opts = ListOptions { include_hidden: false, max_entries: MAX_USER_TEMPLATES };
        let Ok(listing) = self.services.list_entries(&dir, &opts) else { return Vec::new() };
        let mut out: Vec<(String, PathBuf)> = listing
            .entries
            .into_iter()
            .filter(|e| e.kind == EntryKind::File && e.name.to_ascii_lowercase().ends_with(SUFFIX))
            .map(|e| (document_name(&e.path), e.path))
            .collect();
        out.sort_by_key(|t| t.0.to_lowercase());
        out
    }
}

/// `storyboard.templates {}`.
pub(crate) fn sb_templates(s: &mut Session, _p: &Value) -> CmdResult {
    let mut templates: Vec<Value> = BUNDLED
        .iter()
        .map(|t| json!({ "id": t.id, "name": t.name, "description": t.description, "source": "bundled" }))
        .collect();
    for (name, path) in s.user_templates() {
        templates.push(
            json!({ "id": path.to_string_lossy(), "name": name, "source": "user", "path": path.to_string_lossy() }),
        );
    }
    Ok(json!({ "templates": templates, "folder": s.templates_folder().map(|p| p.to_string_lossy().into_owned()) }))
}

/// A board from a bundled id or a template file.
pub(crate) fn board_from_template(s: &Session, which: &str) -> Result<Storyboard, CmdError> {
    let which = which.trim();
    if let Some(bundled) = bundled_template(which) {
        return Ok(Storyboard::from_template(&bundled, bundled.name.as_str()));
    }
    let path = Path::new(which);
    if !path.is_absolute() || !which.to_ascii_lowercase().ends_with(SUFFIX) {
        return Err(CmdError::BadParams(format!(
            "`template` must be one of {} or an absolute `{SUFFIX}` path",
            BUNDLED.iter().map(|t| t.id).collect::<Vec<_>>().join(", ")
        )));
    }
    let limits = Limits::default();
    let bytes = s
        .services
        .read_head(path, limits.max_bytes + 1)
        .map_err(|e| CmdError::Unavailable(format!("could not open template {}: {e}", path.display())))?;
    let template = Storyboard::from_json_with(&bytes, &limits)
        .map_err(|e| CmdError::Unavailable(format!("could not open template {}: {e}", path.display())))?;
    let name = if template.template { template.name.clone() } else { document_name(path) };
    Ok(Storyboard::from_template(&template, &name))
}

/// `storyboard.saveTemplate {name, overwrite?}`.
pub(crate) fn sb_save_template(s: &mut Session, p: &Value) -> CmdResult {
    let board = s.boards.board.as_ref().ok_or_else(crate::storyboard::no_board)?;
    let name = p
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty() && n.len() <= 200)
        .ok_or_else(|| CmdError::BadParams("need `name` (1–200 bytes)".into()))?;
    let clean: String =
        name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '-' } else { c }).collect();
    let clean = clean.trim_matches(['.', ' ', '-']).to_owned();
    if clean.is_empty() {
        return Err(CmdError::BadParams("`name` needs a letter or a digit".into()));
    }
    let dir = s.templates_folder().ok_or_else(|| CmdError::Unavailable("no working folder for templates".into()))?;
    let path = dir.join(format!("{clean}{SUFFIX}"));
    let overwrite = p.get("overwrite").and_then(Value::as_bool).unwrap_or(false);
    if !overwrite && std::fs::symlink_metadata(&path).is_ok() {
        return Err(CmdError::Unavailable(format!(
            "a template called {clean} exists; pass `overwrite: true` to replace it"
        )));
    }
    let template = board.as_template(name);
    let json = template.to_json().map_err(|e| CmdError::Unavailable(format!("could not encode the template: {e}")))?;
    write_atomic(&path, json.as_bytes())
        .map_err(|e| CmdError::Unavailable(format!("could not save {}: {e}", path.display())))?;
    let mut out = crate::storyboard::state(s);
    if let Value::Object(map) = &mut out {
        map.insert("path".into(), json!(path.to_string_lossy()));
        map.insert("message".into(), json!(format!("Saved template {clean}")));
    }
    Ok(out)
}
