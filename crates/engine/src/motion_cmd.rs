//! Clip motion command (SB-3D1 step 1, `docs/design/clip-motion-handoff.md`): set, swap or clear
//! how the camera moves over a clip. The rules live in `assetcraft_storyboard::motion`; this
//! parses params, tries the edit on a copy (a refusal or a no-op leaves no undo step), then
//! records one undo step.
//!
//! Cameras in JSON are `{yaw?, pitch?, zoom?, pan?: [x, y]}` with angles in **degrees** and pan
//! as a fraction of the frame; the data keeps radians. A field left out keeps its value.

use crate::storyboard::{clip_id, no_board};
use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{
    Camera, Ease, Motion, MotionEdit, MotionKind, MotionPreset, clear_motion, set_motion, swap_motion,
};
use serde_json::{Value, json};

/// `storyboard.motion {clip, preset?, start?, amount?, ease?, swap?, clear?}`.
pub(crate) fn sb_motion(s: &mut Session, p: &Value) -> CmdResult {
    let id = clip_id(p.get("clip").unwrap_or(&Value::Null))?;
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let number = board.index_of(id).map(|i| i + 1).unwrap_or(0);
    let bad = |e: assetcraft_storyboard::MotionError| CmdError::BadParams(e.to_string());

    let changed = if flag(p, "clear")? {
        clear_motion(&mut board, id).map_err(bad)?
    } else if flag(p, "swap")? {
        swap_motion(&mut board, id).map_err(bad)?
    } else {
        let preset = match p.get("preset") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.as_str().and_then(MotionPreset::from_id).ok_or_else(|| {
                CmdError::BadParams(
                    "`preset` must be one of turntable|pushIn|pullOut|panLeft|panRight|rise|custom".into(),
                )
            })?),
        };
        let ease = match p.get("ease") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                v.as_str()
                    .and_then(Ease::from_id)
                    .ok_or_else(|| CmdError::BadParams("`ease` must be one of linear|inOut|out".into()))?,
            ),
        };
        if preset.is_none() && ease.is_none() && p.get("start").is_none() && p.get("amount").is_none() {
            return Err(CmdError::BadParams(
                "give at least one of `preset`, `start`, `amount`, `ease`, or `swap` / `clear`".into(),
            ));
        }
        // The preset first, so a start / amount / ease given with it lands on top of it.
        let mut changed = false;
        if preset.is_some() {
            changed |= set_motion(&mut board, id, MotionEdit { preset, ..Default::default() }).map_err(bad)?;
        }
        let current = board.clip(id).and_then(|c| c.motion).unwrap_or_default();
        let start = p.get("start").map(|v| camera(v, current.start, "start")).transpose()?;
        let amount = p.get("amount").map(|v| camera(v, current.amount, "amount")).transpose()?;
        if start.is_some() || amount.is_some() || ease.is_some() {
            changed |= set_motion(&mut board, id, MotionEdit { preset: None, start, amount, ease }).map_err(bad)?;
        }
        changed
    };

    let motion = board.clip(id).and_then(|c| c.motion);
    let kind = board.clip(id).and_then(MotionKind::of_clip);
    if changed {
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    Ok(json!({
        "clip": id.0,
        "changed": changed,
        "kind": kind.map(|k| k.name()),
        "motion": motion.map(motion_json),
        "message": match motion {
            Some(m) => format!("Clip #{number:02}: {}", m.preset.name()),
            None => format!("Clip #{number:02}: no motion"),
        },
    }))
}

/// A motion as `storyboard.state` and the Inspector read it (angles in degrees).
pub(crate) fn motion_json(m: Motion) -> Value {
    json!({
        "preset": m.preset.id(),
        "start": camera_json(m.start),
        "amount": camera_json(m.amount),
        "end": camera_json(m.end()),
        "ease": m.ease.id(),
        "glyph": m.glyph(),
    })
}

fn camera_json(c: Camera) -> Value {
    json!({ "yaw": c.yaw.to_degrees(), "pitch": c.pitch.to_degrees(), "zoom": c.zoom, "pan": c.pan })
}

/// `{yaw?, pitch?, zoom?, pan?}` (degrees) over `base`; a missing field keeps `base`'s.
fn camera(v: &Value, base: Camera, key: &str) -> Result<Camera, CmdError> {
    let Some(obj) = v.as_object() else {
        return Err(CmdError::BadParams(format!("`{key}` must be an object {{yaw?, pitch?, zoom?, pan?}}")));
    };
    let num = |name: &str, current: f32| -> Result<f32, CmdError> {
        match obj.get(name) {
            None | Some(Value::Null) => Ok(current),
            Some(n) => n
                .as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| CmdError::BadParams(format!("`{key}.{name}` must be a number"))),
        }
    };
    let pan = match obj.get("pan") {
        None | Some(Value::Null) => base.pan,
        Some(Value::Array(xy)) if xy.len() == 2 => {
            let x = xy[0].as_f64().ok_or_else(|| CmdError::BadParams(format!("`{key}.pan` must be [x, y]")))?;
            let y = xy[1].as_f64().ok_or_else(|| CmdError::BadParams(format!("`{key}.pan` must be [x, y]")))?;
            [x as f32, y as f32]
        }
        Some(_) => return Err(CmdError::BadParams(format!("`{key}.pan` must be [x, y]"))),
    };
    Ok(Camera {
        yaw: num("yaw", base.yaw.to_degrees())?.to_radians(),
        pitch: num("pitch", base.pitch.to_degrees())?.to_radians(),
        zoom: num("zoom", base.zoom)?,
        pan,
    })
}

fn flag(p: &Value, key: &str) -> Result<bool, CmdError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(CmdError::BadParams(format!("`{key}` must be true or false"))),
    }
}
