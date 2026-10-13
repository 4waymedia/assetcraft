//! The command registry. Each command has a stable id, a label and menu for the UI, an optional
//! shortcut, a one-line parameter description, and a handler.

use crate::prefs::{THUMB_SIZE_RANGE, ViewMode};
use crate::select::ClickMode;
use crate::sort::SortKey;
use crate::{CmdError, CmdResult, Location, Session};
use assetcraft_fs::{EntryKind, KnownFolder};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// One command.
pub struct CommandSpec {
    /// Stable id, e.g. `"nav.back"`.
    pub id: &'static str,
    /// Menu label.
    pub label: &'static str,
    /// Menu it lives in.
    pub menu: &'static str,
    /// Default shortcut, as shown in menus.
    pub shortcut: Option<&'static str>,
    /// Parameters, in a compact human-readable form.
    pub params: &'static str,
    /// Handler.
    pub run: fn(&mut Session, &Value) -> CmdResult,
}

impl std::fmt::Debug for CommandSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandSpec").field("id", &self.id).finish_non_exhaustive()
    }
}

macro_rules! cmd {
    ($id:literal, $label:literal, $menu:literal, $shortcut:expr, $params:literal, $run:expr) => {
        CommandSpec { id: $id, label: $label, menu: $menu, shortcut: $shortcut, params: $params, run: $run }
    };
}

static COMMANDS: &[CommandSpec] = &[
    // Navigate
    cmd!("nav.open", "Open Folder", "Go", None, "{path}", nav_open),
    cmd!("nav.computer", "Computer", "Go", None, "{}", |s, _| s.go(Location::Computer)),
    cmd!(
        "nav.known",
        "Go to Known Folder",
        "Go",
        None,
        "{folder: home|desktop|documents|downloads|pictures|videos}",
        nav_known
    ),
    cmd!("nav.back", "Back", "Go", Some("Alt+Left"), "{}", nav_back),
    cmd!("nav.forward", "Forward", "Go", Some("Alt+Right"), "{}", nav_forward),
    cmd!("nav.up", "Up", "Go", Some("Alt+Up"), "{}", nav_up),
    cmd!("nav.refresh", "Refresh", "View", Some("F5"), "{}", |s, _| s.refresh()),
    cmd!("nav.favorite.add", "Add to Favorites", "Go", None, "{path?} (default: current folder)", fav_add),
    cmd!("nav.favorite.remove", "Remove from Favorites", "Go", None, "{path}", fav_remove),
    // View
    cmd!("view.mode", "View As", "View", None, "{mode: grid|list}", view_mode),
    cmd!("view.thumbSize", "Thumbnail Size", "View", None, "{px: 48..512}", view_thumb_size),
    cmd!("view.sort", "Sort By", "View", None, "{by: name|date|size|type, ascending?: bool}", view_sort),
    cmd!("view.hidden", "Show Hidden Files", "View", Some("Ctrl+H"), "{show: bool}", view_hidden),
    // Select
    cmd!("select.click", "Click Item", "Edit", None, "{path, mode?: replace|toggle|range}", select_click),
    cmd!("select.set", "Select Items", "Edit", None, "{paths: [path]}", select_set),
    cmd!("select.all", "Select All", "Edit", Some("Ctrl+A"), "{}", |s, _| {
        s.selection.all(&s.item_paths);
        Ok(selection_json(s))
    }),
    cmd!("select.none", "Deselect All", "Edit", Some("Ctrl+Shift+A"), "{}", |s, _| {
        s.selection.clear();
        Ok(selection_json(s))
    }),
    cmd!("select.move", "Move Selection", "Edit", Some("Arrows"), "{delta: int, extend?: bool}", select_move),
    // File
    cmd!("file.open", "Open", "File", Some("Enter"), "{path?} (default: focused item); folders navigate", file_open),
    cmd!("file.reveal", "Reveal in Explorer", "File", None, "{path?} (default: focused item)", file_reveal),
    cmd!("item.probe", "File Info", "File", None, "{path}", item_probe),
    // App
    cmd!("app.state", "State", "Help", None, "{}", |s, _| Ok(state_json(s))),
    cmd!("app.commands", "Commands", "Help", None, "{}", |_, _| Ok(commands_json())),
];

/// Every command, in menu order.
pub fn commands() -> &'static [CommandSpec] {
    COMMANDS
}

/// Look a command up by id.
pub fn find(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|c| c.id == id)
}

// ---- parameter helpers ------------------------------------------------------------------

fn str_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

fn need_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, CmdError> {
    str_param(params, key).ok_or_else(|| CmdError::BadParams(format!("need `{key}` (string)")))
}

