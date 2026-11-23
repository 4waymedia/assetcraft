//! Rendered titles for FilmCraft (SF1b-1; D-SF7: every title and card goes to FilmCraft as a
//! finished `.mp4`, an overlay merged with its shot).
//!
//! - `storyboard.renderTitles {dryRun?, overwrite?, wait?}`. With `dryRun` (SF1b-1a), the plan.
//!   For every clip with text, in Cut order: what it is (a card, or an overlay over its shot),
//!   who draws it, how many frames at the board's size and rate, and the file it goes to,
//!   `<document folder>/<name>.media/titles/c<clip>-<hash>.mp4`. The hash ([`title_hash`])
//!   covers everything its pixels follow, so a file already there is reused and a changed title
//!   gets a new name. Then the completeness check (Paul, 2026-11-21: "nothing goes missing"):
//!   anything that would draw short of what was designed is a problem, and any problem means
//!   the send stops with the list.
//! - Without `dryRun` (SF1b-1c), and with no problem, every title not already there (every one
//!   with `overwrite`) is rendered on the export thread: its frames exactly as the Cut draws
//!   them over the clip (less a section's inset, which FilmCraft gets as its own clip), no
//!   sound (the song is A1), H.264 at [`TITLE_QP`] in an `.mp4`, each written atomically.
//!   `storyboard.exportStatus` / `storyboard.exportCancel` follow and stop it; `wait` blocks.
//!
//! Checks: a design the clip asks for didn't load (a built-in pick, or its pack's design for the
//! role), a theme the clip names isn't loaded, a picture or video the title shows is missing
//! (SF1b-1a); and inside the design as it will be filled (SF1b-1b, `assetcraft_titles::Design::
//! gaps`): an effect the renderer doesn't know, a font that isn't there, footage the renderer
//! isn't given (an unfilled Media field, or a file of the designer's). A dry run writes nothing.

use crate::export_job::{ExportProgress, ExportStage, lock, write_atomic};
use crate::{CmdError, CmdResult, Session};
use assetcraft_animatic::Compositor;
use assetcraft_storyboard::{BuiltEffect, Clip, EffectKind, Renderer, Storyboard, TICKS_PER_SECOND, TextKind, Ticks};
use assetcraft_video::{Container, Settings, VideoEncoder};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Bumped when AssetCraft draws titles differently, so every file is rendered again.
pub const RENDER_VERSION: u32 = 1;
/// The folder beside the document that holds its rendered media.
pub const MEDIA_SUFFIX: &str = ".media";
/// The rendered titles' folder inside it.
pub const TITLES_DIR: &str = "titles";

/// One title to render (or reuse).
#[derive(Debug, Clone, PartialEq)]
pub struct TitlePlan {
    /// The clip.
    pub clip: u32,
    /// Its place in the Cut, from 0.
    pub index: usize,
    /// A card on its own, or an overlay merged with its shot.
    pub kind: TextKind,
    /// Who draws it.
    pub renderer: Renderer,
    /// Its frames at the board's rate.
    pub frames: u64,
    /// Its length.
    pub seconds: f64,
    /// [`title_hash`], as 16 hex digits.
    pub hash: String,
    /// The file.
    pub path: PathBuf,
    /// The file is already there: nothing to render.
    pub reuse: bool,
}

/// Something that would make a title draw short of its design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleProblem {
    /// The clip.
    pub clip: u32,
    /// Its place in the Cut, from 0.
    pub index: usize,
    /// `design`, `theme`, `media`, `effect` or `font`.
    pub what: &'static str,
    /// What to tell the user.
    pub why: String,
}

/// The whole plan.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderPlan {
    /// `<name>.media/titles`.
    pub folder: PathBuf,
    /// Frame size and rate.
    pub width: u32,
    /// Frame height.
    pub height: u32,
    /// Frames a second.
    pub fps: u32,
    /// Every title, in Cut order.
    pub titles: Vec<TitlePlan>,
    /// Every problem, in Cut order. Not empty: the send stops.
    pub problems: Vec<TitleProblem>,
}

