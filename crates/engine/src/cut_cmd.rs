//! The cut tools (`docs/design/cut-editing.md` §3), run through FilmCraft's edit algebra via
//! [`crate::cut_adapter`]. C1 `storyboard.roll`: move the cut between two clips. One clip gets
//! longer by what the other loses, so the Cut's total and everything after it stay where they
//! are, and the music stays in place. One undo step; a refused or zero move leaves none.

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
