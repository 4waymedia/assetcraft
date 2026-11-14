//! The command registry. Each command has a stable id, its effects, a label and menu for the
//! UI, an optional shortcut, a one-line parameter description, and a handler.

use crate::effect::Effects;
use crate::prefs::{THUMB_SIZE_RANGE, ThemeMode, ViewMode, panels_json_ok};
use crate::select::ClickMode;
use crate::sort::SortKey;
use crate::storyboard;
use crate::{CmdError, CmdResult, Location, Session, clean_user_path};
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
    cmd!(
        APP,
        "nav.favorite.add",
        "Pin to My Folders",
        "File",
        None,
        "{path?, scope?: all|app} (default: current folder, shared with all Crafting apps) → {favorites, pins}",
        fav_add
    ),
    cmd!(APP, "nav.favorite.remove", "Unpin from My Folders", "Go", None, "{path}", fav_remove),
    cmd!(
        APP,
        "nav.favorite.scope",
        "Share Pinned Folder",
        "Go",
        None,
        "{path, scope: all|app} → {favorites, pins: [{path, shared}]} (all = every Crafting app sees the pin in My Folders; app = AssetCraft only; saved in the suite's places.json)",
        fav_scope
    ),
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
        "{name?, sample?: bool, template?: id | path, steps?: 1..=12, stepSeconds?: 10..=300, cueMap?, music?: structured | chorus | firstChorus | whole, credits?: bool, inset?: {kind?, corner?, size?, mixAudio?, addTo?: every | first}, voiceover?: path} (Manual start, the 24-clip handoff sample, or a template: review | tutorial | instructions or a .storyboard.json — every repeating section a title card + three shots of `stepSeconds` (30); with `cueMap` the song becomes its track as `music` says — intro · a chorus starting on each section's title · outro (default), the chorus looped, the first chorus only, or the whole song — and the shots snap to its beat at the template's own length); opens Storyboard mode",
        storyboard::sb_new
    ),
    cmd!(
        EDIT,
        "storyboard.generate",
        "New Storyboard from Track",
        "File",
        None,
        "{cueMap: path, pacing?: calm|cinematic|energetic|trailer, seconds?: 30|60|90|120|null, fit?: fade|edit, density?: 0.5..2, name?, credits?: bool} (scenes from sections, clips on bars and hits; `credits` appends the credits clip; density: fewer, longer shots below 1, more, shorter above; one undo step)",
        crate::generate_cmd::sb_generate
    ),
    cmd!(
        APP,
        "storyboard.open",
        "Open Storyboard",
        "File",
        Some("Ctrl+O"),
        "{path} → state + {opened, missing: [asset…], message} (a `.storyboard.json` read with the document limits and validated; one undo step like New; a newer format is refused; assets no longer at their path are listed and kept)",
        crate::document_cmd::sb_open
    ),
    cmd!(
        READ,
        "storyboard.templates",
        "New from Template",
        "File",
        None,
        "{} → {templates: [{id, name, description?, source: bundled|user, path?}], folder} (the bundled Review, Tutorial, 5-minute instructions, then the user's <working>/Templates/*.storyboard.json)",
        crate::template_cmd::sb_templates
    ),
    cmd!(
        READ,
        "storyboard.themes",
        "Themes",
        "Storyboard",
        None,
        "{} → {themes: [{id, name, tags, source: builtin|user, colors: {background, text, accent}, display, body, titleIn?, path?, warnings?}], problems: [{path, reason}], folder} (the built-ins Bold, Minimal, Cinematic, Paper, Noir Gold, Neon Night, Field Notes, Blueprint, then <working>/Themes/*.theme.json)",
        crate::theme_cmd::sb_themes
    ),
    cmd!(
        SAVE,
        "storyboard.saveTemplate",
        "Save as Template",
        "File",
        None,
        "{name, overwrite?} → {path, message} + state (the open board with assets and track removed, `template: true`, written to <working>/Templates/<name>.storyboard.json; an existing template only with `overwrite`)",
        crate::template_cmd::sb_save_template
    ),
    cmd!(
        APP,
        "storyboard.credits",
        "Edit Credits Clip",
        "File",
        None,
        "{on?: bool, edit?: bool} → {path, exists, bundled, on} (`on` sets the Start screen's checkbox, remembered; the credits clip template: the user's Templates/Credits.storyboard.json or the bundled one; `edit` writes the default when missing and opens it to edit — File › Save keeps it). New and Generate take `credits: true` to append it last",
        crate::template_cmd::sb_credits
    ),
    cmd!(
        READ,
        "storyboard.recent",
        "Open Recent",
        "File",
        None,
        "{} → {recent: [{path, name, exists}]} (the last 10 storyboards opened or saved, newest first)",
        crate::document_cmd::sb_recent
    ),
    cmd!(
        SAVE,
        "storyboard.save",
        "Save Storyboard",
        "File",
        Some("Ctrl+S"),
        "{path?, overwrite?: bool} → {path, bytes, message} + state (no path: the document's own file; a never-saved storyboard needs one; `<name>.storyboard.json`, written to a temp file then renamed; a different existing file is refused without `overwrite`)",
        crate::document_cmd::sb_save
    ),
    cmd!(
        SAVE,
        "storyboard.saveAs",
        "Save Storyboard As",
        "File",
        None,
        "{path, overwrite?: bool} → {path, bytes, message} + state (Save with the path required; the UI's picker starts in <working folder>/Storyboards)",
        crate::document_cmd::sb_save_as
    ),
    cmd!(
        EXPORT,
        "storyboard.exportVideo",
        "Export Video",
        "File",
        None,
        "{path (.mp4|.mov), size?: '720p'|'1080p'|'4k'|{width,height}, fps?: 24|25|30|50|60, quality?: 16..=36 (QP, default 24), overwrite?, dryRun?, wait?} → the plan + status (a background job: frames through the compositor into H.264, the song through its edit into AAC, written atomically; `dryRun` plans only; `wait` blocks until done)",
        crate::export_job::sb_export_video
    ),
    cmd!(
        READ,
        "storyboard.exportStatus",
        "Export Status",
        "File",
        None,
        "{} → {stage, framesDone, framesTotal, path, bytes, elapsedSecs, problems, message}",
        crate::export_job::sb_export_status
    ),
    cmd!(
        APP,
        "storyboard.exportCancel",
        "Cancel Export",
        "File",
        None,
        "{} → {cancelled, message} (stops at the next frame; the temp file is removed)",
        crate::export_job::sb_export_cancel
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
        "asset.meshInfo",
        "Mesh Info",
        "Help",
        None,
        "{path} → {path, format, bytes, vertices, triangles, materials, normals, uvs, bounds, images: [{index, mime, bytes, width, height, drawn}], warnings} (parses the whole file under the viewer's limits; image sizes from the headers)",
        crate::model_cmd::mesh_info
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
        "track.analyze",
        "Analyze Track",
        "Storyboard",
        None,
        "{path} → {path, cueMapPath, cueMap, seconds, tempo: {bpm, candidates, half, double, pulseClarity, stabilityCv, rhythm}} (decodes and analyses the whole file: beats, bars, sections, hits, fits; writes nothing; cueMapPath is the sidecar storyboard.tracks reads)",
        crate::track_cmd::track_analyze
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
        "app.workingFolder",
        "Working Folder",
        "Storyboard",
        None,
        "{path?: folder | null} → {workingFolder, default, music, custom} (no path = read; null = the default, Documents/AssetCraft; saved; created on first use)",
        crate::working_cmd::working_folder
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
        "{} → {musicFolder, tracks: [{audio, cueMap, analysed, title, artist, license, seconds, bpm, lengths, status?: new|queued|analysing|failed, error?}], analysis: {pending, done, failed, batches, available}} (music folder + the open folder)",
        crate::library_cmd::sb_tracks
    ),
    cmd!(
        APP,
        "storyboard.analyse",
        "Analyse Tracks",
        "Storyboard",
        None,
        "{paths?: [audio]} → {queued, pending, message} (analyses in the background: beats, bars, sections, hits, fits; default every listed track not analysed, queued or failed (Retry passes `paths`); results kept in AssetCraft's own analysis folder, never beside the music; storyboard.tracks shows queued / analysing / failed and counts the queue in `analysis`)",
        crate::analyse_job::sb_analyse
    ),
    cmd!(
        COPY,
        "storyboard.musicImport",
        "Copy Music Into AssetCraft",
        "Storyboard",
        None,
        "{paths?: [audio], dryRun?: bool} → {music, dryRun, copied, present, failed, items: [{from, to?, action: copy|present|failed, error?}], tracks, message} (copies songs into <working folder>/Music; default every song in the music folder; never overwrites, originals untouched, same name + size = already there, else `name (2).ext`; then the music folder is Music)",
        crate::working_cmd::music_import
    ),
    cmd!(
        READ,
        "storyboard.preview",
        "Preview Storyboard",
        "Storyboard",
        None,
        "{cueMap, pacing?, seconds?, density?, shape?} or {cueMap, template, steps?, stepSeconds?, credits?, music?, inset?} → {clips: [{shot, start, seconds, scene, text?, in, out}], count, titleCards, scenes, sections: [{scene, title, start, seconds, shots, inset?}], length, avgSeconds, avgBeats, trackSeconds, edit: {segments: [[start, end]], fadeOut?}} (what storyboard.generate or storyboard.new {template} would build; times in seconds, edit in track time; changes nothing)",
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
        "storyboard.text.set",
        "Clip Text",
        "Storyboard",
        None,
        "{clip: number, text?: string, description?: string} (the words on screen — a title card's text, an overlay's text; on a plain shot `text` adds an overlay, `\"\"` on an overlay removes it — and the shot note; line breaks kept; one undo step)",
        crate::text_cmd::sb_text_set
    ),
    cmd!(
        EDIT,
        "storyboard.text.style",
        "Text Clip Style",
        "Storyboard",
        None,
        "{clip: number, theme?: bold|minimal|cinematic|paper|noir-gold|neon-night|field-notes|blueprint, animation?: none|fade|rise|zoom|typewriter|theme (the in), duration?: seconds 0.5–600 (how long the text stays up from the clip's start, in and out included; 0 = until the clip ends) or \"theme\", effect?: none|push|drift|pulse|theme (while it is up), exit?: none|fade|sink|zoom|erase|theme (the out); \"theme\" follows the theme's moves again; the reply gives the moves in effect and `fromTheme`, position?: lowerThirdLeft|lowerThirdRight|center|top|bottom} (one undo step)",
        crate::text_cmd::sb_text_style
    ),
    cmd!(
        EDIT,
        "storyboard.motion",
        "Clip Motion",
        "Storyboard",
        None,
        "{clip: number, preset?: turntable|pushIn|pullOut|panLeft|panRight|rise|custom, start?: {yaw?, pitch?, zoom?, pan?: [x, y]}, amount?: {…}, ease?: linear|inOut|out, swap?: true, clear?: true} (angles in degrees; End = Start + amount; one undo step)",
        crate::motion_cmd::sb_motion
    ),
    cmd!(
        EDIT,
        "storyboard.resize",
        "Clip Length",
        "Storyboard",
        None,
        "{clip: number, seconds?: number | beats?: number} (one clip's length; the clips after it move and the Cut's total follows; the track is left alone; one undo step)",
        crate::resize_cmd::sb_resize
    ),
    cmd!(
        EDIT,
        "storyboard.roll",
        "Move Cut",
        "Storyboard",
        None,
        "{left: number, seconds?: number | beats?: number | to?: number} (move the cut after clip `left` by seconds or beats, signed, or to a time in the Cut; the clip after it gives or takes the difference, so the Cut's total and the music stay put; neither clip goes under half a second, and the reply says how far it moved; FilmCraft's roll; one undo step)",
        crate::cut_cmd::sb_roll
    ),
    cmd!(
        EDIT,
        "storyboard.inset",
        "Section Inset",
        "Storyboard",
        None,
        "{scene: number, asset?: path | null, kind?: video | graphic (else by extension), corner?: topLeft | topRight | bottomLeft | bottomRight, size?: small | medium | large, mixAudio?: bool, swap?: bool, musicDb?: -40..=0, voiceDb?: -24..=12, note?: text | null, seconds?: number, fit?: bool} (a section's inset lane over its shots; omitted fields keep their value; `seconds` stretches the section's shots to the recording, else a new video's own length does (`fit`, default; MP4 / MOV); one undo step)",
        crate::inset_cmd::sb_inset
    ),
    cmd!(
        EDIT,
        "storyboard.voiceover",
        "Voiceover",
        "Storyboard",
        None,
        "{asset?: path | null, musicDb?: -40..=0, voiceDb?: -24..=12} (a narration under the whole Cut from the top; the song ducks to musicDb while it speaks; null removes it; one undo step)",
        crate::inset_cmd::sb_voiceover
    ),
    cmd!(
        EDIT,
        "storyboard.insetClear",
        "Clear Section Inset",
        "Storyboard",
        None,
        "{scene: number, remove?: bool} (empty the slot, or drop the lane with `remove`; one undo step)",
        crate::inset_cmd::sb_inset_clear
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

/// My Folders as `{favorites, pins: [{path, shared}]}`.
fn pins_json(s: &Session) -> Value {
    let pins: Vec<Value> =
        s.favorites.iter().map(|f| json!({ "path": lossy(f), "shared": s.pin_shared(f).unwrap_or(true) })).collect();
    json!({ "favorites": lossy_all(&s.favorites), "pins": pins })
}

fn fav_add(s: &mut Session, p: &Value) -> CmdResult {
    let path = match str_param(p, "path") {
        Some(p) => PathBuf::from(p),
        None => s.location.path().map(Path::to_path_buf).ok_or_else(|| CmdError::BadParams("need `path`".into()))?,
    };
    if !path.has_root() {
        return Err(CmdError::BadParams("`path` must be a full path".into()));
    }
    let scope = crate::places::scope_param(p, crate::places::PinScope::All)?;
    let entry = s.services.entry(&path)?;
    if entry.kind != EntryKind::Dir {
        return Err(CmdError::BadParams("favorites are folders".into()));
    }
    s.edit_pins(|pins| crate::places::pin(pins, path, scope))?;
    Ok(pins_json(s))
}

fn fav_remove(s: &mut Session, p: &Value) -> CmdResult {
    let path = PathBuf::from(need_str(p, "path")?);
    s.edit_pins(|pins| crate::places::unpin(pins, &path))?;
    Ok(pins_json(s))
}

fn fav_scope(s: &mut Session, p: &Value) -> CmdResult {
    let path = PathBuf::from(need_str(p, "path")?);
    if p.get("scope").is_none_or(Value::is_null) {
        return Err(CmdError::BadParams("need `scope`: all or app".into()));
    }
    let scope = crate::places::scope_param(p, crate::places::PinScope::All)?;
    if s.pin_shared(&path).is_none() {
        return Err(CmdError::BadParams("that folder isn't in My Folders".into()));
    }
    s.edit_pins(|pins| {
        crate::places::set_scope(pins, &path, scope);
    })?;
    Ok(pins_json(s))
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
        next.mode =
            crate::ModelDisplay::parse(text).ok_or_else(|| CmdError::BadParams(format!("unknown mode `{text}`")))?;
    }
    if let Some(v) = p.get("background") {
        let text = v.as_str().unwrap_or_default();
        next.background = crate::ModelBackground::parse(text)
            .ok_or_else(|| CmdError::BadParams(format!("unknown background `{text}`")))?;
    }
    if let Some(v) = p.get("exposure") {
        let (lo, hi) = crate::prefs::EXPOSURE_RANGE;
        let e = v.as_u64().filter(|e| (u64::from(lo)..=u64::from(hi)).contains(e));
        next.exposure =
            e.map(|e| e as u16).ok_or_else(|| CmdError::BadParams(format!("`exposure` must be {lo}..{hi}")))?;
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
        "recent": lossy_all(&s.recent),
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
