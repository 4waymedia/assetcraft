//! The command registry. Each command has a stable id, its effects, a label and menu for the
//! UI, an optional shortcut, a one-line parameter description, and a handler.

use crate::effect::Effects;
use crate::prefs::{THUMB_SIZE_RANGE, ThemeMode, ViewMode, panels_json_ok};
use crate::select::ClickMode;
use crate::sort::SortKey;
use crate::storyboard;
use crate::{CmdError, CmdResult, Location, Session, clean_user_path, same_path};
use assetcraft_fs::{EntryKind, KnownFolder};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// One command.
pub struct CommandSpec {
    /// Stable id, e.g. `"nav.back"`.
    pub id: &'static str,
    /// What it touches (suite command effects, PipelineCraft spec §6.4); remote doors and MCP
    /// hints read it.
    pub effects: Effects,
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
        f.debug_struct("CommandSpec")
            .field("id", &self.id)
            .field("effects", &self.effects.classes)
            .finish_non_exhaustive()
    }
}

macro_rules! cmd {
    ($effects:ident, $id:literal, $label:literal, $menu:literal, $shortcut:expr, $params:literal, $run:expr) => {
        CommandSpec {
            id: $id,
            effects: Effects::$effects,
            label: $label,
            menu: $menu,
            shortcut: $shortcut,
            params: $params,
            run: $run,
        }
    };
}

