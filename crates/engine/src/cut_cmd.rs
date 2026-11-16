//! The cut tools (`docs/design/cut-editing.md` §3), run through FilmCraft's edit algebra via
//! [`crate::cut_adapter`]. C1 `storyboard.roll`: move the cut between two clips. One clip gets
//! longer by what the other loses, so the Cut's total and everything after it stay where they
//! are, and the music stays in place. C3 `storyboard.split`: split a clip in two (FilmCraft's
//! razor). One undo step each; a refused or zero edit leaves none.

use crate::cut_adapter::{self, apply_lengths, to_sequence};
use crate::resize_cmd::MIN_CLIP_SECS;
use crate::storyboard::{clip_id, no_board};
use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::Ticks;
use filmcraft_project::ClipId as FcClipId;
use filmcraft_time::{TICKS_PER_SECOND, Tick};
use serde_json::{Value, json};

/// Seconds → FilmCraft ticks, signed; finite values only.
fn secs_to_tick(secs: f64) -> Option<Tick> {
    let t = (secs * TICKS_PER_SECOND as f64).round();
    (t.is_finite() && t.abs() < i64::MAX as f64).then_some(Tick(t as i64))
}

fn tick_secs(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_SECOND as f64
}

/// `storyboard.slip {clip, seconds? | to?}` (C6): change where in its video clip `clip` starts,
/// by `seconds` (signed) or `to` a time in the video, without moving it or changing its length.
/// Limited to the video's start and the last in-point that still fills the clip; the reply says
/// where it landed. Video clips only.
pub(crate) fn sb_slip(s: &mut Session, p: &Value) -> CmdResult {
    let id = clip_id(p.get("clip").unwrap_or(&Value::Null))?;
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let index = board.index_of(id).ok_or_else(|| CmdError::BadParams(format!("no clip #{}", id.0)))?;
    let clip = &board.clips[index];
    let hero = clip
        .asset_ids
        .first()
        .filter(|a| {
            clip.shot_type != assetcraft_storyboard::ShotType::Ttl && crate::is_video(std::path::Path::new(&a.0))
        })
        .ok_or_else(|| {
            CmdError::BadParams(format!("clip #{} has no video: only a video has an in-point", index + 1))
        })?;
    let video = assetcraft_media::seconds(std::path::Path::new(&hero.0))
        .map_err(|e| CmdError::Unavailable(format!("can't read the video's length: {e}")))?;
    let video = secs_to_tick(video).ok_or_else(|| CmdError::Unavailable("the video is too long to edit".into()))?;
    let was = cut_adapter::tick(clip.source_in)?;

    let given = ["seconds", "to"].iter().filter(|k| p.get(**k).is_some_and(|v| !v.is_null())).count();
    if given != 1 {
        return Err(CmdError::BadParams("give one of `seconds` or `to`".into()));
    }
    let asked = if let Some(v) = p.get("seconds").filter(|v| !v.is_null()) {
        v.as_f64().and_then(secs_to_tick).ok_or_else(|| CmdError::BadParams("`seconds` must be a number".into()))?
    } else {
        let to = p
            .get("to")
            .and_then(Value::as_f64)
            .filter(|t| *t >= 0.0)
            .and_then(secs_to_tick)
            .ok_or_else(|| CmdError::BadParams("`to` must be a time in the video, in seconds".into()))?;
        Tick(to.0.saturating_sub(was.0))
    };

    let min = Ticks::from_secs_f64(MIN_CLIP_SECS).unwrap_or(Ticks(1));
    let (now, moved) = cut_adapter::slip(&board, id, asked, video, min)?;
    let changed = moved != Tick::ZERO;
    if changed {
        board.clips[index].source_in = now;
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    let at = now.as_secs_f64();
    Ok(json!({
        "clip": id.0,
        "changed": changed,
        "moved": tick_secs(moved),
        "in": at,
        "video": tick_secs(video),
        "limited": moved != asked,
        "message": if changed {
            format!("Clip #{:02} now starts {at:.2} s into its video ({:+.2} s)", index + 1, tick_secs(moved))
        } else {
            format!("Clip #{:02} can't slip that way: it starts {at:.2} s into its video", index + 1)
        },
    }))
}

/// `storyboard.roll {left, seconds? | beats? | to?}`: move the cut after clip `left` by `seconds`
/// (signed), by `beats` of the track (signed), or `to` a time in the Cut (seconds). The move is
/// limited so neither clip gets shorter than half a second; the reply says how far it went.
pub(crate) fn sb_roll(s: &mut Session, p: &Value) -> CmdResult {
    let left = clip_id(p.get("left").unwrap_or(&Value::Null))?;
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let index = board.index_of(left).ok_or_else(|| CmdError::BadParams(format!("no clip #{}", left.0)))?;
    let right =
        board.clips.get(index + 1).map(|c| c.id).ok_or_else(|| {
            CmdError::BadParams(format!("clip #{} is the last one: there is no cut after it", left.0))
        })?;
    // Where the cut is now: the end of `left`.
    let cut_at = board.clips[..=index].iter().try_fold(Ticks::ZERO, |t, c| t.checked_add(c.duration));
    let cut_at = cut_adapter::tick(cut_at.ok_or_else(|| CmdError::BadParams("the Cut is too long to edit".into()))?)?;

    let given = ["seconds", "beats", "to"].iter().filter(|k| p.get(**k).is_some_and(|v| !v.is_null())).count();
    if given != 1 {
        return Err(CmdError::BadParams("give one of `seconds`, `beats` or `to`".into()));
    }
    let asked = if let Some(v) = p.get("seconds").filter(|v| !v.is_null()) {
        v.as_f64().and_then(secs_to_tick).ok_or_else(|| CmdError::BadParams("`seconds` must be a number".into()))?
    } else if let Some(v) = p.get("beats").filter(|v| !v.is_null()) {
        let track =
            board.track.as_ref().ok_or_else(|| CmdError::Unavailable("add a track to move a cut by beats".into()))?;
        let n = v.as_i64().filter(|n| n.unsigned_abs() <= 100_000).ok_or_else(|| {
            CmdError::BadParams("`beats` must be a whole number of beats (negative moves the cut earlier)".into())
        })?;
        let beat = cut_adapter::tick(track.bpm.beat())?;
        Tick(beat.0.saturating_mul(n))
    } else {
        let to = p
            .get("to")
            .and_then(Value::as_f64)
            .filter(|t| *t >= 0.0)
            .and_then(secs_to_tick)
            .ok_or_else(|| CmdError::BadParams("`to` must be a time in the Cut, in seconds".into()))?;
        Tick(to.0 - cut_at.0)
    };

    let min = Ticks::from_secs_f64(MIN_CLIP_SECS).unwrap_or(Ticks(1));
    let mut cut = to_sequence(&board, min)?;
    let (l, r) = (FcClipId(u64::from(left.0)), FcClipId(u64::from(right.0)));
    let moved = cut_adapter::edit(&mut cut, |seq, ctx| filmcraft_edit::roll(seq, l, r, asked, ctx))
        .map_err(|e| CmdError::Unavailable(format!("the cut can't move: {e}")))?;
    let changed = moved != Tick::ZERO;
    if changed {
        apply_lengths(&mut board, &cut)?;
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    let now = Tick(cut_at.0 + moved.0);
    let total = s.boards.board.as_ref().map(|b| b.total().timecode()).unwrap_or_default();
    let limited = moved != asked;
    Ok(json!({
        "left": left.0,
        "right": right.0,
        "changed": changed,
        "moved": tick_secs(moved),
        "cut": tick_secs(now),
        "limited": limited,
        "message": if changed {
            format!(
                "Cut after #{:02} moved {:+.2} s to {:.2} s{} · Cut {total}",
                index + 1,
                tick_secs(moved),
                tick_secs(now),
                if limited { " (as far as it goes: clips keep half a second)" } else { "" }
            )
        } else {
            "The cut can't move that way: a clip would be shorter than half a second".to_string()
        },
    }))
}

/// `storyboard.split {at, clip?}` (C3): split the clip under `at` (seconds in the Cut) into two
/// through FilmCraft's razor. The first half keeps its in-transition, notes and overlay text; the
/// second is the same shot (assets, description, camera move, text style) with the original's
/// out-transition, starting on a plain cut. Both halves keep half a second at least. A title
/// card isn't split (half a card has no words: shorten it instead). `clip`, when given, must be
/// the clip under `at`. One undo step.
pub(crate) fn sb_split(s: &mut Session, p: &Value) -> CmdResult {
    let at = p
        .get("at")
        .and_then(Value::as_f64)
        .filter(|t| *t > 0.0)
        .and_then(secs_to_tick)
        .ok_or_else(|| CmdError::BadParams("`at` must be a time in the Cut, in seconds".into()))?;
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    // The clip under `at`, and where it starts.
    let mut start = Tick::ZERO;
    let mut found = None;
    for (i, c) in board.clips.iter().enumerate() {
        let end = Tick(start.0.saturating_add(cut_adapter::tick(c.duration)?.0));
        if at.0 > start.0 && at.0 < end.0 {
            found = Some((i, start, end));
            break;
        }
        start = end;
    }
    let (index, start, end) = found
        .ok_or_else(|| CmdError::BadParams("`at` must fall inside a clip, not on a cut or past the end".into()))?;
    let source = board.clips[index].clone();
    if let Some(want) = p.get("clip").filter(|v| !v.is_null()).map(clip_id).transpose()?
        && want != source.id
    {
        return Err(CmdError::BadParams(format!("clip #{} isn't under that time", want.0)));
    }
    if source.shot_type == assetcraft_storyboard::ShotType::Ttl {
        return Err(CmdError::BadParams("a title card can't be split; shorten it instead".into()));
    }
    let min = Ticks::from_secs_f64(MIN_CLIP_SECS).unwrap_or(Ticks(1));
    let min_t = cut_adapter::tick(min)?;
    if at.0 - start.0 < min_t.0 || end.0 - at.0 < min_t.0 {
        return Err(CmdError::BadParams(format!("both halves need {MIN_CLIP_SECS} s at least")));
    }
    let new_id = board.next_clip_id().ok_or_else(|| CmdError::BadParams("no clip ids left".into()))?;

    let mut cut = to_sequence(&board, min)?;
    let fc = FcClipId(u64::from(source.id.0));
    let made = cut_adapter::edit(&mut cut, |seq, ctx| filmcraft_edit::razor_items(seq, &[fc], at, ctx));
    let [right] = made.as_slice() else {
        return Err(CmdError::Unavailable("the clip couldn't be split there".into()));
    };
    // Rebuild the clip list from the edited track: existing clips by id, the new right half
    // as a copy of the source.
    let mut clips = Vec::with_capacity(board.clips.len() + 1);
    for (id, len) in cut_adapter::lengths(&cut)? {
        if id == right.0 {
            let mut half = source.clone();
            half.id = new_id;
            half.duration = len;
            // The second half carries on where the first stops in the video (C6).
            if source.asset_ids.first().is_some_and(|a| crate::is_video(std::path::Path::new(&a.0))) {
                let first = Ticks(source.duration.0.saturating_sub(len.0));
                half.source_in = source.source_in.checked_add(first).unwrap_or(source.source_in);
            }
            half.transition_in = assetcraft_storyboard::Transition::Cut;
            half.notes.clear();
            half.overlay = None;
            half.sync_to_downbeat = false;
            clips.push(half);
        } else {
            let mut c = board
                .clips
                .iter()
                .find(|c| u64::from(c.id.0) == id)
                .cloned()
                .ok_or_else(|| CmdError::Unavailable("the split lost a clip".into()))?;
            c.duration = len;
            if c.id == source.id {
                c.transition_out = assetcraft_storyboard::Transition::Cut;
            }
            clips.push(c);
        }
    }
    board.clips = clips;
    s.boards.checkpoint();
    s.boards.board = Some(board);
    Ok(json!({
        "clip": source.id.0,
        "new": new_id.0,
        "at": tick_secs(at),
        "first": tick_secs(Tick(at.0 - start.0)),
        "second": tick_secs(Tick(end.0 - at.0)),
        "message": format!(
            "Clip #{:02} split at {:.2} s: {:.2} s + {:.2} s",
            index + 1,
            tick_secs(at),
            tick_secs(Tick(at.0 - start.0)),
            tick_secs(Tick(end.0 - at.0))
        ),
    }))
}

/// `storyboard.snapping {on?}` (C5): turn snapping for the cut tools on or off (remembered), or
/// toggle it with no `on`. Replies `{on, message}`. Holding Shift while dragging still snaps to
/// nothing for that drag.
pub(crate) fn sb_snapping(s: &mut Session, p: &Value) -> CmdResult {
    let on = match p.get("on") {
        None | Some(Value::Null) => !s.snapping,
        Some(v) => v.as_bool().ok_or_else(|| CmdError::BadParams("`on` must be true/false".into()))?,
    };
    s.snapping = on;
    let message = if on { "Snapping on · cuts snap to cuts, the playhead and the music" } else { "Snapping off" };
    Ok(json!({ "on": on, "message": message }))
}

/// The Strip's zoom range (Z1): a quarter (whole Cuts at a glance) to sixteen times (single
/// frames and the waveform's detail).
pub const STRIP_ZOOM_RANGE: (f32, f32) = (0.25, 16.0);

/// `storyboard.zoom {zoom? | by?}` (Z1; Paul, 2026-11-15: "zoom in on the play section so that
/// they can clearly see the audio"): set the Strip's zoom (1 = 44 pt a second) or multiply it,
/// kept within `STRIP_ZOOM_RANGE`. A view setting: no undo step. Replies `{zoom, message}`.
pub(crate) fn sb_zoom(s: &mut Session, p: &Value) -> CmdResult {
    let num = |k: &str| -> Result<Option<f64>, CmdError> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_f64()
                .filter(|v| v.is_finite() && *v > 0.0)
                .map(Some)
                .ok_or_else(|| CmdError::BadParams(format!("`{k}` must be a positive number"))),
        }
    };
    let zoom = match (num("zoom")?, num("by")?) {
        (Some(z), None) => z,
        (None, Some(by)) => f64::from(s.strip_zoom) * by,
        _ => return Err(CmdError::BadParams("give one of `zoom` or `by`".into())),
    };
    let (lo, hi) = STRIP_ZOOM_RANGE;
    s.strip_zoom = (zoom as f32).clamp(lo, hi);
    let z = s.strip_zoom;
    Ok(json!({ "zoom": z, "message": format!("Zoom {:.0}%", z * 100.0) }))
}
