//! Storyboard mode in the session: the open storyboard, the selected clip, and undo.
//!
//! Stage 4 step 2 (`docs/storyboard-plan.md`). One storyboard is open at a time and lives in
//! memory; saving comes with step 7 (decision D3: AssetCraft's own files only). Every edit
//! pushes the previous board onto a bounded undo stack, so `storyboard.undo` restores it whole.
//! The rules themselves (Quick fill, Re-cut, limits) live in `assetcraft-storyboard`.

use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{
    AssetId, Bpm, Clip, ClipId, Limits, RecutOptions, Scene, SceneId, ShotType, Storyboard, Ticks, Track, next_empty,
    place, recut, sample,
};
use serde_json::{Value, json};

/// Most undo steps kept.
pub const MAX_UNDO: usize = 100;

/// Which mode the window shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// Browse, preview and bulk-edit assets.
    #[default]
    Organizer,
    /// Build a Cut.
    Storyboard,
}

impl Mode {
    /// Name used in commands and `app.state`.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Organizer => "organizer",
            Mode::Storyboard => "storyboard",
        }
    }
}

/// Storyboard state held by the session.
#[derive(Debug, Default)]
pub struct Boards {
    mode: Mode,
    board: Option<Storyboard>,
    selected: Option<ClipId>,
    undo: Vec<(Option<Storyboard>, Option<ClipId>)>,
    redo: Vec<(Option<Storyboard>, Option<ClipId>)>,
}

impl Boards {
    /// Record the current state before an edit (clears redo).
    fn checkpoint(&mut self) {
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push((self.board.clone(), self.selected));
        self.redo.clear();
    }
}

fn no_board() -> CmdError {
    CmdError::Unavailable("no storyboard is open (File → New Storyboard)".into())
}

impl Session {
    /// The mode the window shows.
    pub fn mode(&self) -> Mode {
        self.boards.mode
    }

    /// The open storyboard.
    pub fn storyboard(&self) -> Option<&Storyboard> {
        self.boards.board.as_ref()
    }

    /// The selected clip (shared by every storyboard view).
    pub fn selected_clip(&self) -> Option<ClipId> {
        self.boards.selected
    }

    /// Whether `storyboard.undo` has anything to undo.
    pub fn can_undo_storyboard(&self) -> bool {
        !self.boards.undo.is_empty()
    }

    /// Whether `storyboard.redo` has anything to redo.
    pub fn can_redo_storyboard(&self) -> bool {
        !self.boards.redo.is_empty()
    }
}

// ---- commands ------------------------------------------------------------------------------

pub(crate) fn app_mode(s: &mut Session, p: &Value) -> CmdResult {
    let mode = match p.get("mode").and_then(Value::as_str) {
        Some("organizer") => Mode::Organizer,
        Some("storyboard") => Mode::Storyboard,
        _ => return Err(CmdError::BadParams("need `mode`: organizer|storyboard".into())),
    };
    s.boards.mode = mode;
    Ok(json!({ "mode": mode.as_str() }))
}

pub(crate) fn sb_new(s: &mut Session, p: &Value) -> CmdResult {
    let from_sample = p.get("sample").and_then(Value::as_bool).unwrap_or(false);
    let mut board = if from_sample { sample() } else { manual_start() };
    if let Some(name) = p.get("name").and_then(Value::as_str) {
        let name = name.trim();
        if name.is_empty() || name.len() > Limits::default().max_string {
            return Err(CmdError::BadParams("`name` must be 1–8192 bytes".into()));
        }
        board.name = name.to_owned();
    }
    s.boards.checkpoint();
    s.boards.selected = next_empty(&board).or_else(|| board.clips.first().map(|c| c.id));
    s.boards.board = Some(board);
    s.boards.mode = Mode::Storyboard;
    Ok(state(s))
}

/// The Manual start (design 3e): one scene with a placeholder title card and no track.
fn manual_start() -> Storyboard {
    let mut board = Storyboard::new("Untitled storyboard");
    board.scenes.push(Scene { id: SceneId(1), song_section: None });
    let len = Ticks::from_secs(2).unwrap_or(Ticks::ZERO);
    let mut card = Clip::new(ClipId(1), SceneId(1), ShotType::Ttl, len);
    card.description = "Untitled scene".into();
    board.clips.push(card);
    board
}

pub(crate) fn sb_select(s: &mut Session, p: &Value) -> CmdResult {
    let board = s.boards.board.as_ref().ok_or_else(no_board)?;
    let clip = match p.get("clip") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let id = clip_id(v)?;
            if board.clip(id).is_none() {
                return Err(CmdError::BadParams(format!("no clip #{}", id.0)));
            }
            Some(id)
        }
    };
    s.boards.selected = clip;
    Ok(json!({ "selected": clip.map(|c| c.0) }))
}

