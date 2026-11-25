//! Session, command registry and navigation state. Every action is a command.
//!
//! Layer 5. The UI, keyboard, CLI and (later) MCP all call [`Session::execute`] with a command
//! id and JSON params; nothing else changes state. The UI then reads state back through the
//! getters ([`Session::location`], [`Session::items`], [`Session::selection`] …) and draws it.
//! Commands are listed in [`commands`]; `assetcraft-cli commands` prints them.

mod analyse_job;
mod audio_out;
mod commands;
mod cut_adapter;
mod cut_cmd;
mod decode;
pub mod document_cmd;
pub mod effect;
mod enabled;
pub mod export_job;
mod frame_cmd;
pub mod generate_cmd;
mod inset_cmd;
pub mod launch;
mod library_cmd;
mod model_cmd;
mod motion_cmd;
pub mod open_in;
pub mod otio_out;
pub mod paths;
pub mod places;
pub mod prefs;
pub mod rename;
mod resize_cmd;
pub mod select;
pub mod sort;
pub mod storyboard;
pub mod template_cmd;
mod text_cmd;
pub mod theme_cmd;
pub mod title_render;
mod track_cmd;
mod watching;
mod waveform;
mod working_cmd;

pub use analyse_job::{AnalyseProgress, AnalyseState};
pub use assetcraft_audio::{Player, RecordingPlayer, SilentPlayer};
pub use assetcraft_media::{Frame as VideoFrame, Lookup as VideoLookup, is_video, seconds as video_seconds};
pub use assetcraft_thumbs::DiskCache;
/// A pack design's Viewer frame (TP2b), as [`Session::design_frame`] gives it.
pub use assetcraft_titles::frames::DesignLookup;
pub use audio_out::PreviewMode;
pub use commands::{CommandSpec, NO_HEIC_BUILD, commands, find};
pub use cut_cmd::STRIP_ZOOM_RANGE;
pub use effect::{EffectClass, Effects};
pub use export_job::{ExportPlan, ExportProgress, ExportStage};
pub use generate_cmd::{CUEMAP_SUFFIX, GeneratedFrom, LENGTHS, cue_map_label};
pub use launch::{
    LaunchAction, Launcher, PROGRAM_EXTENSIONS, RecordingLauncher, SystemLauncher, explorer_command_line,
    has_program_extension,
};
pub use open_in::{AppLocator, FakeLocator, OpenInTarget, SIBLING_APPS, SiblingApp, SystemLocator, find_app};
pub use paths::{clean_user_path, same_path};
pub use prefs::{FormatPrefs, ModelBackground, ModelDisplay, ModelPrefs, Prefs, ThemeMode, ViewMode, ViewPrefs};
pub use resize_cmd::MIN_CLIP_SECS;
pub use select::{ClickMode, Selection};
pub use sort::SortKey;
pub use storyboard::{BoardView, MAX_AUTOFILL_ASSETS, MAX_UNDO, Mode, Transport};
pub use title_render::{RenderPlan, TitlePlan, TitleProblem};
pub use watching::{MAX_INCREMENTAL, WatchStatus};

use assetcraft_fs::{Entry, EntryKind, FsError, KnownFolder, ListOptions, Services, Volume};
use assetcraft_probe::Probe;
use assetcraft_thumbs::{DEFAULT_PROBE_WORKERS, ProbePool};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// Most entries shown for one folder. Beyond this the listing is marked truncated.
pub const MAX_ITEMS: usize = 50_000;

/// Back-history depth.
pub const MAX_HISTORY: usize = 100;

/// Where the content panel is pointed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    /// "Computer": the list of drives / volumes.
    Computer,
    /// A folder on disk.
    Folder(PathBuf),
}

impl Location {
    /// The folder path, if this is a folder.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Computer => None,
            Self::Folder(p) => Some(p),
        }
    }

    /// Display label (folder name, or "Computer").
    pub fn label(&self) -> String {
        match self {
            Self::Computer => "Computer".into(),
            Self::Folder(p) => assetcraft_fs::display_name(p),
        }
    }
}