impl RenderPlan {
    fn json(&self) -> Value {
        let render = self.titles.iter().filter(|t| !t.reuse).count();
        let reuse = self.titles.len() - render;
        let ready = self.problems.is_empty();
        let message = if !ready {
            format!("{} problem(s) stop the render: {}", self.problems.len(), self.problems[0].why)
        } else if self.titles.is_empty() {
            "No titles to render".to_string()
        } else {
            format!("Would render {render} title(s) and reuse {reuse}")
        };
        json!({
            "folder": self.folder.to_string_lossy(),
            "width": self.width,
            "height": self.height,
            "fps": self.fps,
            "titles": self.titles.iter().map(|t| json!({
                "clip": t.clip,
                "index": t.index,
                "kind": kind_name(t.kind),
                "renderer": match t.renderer { Renderer::InApp => "inApp", Renderer::EffectCraft => "effectCraft" },
                "frames": t.frames,
                "seconds": t.seconds,
                "hash": t.hash,
                "path": t.path.to_string_lossy(),
                "reuse": t.reuse,
            })).collect::<Vec<_>>(),
            "problems": self.problems.iter().map(|p| json!({
                "clip": p.clip, "index": p.index, "what": p.what, "why": p.why,
            })).collect::<Vec<_>>(),
            "ready": ready,
            "render": render,
            "reuse": reuse,
            "message": message,
        })
    }
}

fn kind_name(kind: TextKind) -> &'static str {
    match kind {
        TextKind::Card => "card",
        TextKind::Overlay => "overlay",
    }
}

/// `<folder>/<name>.media/titles` for the document at `document`.
pub fn titles_folder(document: &Path) -> Option<PathBuf> {
    let dir = document.parent()?;
    let name = crate::document_cmd::document_name(document);
    (!name.is_empty()).then(|| dir.join(format!("{name}{MEDIA_SUFFIX}")).join(TITLES_DIR))
}

/// Frames `t` lasts at `fps`, rounded to the nearest frame.
fn frames_of(t: Ticks, fps: u32) -> u64 {
    let n = u128::from(t.0) * u128::from(fps) + u128::from(TICKS_PER_SECOND / 2);
    u64::try_from(n / u128::from(TICKS_PER_SECOND)).unwrap_or(u64::MAX)
}

/// FNV-1a, 64 bits: stable across builds and platforms, so a file name means the same title on
/// every run (`DefaultHasher` makes no such promise).
#[derive(Debug, Clone, Copy)]
struct Fnv(u64);

impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    /// Add one named part; the length goes first so parts can't run into each other.
    fn part(&mut self, bytes: &[u8]) {
        for b in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

/// The hash of everything `clip`'s rendered pixels follow: the renderer's version, the board's
/// frame, the clip's words, style, design fields, length, shot type, in-point and motion, the
/// look its theme gives it, the design that draws it (its file and the stamp of its bytes, or
/// the built-in's id), and each picture or video it shows (its path, size and modified time, so a
/// replaced file renders again).
pub fn title_hash(s: &Session, clip: &Clip, frame: &assetcraft_storyboard::FrameSpec) -> u64 {
    let mut h = Fnv::new();
    h.part(&RENDER_VERSION.to_le_bytes());
    h.part(serde_json::to_string(frame).unwrap_or_default().as_bytes());
    h.part(clip.text().unwrap_or_default().as_bytes());
    h.part(serde_json::to_string(&clip.text_style).unwrap_or_default().as_bytes());
    h.part(serde_json::to_string(&clip.design_fields).unwrap_or_default().as_bytes());
    h.part(&clip.duration.0.to_le_bytes());
    h.part(clip.shot_type.code().as_bytes());
    h.part(&clip.source_in.0.to_le_bytes());
    h.part(serde_json::to_string(&clip.motion).unwrap_or_default().as_bytes());
    let designs = s.theme_designs();
    let title = assetcraft_animatic::title_of(&designs, s.user_looks(), clip);
    h.part(format!("{:?}", title.as_ref().map(|t| &t.look)).as_bytes());
    match title.as_ref().and_then(|t| t.effects.first()).map(|e| &e.kind) {
        Some(EffectKind::Design { .. }) => {
            if let Some(d) = assetcraft_animatic::find_design(&designs, clip) {
                h.part(d.path.to_string_lossy().as_bytes());
                h.part(&d.stamp.to_le_bytes());
            }
        }
        Some(EffectKind::Built { effect }) => h.part(effect.id().as_bytes()),
        _ => h.part(b"-"),
    }
    for asset in shown_assets(clip) {
        h.part(asset.as_bytes());
        match s.services.entry(Path::new(asset)) {
            Ok(e) => {
                h.part(&e.size.to_le_bytes());
                let secs =
                    e.modified.and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
                h.part(&secs.to_le_bytes());
            }
            Err(_) => h.part(b"missing"),
        }
    }
    h.0
}

/// The pictures and videos a title shows: an overlay's shot (merged under it), a card's own
/// background picture. A plain card shows none.
fn shown_assets(clip: &Clip) -> Vec<&str> {
    match clip.text_kind() {
        Some(TextKind::Overlay) => clip.asset_ids.first().map(|a| a.0.as_str()).into_iter().collect(),
        Some(TextKind::Card) => clip.background().map(|a| a.0.as_str()).into_iter().collect(),
        None => Vec::new(),
    }
}

/// What would make `clip`'s title draw short of its design (this step's checks; see the module
/// docs).
fn check(s: &Session, clip: &Clip, index: usize) -> Vec<TitleProblem> {
    let mut out = Vec::new();
    let mut problem = |what, why: String| out.push(TitleProblem { clip: clip.id.0, index, what, why });
    let Some(kind) = clip.text_kind() else { return Vec::new() };
    let role = assetcraft_storyboard::design_role(kind);
    let designs = s.theme_designs();
    let style = &clip.text_style;
    let theme = style.theme_id.map(|t| t.as_str().to_string());
    // The theme it names is loaded (a built-in theme or one of the user's).
    if let Some(id) = &theme
        && !designs.fills.iter().any(|(f, _)| f == id)
    {
        problem("theme", format!("theme “{id}” isn't loaded (Themes › Import, or pick another)"));
    }
    // What was designed: the title as it would be if every design loaded.
    let designed = assetcraft_storyboard::title(clip, s.user_looks(), |_| true);
    match designed.as_ref().and_then(|t| t.effects.first()).map(|e| &e.kind) {
        Some(EffectKind::Built { effect }) => {
            if !designs.builtins.iter().any(|(id, _)| id == effect.id()) {
                problem("design", format!("the built-in design “{}” didn't load", built_name(*effect)));
            }
        }
        Some(EffectKind::Design { theme: t, role: r }) => {
            // A pack that has no design for this role draws the theme's words: as designed.
            let declared = s
                .loaded_themes()
                .iter()
                .find(|u| u.theme.id == t.as_str())
                .is_some_and(|u| u.theme.designs.contains_key(*r));
            let loaded = designs.packs.iter().any(|d| d.theme == *t && d.role == *r);
            if declared && !loaded {
                problem("design", format!("the {role} design of theme “{}” didn't open", t.as_str()));
            }
        }
        _ => {}
    }
    // Inside the design that draws it, as it will be filled (SF1b-1b): effects, fonts, media.
    if let Some(d) = assetcraft_animatic::find_design(&designs, clip) {
        let fill = assetcraft_animatic::design_fill_with(&d, clip.text().unwrap_or_default(), &clip.design_fields);
        for gap in d.design.gaps(&fill) {
            problem(gap.kind.name(), format!("{}: {gap}", d.design.name()));
        }
    }
    for asset in shown_assets(clip) {
        let ok = s.services.entry(Path::new(asset)).is_ok_and(|e| e.size > 0);
        if !ok {
            let what = if kind == TextKind::Overlay { "its shot" } else { "its background picture" };
            problem("media", format!("{what} isn't there: {asset}"));
        }
    }
    out
}

fn built_name(effect: BuiltEffect) -> &'static str {
    match effect {
        BuiltEffect::LaserTrace => "Laser Trace",
        BuiltEffect::Stack => "Stack",
    }
}

/// The plan for the open, saved storyboard.
pub fn plan(s: &Session) -> Result<RenderPlan, CmdError> {
    let board = s.boards.board.as_ref().ok_or_else(crate::storyboard::no_board)?;
    let document = s.document().ok_or_else(|| {
        CmdError::Unavailable("save the storyboard first: its titles go in a folder beside it".into())
    })?;
    let folder = titles_folder(document)
        .ok_or_else(|| CmdError::Unavailable(format!("{} has no folder to render beside", document.display())))?;
    let frame = board.frame_spec();
    let designs = s.theme_designs();
    let mut titles = Vec::new();
    let mut problems = Vec::new();
    for (index, clip) in board.clips.iter().enumerate() {
        let Some(kind) = clip.text_kind() else { continue };
        let renderer =
            assetcraft_animatic::title_of(&designs, s.user_looks(), clip).map_or(Renderer::InApp, |t| t.renderer());
        let hash = format!("{:016x}", title_hash(s, clip, &frame));
        let path = folder.join(format!("c{}-{hash}.mp4", clip.id.0));
        let reuse = s.services.entry(&path).is_ok_and(|e| e.size > 0);
        titles.push(TitlePlan {
            clip: clip.id.0,
            index,
            kind,
            renderer,
            frames: frames_of(clip.duration, frame.fps),
            seconds: clip.duration.as_secs_f64(),
            hash,
            path,
            reuse,
        });
        problems.extend(check(s, clip, index));
    }
    Ok(RenderPlan { folder, width: frame.width, height: frame.height, fps: frame.fps, titles, problems })
}