pub(crate) fn sb_place(s: &mut Session, p: &Value) -> CmdResult {
    let asset = p
        .get("asset")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| CmdError::BadParams("need `asset` (path)".into()))?;
    if asset.len() > Limits::default().max_string {
        return Err(CmdError::BadParams("`asset` is too long".into()));
    }
    let target = match p.get("clip") {
        None | Some(Value::Null) => None,
        Some(v) => Some(clip_id(v)?),
    };
    // Try on a copy so a refused placement leaves no undo step behind.
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let placed =
        place(&mut board, AssetId(asset.to_owned()), target).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    s.boards.checkpoint();
    s.boards.board = Some(board);
    let next = s.boards.board.as_ref().and_then(next_empty);
    s.boards.selected = next.or(Some(placed));
    let (filled, fillable) = s.boards.board.as_ref().map(Storyboard::fill_progress).unwrap_or((0, 0));
    Ok(json!({
        "placed": placed.0,
        "next": next.map(|c| c.0),
        "filled": filled,
        "fillable": fillable,
        "message": format!("Placed in clip #{} · {filled} / {fillable} filled", placed.0),
    }))
}

pub(crate) fn sb_recut(s: &mut Session, p: &Value) -> CmdResult {
    let board = s.boards.board.as_ref().ok_or_else(no_board)?;
    let track = board.track.as_ref().ok_or_else(|| CmdError::Unavailable("add a track first".into()))?;
    let bpm = match p.get("bpm") {
        None => track.bpm,
        Some(v) => v.as_f64().and_then(Bpm::new).ok_or_else(|| CmdError::BadParams("`bpm` must be 20–400".into()))?,
    };
    let len = match p.get("seconds") {
        None => track.duration,
        Some(v) => v
            .as_f64()
            .filter(|s| *s > 0.0)
            .and_then(Ticks::from_secs_f64)
            .filter(|t| *t <= Limits::default().max_duration)
            .ok_or_else(|| CmdError::BadParams("`seconds` must be a positive length up to 6 hours".into()))?,
    };
    let keep_title_cards = p.get("keepTitleCards").and_then(Value::as_bool).unwrap_or(true);
    let report = recut(board, bpm, len, RecutOptions { keep_title_cards });
    let mut next = board.clone();
    for (clip, d) in next.clips.iter_mut().zip(&report.durations) {
        clip.duration = *d;
    }
    if let Some(t) = next.track.as_mut() {
        *t = Track { bpm, duration: len, ..t.clone() };
    }
    let name = next.track.as_ref().map(|t| t.name.clone()).unwrap_or_default();
    s.boards.checkpoint();
    s.boards.board = Some(next);
    let fit = if report.fits() { String::new() } else { " (the clips don't fit: too many for the track)".into() };
    Ok(json!({
        "resized": report.resized,
        "totalBeats": report.total_beats,
        "availableBeats": report.available_beats,
        "fits": report.fits(),
        "message": format!("Re-cut to {name} · {} BPM: {} clips resized to the beat grid{fit}", bpm.get(), report.resized),
    }))
}

pub(crate) fn sb_undo(s: &mut Session, _: &Value) -> CmdResult {
    let (board, selected) = s.boards.undo.pop().ok_or_else(|| CmdError::Unavailable("nothing to undo".into()))?;
    let current = (std::mem::replace(&mut s.boards.board, board), s.boards.selected);
    s.boards.redo.push(current);
    s.boards.selected = selected;
    Ok(state(s))
}

pub(crate) fn sb_redo(s: &mut Session, _: &Value) -> CmdResult {
    let (board, selected) = s.boards.redo.pop().ok_or_else(|| CmdError::Unavailable("nothing to redo".into()))?;
    let current = (std::mem::replace(&mut s.boards.board, board), s.boards.selected);
    s.boards.undo.push(current);
    s.boards.selected = selected;
    Ok(state(s))
}

pub(crate) fn sb_state(s: &mut Session, _: &Value) -> CmdResult {
    Ok(state(s))
}

/// `storyboard.state`: mode, the board as saved JSON, selection, undo, and the status-bar facts.
pub(crate) fn state(s: &Session) -> Value {
    let b = &s.boards;
    let summary = b.board.as_ref().map(|board| {
        let (filled, fillable) = board.fill_progress();
        json!({
            "clips": board.clips.len(),
            "filled": filled,
            "fillable": fillable,
            "assetsAssigned": board.assets_assigned(),
            "next": next_empty(board).map(|c| c.0),
            "total": board.total().timecode(),
        })
    });
    json!({
        "mode": b.mode.as_str(),
        "board": b.board.as_ref().and_then(|board| serde_json::to_value(board).ok()),
        "summary": summary,
        "selected": b.selected.map(|c| c.0),
        "canUndo": !b.undo.is_empty(),
        "canRedo": !b.redo.is_empty(),
    })
}

fn clip_id(v: &Value) -> Result<ClipId, CmdError> {
    v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .map(ClipId)
        .ok_or_else(|| CmdError::BadParams("`clip` must be a clip number".into()))
}

#[cfg(test)]
impl Boards {
    pub(crate) fn undo_len(&self) -> usize {
        self.undo.len()
    }
}