/// Why a command failed. Shown to the user as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdError {
    /// No command with that id.
    Unknown(String),
    /// Missing or invalid parameter.
    BadParams(String),
    /// The file system refused.
    Fs(FsError),
    /// The command can't run right now (e.g. "no folder above this one").
    Unavailable(String),
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(id) => write!(f, "unknown command `{id}`"),
            Self::BadParams(m) => write!(f, "bad parameters: {m}"),
            Self::Fs(e) => write!(f, "{e}"),
            Self::Unavailable(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for CmdError {}

impl From<FsError> for CmdError {
    fn from(e: FsError) -> Self {
        Self::Fs(e)
    }
}

/// What is known about a file's header right now (see [`Session::file_info`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileInfo<'a> {
    /// Being probed on a worker thread; draw "…".
    Pending,
    /// Probed.
    Ready(&'a Probe),
    /// The file couldn't be read.
    Failed(&'a str),
}

/// The result of a command: a JSON value describing what changed.
pub type CmdResult = Result<Value, CmdError>;

/// One window's worth of navigator state.
pub struct Session {
    services: Arc<dyn Services>,
    launcher: Arc<dyn Launcher>,
    location: Location,
    back: Vec<Location>,
    forward: Vec<Location>,
    items: Vec<Entry>,
    item_paths: Vec<PathBuf>,
    truncated: bool,
    skipped: usize,
    selection: Selection,
    /// My Folders as this app shows them (derived from `pins`).
    favorites: Vec<PathBuf>,
    /// Every pin in My Folders, including those other Crafting apps keep to themselves.
    pins: Vec<places::Pin>,
    /// The suite-wide `places.json`, when the app gave one.
    pins_file: Option<PathBuf>,
    /// Recent folders, newest first (saved).
    recent: Vec<PathBuf>,
    /// Favorites hidden from Favorites › System in AssetCraft (`nav.place.hide`; saved). The OS's
    /// own sidebar is never changed.
    hidden_places: Vec<PathBuf>,
    /// Whether the working folder existed at the last check (see `working_folder_exists`).
    working_exists: bool,
    /// The OS sidebar's folders (rescanned with the volumes).
    sidebar: Vec<assetcraft_fs::SidebarItem>,
    view: ViewPrefs,
    probes: HashMap<PathBuf, Result<Probe, String>>,
    /// Background probes for the UI (started on first use, so the CLI never spawns threads).
    probe_pool: Option<ProbePool>,
    /// The disk thumbnail cache, if the app gave one (`cache.clear`, `app.state`).
    thumb_cache: Option<Arc<DiskCache>>,
    panels: Option<Value>,
    preview_open: bool,
    /// Volumes and known folders, scanned at startup and by `nav.computer` / `nav.refresh`.
    /// The Folders panel reads these every frame; on Windows a scan stats 26 drive letters
    /// (and an offline mapped drive can stall), so it must never happen per frame.
    volumes: Vec<Volume>,
    known: Vec<(KnownFolder, PathBuf)>,
    /// Live updates for the open folder (see [`Session::set_watcher`]).
    watcher: Option<Box<dyn assetcraft_watch::Watcher>>,
    watched: Option<PathBuf>,
    watch_status: WatchStatus,
    /// Finds the sibling apps for "Open In"; the result is cached in `apps` and refreshed with
    /// the volumes (startup, `nav.computer`, `nav.refresh`).
    app_locator: Arc<dyn AppLocator>,
    apps: Vec<(&'static str, PathBuf)>,
    /// Bumped by every [`Session::execute`], so the UI can cache what it derives from state.
    generation: u64,
    /// Optional formats as saved (`formats.heic`).
    formats: FormatPrefs,
    /// 3D preview settings (`view.model`).
    model: ModelPrefs,
    /// Storyboard mode: the open storyboard, selected clip, undo (`storyboard.*`).
    boards: storyboard::Boards,
    /// Track audio for Watch your Cut: player, decode, program (`audio_out`).
    audio: audio_out::TrackAudio,
    /// Waveform overviews for the Start screen's timeline (`track.peaks`).
    waveforms: waveform::Waveforms,
    decodes: decode::Decodes,
    /// Where Storyboard mode's Start screen looks for music (`storyboard.musicFolder`).
    music_folder: Option<PathBuf>,
    /// Track preview volume, 0–1 (`track.previewVolume`).
    preview_volume: f32,
    /// How loud the Cut plays while editing, 0–1 (`storyboard.listenVolume`; saved). Not exported.
    listen_volume: f32,
    /// The AssetCraft working folder as set (`app.workingFolder`; `None` = the default).
    working_folder: Option<PathBuf>,
    /// Where in-app track analyses are kept (`storyboard.analyse`; set by the app).
    analysis_dir: Option<std::path::PathBuf>,
    /// The background track analyser (`storyboard.analyse`).
    analyser: analyse_job::Analyser,
    /// The video export job (`storyboard.exportVideo`).
    exporter: export_job::Exporter,
    /// Storyboards opened or saved, newest first (`storyboard.recent`; saved in prefs).
    recent_storyboards: Vec<PathBuf>,
    /// The Start screen's "Add the credits clip" (saved in prefs).
    credits: bool,
    /// Snapping for the cut tools (`storyboard.snapping`; saved in prefs).
    snapping: bool,
    /// The Viewer's safe-zone guides (`storyboard.safeZones`; saved in prefs).
    safe_zones: bool,
    /// The frame new storyboards start with (`storyboard.defaultFrame`; saved in prefs).
    pub(crate) default_frame: Option<assetcraft_storyboard::FrameSpec>,
    /// The Strip's zoom (`storyboard.zoom`, Z1): 1 = 44 pt a second.
    strip_zoom: f32,
    /// Video frames for the Viewer (P1), started on first use.
    video: std::sync::OnceLock<assetcraft_media::VideoFrames>,
    /// The user's themes and theme packs from `<working>/Themes`, loaded at startup and again on
    /// `storyboard.themes {reload: true}`, `storyboard.importTheme` and a new working folder (TP2a).
    themes: theme_cmd::ThemeLibrary,
    /// Pack design frames for the Viewer (TP2b), started on first use.
    design_frames: std::sync::OnceLock<assetcraft_titles::frames::DesignFrames>,
}

impl Session {
    /// A session reading through `services`, launching through `launcher`, restoring `prefs`.
    /// Opens the last folder if it still exists, else Pictures, else home, else Computer.
    pub fn new(services: Arc<dyn Services>, launcher: Arc<dyn Launcher>, prefs: Prefs) -> Self {
        let mut session = Self {
            services,
            launcher,
            location: Location::Computer,
            back: Vec::new(),
            forward: Vec::new(),
            items: Vec::new(),
            item_paths: Vec::new(),
            truncated: false,
            skipped: 0,
            selection: Selection::default(),
            favorites: Vec::new(),
            pins: prefs.favorites.iter().map(|p| places::Pin::legacy(p.clone())).collect(),
            pins_file: None,
            recent: prefs.recent.clone(),
            hidden_places: prefs.hidden_places.clone(),
            working_exists: false,
            sidebar: Vec::new(),
            view: prefs.view,
            probes: HashMap::new(),
            probe_pool: None,
            thumb_cache: None,
            panels: prefs.panels,
            preview_open: false,
            volumes: Vec::new(),
            known: Vec::new(),
            watcher: None,
            watched: None,
            watch_status: WatchStatus::Off,
            app_locator: Arc::new(SystemLocator),
            apps: Vec::new(),
            generation: 0,
            formats: prefs.formats,
            model: prefs.model,
            boards: storyboard::Boards::default(),
            audio: audio_out::TrackAudio::default(),
            waveforms: waveform::Waveforms::default(),
            decodes: decode::Decodes::default(),
            music_folder: prefs.music_folder.clone(),
            preview_volume: prefs.preview_volume.map_or(audio_out::DEFAULT_PREVIEW_VOLUME, |p| f32::from(p) / 100.0),
            listen_volume: prefs.listen_volume.map_or(1.0, |p| f32::from(p) / 100.0),
            working_folder: prefs.working_folder.clone(),
            analysis_dir: None,
            analyser: analyse_job::Analyser::default(),
            exporter: export_job::Exporter::default(),
            recent_storyboards: prefs.recent_storyboards.iter().take(document_cmd::MAX_RECENT).cloned().collect(),
            credits: prefs.credits,
            snapping: prefs.snapping,
            safe_zones: prefs.safe_zones,
            default_frame: prefs.frame.clone(),
            strip_zoom: 1.0,
            video: std::sync::OnceLock::new(),
            themes: theme_cmd::ThemeLibrary::default(),
            design_frames: std::sync::OnceLock::new(),
        };
        session.refresh_favorites();
        session.rescan_places();
        // After the places: the default working folder (Documents, else home) is one of them,
        // and the user's themes live in it.
        session.reload_themes();
        let start = prefs
            .last_folder
            .into_iter()
            .chain(session.services.known_folder(KnownFolder::Pictures))
            .chain(session.services.home_dir())
            .map(Location::Folder)
            .find(|loc| session.load(loc).is_ok());
        match start {
            Some(loc) => session.location = loc,
            None => {
                let _ = session.load(&Location::Computer);
            }
        }
        session
    }

    /// Run a command. This is the only way state changes.
    pub fn execute(&mut self, id: &str, params: &Value) -> CmdResult {
        let spec = find(id).ok_or_else(|| CmdError::Unknown(id.to_string()))?;
        // A read changes nothing a cache could derive from, so it leaves the generation alone:
        // the Start screen polls `track.peaks` every frame while a song loads, and that used to
        // empty every generation-keyed UI cache each frame (audit AR-5).
        if !spec.effects.read_only() {
            self.generation = self.generation.wrapping_add(1);
        }
        (spec.run)(self, params)
    }

    /// A counter that changes whenever a command that can change state has run (successfully or
    /// not); read-only commands (`Effects::read_only`) leave it alone. State only changes through
    /// commands, so anything derived from it can be cached until this moves.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Current location.
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Items in the current location, filtered and sorted for display.
    pub fn items(&self) -> &[Entry] {
        &self.items
    }

    /// `true` if the folder had more than [`MAX_ITEMS`] entries.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Entries the OS wouldn't let us read in the current folder.
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// The selection.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Selected paths in display order.
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selection.paths(&self.item_paths)
    }

    /// Favorites in the user's order.
    pub fn favorites(&self) -> &[PathBuf] {
        &self.favorites
    }

    /// View settings.
    pub fn view(&self) -> &ViewPrefs {
        &self.view
    }

    /// 3D preview settings (`view.model`).
    pub fn model(&self) -> ModelPrefs {
        self.model
    }

    /// The saved panel arrangement (opaque JSON owned by the UI; see `view.panels`).
    pub fn panels(&self) -> Option<&Value> {
        self.panels.as_ref()
    }

    /// `true` while the full-window preview of the focused item is open (`view.preview`).
    pub fn preview_open(&self) -> bool {
        self.preview_open
    }

    /// `true` if `nav.back` would do something.
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    /// `true` if `nav.forward` would do something.
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// `true` if `nav.up` would do something.
    pub fn can_up(&self) -> bool {
        matches!(self.location, Location::Folder(_))
    }

    /// Breadcrumb from the volume root to the current folder.
    pub fn breadcrumb(&self) -> Vec<PathBuf> {
        let Some(path) = self.location.path() else { return Vec::new() };
        let mut crumbs: Vec<PathBuf> = path.ancestors().map(Path::to_path_buf).collect();
        crumbs.reverse();
        crumbs
    }

    /// Drives / volumes for the Folders panel (cached; rescanned by `nav.computer` and
    /// `nav.refresh`).
    pub fn volumes(&self) -> &[Volume] {
        &self.volumes
    }

    /// Known folders that exist, for the Folders panel (cached like [`Session::volumes`]).
    pub fn known_folders(&self) -> &[(KnownFolder, PathBuf)] {
        &self.known
    }

    /// Sub-folders of `dir` for the folder tree (lazy: call when a node is expanded).
    pub fn subfolders(&self, dir: &Path) -> Result<Vec<PathBuf>, CmdError> {
        let opts = ListOptions { include_hidden: self.view.show_hidden, max_entries: MAX_ITEMS };
        let mut dirs: Vec<Entry> =
            self.services.list_entries(dir, &opts)?.entries.into_iter().filter(|e| e.kind == EntryKind::Dir).collect();
        sort::sort_entries(&mut dirs, SortKey::Name, true);
        Ok(dirs.into_iter().map(|e| e.path).collect())
    }

    /// Header facts for a file without blocking: cached, else queued on a worker thread
    /// ([`FileInfo::Pending`] until [`Session::poll_file_info`] collects it). This is what the
    /// UI uses; it never reads the file on the calling thread.
    pub fn file_info(&mut self, path: &Path) -> FileInfo<'_> {
        if !self.probes.contains_key(path) {
            let services = &self.services;
            let pool =
                self.probe_pool.get_or_insert_with(|| ProbePool::new(Arc::clone(services), DEFAULT_PROBE_WORKERS));
            if pool.workers() > 0 {
                pool.request(path.to_path_buf());
                return FileInfo::Pending;
            }
            // No thread could be started: probe here rather than show "…" forever.
            let result = assetcraft_probe::probe(self.services.as_ref(), path).map_err(|e| e.to_string());
            self.probes.insert(path.to_path_buf(), result);
        }
        match self.probes.get(path) {
            Some(Ok(probe)) => FileInfo::Ready(probe),
            Some(Err(error)) => FileInfo::Failed(error),
            None => FileInfo::Pending,
        }
    }

    /// Collect finished background probes into the cache. Returns how many arrived.
    pub fn poll_file_info(&mut self) -> usize {
        let Some(pool) = &self.probe_pool else { return 0 };
        let done = pool.poll();
        let n = done.len();
        for (path, result) in done {
            self.probes.insert(path, result.map_err(|e| e.to_string()));
        }
        n
    }

    /// Drop queued background probes for files `keep` rejects (rows that scrolled away).
    pub fn retain_file_info(&self, keep: impl FnMut(&PathBuf) -> bool) {
        if let Some(pool) = &self.probe_pool {
            pool.retain(keep);
        }
    }

    /// Background probes queued or running.
    pub fn file_info_busy(&self) -> usize {
        self.probe_pool.as_ref().map_or(0, ProbePool::busy)
    }

    /// Attach the disk thumbnail cache (the app computes its folder; see `cache.clear`).
    pub fn with_thumb_cache(mut self, cache: Arc<DiskCache>) -> Self {
        self.thumb_cache = Some(cache);
        self
    }

    /// Use `locator` to find the sibling apps for "Open In" (tests pass a [`FakeLocator`]),
    /// and look for them now.
    pub fn with_app_locator(mut self, locator: Arc<dyn AppLocator>) -> Self {
        self.app_locator = locator;
        self.rescan_apps();
        self.generation = self.generation.wrapping_add(1);
        self
    }

    /// Installed sibling apps and their programs (cached; looked up again on `nav.refresh`).
    pub fn installed_apps(&self) -> &[(&'static str, PathBuf)] {
        &self.apps
    }

    /// The program of the sibling app `id`, if it was found.
    pub fn app_program(&self, id: &str) -> Option<&Path> {
        self.apps.iter().find(|(a, _)| *a == id).map(|(_, p)| p.as_path())
    }

    /// The Open In entries for `paths`: every sibling app that opens *all* of them (by
    /// extension; folders only for apps that import folders), installed or not, in catalog
    /// order. Empty when `paths` is empty. Reads no files (kinds come from the listing).
    pub fn open_in_targets(&self, paths: &[PathBuf]) -> Vec<OpenInTarget> {
        if paths.is_empty() {
            return Vec::new();
        }
        // Selected paths come from the listing, so an exact match finds their kind.
        let dirs: std::collections::HashSet<&Path> =
            self.items.iter().filter(|e| e.kind == EntryKind::Dir).map(|e| e.path.as_path()).collect();
        let is_dir = |p: &PathBuf| dirs.contains(p.as_path());
        SIBLING_APPS
            .iter()
            .filter(|app| paths.iter().all(|p| app.opens_extension(p) || (app.folders && is_dir(p))))
            .map(|app| OpenInTarget { app, program: self.app_program(app.id).map(Path::to_path_buf) })
            .collect()
    }

    /// The decode options thumbnails should use now: HEIC only when this build has the decoder
    /// *and* the user turned it on (`formats.heic`).
    pub fn decode_options(&self) -> assetcraft_thumbs::DecodeOptions {
        assetcraft_thumbs::DecodeOptions { heic: self.formats.heic && assetcraft_thumbs::HEIC_BUILT }
    }

    /// Optional formats as saved (`formats.heic` may be on in prefs from a build that had it).
    pub fn formats(&self) -> FormatPrefs {
        self.formats
    }

    /// The disk thumbnail cache, if one is attached.
    pub fn thumb_cache(&self) -> Option<&Arc<DiskCache>> {
        self.thumb_cache.as_ref()
    }

    /// Header facts for a file (format, dimensions …), cached until the folder is refreshed.
    /// Blocking (reads up to 256 KiB): for the CLI and commands; the UI uses
    /// [`Session::file_info`].
    pub fn probe(&mut self, path: &Path) -> Result<&Probe, CmdError> {
        let services = Arc::clone(&self.services);
        let entry = self
            .probes
            .entry(path.to_path_buf())
            .or_insert_with(|| assetcraft_probe::probe(services.as_ref(), path).map_err(|e| e.to_string()));
        entry.as_ref().map_err(|e| CmdError::Unavailable(e.clone()))
    }

    /// `true` if opening `path` would run code: a program, script or shortcut by extension
    /// (`.exe`, `.bat`, `.lnk`, `.sh`, `.app` … see [`PROGRAM_EXTENSIONS`]) or, on Unix, a file
    /// with an execute bit. `file.open` reveals these; `file.run` runs them.
    pub fn is_program(&self, path: &Path) -> bool {
        has_program_extension(path) || self.services.is_executable(path)
    }

    /// Everything that should be saved on exit.
    pub fn prefs(&self) -> Prefs {
        Prefs {
            version: prefs::PREFS_VERSION,
            favorites: self.favorites.clone(),
            recent: self.recent.iter().filter(|p| p.to_str().is_some()).cloned().collect(),
            // A non-UTF-8 folder can't be written to prefs JSON; don't let it void the file.
            last_folder: self.location.path().filter(|p| p.to_str().is_some()).map(Path::to_path_buf),
            view: self.view.clone(),
            panels: self.panels.clone(),
            formats: self.formats,
            music_folder: self.music_folder.clone().filter(|p| p.to_str().is_some()),
            preview_volume: Some((self.preview_volume * 100.0).round().clamp(0.0, 100.0) as u8),
            listen_volume: Some((self.listen_volume * 100.0).round().clamp(0.0, 100.0) as u8),
            model: self.model,
            working_folder: self.working_folder.clone().filter(|p| p.to_str().is_some()),
            recent_storyboards: self.recent_storyboards.iter().filter(|p| p.to_str().is_some()).cloned().collect(),
            credits: self.credits,
            snapping: self.snapping,
            safe_zones: self.safe_zones,
            frame: self.default_frame.clone(),
            hidden_places: self.hidden_places.iter().filter(|p| p.to_str().is_some()).cloned().collect(),
        }
    }

    /// Whether the Start screen adds the credits clip (the checkbox; remembered).
    pub fn credits_on(&self) -> bool {
        self.credits
    }

    /// The frame of the video at `path` shown `secs` into it, at most `max_side` pixels a side,
    /// for the Viewer (P1): answers at once, decoding in the background
    /// (`assetcraft_media::VideoFrames`).
    pub fn video_frame(&self, path: &std::path::Path, secs: f64, max_side: u32) -> assetcraft_media::Lookup {
        self.video.get_or_init(assetcraft_media::VideoFrames::new).frame(path, secs, max_side)
    }

    /// The pack design `clip` draws through, if any (TP2a-3): its name and role.
    pub fn clip_design(&self, clip: &assetcraft_storyboard::Clip) -> Option<(String, String)> {
        let designs = self.theme_designs();
        assetcraft_animatic::find_design(&designs, clip).map(|d| (d.design.name().to_string(), d.role.clone()))
    }

    /// The text fields of the design `clip` draws through that a clip can fill (R-T18): each
    /// field's name, the designer's words and the clip's own, if it fills it. Empty when no
    /// design draws it.
    pub fn design_fields(&self, clip: &assetcraft_storyboard::Clip) -> Vec<(String, String, Option<String>)> {
        let designs = self.theme_designs();
        let Some(d) = assetcraft_animatic::find_design(&designs, clip) else { return Vec::new() };
        d.design
            .text_fields()
            .into_iter()
            .map(|(name, designer)| {
                let own =
                    clip.design_fields.iter().find(|f| f.control.eq_ignore_ascii_case(&name)).map(|f| f.text.clone());
                (name, designer, own)
            })
            .collect()
    }

    /// The Viewer's render job for `clip`'s design (its key names the design and the fill).
    fn design_job(
        &self,
        clip: &assetcraft_storyboard::Clip,
    ) -> Option<(assetcraft_animatic::ThemeDesign, std::sync::Arc<assetcraft_titles::frames::DesignJob>)> {
        use std::hash::{Hash, Hasher};
        let designs = self.theme_designs();
        let d = assetcraft_animatic::find_design(&designs, clip)?;
        let fill = assetcraft_animatic::design_fill_with(&d, clip.text().unwrap_or_default(), &clip.design_fields);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (&d.path, &d.role, d.stamp, format!("{fill:?}")).hash(&mut h);
        let job = std::sync::Arc::new(assetcraft_titles::frames::DesignJob {
            key: h.finish(),
            design: std::sync::Arc::clone(&d.design),
            fill,
        });
        Some((d, job))
    }

    /// Why `clip`'s design couldn't draw in the Viewer, if it failed (it shows the theme's
    /// words instead).
    pub fn design_problem(&self, clip: &assetcraft_storyboard::Clip) -> Option<String> {
        let frames = self.design_frames.get()?;
        // The Viewer draws titles whole (L3): a design that fell back says so on its frames.
        let title = self.title_job(clip).and_then(|job| frames.problem(job.key));
        title.or_else(|| self.design_job(clip).and_then(|(_, job)| frames.failure(job.key)))
    }

    /// `clip`'s pack design frame `elapsed` seconds in (at rest: the hold), `width` pixels wide,
    /// for the Viewer (TP2b); `None` when the clip doesn't draw through a design. Never waits:
    /// rendered on background threads, read ahead (see `assetcraft_titles::frames`).
    pub fn design_frame(&self, clip: &assetcraft_storyboard::Clip, elapsed: f64, width: u32) -> Option<DesignLookup> {
        let (d, job) = self.design_job(clip)?;
        let frames = self.design_frames.get_or_init(assetcraft_titles::frames::DesignFrames::new);
        // Fitted to the text's own length (R-T15), as the export does.
        let shown = clip.text_style.shown_secs(clip.duration.as_secs_f64());
        Some(frames.frame(&job, assetcraft_animatic::design_secs(&d, elapsed, shown), width))
    }

    /// `clip`'s title as a job for the frame service (L3): its key covers everything the
    /// title's pixels follow — its words, style, length and kind, whether a card lies over its
    /// own picture, the board's frame, and the loaded themes (one load, one set). `None` for a
    /// clip without text or no open storyboard.
    fn title_job(&self, clip: &assetcraft_storyboard::Clip) -> Option<std::sync::Arc<assetcraft_animatic::TitleJob>> {
        use std::hash::{Hash, Hasher};
        clip.text_kind()?;
        let frame = self.storyboard()?.frame.clone().unwrap_or_default();
        let designs = self.theme_designs();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (
            clip.text(),
            serde_json::to_string(&clip.text_style).unwrap_or_default(),
            serde_json::to_string(&clip.design_fields).unwrap_or_default(),
            clip.duration,
            clip.shot_type,
            clip.background().is_some(),
            serde_json::to_string(&frame).unwrap_or_default(),
            std::sync::Arc::as_ptr(&designs) as usize,
        )
            .hash(&mut h);
        Some(std::sync::Arc::new(assetcraft_animatic::TitleJob {
            key: h.finish(),
            clip: clip.clone(),
            frame,
            looks: self.user_looks_shared(),
            designs,
            services: std::sync::Arc::clone(&self.services),
        }))
    }

    /// `clip`'s title `elapsed` seconds in (`f64::INFINITY`: at rest), `width` pixels wide, for
    /// the Viewer (L3): drawn by the compositor as the export draws it, alone on a clear
    /// picture to lay over the shot; `None` for a clip without text or no open storyboard.
    /// Never waits: rendered on the background frame service, read ahead while playing.
    pub fn title_frame(&self, clip: &assetcraft_storyboard::Clip, elapsed: f64, width: u32) -> Option<DesignLookup> {
        let job: std::sync::Arc<dyn assetcraft_titles::frames::FrameJob> = self.title_job(clip)?;
        let frames = self.design_frames.get_or_init(assetcraft_titles::frames::DesignFrames::new);
        Some(frames.frame_of(&job, assetcraft_animatic::TitleJob::index_at(elapsed), width))
    }

    /// While playing (P2): decode `seconds` of `path` from `from` ahead of the playhead, on the
    /// idle cores. Cheap to call every frame.
    pub fn video_ahead(&self, path: &std::path::Path, from: f64, seconds: f64, max_side: u32) {
        self.video.get_or_init(assetcraft_media::VideoFrames::new).ahead(path, from, seconds, max_side);
    }

    /// Paused or scrubbing (P2): stop reading ahead.
    pub fn video_stop_ahead(&self) {
        if let Some(v) = self.video.get() {
            v.stop_ahead();
        }
    }

    /// The Strip's zoom (Z1): 1 = 44 pt a second, from [`STRIP_ZOOM_RANGE`].
    pub fn strip_zoom(&self) -> f32 {
        self.strip_zoom
    }

    /// The frame new storyboards start with (`storyboard.defaultFrame`; F4).
    pub fn default_frame(&self) -> assetcraft_storyboard::FrameSpec {
        self.default_frame.clone().unwrap_or_default()
    }

    /// Whether the Viewer draws safe-zone guides (`storyboard.safeZones`; remembered).
    pub fn safe_zones_on(&self) -> bool {
        self.safe_zones
    }

    /// Whether the cut tools snap (`storyboard.snapping`; remembered).
    pub fn snapping_on(&self) -> bool {
        self.snapping
    }

    /// Set the Start screen's credits checkbox (remembered in prefs).
    pub fn set_credits_on(&mut self, on: bool) {
        self.credits = on;
    }

    /// The services this session reads through (the thumbnail pool shares them).
    pub fn services(&self) -> Arc<dyn Services> {
        Arc::clone(&self.services)
    }

    // ---- internals used by commands --------------------------------------------------------

    /// Re-read volumes, known folders and installed sibling apps.
    fn rescan_places(&mut self) {
        self.volumes = self.services.volumes();
        self.sidebar = self.services.sidebar();
        self.known = KnownFolder::ALL.iter().filter_map(|k| self.services.known_folder(*k).map(|p| (*k, p))).collect();
        self.recheck_working_folder();
        self.rescan_apps();
    }

    fn rescan_apps(&mut self) {
        let locator = &self.app_locator;
        self.apps = SIBLING_APPS.iter().filter_map(|a| locator.locate(a).map(|p| (a.id, p))).collect();
    }

    /// Navigate, recording history. On failure nothing changes.
    fn go(&mut self, target: Location) -> CmdResult {
        if same_location(&target, &self.location) {
            return self.refresh();
        }
        self.load(&target)?;
        let previous = std::mem::replace(&mut self.location, target);
        self.back.push(previous);
        if self.back.len() > MAX_HISTORY {
            self.back.remove(0);
        }
        self.forward.clear();
        Ok(self.location_json())
    }

    /// Re-list the current location, keeping the selection where items still exist.
    fn refresh(&mut self) -> CmdResult {
        self.rescan_places();
        // F5 always re-arms the folder watch: notify on Windows drops a watch silently when the
        // watched folder is deleted (or its drive goes away) while the status still says Live.
        self.watched = None;
        let loc = self.location.clone();
        let keep = self.selection.clone();
        self.load(&loc)?;
        self.selection = keep;
        self.selection.retain_existing(&self.item_paths);
        Ok(self.location_json())
    }

    /// List `loc` into `items` (the caller sets `location`). Clears selection and probe cache,
    /// and points the folder watcher at it.
    fn load(&mut self, loc: &Location) -> Result<(), CmdError> {
        let (items, truncated, skipped) = self.list(loc)?;
        if let Location::Folder(path) = loc {
            self.note_recent(path);
        }
        self.item_paths = items.iter().map(|e| e.path.clone()).collect();
        self.items = items;
        self.truncated = truncated;
        self.skipped = skipped;
        self.selection.clear();
        self.probes.clear();
        if let Some(pool) = &self.probe_pool {
            pool.clear();
        }
        self.preview_open = false;
        self.retarget_watch(loc);
        Ok(())
    }

    /// Entries of `loc`, filtered and sorted for display, with the truncated / skipped counts.
    fn list(&self, loc: &Location) -> Result<(Vec<Entry>, bool, usize), CmdError> {
        let (mut items, truncated, skipped) = match loc {
            Location::Computer => {
                let items = self
                    .volumes
                    .clone()
                    .into_iter()
                    .map(|v| Entry {
                        name: v.name,
                        path: v.path,
                        kind: EntryKind::Dir,
                        size: 0,
                        modified: None,
                        created: None,
                        hidden: false,
                    })
                    .collect();
                (items, false, 0)
            }
            Location::Folder(path) => {
                let opts = ListOptions { include_hidden: self.view.show_hidden, max_entries: MAX_ITEMS };
                let listing = self.services.list_entries(path, &opts)?;
                (listing.entries, listing.truncated, listing.skipped)
            }
        };
        if matches!(loc, Location::Folder(_)) {
            sort::sort_entries(&mut items, self.view.sort, self.view.ascending);
        }
        Ok((items, truncated, skipped))
    }

    fn resort(&mut self) {
        sort::sort_entries(&mut self.items, self.view.sort, self.view.ascending);
        self.item_paths = self.items.iter().map(|e| e.path.clone()).collect();
    }

    fn location_json(&self) -> Value {
        serde_json::json!({
            "location": match &self.location { Location::Computer => Value::Null, Location::Folder(p) => p.display().to_string().into() },
            "label": self.location.label(),
            "items": self.items.len(),
            "truncated": self.truncated,
        })
    }
}

/// Same place, comparing folder paths with [`same_path`] (case-insensitive on Windows).
fn same_location(a: &Location, b: &Location) -> bool {
    match (a, b) {
        (Location::Computer, Location::Computer) => true,
        (Location::Folder(a), Location::Folder(b)) => same_path(a, b),
        _ => false,
    }
}

#[cfg(test)]
mod analyse_job_tests;
#[cfg(test)]
mod audio_tests;
#[cfg(test)]
mod document_tests;
#[cfg(test)]
mod enabled_tests;
#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod frame_tests;
#[cfg(test)]
mod generate_tests;
#[cfg(test)]
mod inset_tests;
#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod motion_tests;
#[cfg(test)]
mod open_in_tests;
#[cfg(test)]
mod otio_out_tests;
#[cfg(test)]
mod places_tests;
#[cfg(test)]
mod rename_tests;
#[cfg(test)]
mod resize_tests;
#[cfg(test)]
mod roll_tests;
#[cfg(test)]
mod slip_tests;
#[cfg(test)]
mod storyboard_tests;
#[cfg(test)]
mod template_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_tests;
#[cfg(test)]
mod theme_tests;
#[cfg(test)]
mod title_render_tests;
#[cfg(test)]
mod track_tests;
#[cfg(test)]
mod working_cmd_tests;