fn need_bool(params: &Value, key: &str) -> Result<bool, CmdError> {
    params.get(key).and_then(Value::as_bool).ok_or_else(|| CmdError::BadParams(format!("need `{key}` (true/false)")))
}

/// `path` param, or the focused item, or (for some commands) the current folder.
fn target_path(s: &Session, params: &Value, fallback_to_folder: bool) -> Result<PathBuf, CmdError> {
    if let Some(p) = str_param(params, "path") {
        return Ok(PathBuf::from(p));
    }
    if let Some(f) = s.selection.focus() {
        return Ok(f.to_path_buf());
    }
    match (fallback_to_folder, s.location.path()) {
        (true, Some(p)) => Ok(p.to_path_buf()),
        _ => Err(CmdError::BadParams("need `path`, or a focused item".into())),
    }
}

// ---- handlers ----------------------------------------------------------------------------

fn nav_open(s: &mut Session, p: &Value) -> CmdResult {
    let path = PathBuf::from(need_str(p, "path")?);
    s.go(Location::Folder(path))
}

fn nav_known(s: &mut Session, p: &Value) -> CmdResult {
    let which = match need_str(p, "folder")? {
        "home" => KnownFolder::Home,
        "desktop" => KnownFolder::Desktop,
        "documents" => KnownFolder::Documents,
        "downloads" => KnownFolder::Downloads,
        "pictures" => KnownFolder::Pictures,
        "videos" => KnownFolder::Videos,
        other => return Err(CmdError::BadParams(format!("unknown folder `{other}`"))),
    };
    let path =
        s.services.known_folder(which).ok_or_else(|| CmdError::Unavailable(format!("no {} folder", which.label())))?;
    s.go(Location::Folder(path))
}

fn nav_back(s: &mut Session, _: &Value) -> CmdResult {
    let target = s.back.last().cloned().ok_or_else(|| CmdError::Unavailable("nothing to go back to".into()))?;
    s.load(&target)?;
    s.back.pop();
    let current = std::mem::replace(&mut s.location, target);
    focus_child(s, &current);
    s.forward.push(current);
    Ok(s.location_json())
}

fn nav_forward(s: &mut Session, _: &Value) -> CmdResult {
    let target = s.forward.last().cloned().ok_or_else(|| CmdError::Unavailable("nothing to go forward to".into()))?;
    s.load(&target)?;
    s.forward.pop();
    let current = std::mem::replace(&mut s.location, target);
    focus_child(s, &current);
    s.back.push(current);
    Ok(s.location_json())
}

fn nav_up(s: &mut Session, _: &Value) -> CmdResult {
    let target = match s.location.path() {
        None => return Err(CmdError::Unavailable("already at Computer".into())),
        Some(path) => path.parent().map_or(Location::Computer, |p| Location::Folder(p.to_path_buf())),
    };
    let child = s.location.clone();
    let result = s.go(target)?;
    focus_child(s, &child);
    Ok(result)
}

/// After moving to a parent (up, or back out of a sub-folder), focus the folder we came from,
/// like a file manager does. No-op if it isn't in the new listing.
fn focus_child(s: &mut Session, came_from: &Location) {
    if let Some(path) = came_from.path().filter(|p| s.item_paths.iter().any(|i| i == p)) {
        s.selection.click(path, ClickMode::Replace, &s.item_paths);
    }
}

fn fav_add(s: &mut Session, p: &Value) -> CmdResult {
    let path = match str_param(p, "path") {
        Some(p) => PathBuf::from(p),
        None => s.location.path().map(Path::to_path_buf).ok_or_else(|| CmdError::BadParams("need `path`".into()))?,
    };
    let entry = s.services.entry(&path)?;
    if entry.kind != EntryKind::Dir {
        return Err(CmdError::BadParams("favorites are folders".into()));
    }
    if !s.favorites.contains(&path) {
        s.favorites.push(path);
    }
    Ok(json!({ "favorites": s.favorites }))
}

fn fav_remove(s: &mut Session, p: &Value) -> CmdResult {
    let path = PathBuf::from(need_str(p, "path")?);
    s.favorites.retain(|f| *f != path);
    Ok(json!({ "favorites": s.favorites }))
}

fn view_mode(s: &mut Session, p: &Value) -> CmdResult {
    s.view.mode = match need_str(p, "mode")? {
        "grid" => ViewMode::Grid,
        "list" => ViewMode::List,
        other => return Err(CmdError::BadParams(format!("unknown mode `{other}`"))),
    };
    Ok(json!({ "view": s.view }))
}

fn view_thumb_size(s: &mut Session, p: &Value) -> CmdResult {
    let px = p.get("px").and_then(Value::as_u64).ok_or_else(|| CmdError::BadParams("need `px` (number)".into()))?;
    s.view.thumb_size = u32::try_from(px).unwrap_or(u32::MAX).clamp(THUMB_SIZE_RANGE.0, THUMB_SIZE_RANGE.1);
    Ok(json!({ "view": s.view }))
}

