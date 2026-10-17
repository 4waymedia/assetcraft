//! Storyboard mode in the session: the open storyboard, the selected clip, and undo.
//!
//! Stage 4 step 2 (`docs/storyboard-plan.md`). One storyboard is open at a time and lives in
//! memory; saving comes with step 7 (decision D3: AssetCraft's own files only). Every edit
//! pushes the previous board onto a bounded undo stack, so `storyboard.undo` restores it whole.
//! The rules themselves (Quick fill, Re-cut, limits) live in `assetcraft-storyboard`.

use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{
    AssetId, Bpm, Clip, ClipId, Deleted, Limits, RecutOptions, Scene, SceneId, ShotType, Storyboard, Ticks, Track,
    auto_fill, clear, delete, next_empty, place, recut, sample,
};
use serde_json::{Value, json};

/// Most undo steps kept.
pub const MAX_UNDO: usize = 100;

/// Most assets `storyboard.autofill` takes.
pub const MAX_AUTOFILL_ASSETS: usize = 10_000;

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

/// Which Shot map view Storyboard mode shows (design 1a and 2b; Scenes and Tracks come later).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BoardView {
    /// Page-map tiles (1a).
    #[default]
    Map,
    /// Editor layout: Content panel, Viewer, Inspector, full-width timeline (2b).
    Strip,
    /// Quick fill (4a): click assets into empty clips in order, then Watch your Cut.
    Fill,
}

impl BoardView {
    /// Name used in commands and `storyboard.state`.
    pub fn as_str(self) -> &'static str {
        match self {
            BoardView::Map => "map",
            BoardView::Strip => "strip",
            BoardView::Fill => "fill",
        }
    }
}

/// Watch your Cut: whether the Cut is playing and where it was started or paused.
///
/// The engine records play, pause and seek (every user action is a command); while playing,
/// the window advances the playhead from `position` (on the audio clock while the track plays,
/// [`Session::audio_clock`], else its own) and reports where it stopped through
/// `storyboard.pause {at}`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Transport {
    /// Playing now.
    pub playing: bool,
    /// Where playback started (when playing) or the paused playhead (when not).
    pub position: Ticks,
}

/// What an undo step restores: the board, the selected clip, and what the board was generated from.
pub(crate) type Snapshot = (Option<Storyboard>, Option<ClipId>, Option<crate::generate_cmd::GeneratedFrom>);

/// Storyboard state held by the session.
#[derive(Debug, Default)]
pub struct Boards {
    pub(crate) mode: Mode,
    pub(crate) view: BoardView,
    pub(crate) transport: Transport,
    pub(crate) board: Option<Storyboard>,
    pub(crate) selected: Option<ClipId>,
    /// Shift+click range: `(anchor, end)`. It holds only while `selected == Some(end)`, so any
    /// command that moves the selection on its own ends the range without having to know of it.
    pub(crate) range: Option<(ClipId, ClipId)>,
    pub(crate) undo: Vec<Snapshot>,
    pub(crate) redo: Vec<Snapshot>,
    /// What the open storyboard was generated from (`storyboard.generate`).
    pub(crate) generated: Option<crate::generate_cmd::GeneratedFrom>,
}

impl Boards {
    /// Record the current state before an edit (clears redo).
    pub(crate) fn checkpoint(&mut self) {
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push((self.board.clone(), self.selected, self.generated.clone()));
        self.redo.clear();
    }
}

pub(crate) fn no_board() -> CmdError {
    CmdError::Unavailable("no storyboard is open (File → New Storyboard)".into())
}

impl Session {
    /// The mode the window shows.
    pub fn mode(&self) -> Mode {
        self.boards.mode
    }

    /// The Shot map view Storyboard mode shows.
    pub fn board_view(&self) -> BoardView {
        self.boards.view
    }

    /// Watch your Cut transport.
    pub fn transport(&self) -> Transport {
        self.boards.transport
    }

    /// The open storyboard.
    pub fn storyboard(&self) -> Option<&Storyboard> {
        self.boards.board.as_ref()
    }

    /// The selected clip (shared by every storyboard view).
    pub fn selected_clip(&self) -> Option<ClipId> {
        self.boards.selected
    }