/// `storyboard.renderTitles {dryRun?, overwrite?, wait?}`: the plan with `dryRun`; otherwise,
/// when nothing stops it, every title not already rendered (all of them with `overwrite`) is
/// rendered on the export thread. Progress and cancel are `storyboard.exportStatus` /
/// `storyboard.exportCancel`, as for the Cut; `wait` blocks until it ends.
pub(crate) fn sb_render_titles(s: &mut Session, p: &Value) -> CmdResult {
    let plan = plan(s)?;
    let flag = |name: &str| p.get(name).and_then(Value::as_bool).unwrap_or(false);
    if flag("dryRun") {
        let mut out = plan.json();
        if let Value::Object(map) = &mut out {
            map.insert("dryRun".into(), json!(true));
        }
        return Ok(out);
    }
    if !plan.problems.is_empty() {
        let list: Vec<String> =
            plan.problems.iter().map(|x| format!("clip {} ({}): {}", x.clip, x.what, x.why)).collect();
        return Err(CmdError::Unavailable(format!(
            "{} problem(s) stop the render, so nothing would go missing: {}",
            list.len(),
            list.join("; ")
        )));
    }
    if s.exporter.running() {
        return Err(CmdError::Unavailable("an export is already running".into()));
    }
    let Some(board) = s.boards.board.as_ref() else { return Err(crate::storyboard::no_board()) };
    let overwrite = flag("overwrite");
    let todo: Vec<TitlePlan> = plan.titles.iter().filter(|t| overwrite || !t.reuse).cloned().collect();
    let job = TitleJob {
        board: drawn_board(board),
        settings: Settings { width: plan.width, height: plan.height, fps: plan.fps, qp: TITLE_QP },
        todo,
        services: Arc::clone(&s.services),
        look: s.model.render_look(),
        decode: s.decode_options(),
        user_looks: s.user_looks().clone(),
        designs: s.theme_designs(),
        progress: Arc::clone(&s.exporter.progress),
        cancel: Arc::clone(&s.exporter.cancel),
    };
    let frames: u64 = job.todo.iter().map(|t| t.frames).sum();
    s.exporter.cancel.store(false, Ordering::SeqCst);
    *lock(&s.exporter.progress) = ExportProgress {
        stage: ExportStage::Rendering,
        frames_total: usize::try_from(frames).unwrap_or(usize::MAX),
        files_total: job.todo.len(),
        path: job.todo.first().map(|t| t.path.clone()),
        message: format!("Rendering {} title(s)…", job.todo.len()),
        ..ExportProgress::default()
    };
    if let Some(old) = s.exporter.thread.take() {
        let _ = old.join();
    }
    match std::thread::Builder::new().name("assetcraft-titles".into()).spawn(move || job.run()) {
        Ok(h) => s.exporter.thread = Some(h),
        Err(e) => {
            let mut pr = lock(&s.exporter.progress);
            pr.stage = ExportStage::Failed;
            pr.message = format!("could not start rendering titles: {e}");
            return Err(CmdError::Unavailable(pr.message.clone()));
        }
    }
    if flag("wait")
        && let Some(h) = s.exporter.thread.take()
    {
        let _ = h.join();
    }
    let progress = s.exporter.progress();
    let mut out = plan.json();
    if let Value::Object(map) = &mut out {
        map.insert("started".into(), json!(true));
        map.insert("status".into(), serde_json::to_value(&progress).unwrap_or(Value::Null));
        map.insert("message".into(), json!(progress.message));
    }
    if progress.stage == ExportStage::Failed {
        return Err(CmdError::Unavailable(progress.message));
    }
    Ok(out)
}