fn view_sort(s: &mut Session, p: &Value) -> CmdResult {
    let by = need_str(p, "by")?;
    s.view.sort = SortKey::parse(by).ok_or_else(|| CmdError::BadParams(format!("unknown sort `{by}`")))?;
    if let Some(asc) = p.get("ascending").and_then(Value::as_bool) {
        s.view.ascending = asc;
    }
    s.resort();
    Ok(json!({ "view": s.view }))
}

fn view_hidden(s: &mut Session, p: &Value) -> CmdResult {
    s.view.show_hidden = need_bool(p, "show")?;
    s.refresh()
}

fn select_click(s: &mut Session, p: &Value) -> CmdResult {
    let path = PathBuf::from(need_str(p, "path")?);
    let mode = match str_param(p, "mode").unwrap_or("replace") {
        "replace" => ClickMode::Replace,
        "toggle" => ClickMode::Toggle,
        "range" => ClickMode::Range,
        other => return Err(CmdError::BadParams(format!("unknown mode `{other}`"))),
    };
    s.selection.click(&path, mode, &s.item_paths);
    Ok(selection_json(s))
}

fn select_set(s: &mut Session, p: &Value) -> CmdResult {
    let paths: Vec<PathBuf> = p
        .get("paths")
        .and_then(Value::as_array)
        .ok_or_else(|| CmdError::BadParams("need `paths` (list)".into()))?
        .iter()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .collect();
    s.selection.set(&paths, &s.item_paths);
    Ok(selection_json(s))
}

fn select_move(s: &mut Session, p: &Value) -> CmdResult {
    let delta =
        p.get("delta").and_then(Value::as_i64).ok_or_else(|| CmdError::BadParams("need `delta` (integer)".into()))?;
    let extend = p.get("extend").and_then(Value::as_bool).unwrap_or(false);
    s.selection.move_focus(delta, extend, &s.item_paths);
    Ok(selection_json(s))
}

fn file_open(s: &mut Session, p: &Value) -> CmdResult {
    let path = target_path(s, p, false)?;
    let entry = s.services.entry(&path)?;
    if entry.kind == EntryKind::Dir {
        return s.go(Location::Folder(path));
    }
    s.launcher.open(&path).map_err(CmdError::Unavailable)?;
    Ok(json!({ "opened": path }))
}

fn file_reveal(s: &mut Session, p: &Value) -> CmdResult {
    let path = target_path(s, p, true)?;
    s.launcher.reveal(&path).map_err(CmdError::Unavailable)?;
    Ok(json!({ "revealed": path }))
}

fn item_probe(s: &mut Session, p: &Value) -> CmdResult {
    let path = PathBuf::from(need_str(p, "path")?);
    let probe = s.probe(&path)?;
    Ok(json!({
        "path": probe.path,
        "name": probe.name,
        "format": probe.format.label(),
        "category": format!("{:?}", probe.category()),
        "width": probe.dimensions.map(|d| d.width),
        "height": probe.dimensions.map(|d| d.height),
        "size": probe.size,
        "modified": probe.modified.and_then(unix_secs),
        "status": format!("{:?}", probe.status),
        "unreadable": probe.is_unreadable(),
        "extension_mismatch": probe.extension_mismatch(),
    }))
}

// ---- JSON views --------------------------------------------------------------------------

fn unix_secs(t: std::time::SystemTime) -> Option<u64> {
    t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

fn selection_json(s: &Session) -> Value {
    json!({ "selected": s.selected_paths(), "focus": s.selection.focus() })
}

pub(crate) fn state_json(s: &Session) -> Value {
    json!({
        "location": s.location.path(),
        "label": s.location.label(),
        "can_back": s.can_back(),
        "can_forward": s.can_forward(),
        "can_up": s.can_up(),
        "truncated": s.truncated,
        "skipped": s.skipped,
        "view": s.view,
        "favorites": s.favorites,
        "selected": s.selected_paths(),
        "focus": s.selection.focus(),
        "items": s.items.iter().map(|e| json!({
            "name": e.name,
            "path": e.path,
            "kind": format!("{:?}", e.kind),
            "size": e.size,
            "modified": e.modified.and_then(unix_secs),
        })).collect::<Vec<_>>(),
    })
}

fn commands_json() -> Value {
    Value::Array(
        COMMANDS
            .iter()
            .map(
                |c| json!({ "id": c.id, "label": c.label, "menu": c.menu, "shortcut": c.shortcut, "params": c.params }),
            )
            .collect(),
    )
}