static COMMANDS: &[CommandSpec] = &[
    // Navigate
    cmd!(
        APP,
        "nav.open",
        "Open Folder",
        "Go",
        None,
        "{path} (quotes trimmed; made absolute; `C:` means `C:\\`)",
        nav_open
    ),
    cmd!(APP, "nav.computer", "Computer", "Go", None, "{} (also rescans drives and known folders)", |s, _| {
        s.rescan_places();
        s.go(Location::Computer)
    }),
    cmd!(
        APP,
        "nav.known",
        "Go to Known Folder",
        "Go",
        None,
        "{folder: home|desktop|documents|downloads|pictures|videos}",
        nav_known
    ),
    cmd!(APP_STEP, "nav.back", "Back", "Go", Some("Alt+Left"), "{}", nav_back),
    cmd!(APP_STEP, "nav.forward", "Forward", "Go", Some("Alt+Right"), "{}", nav_forward),
    cmd!(APP_STEP, "nav.up", "Up", "Go", Some("Alt+Up"), "{}", nav_up),
    cmd!(APP, "nav.refresh", "Refresh", "View", Some("F5"), "{} (also rescans drives and known folders)", |s, _| s
        .refresh()),
    cmd!(
        APP,
        "nav.changed",
        "Apply Folder Changes",
        "View",
        None,
        "{paths: [path]} | {rescan: true} (from the folder watcher; keeps selection)",
        |s, p| s.apply_changes(p)
    ),
    cmd!(APP, "nav.favorite.add", "Add to Favorites", "File", None, "{path?} (default: current folder)", fav_add),
    cmd!(APP, "nav.favorite.remove", "Remove from Favorites", "Go", None, "{path}", fav_remove),
    // View
    cmd!(APP, "view.mode", "View As", "View", None, "{mode: grid|list}", view_mode),
    cmd!(APP, "view.thumbSize", "Thumbnail Size", "View", None, "{px: 48..512}", view_thumb_size),
    cmd!(APP, "view.sort", "Sort By", "View", None, "{by: name|date|size|type, ascending?: bool}", view_sort),
    cmd!(APP, "view.hidden", "Show Hidden Files", "View", Some("Ctrl+H"), "{show: bool}", view_hidden),
    cmd!(APP, "view.theme", "Theme", "View", None, "{mode: system|dark|light}", view_theme),
    cmd!(
        APP,
        "view.model",
        "3D Preview",
        "View",
        None,
        "{turntable?: bool, mode?: shaded|wireframe|normals|uv, background?: panel|dark|grey|light, exposure?: 25..400 (percent)}",
        view_model
    ),
    cmd!(
        APP,
        "formats.heic",
        "HEIC / HEIF thumbnails (HEVC)",
        "View",
        None,
        "{enabled: bool} (off by default; needs a build with --features heic)",
        formats_heic
    ),
    cmd!(
        APP_STEP,
        "view.preview",
        "Full-Window Preview",
        "View",
        Some("Space"),
        "{open?: bool} (default: toggle)",
        view_preview
    ),
    cmd!(
        APP,
        "view.panels",
        "Panel Layout",
        "View",
        None,
        "{layout: object | null} (UI-owned, saved in prefs)",
        view_panels
    ),
    // Select
    cmd!(APP_STEP, "select.click", "Click Item", "Edit", None, "{path, mode?: replace|toggle|range}", select_click),
    cmd!(APP, "select.set", "Select Items", "Edit", None, "{paths: [path]}", select_set),
    cmd!(APP, "select.all", "Select All", "Edit", Some("Ctrl+A"), "{}", |s, _| {
        s.selection.all(&s.item_paths);
        Ok(selection_json(s))
    }),
    cmd!(APP, "select.none", "Deselect All", "Edit", Some("Ctrl+Shift+A"), "{}", |s, _| {
        s.selection.clear();
        Ok(selection_json(s))
    }),
    cmd!(APP_STEP, "select.move", "Move Selection", "Edit", Some("Arrows"), "{delta: int, extend?: bool}", select_move),
    // File
    cmd!(
        LAUNCH,
        "file.open",
        "Open",
        "File",
        Some("Enter"),
        "{path?} (default: focused item); folders navigate; programs are revealed, not run",
        file_open
    ),
    cmd!(
        RUN,
        "file.run",
        "Run…",
        "File",
        Some("Shift+Enter"),
        "{path?} (default: focused item); runs programs too",
        file_run
    ),
    cmd!(
        LAUNCH,
        "file.openIn",
        "Open In",
        "File",
        None,
        "{app: photocraft|lightcraft|filmcraft|pdfcraft|designcraft|wordcraft, paths?: [path]} (default: selection)",
        file_open_in
    ),
    cmd!(LAUNCH, "file.reveal", "Reveal in Explorer", "File", None, "{path?} (default: focused item)", file_reveal),
    cmd!(READ, "item.probe", "File Info", "File", None, "{path}", item_probe),
    // Tools (Stage 2; T1 previews only, nothing is written)
    cmd!(
        READ,
        "file.renamePreview",
        "Batch Rename",
        "Tools",
        None,
        "{template, start?: 0.. (default 1), paths?: [path]} (default: selection) → {plans: [{from, to, name, newName, changes, suffixed}], count, changed, suffixed, skipped, unknownTokens} (tokens {name} {num} {seq} {seq:3} {folder} {ext}; never overwrites: a taken name gets -1, -2…; changes nothing)",
        crate::rename::rename_preview
    ),
    // Cache
    cmd!(
        APP,
        "cache.clear",
        "Clear Thumbnail Cache",
        "View",
        None,
        "{} (deletes the disk cache in the background)",
        cache_clear
    ),
    // Storyboard (Stage 4; in memory until saving lands, see docs/storyboard-plan.md)
    cmd!(APP, "app.mode", "Mode", "View", None, "{mode: organizer|storyboard}", storyboard::app_mode),
    cmd!(
        APP,
        "storyboard.new",
        "New Storyboard",
        "File",
        None,
        "{name?, sample?: bool} (Manual start, or the 24-clip handoff sample); opens Storyboard mode",
        storyboard::sb_new
    ),
    cmd!(
        EDIT,
        "storyboard.generate",
        "New Storyboard from Track",
        "File",
        None,
        "{cueMap: path, pacing?: calm|cinematic|energetic|trailer, seconds?: 30|60|90|120|null, fit?: fade|edit, density?: 0.5..2, name?} (scenes from sections, clips on bars and hits; density: fewer, longer shots below 1, more, shorter above; one undo step)",
        crate::generate_cmd::sb_generate
    ),
    cmd!(
        READ,
        "storyboard.cueMaps",
        "Cue Maps Here",
        "Help",
        None,
        "{} → the .cuemap.json files in the open folder",
        crate::generate_cmd::sb_cue_maps
    ),
    cmd!(
        READ,
        "track.summary",
        "Track Summary",
        "Help",
        None,
        "{path} → {path, format, rate, channels, frames, seconds, peakDbfs, rmsDbfs} (decodes the whole file; ≤ 20 min, ≤ 8 channels, 8–384 kHz)",
        crate::track_cmd::track_summary
    ),
    cmd!(
        READ,
        "track.peaks",
        "Track Waveform",
        "Storyboard",
        None,
        "{path, buckets?: 16..4096} → {path, status: loading|ready|failed, seconds?, peaks?: [[min, max]], error?} (decodes in the background; ask again while loading)",
        crate::waveform::track_peaks
    ),
    cmd!(
        APP,
        "storyboard.musicFolder",
        "Music Folder",
        "Storyboard",
        None,
        "{path: folder | null} (where the Start screen looks for tracks; saved)",
        crate::library_cmd::sb_music_folder
    ),
    cmd!(
        READ,
        "storyboard.tracks",
        "Music Library",
        "Storyboard",
        None,
        "{} → {musicFolder, tracks: [{audio, cueMap, analysed, title, artist, license, seconds, bpm, lengths}]} (music folder + the open folder)",
        crate::library_cmd::sb_tracks
    ),
    cmd!(
        READ,
        "storyboard.preview",
        "Preview Storyboard",
        "Storyboard",
        None,
        "{cueMap, pacing?, seconds?, density?, shape?} → {clips: [{shot, start, seconds, scene, text?, in, out}], count, titleCards, scenes, length, avgSeconds, avgBeats, trackSeconds, edit: {segments: [[start, end]], fadeOut?}} (what storyboard.generate would build; times in seconds, edit in track time; changes nothing)",
        crate::library_cmd::sb_preview
    ),
    cmd!(
        APP,
        "track.preview",
        "Preview Track",
        "Storyboard",
        None,
        "{path: audio file | null, from?: seconds, cut?: {cueMap, seconds?, fit?: fade|edit}} (null stops; without `cut` plays the whole song, from = song seconds; with `cut` plays it through that length's edit, from = cut seconds; one preview at a time; the Cut's ▶ stops it)",
        crate::track_cmd::track_preview
    ),
    cmd!(
        APP,
        "track.previewVolume",
        "Preview Volume",
        "Storyboard",
        None,
        "{volume: 0..1} (track preview level, saved; Watch your Cut stays at full level)",
        crate::track_cmd::track_preview_volume
    ),
    cmd!(APP, "storyboard.view", "Shot Map View", "View", None, "{view: map|strip|fill}", storyboard::sb_view),
    cmd!(
        APP,
        "storyboard.select",
        "Select Clip",
        "Storyboard",
        None,
        "{clip: number | null, extend?: bool} (extend = Shift+click: select the run from the last clicked clip)",
        storyboard::sb_select
    ),
    cmd!(
        EDIT,
        "storyboard.delete",
        "Delete",
        "Edit",
        Some("Delete"),
        "{clips?: [number]} (default: the selection) — removes the shots; with no shot left in them, removes the clips",
        storyboard::sb_delete
    ),
    cmd!(
        EDIT,
        "storyboard.place",
        "Place in Next Empty Clip",
        "Edit",
        None,
        "{asset: path, clip?: number} (Quick fill; with `clip`, replaces that clip's shot)",
        storyboard::sb_place
    ),
    cmd!(
        EDIT,
        "storyboard.text.kind",
        "Switch Text Clip Type",
        "Storyboard",
        None,
        "{clip: number, kind: card|overlay} (Card: a whole clip with the theme's background; Overlay: text over the shot. Keeps the clip's place and length; one undo step)",
        crate::text_cmd::sb_text_kind
    ),
    cmd!(
        EDIT,
        "storyboard.text.style",
        "Text Clip Style",
        "Storyboard",
        None,
        "{clip: number, theme?: bold|minimal|cinematic|paper, animation?: none|fade|rise|zoom|typewriter, position?: lowerThirdLeft|lowerThirdRight|center|top|bottom}",
        crate::text_cmd::sb_text_style
    ),
    cmd!(
        EDIT,
        "storyboard.recut",
        "Re-cut to Track",
        "Storyboard",
        None,
        "{bpm?: 20..400, seconds?, keepTitleCards?: bool} (defaults: the current track)",
        storyboard::sb_recut
    ),
    cmd!(
        EDIT,
        "storyboard.autofill",
        "Auto-fill Rest",
        "Storyboard",
        None,
        "{assets?: [path]} (default: the open folder's files; unused ones first, then repeats)",
        storyboard::sb_autofill
    ),
    cmd!(
        EDIT,
        "storyboard.clear",
        "Clear Shots",
        "Storyboard",
        None,
        "{} (every clip's assets; undoable)",
        storyboard::sb_clear
    ),
    cmd!(
        APP,
        "storyboard.play",
        "Watch your Cut",
        "Storyboard",
        Some("Space"),
        "{from?: seconds} (from the end starts over)",
        storyboard::sb_play
    ),
    cmd!(
        APP,
        "storyboard.pause",
        "Pause",
        "Storyboard",
        Some("Space"),
        "{at?: seconds} (where the playhead stopped)",
        storyboard::sb_pause
    ),
    cmd!(APP, "storyboard.seek", "Seek", "Storyboard", None, "{seconds}", storyboard::sb_seek),
    cmd!(APP_STEP, "storyboard.undo", "Undo", "Storyboard", Some("Ctrl+Z"), "{}", storyboard::sb_undo),
    cmd!(APP_STEP, "storyboard.redo", "Redo", "Storyboard", Some("Ctrl+Y"), "{}", storyboard::sb_redo),
    cmd!(READ, "storyboard.state", "Storyboard State", "Help", None, "{}", storyboard::sb_state),
    // App
    cmd!(READ, "app.state", "State", "Help", None, "{}", |s, _| Ok(state_json(s))),
    cmd!(READ, "app.commands", "Commands", "Help", None, "{}", |s, _| Ok(s.command_infos())),
    cmd!(
        READ,
        "app.openInTargets",
        "Open In Targets",
        "Help",
        None,
        "{paths?: [path]} (default: selection) → apps that open all of them, installed or not",
        open_in_targets
    ),
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
    let raw = need_str(p, "path")?;
    let cleaned = clean_user_path(raw, cfg!(windows));
    if cleaned.is_empty() {
        return Err(CmdError::BadParams("`path` is empty".into()));
    }
    // Relative paths resolve against the process's working directory, once, here, so the
    // session never holds a relative folder. Paths that already start at a root are kept as
    // typed: on Windows `\Users` (root of the current drive) is valid, and `std::path::absolute`
    // would turn it, and every Unix-style test path, into `C:\Users`.
    let cleaned = PathBuf::from(cleaned);
    let path = if cleaned.has_root() {
        cleaned
    } else {
        std::path::absolute(&cleaned).map_err(|e| CmdError::BadParams(format!("bad path `{raw}`: {e}")))?
    };
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
    if !s.favorites.iter().any(|f| same_path(f, &path)) {
        s.favorites.push(path);
    }
    Ok(json!({ "favorites": lossy_all(&s.favorites) }))
}

