//! Rendered titles for FilmCraft (SF1b-1; D-SF7: every title and card goes to FilmCraft as a
//! finished `.mp4`, an overlay merged with its shot).
//!
//! - `storyboard.renderTitles {dryRun?, overwrite?}`. **SF1b-1a (this step): the plan only.**
//!   For every clip with text, in Cut order: what it is (a card, or an overlay over its shot),
//!   who draws it, how many frames at the board's size and rate, and the file it goes to,
//!   `<document folder>/<name>.media/titles/c<clip>-<hash>.mp4`. The hash ([`title_hash`])
//!   covers everything its pixels follow, so a file already there is reused and a changed title
//!   gets a new name. Then the completeness check (Paul, 2026-11-21: "nothing goes missing"):
//!   anything that would draw short of what was designed is a problem, and any problem means
//!   the send stops with the list. Rendering lands in SF1b-1c; until then the call needs
//!   `dryRun: true`.
//!
//! Checks in this step: a design the clip asks for didn't load (a built-in pick, or its pack's
//! design for the role), a theme the clip names isn't loaded, a picture or video the title shows
//! is missing. SF1b-1b adds the checks inside a design (every effect one the renderer knows,
//! every font resolving, every media slot filled). Nothing is written.

use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{BuiltEffect, Clip, EffectKind, Renderer, TICKS_PER_SECOND, TextKind, Ticks};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

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
    /// `design`, `theme` or `media`.
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

/// `storyboard.renderTitles {dryRun?, overwrite?}` → the plan (SF1b-1a: `dryRun` only).
pub(crate) fn sb_render_titles(s: &mut Session, p: &Value) -> CmdResult {
    let plan = plan(s)?;
    if !p.get("dryRun").and_then(Value::as_bool).unwrap_or(false) {
        return Err(CmdError::Unavailable(
            "rendering titles comes in SF1b-1c; pass `dryRun: true` for the plan and the check".into(),
        ));
    }
    let mut out = plan.json();
    if let Value::Object(map) = &mut out {
        map.insert("dryRun".into(), json!(true));
    }
    Ok(out)
}
