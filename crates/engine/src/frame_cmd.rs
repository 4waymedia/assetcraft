//! `storyboard.frame` / `storyboard.framePresets` (frame-settings.md F1): the Cut's frame — its
//! pixel size, frame rate and default crop — from a film or social preset or set by hand.
//! F1 keeps the setting; the Viewer, the compositor and the export follow it from F2 and F3.

use crate::storyboard::no_board;
use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{FRAME_PRESETS, FRAME_RATES, FRAME_SIDE_RANGE, Fill, FrameSpec, Snap, frame_preset};
use serde_json::{Value, json};

/// A frame as the commands give it.
pub(crate) fn frame_json(f: &FrameSpec) -> Value {
    json!({
        "width": f.width,
        "height": f.height,
        "fps": f.fps,
        "ratio": f.ratio_label(),
        "vertical": f.is_vertical(),
        "fill": f.fill,
        "snap": f.snap,
        "preset": f.preset,
    })
}

/// `storyboard.framePresets {}` → `{presets: [{id, group, name, for, width, height, fps, ratio,
/// fill}], rates, sides, fills, snaps}`: everything the setup offers, for the UI, CLI and MCP.
pub(crate) fn sb_frame_presets(_s: &mut Session, _p: &Value) -> CmdResult {
    let presets: Vec<Value> = FRAME_PRESETS
        .iter()
        .map(|p| {
            let spec = p.spec();
            json!({
                "id": p.id,
                "group": p.group,
                "name": p.name,
                "for": p.for_,
                "width": p.width,
                "height": p.height,
                "fps": p.fps,
                "ratio": spec.ratio_label(),
                "fill": p.fill,
            })
        })
        .collect();
    Ok(json!({
        "presets": presets,
        "rates": FRAME_RATES,
        "sides": [FRAME_SIDE_RANGE.0, FRAME_SIDE_RANGE.1],
        "fills": [Fill::Vertical, Fill::Horizontal],
        "snaps": Snap::ALL,
    }))
}

/// `storyboard.frame {preset?, width?, height?, fps?, fill?, snap?}` → the board's frame
/// `{width, height, fps, ratio, vertical, fill, snap, preset, changed, message}`.
///
/// A `preset` starts from that preset's frame; anything else given then changes it, and a frame
/// whose size or rate differs from its preset's is custom (`preset: null`). With neither, the
/// current frame is changed. `{}` only reports it. One undo step; the same frame again is none.
pub(crate) fn sb_frame(s: &mut Session, p: &Value) -> CmdResult {
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let mut frame = match p.get("preset").filter(|v| !v.is_null()) {
        Some(v) => {
            let id = v.as_str().ok_or_else(|| CmdError::BadParams("`preset` must be a preset's id".into()))?;
            frame_preset(id).map(|p| p.spec()).ok_or_else(|| {
                CmdError::BadParams(format!("no frame preset {id:?} (storyboard.framePresets lists them)"))
            })?
        }
        None => board.frame_spec(),
    };
    let number = |k: &str| -> Result<Option<u32>, CmdError> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(Some)
                .ok_or_else(|| CmdError::BadParams(format!("`{k}` must be a whole number"))),
        }
    };
    let sized = (number("width")?, number("height")?, number("fps")?);
    if let Some(w) = sized.0 {
        frame.width = w;
    }
    if let Some(h) = sized.1 {
        frame.height = h;
    }
    if let Some(f) = sized.2 {
        frame.fps = f;
    }
    if let Some(v) = p.get("fill").filter(|v| !v.is_null()) {
        frame.fill = serde_json::from_value(v.clone())
            .map_err(|_| CmdError::BadParams("`fill` must be \"vertical\" or \"horizontal\"".into()))?;
    }
    if let Some(v) = p.get("snap").filter(|v| !v.is_null()) {
        frame.snap = serde_json::from_value(v.clone())
            .map_err(|_| CmdError::BadParams("`snap` must be center, top, bottom, left or right".into()))?;
    }
    if let Some(problem) = frame.problem() {
        return Err(CmdError::BadParams(format!("can't use that frame: {problem}")));
    }
    // Size and rate decide the preset; the crop is the board's own choice.
    frame.preset = frame
        .preset
        .take()
        .and_then(|id| frame_preset(&id))
        .filter(|p| (p.width, p.height, p.fps) == (frame.width, frame.height, frame.fps))
        .map(|p| p.id.to_owned());
    let changed = board.frame_spec() != frame;
    let name = frame.preset.as_deref().and_then(frame_preset).map_or("Custom", |p| p.name);
    let message = format!(
        "{name}: {} × {} ({}) at {} fps, {} from the {}",
        frame.width,
        frame.height,
        frame.ratio_label(),
        frame.fps,
        frame.fill.label().to_lowercase(),
        frame.snap.label().to_lowercase()
    );
    let mut out = frame_json(&frame);
    if changed {
        board.frame = (frame != FrameSpec::default()).then_some(frame);
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    if let Some(o) = out.as_object_mut() {
        o.insert("changed".into(), json!(changed));
        o.insert("message".into(), json!(message));
    }
    Ok(out)
}
