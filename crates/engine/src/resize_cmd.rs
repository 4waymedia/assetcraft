//! `storyboard.resize` (SR1, `docs/storyboard-plan.md` § Sections): set one clip's length, in
//! seconds or in beats of the track. The clips after it move; the Cut's total follows. The
//! track is left alone (the music simply ends, or plays on, where it did). One undo step; a
//! refused or unchanged length leaves none.

use crate::storyboard::{clip_id, no_board};
use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{Limits, Ticks};
use serde_json::{Value, json};

/// Shortest a clip can be made: half a second (22 px on the Strip, one short beat).
pub const MIN_CLIP_SECS: f64 = 0.5;

/// `storyboard.resize {clip, seconds? | beats?}`.
pub(crate) fn sb_resize(s: &mut Session, p: &Value) -> CmdResult {
    let id = clip_id(p.get("clip").unwrap_or(&Value::Null))?;
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let index = board.index_of(id).ok_or_else(|| CmdError::BadParams(format!("no clip #{}", id.0)))?;
    let limits = Limits::default();
    let min = Ticks::from_secs_f64(MIN_CLIP_SECS).unwrap_or(Ticks(1));

    let want = match (p.get("seconds"), p.get("beats")) {
        (Some(secs), None) => secs
            .as_f64()
            .filter(|v| v.is_finite() && *v > 0.0)
            .and_then(Ticks::from_secs_f64)
            .ok_or_else(|| CmdError::BadParams("`seconds` must be a positive length".into()))?,
        (None, Some(beats)) => {
            let track =
                board.track.as_ref().ok_or_else(|| CmdError::Unavailable("add a track to size in beats".into()))?;
            let n = beats
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| CmdError::BadParams("`beats` must be a whole number of beats, 1 or more".into()))?;
            track.bpm.beats_to_ticks(n)
        }
        (None, None) => return Err(CmdError::BadParams("give `seconds` or `beats`".into())),
        (Some(_), Some(_)) => return Err(CmdError::BadParams("give `seconds` or `beats`, not both".into())),
    };
    if want < min {
        return Err(CmdError::BadParams(format!("a clip can't be shorter than {MIN_CLIP_SECS} s")));
    }
    if want > limits.max_duration {
        return Err(CmdError::BadParams("a clip can't be longer than 6 hours".into()));
    }
    let before = board.clips[index].duration;
    let rest = board
        .clips
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .try_fold(Ticks::ZERO, |t, (_, c)| t.checked_add(c.duration));
    let total = rest
        .and_then(|t| t.checked_add(want))
        .filter(|t| *t <= limits.max_duration)
        .ok_or_else(|| CmdError::BadParams("the Cut would be longer than the limit (6 hours)".into()))?;

    let changed = want != before;
    if changed {
        board.clips[index].duration = want;
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    let board = s.boards.board.as_ref().ok_or_else(no_board)?;
    let beats = board.track.as_ref().map(|t| t.bpm.ticks_to_beats(want));
    let secs = want.as_secs_f64();
    let shown = match beats {
        Some(b) => format!("{secs:.1} s · {b} beat{}", if b == 1 { "" } else { "s" }),
        None => format!("{secs:.1} s"),
    };
    Ok(json!({
        "clip": id.0,
        "changed": changed,
        "seconds": secs,
        "beats": beats,
        "total": total.timecode(),
        "message": format!("Clip #{:02}: {shown} · Cut {}", index + 1, total.timecode()),
    }))
}