fn fav_remove(s: &mut Session, p: &Value) -> CmdResult {
    let path = PathBuf::from(need_str(p, "path")?);
    s.favorites.retain(|f| !same_path(f, &path));
    Ok(json!({ "favorites": lossy_all(&s.favorites) }))
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

fn view_theme(s: &mut Session, p: &Value) -> CmdResult {
    let mode = need_str(p, "mode")?;
    s.view.theme = ThemeMode::parse(mode).ok_or_else(|| CmdError::BadParams(format!("unknown theme `{mode}`")))?;
    Ok(json!({ "view": s.view }))
}

/// `view.model`: every field optional; a bad value refuses the whole call and changes nothing.
fn view_model(s: &mut Session, p: &Value) -> CmdResult {
    let mut next = s.model;
    if let Some(v) = p.get("turntable") {
        next.turntable = v.as_bool().ok_or_else(|| CmdError::BadParams("`turntable` must be true/false".into()))?;
    }
    if let Some(v) = p.get("mode") {
        let text = v.as_str().unwrap_or_default();
        next.mode = crate::ModelDisplay::parse(text).ok_or_else(|| CmdError::BadParams(format!("unknown mode `{text}`")))?;
    }
    if let Some(v) = p.get("background") {
        let text = v.as_str().unwrap_or_default();
        next.background = crate::ModelBackground::parse(text)
            .ok_or_else(|| CmdError::BadParams(format!("unknown background `{text}`")))?;
    }
    if let Some(v) = p.get("exposure") {
        let (lo, hi) = crate::prefs::EXPOSURE_RANGE;
        let e = v.as_u64().filter(|e| (u64::from(lo)..=u64::from(hi)).contains(e));
        next.exposure = e.map(|e| e as u16).ok_or_else(|| CmdError::BadParams(format!("`exposure` must be {lo}..{hi}")))?;
    }
    s.model = next;
    Ok(json!({ "model": s.model }))
}

/// The message shown when HEIC is asked for in a build without the decoder.
pub const NO_HEIC_BUILD: &str = "this build has no HEIC support; rebuild with --features heic";

fn formats_heic(s: &mut Session, p: &Value) -> CmdResult {
    let enabled = need_bool(p, "enabled")?;
    if enabled && !assetcraft_thumbs::HEIC_BUILT {
        return Err(CmdError::Unavailable(NO_HEIC_BUILD.into()));
    }
    s.formats.heic = enabled;
    Ok(formats_json(s))
}

fn formats_json(s: &Session) -> Value {
    json!({ "heic": s.formats.heic, "heic_built": assetcraft_thumbs::HEIC_BUILT, "heic_active": s.decode_options().heic })
}

fn view_preview(s: &mut Session, p: &Value) -> CmdResult {
    let open = p.get("open").and_then(Value::as_bool).unwrap_or(!s.preview_open);
    if open && s.selection.focus().is_none() {
        return Err(CmdError::Unavailable("nothing focused to preview".into()));
    }
    s.preview_open = open;
    Ok(json!({ "preview": s.preview_open, "focus": s.selection.focus() }))
}

fn view_panels(s: &mut Session, p: &Value) -> CmdResult {
    let layout = p.get("layout").ok_or_else(|| CmdError::BadParams("need `layout` (object or null)".into()))?;
    if layout.is_null() {
        s.panels = None;
    } else if panels_json_ok(layout) {
        s.panels = Some(layout.clone());
    } else {
        return Err(CmdError::BadParams("`layout` must be an object under 64 KiB".into()));
    }
    Ok(json!({ "panels": s.panels }))
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
    // The full-window preview follows the focus; it closes if nothing is focused.
    s.preview_open &= s.selection.focus().is_some();
    Ok(selection_json(s))
}

/// Enter / double-click / File → Open. Programs, scripts and shortcuts are shown in the file
/// manager instead of run (Paul's decision, 2026-10-13); `file.run` runs them.
fn file_open(s: &mut Session, p: &Value) -> CmdResult {
    let path = target_path(s, p, false)?;
    if s.is_program(&path) {
        s.launcher.reveal(&path).map_err(CmdError::Unavailable)?;
        let name = assetcraft_fs::display_name(&path);
        return Ok(json!({
            "revealed": lossy(&path),
            "program": true,
            "message": format!("Revealed {name} (programs aren't run on Enter; use Shift+Enter)"),
        }));
    }
    file_run(s, p)
}

/// Shift+Enter / File → Run…: open anything in its default app, programs included.
fn file_run(s: &mut Session, p: &Value) -> CmdResult {
    let path = target_path(s, p, false)?;
    let entry = s.services.entry(&path)?;
    if entry.kind == EntryKind::Dir && !s.is_program(&path) {
        return s.go(Location::Folder(path));
    }
    s.launcher.open(&path).map_err(CmdError::Unavailable)?;
    Ok(json!({ "opened": lossy(&path) }))
}

/// `paths` param, or the selection.
pub(crate) fn paths_param(s: &Session, params: &Value) -> Result<Vec<PathBuf>, CmdError> {
    match params.get("paths") {
        None => Ok(s.selected_paths()),
        Some(list) => list
            .as_array()
            .ok_or_else(|| CmdError::BadParams("`paths` must be a list of strings".into()))?
            .iter()
            .map(|v| v.as_str().map(PathBuf::from).ok_or_else(|| CmdError::BadParams("`paths` must be strings".into())))
            .collect(),
    }
}

/// File → Open In ▸ <app>: hand the files to an installed sibling app.
fn file_open_in(s: &mut Session, p: &Value) -> CmdResult {
    use crate::open_in::{MAX_OPEN_IN_CHARS, MAX_OPEN_IN_FILES};
    let id = need_str(p, "app")?;
    let app = crate::find_app(id).ok_or_else(|| CmdError::BadParams(format!("unknown app `{id}`")))?;
    let paths = paths_param(s, p)?;
    if paths.is_empty() {
        return Err(CmdError::BadParams("nothing selected to open".into()));
    }
    if paths.len() > MAX_OPEN_IN_FILES {
        return Err(CmdError::Unavailable(format!("{} opens at most {MAX_OPEN_IN_FILES} files at once", app.label)));
    }
    let mut chars = 0;
    for path in &paths {
        // Most siblings read their arguments with `std::env::args()`, which stops on
        // non-Unicode text, and a relative path would resolve against their working folder.
        let text = path.to_str().ok_or_else(|| CmdError::BadParams(format!("not valid Unicode: {}", lossy(path))))?;
        // `has_root`, not `is_absolute`: listing paths always have one, and a rooted path can
        // never start with `-`, so no sibling mistakes it for an option.
        if !path.has_root() {
            return Err(CmdError::BadParams(format!("not a full path: {text}")));
        }
        chars += text.len() + 3;
        let entry = s.services.entry(path)?;
        let ok = match entry.kind {
            EntryKind::File => app.opens_extension(path),
            EntryKind::Dir => app.folders,
            _ => false,
        };
        if !ok {
            return Err(CmdError::BadParams(format!("{} doesn't open {}", app.label, entry.name)));
        }
    }
    if chars > MAX_OPEN_IN_CHARS {
        return Err(CmdError::Unavailable("too many files for one command line; select fewer".into()));
    }
    let program = s.app_program(app.id).map(Path::to_path_buf).ok_or_else(|| {
        CmdError::Unavailable(format!(
            "{} isn't installed (or set {} to its program, then press F5)",
            app.label,
            app.env_var()
        ))
    })?;
    s.launcher.spawn(&program, &app.argv(&paths)).map_err(CmdError::Unavailable)?;
    let n = paths.len();
    let what = if n == 1 {
        paths.first().map(|p| assetcraft_fs::display_name(p)).unwrap_or_default()
    } else {
        format!("{n} items")
    };
    Ok(json!({
        "app": app.id,
        "program": lossy(&program),
        "opened": lossy_all(&paths),
        "message": format!("Opened {what} in {}", app.label),
    }))
}

fn open_in_targets(s: &mut Session, p: &Value) -> CmdResult {
    let paths = paths_param(s, p)?;
    Ok(Value::Array(
        s.open_in_targets(&paths)
            .iter()
            .map(|t| json!({ "app": t.app.id, "label": t.app.label, "installed": t.installed(), "program": t.program.as_deref().map(lossy) }))
            .collect(),
    ))
}

fn file_reveal(s: &mut Session, p: &Value) -> CmdResult {
    let path = target_path(s, p, true)?;
    s.launcher.reveal(&path).map_err(CmdError::Unavailable)?;
    Ok(json!({ "revealed": lossy(&path) }))
}

fn item_probe(s: &mut Session, p: &Value) -> CmdResult {
    let path = PathBuf::from(need_str(p, "path")?);
    let probe = s.probe(&path)?;
    Ok(json!({
        "path": lossy(&probe.path),
        "name": probe.name,
        "format": probe.format.label(),
        "category": format!("{:?}", probe.category()),
        "width": probe.dimensions.map(|d| d.width),
        "height": probe.dimensions.map(|d| d.height),
        "size": probe.size,
        "modified": probe.modified.and_then(unix_secs),
        "created": probe.created.and_then(unix_secs),
        "status": format!("{:?}", probe.status),
        "unreadable": probe.is_unreadable(),
        "extension_mismatch": probe.extension_mismatch(),
    }))
}

fn cache_clear(s: &mut Session, _: &Value) -> CmdResult {
    let cache = s.thumb_cache.as_ref().ok_or_else(|| CmdError::Unavailable("no thumbnail cache".into()))?;
    if !cache.clear_in_background() {
        return Err(CmdError::Unavailable("the thumbnail cache is busy; try again in a moment".into()));
    }
    Ok(json!({ "clearing": lossy(cache.dir()) }))
}

fn cache_json(s: &Session) -> Value {
    match &s.thumb_cache {
        None => Value::Null,
        Some(c) => json!({ "dir": lossy(c.dir()), "bytes": c.size_known(), "budget": c.budget(), "busy": c.is_busy() }),
    }
}

// ---- JSON views --------------------------------------------------------------------------

fn unix_secs(t: std::time::SystemTime) -> Option<u64> {
    t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// Paths in command results. `json!` would panic serializing a non-UTF-8 `Path`, and file
/// names on disk are hostile input, so results carry lossy strings instead.
fn lossy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn lossy_all(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| lossy(p)).collect()
}

fn selection_json(s: &Session) -> Value {
    json!({ "selected": lossy_all(&s.selected_paths()), "focus": s.selection.focus().map(lossy) })
}

pub(crate) fn state_json(s: &Session) -> Value {
    json!({
        "location": s.location.path().map(lossy),
        "label": s.location.label(),
        "can_back": s.can_back(),
        "can_forward": s.can_forward(),
        "can_up": s.can_up(),
        "truncated": s.truncated,
        "skipped": s.skipped,
        "view": s.view,
        "favorites": lossy_all(&s.favorites),
        "selected": lossy_all(&s.selected_paths()),
        "focus": s.selection.focus().map(lossy),
        "mode": s.mode().as_str(),
        "preview": s.preview_open,
        "thumb_cache": cache_json(s),
        "formats": formats_json(s),
        "model": s.model,
        "apps": s.installed_apps().iter().map(|(id, p)| json!({ "app": id, "program": lossy(p) })).collect::<Vec<_>>(),
        "items": s.items.iter().map(|e| json!({
            "name": e.name,
            "path": lossy(&e.path),
            "kind": format!("{:?}", e.kind),
            "size": e.size,
            "modified": e.modified.and_then(unix_secs),
            "created": e.created.and_then(unix_secs),
        })).collect::<Vec<_>>(),
    })
}