    /// The selected clips in Cut order: the Shift+click range, else the selected clip, else none.
    pub fn selected_clips(&self) -> Vec<ClipId> {
        let Some(board) = self.boards.board.as_ref() else { return Vec::new() };
        let Some(focus) = self.boards.selected.filter(|id| board.clip(*id).is_some()) else { return Vec::new() };
        let span = self.boards.range.filter(|(_, end)| *end == focus).and_then(|(anchor, end)| {
            let (a, b) = (board.index_of(anchor)?, board.index_of(end)?);
            Some((a.min(b), a.max(b)))
        });
        match span {
            Some((lo, hi)) => board.clips[lo..=hi].iter().map(|c| c.id).collect(),
            None => vec![focus],
        }
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

pub(crate) fn sb_view(s: &mut Session, p: &Value) -> CmdResult {
    let view = match p.get("view").and_then(Value::as_str) {
        Some("map") => BoardView::Map,
        Some("strip") => BoardView::Strip,
        Some("fill") => BoardView::Fill,
        _ => return Err(CmdError::BadParams("need `view`: map|strip|fill".into())),
    };
    s.boards.view = view;
    Ok(json!({ "view": view.as_str() }))
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
    s.boards.generated = None;
    s.boards.board = Some(board);
    s.boards.mode = Mode::Storyboard;
    // A new storyboard opens in the Strip view: Viewer, Content panel and timeline (Paul, 2026-10-16).
    s.boards.view = crate::storyboard::BoardView::Strip;
    s.boards.transport = Transport::default();
    s.audio_stop();
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
    // Shift+click: from the anchor (or the clip selected before) to this one.
    let extend = p.get("extend").and_then(Value::as_bool).unwrap_or(false);
    let anchor =
        s.boards.range.filter(|(_, end)| Some(*end) == s.boards.selected).map(|(a, _)| a).or(s.boards.selected);
    s.boards.range = match (extend, anchor, clip) {
        (true, Some(a), Some(c)) => Some((a, c)),
        _ => None,
    };
    s.boards.selected = clip;
    let range: Vec<u32> = s.selected_clips().iter().map(|c| c.0).collect();
    Ok(json!({ "selected": clip.map(|c| c.0), "selection": range }))
}

/// Delete / Backspace (findings B2, B4; Paul, 2026-10-17: "shot first, then clip"): on the
/// selection (or `clips`), empty the clips that hold assets; when none does, remove the clips.
pub(crate) fn sb_delete(s: &mut Session, p: &Value) -> CmdResult {
    let ids: Vec<ClipId> = match p.get("clips") {
        None | Some(Value::Null) => s.selected_clips(),
        Some(Value::Array(list)) => {
            if list.len() > Limits::default().max_clips {
                return Err(CmdError::BadParams("too many `clips`".into()));
            }
            list.iter().map(clip_id).collect::<Result<_, _>>()?
        }
        Some(_) => return Err(CmdError::BadParams("`clips` must be a list of clip numbers".into())),
    };
    if ids.is_empty() {
        return Err(CmdError::Unavailable("select a clip first".into()));
    }
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let first = ids.iter().filter_map(|id| board.index_of(*id)).min().unwrap_or(0);
    let done = delete(&mut board, &ids).map_err(|e| CmdError::BadParams(e.to_string()))?;
    s.boards.checkpoint();
    let message = match done {
        Deleted::Shots(n) => format!("Removed the shot from {n} clip{} (press again to remove the clips)", plural(n)),
        Deleted::Clips(n) => {
            // Select the clip that moved up into the gap (or the new last one); the range ends.
            s.boards.selected = board.clips.get(first).or_else(|| board.clips.last()).map(|c| c.id);
            s.boards.range = None;
            format!("Removed {n} clip{} from the Cut", plural(n))
        }
    };
    let (kind, n) = match done {
        Deleted::Shots(n) => ("shots", n),
        Deleted::Clips(n) => ("clips", n),
    };
    s.boards.board = Some(board);
    let selection: Vec<u32> = s.selected_clips().iter().map(|c| c.0).collect();
    Ok(json!({ "deleted": kind, "count": n, "selection": selection, "message": format!("{message} · Ctrl+Z undoes") }))
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
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
    let background = board.clip(placed).is_some_and(|c| c.shot_type == ShotType::Ttl);
    s.boards.checkpoint();
    s.boards.board = Some(board);
    let (filled, fillable) = s.boards.board.as_ref().map(Storyboard::fill_progress).unwrap_or((0, 0));
    if background {
        // A title card's background: the card stays selected (the Quick fill cursor doesn't move).
        s.boards.selected = Some(placed);
        s.boards.range = None;
        return Ok(json!({
            "placed": placed.0,
            "background": true,
            "next": s.boards.board.as_ref().and_then(next_empty).map(|c| c.0),
            "filled": filled,
            "fillable": fillable,
            "message": format!("Background set on title card #{}", placed.0),
        }));
    }
    // The cursor moves on to the next empty clip; once every clip is filled nothing is selected,
    // so Quick fill shows "All n clips filled" rather than offering to replace the last one.
    let next = s.boards.board.as_ref().and_then(next_empty);
    s.boards.selected = next;
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

pub(crate) fn sb_autofill(s: &mut Session, p: &Value) -> CmdResult {
    let assets: Vec<AssetId> = match p.get("assets") {
        Some(Value::Array(list)) => {
            if list.len() > MAX_AUTOFILL_ASSETS {
                return Err(CmdError::BadParams(format!("at most {MAX_AUTOFILL_ASSETS} `assets`")));
            }
            let mut out = Vec::with_capacity(list.len());
            for v in list {
                let a = v
                    .as_str()
                    .map(str::trim)
                    .filter(|a| !a.is_empty() && a.len() <= Limits::default().max_string)
                    .ok_or_else(|| CmdError::BadParams("`assets` must be paths".into()))?;
                out.push(AssetId(a.to_owned()));
            }
            out
        }
        None => s
            .items
            .iter()
            .filter(|e| e.kind == assetcraft_fs::EntryKind::File)
            .take(MAX_AUTOFILL_ASSETS)
            .map(|e| AssetId(e.path.to_string_lossy().into_owned()))
            .collect(),
        Some(_) => return Err(CmdError::BadParams("`assets` must be a list of paths".into())),
    };
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    if next_empty(&board).is_none() {
        return Err(CmdError::Unavailable("every clip already has a shot".into()));
    }
    if assets.is_empty() {
        return Err(CmdError::Unavailable("no assets to fill with (open a folder in the Organizer)".into()));
    }
    let filled = auto_fill(&mut board, &assets);
    s.boards.checkpoint();
    s.boards.board = Some(board);
    s.boards.selected = None;
    Ok(json!({
        "filled": filled.iter().map(|c| c.0).collect::<Vec<_>>(),
        "message": format!("Auto-filled {} clips", filled.len()),
    }))
}

pub(crate) fn sb_clear(s: &mut Session, _: &Value) -> CmdResult {
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let n = clear(&mut board);
    if n == 0 {
        return Err(CmdError::Unavailable("no clip has a shot yet".into()));
    }
    s.boards.checkpoint();
    s.boards.selected = next_empty(&board);
    s.boards.board = Some(board);
    Ok(json!({ "cleared": n, "message": format!("Cleared {n} clips (Ctrl+Z to undo)") }))
}

pub(crate) fn sb_play(s: &mut Session, p: &Value) -> CmdResult {
    let total = s.boards.board.as_ref().ok_or_else(no_board)?.total();
    if total == Ticks::ZERO {
        return Err(CmdError::Unavailable("the Cut is empty".into()));
    }
    let from = match p.get("from") {
        None => s.boards.transport.position,
        Some(v) => seconds(v, "from")?,
    };
    // Playing from the end starts over.
    let from = if from >= total { Ticks::ZERO } else { from };
    s.boards.transport = Transport { playing: true, position: from };
    let note = s.audio_start(from);
    let mut v = transport_json(s);
    if let (Some(n), Value::Object(m)) = (note, &mut v) {
        m.insert("message".into(), json!(n));
    }
    Ok(v)
}

pub(crate) fn sb_pause(s: &mut Session, p: &Value) -> CmdResult {
    let total = s.boards.board.as_ref().ok_or_else(no_board)?.total();
    let at = match p.get("at") {
        None => s.boards.transport.position,
        Some(v) => seconds(v, "at")?,
    };
    s.boards.transport = Transport { playing: false, position: at.min(total) };
    s.audio_stop();
    Ok(transport_json(s))
}

pub(crate) fn sb_seek(s: &mut Session, p: &Value) -> CmdResult {
    let total = s.boards.board.as_ref().ok_or_else(no_board)?.total();
    let at = seconds(p.get("seconds").unwrap_or(&Value::Null), "seconds")?;
    s.boards.transport = Transport { playing: false, position: at.min(total) };
    s.audio_stop();
    Ok(transport_json(s))
}

fn seconds(v: &Value, key: &str) -> Result<Ticks, CmdError> {
    v.as_f64()
        .filter(|s| *s >= 0.0)
        .and_then(Ticks::from_secs_f64)
        .ok_or_else(|| CmdError::BadParams(format!("`{key}` must be seconds, 0 or more")))
}

fn transport_json(s: &Session) -> Value {
    let t = s.boards.transport;
    json!({
        "playing": t.playing,
        "position": t.position.as_secs_f64(),
        "timecode": t.position.timecode(),
        "audio": s.audio_status(),
    })
}

pub(crate) fn sb_undo(s: &mut Session, _: &Value) -> CmdResult {
    let (board, selected, generated) =
        s.boards.undo.pop().ok_or_else(|| CmdError::Unavailable("nothing to undo".into()))?;
    let current = (
        std::mem::replace(&mut s.boards.board, board),
        s.boards.selected,
        std::mem::replace(&mut s.boards.generated, generated),
    );
    s.boards.redo.push(current);
    s.boards.selected = selected;
    s.audio_stop();
    Ok(state(s))
}

pub(crate) fn sb_redo(s: &mut Session, _: &Value) -> CmdResult {
    let (board, selected, generated) =
        s.boards.redo.pop().ok_or_else(|| CmdError::Unavailable("nothing to redo".into()))?;
    let current = (
        std::mem::replace(&mut s.boards.board, board),
        s.boards.selected,
        std::mem::replace(&mut s.boards.generated, generated),
    );
    s.boards.undo.push(current);
    s.boards.selected = selected;
    s.audio_stop();
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
        "view": b.view.as_str(),
        "transport": transport_json(s),
        "generatedFrom": crate::generate_cmd::generated_json(s),
        "board": b.board.as_ref().and_then(|board| serde_json::to_value(board).ok()),
        "summary": summary,
        "selected": b.selected.map(|c| c.0),
        "selection": s.selected_clips().iter().map(|c| c.0).collect::<Vec<_>>(),
        "canUndo": !b.undo.is_empty(),
        "canRedo": !b.redo.is_empty(),
    })
}

pub(crate) fn clip_id(v: &Value) -> Result<ClipId, CmdError> {
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