/// The quantizer titles are encoded at: near the best of `QUALITY_RANGE`, since FilmCraft
/// encodes them again in its own export.
pub const TITLE_QP: u8 = 18;

/// The board as a title's frames are drawn from: the Cut as it is, less each section's inset
/// (FilmCraft gets the inset as its own clip from the OTIO, SF1b-2, so it isn't baked into the
/// title under it).
fn drawn_board(board: &Storyboard) -> Storyboard {
    let mut b = board.clone();
    for scene in &mut b.scenes {
        scene.inset = None;
    }
    b
}

/// Everything the title thread needs; the session is not touched.
struct TitleJob {
    board: Storyboard,
    settings: Settings,
    todo: Vec<TitlePlan>,
    services: Arc<dyn assetcraft_fs::Services>,
    look: assetcraft_mesh::RenderOptions,
    decode: assetcraft_thumbs::DecodeOptions,
    user_looks: assetcraft_storyboard::UserLooks,
    designs: Arc<assetcraft_animatic::DesignSet>,
    progress: Arc<Mutex<ExportProgress>>,
    cancel: Arc<AtomicBool>,
}

impl TitleJob {
    fn run(self) {
        let started = std::time::Instant::now();
        let outcome = self.render(started);
        let mut pr = lock(&self.progress);
        pr.elapsed_secs = started.elapsed().as_secs_f64();
        match outcome {
            Ok(bytes) => {
                pr.stage = ExportStage::Done;
                pr.bytes = Some(bytes);
                pr.frames_done = pr.frames_total;
                pr.files_done = pr.files_total;
                pr.message = format!("Rendered {} title(s) ({:.1} MiB)", self.todo.len(), bytes as f64 / 1_048_576.0);
            }
            Err(None) => {
                pr.stage = ExportStage::Cancelled;
                pr.message = format!("Rendering titles cancelled after {} of {}", pr.files_done, pr.files_total);
            }
            Err(Some(why)) => {
                pr.stage = ExportStage::Failed;
                pr.message = why;
            }
        }
    }

    /// Each title in turn: its frames from the Cut through a fresh encoder, then its file
    /// written atomically (a cancel or a failure never leaves a half file under a real name, and
    /// files already finished stay). `Err(None)` is a cancel.
    fn render(&self, started: std::time::Instant) -> Result<u64, Option<String>> {
        let s = self.settings;
        let mut compositor = Compositor::new(Arc::clone(&self.services), s.width, s.height, self.look, self.decode)
            .map_err(|e| Some(e.to_string()))?
            .with_crop(self.board.frame.as_ref())
            .with_user_looks(self.user_looks.clone())
            .with_designs(Arc::clone(&self.designs));
        let tick = Ticks(TICKS_PER_SECOND / u64::from(s.fps.max(1)));
        let starts = self.board.starts();
        let mut bytes = 0u64;
        let mut done = 0usize;
        for title in &self.todo {
            let (Some(clip), Some(start)) = (self.board.clips.get(title.index), starts.get(title.index).copied())
            else {
                return Err(Some(format!("clip {} is no longer on the board", title.clip)));
            };
            // The last tick inside the clip: a rounded-up last frame stays on this title.
            let last = Ticks(start.0.saturating_add(clip.duration.0.saturating_sub(1)));
            lock(&self.progress).path = Some(title.path.clone());
            let mut encoder = VideoEncoder::new(s).map_err(|e| Some(e.to_string()))?;
            for i in 0..title.frames {
                if self.cancel.load(Ordering::SeqCst) {
                    return Err(None);
                }
                let at = Ticks(start.0.saturating_add(tick.0.saturating_mul(i)).min(last.0));
                let frame = compositor.frame(&self.board, at);
                encoder.push_rgba(&frame.data).map_err(|e| Some(e.to_string()))?;
                let mut pr = lock(&self.progress);
                done = done.saturating_add(1);
                pr.frames_done = done;
                pr.elapsed_secs = started.elapsed().as_secs_f64();
            }
            let video = encoder.finish().map_err(|e| Some(e.to_string()))?;
            if self.cancel.load(Ordering::SeqCst) {
                return Err(None);
            }
            bytes += write_atomic(&title.path, |out| assetcraft_video::mux(&video, None, Container::Mp4, out))
                .map_err(Some)?;
            lock(&self.progress).files_done += 1;
        }
        Ok(bytes)
    }
}
