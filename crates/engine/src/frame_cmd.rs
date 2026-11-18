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
/// A frame from `storyboard.frame`'s params over `base`: a `preset` starts from that preset,
/// then `width`, `height`, `fps`, `fill` and `snap` change it; a size or rate off its preset is
/// custom. Refused when it isn't a usable frame. Shared by the board, the workspace default and
/// a new storyboard's `frame` (F4).
pub(crate) fn frame_from(p: &Value, base: FrameSpec) -> Result<FrameSpec, CmdError> {
    let mut frame = match p.get("preset").filter(|v| !v.is_null()) {
        Some(v) => {
            let id = v.as_str().ok_or_else(|| CmdError::BadParams("`preset` must be a preset's id".into()))?;
            let mut f = frame_preset(id).map(|p| p.spec()).ok_or_else(|| {
                CmdError::BadParams(format!("no frame preset {id:?} (storyboard.framePresets lists them)"))
            })?;
            // A preset is a size and rate; the crop stays what it was.
            (f.fill, f.snap) = (base.fill, base.snap);
            f
        }
        None => base,
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
    Ok(frame)
}

/// A frame in words: `Vertical 9:16: 1080 × 1920 (9:16) at 30 fps, fill vertical from the center`.
fn describe(frame: &FrameSpec) -> String {
    let name = frame.preset.as_deref().and_then(frame_preset).map_or("Custom", |p| p.name);
    format!(
        "{name}: {} × {} ({}) at {} fps, {} from the {}",
        frame.width,
        frame.height,
        frame.ratio_label(),
        frame.fps,
        frame.fill.label().to_lowercase(),
        frame.snap.label().to_lowercase()
    )
}

/// `storyboard.defaultFrame {preset?, width?, height?, fps?, fill?, snap?} | {fromBoard: true} |
/// {reset: true}` (F4): the frame new storyboards start with, kept in prefs — set as
/// `storyboard.frame` sets a board's, copied from the open board, or back to HD 24. `{}`
/// reports it. A setting, not an edit: no undo step.
pub(crate) fn sb_default_frame(s: &mut Session, p: &Value) -> CmdResult {
    let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
    let frame = if flag("reset") {
        FrameSpec::default()
    } else if flag("fromBoard") {
        s.boards.board.as_ref().ok_or_else(no_board)?.frame_spec()
    } else {
        frame_from(p, s.default_frame.clone().unwrap_or_default())?
    };
    let changed = s.default_frame.clone().unwrap_or_default() != frame;
    s.default_frame = (frame != FrameSpec::default()).then(|| frame.clone());
    let mut out = frame_json(&frame);
    if let Some(o) = out.as_object_mut() {
        o.insert("changed".into(), json!(changed));
        o.insert("message".into(), json!(format!("New storyboards: {}", describe(&frame))));
    }
    Ok(out)
}

pub(crate) fn sb_frame(s: &mut Session, p: &Value) -> CmdResult {
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let frame = frame_from(p, board.frame_spec())?;
    let changed = board.frame_spec() != frame;
    let message = describe(&frame);
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

/// `storyboard.layout {kind: "full" | "split" | "framed", …}` (F5) → `{kind, layout, label,
/// changed, message}`: the stage's layout theme.
///
/// - **split** (L1) `{stack?, share?}`: each clip in region A or B (`storyboard.region`), cropped
///   into it; `stack` (default true) is top and bottom, else side by side; `share` is A's per
///   cent, 50, 60 or 70 (default 50).
/// - **framed** (L2) `{top?, bottom?, headline?, caption?, theme?}`: the shots in a picture
///   region between a headline band and a caption band (per cent of the frame, 0–35; default 14
///   and 12), coloured by a text theme (default bold).
///
/// Without `kind`, the current layout is changed (a split when there is none and split fields
/// are given). Fields not given keep their value. One undo step; the same layout is none.
pub(crate) fn sb_layout(s: &mut Session, p: &Value) -> CmdResult {
    use assetcraft_storyboard::{Layout, TextTheme};
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let current = board.layout.clone();
    let given = |k: &str| p.get(k).is_some_and(|v| !v.is_null());
    let kind = match p.get("kind") {
        None | Some(Value::Null) => match &current {
            Some(Layout::Framed { .. }) => "framed",
            Some(Layout::Split { .. }) => "split",
            None if ["top", "bottom", "headline", "caption", "theme"].iter().any(|k| given(k)) => "framed",
            None if given("stack") || given("share") => "split",
            None => "full",
        },
        Some(v) => {
            v.as_str().ok_or_else(|| CmdError::BadParams("`kind` must be \"full\", \"split\" or \"framed\"".into()))?
        }
    };
    let byte = |k: &str, was: u8, what: &str| -> Result<u8, CmdError> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(was),
            Some(v) => v.as_u64().and_then(|n| u8::try_from(n).ok()).ok_or_else(|| CmdError::BadParams(what.into())),
        }
    };
    let text = |k: &str, was: &str| -> Result<String, CmdError> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(was.to_owned()),
            Some(v) => v.as_str().map(str::to_owned).ok_or_else(|| CmdError::BadParams(format!("`{k}` must be text"))),
        }
    };
    let layout = match kind {
        "full" => None,
        "split" => {
            let (was_stack, was_share) = match &current {
                Some(Layout::Split { stack, share }) => (*stack, *share),
                _ => (true, 50),
            };
            let stack = match p.get("stack") {
                None | Some(Value::Null) => was_stack,
                Some(v) => v.as_bool().ok_or_else(|| CmdError::BadParams("`stack` must be true or false".into()))?,
            };
            let share = byte("share", was_share, "`share` must be 50, 60 or 70")?;
            Some(Layout::Split { stack, share })
        }
        "framed" => {
            let (top, bottom, headline, caption, theme) = match &current {
                Some(Layout::Framed { top, bottom, headline, caption, theme }) => {
                    (*top, *bottom, headline.clone(), caption.clone(), *theme)
                }
                _ => (14, 12, board.name.clone(), String::new(), TextTheme::Bold),
            };
            let theme = match p.get("theme") {
                None | Some(Value::Null) => theme,
                Some(v) => serde_json::from_value(v.clone())
                    .map_err(|_| CmdError::BadParams("`theme` must be a text theme's name".into()))?,
            };
            Some(Layout::Framed {
                top: byte("top", top, "`top` must be a whole per cent")?,
                bottom: byte("bottom", bottom, "`bottom` must be a whole per cent")?,
                headline: text("headline", &headline)?,
                caption: text("caption", &caption)?,
                theme,
            })
        }
        other => return Err(CmdError::BadParams(format!("no layout {other:?}: \"full\", \"split\" or \"framed\""))),
    };
    if let Some(problem) = layout.as_ref().and_then(Layout::problem) {
        return Err(CmdError::BadParams(problem));
    }
    let changed = layout != current;
    let label = layout.as_ref().map_or_else(|| "Full frame".to_owned(), Layout::label);
    let (stack, share) = match &layout {
        Some(Layout::Split { stack, share }) => (Some(*stack), Some(*share)),
        _ => (None, None),
    };
    let out = json!({
        "kind": kind,
        "layout": layout,
        "stack": stack,
        "share": share,
        "label": label,
        "changed": changed,
        "message": format!("Layout: {label}"),
    });
    if changed {
        board.layout = layout;
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    Ok(out)
}

/// `storyboard.region {clip | clips: [..], region: "a" | "b"}` (F5, L1) → `{region, clips,
/// changed, message}`: which region of a split stage the clips play in. Title cards take the
/// whole frame and are left as they are. One undo step for all of them; none when nothing moves.
pub(crate) fn sb_region(s: &mut Session, p: &Value) -> CmdResult {
    use assetcraft_storyboard::{ClipId, Region, ShotType};
    let region = match p.get("region").and_then(Value::as_str).map(str::to_ascii_lowercase).as_deref() {
        Some("a") => Region::A,
        Some("b") => Region::B,
        _ => return Err(CmdError::BadParams("`region` must be \"a\" or \"b\"".into())),
    };
    let ids: Vec<u32> = match (p.get("clip"), p.get("clips")) {
        (Some(v), None) => vec![
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| CmdError::BadParams("`clip` must be a clip's id".into()))?,
        ],
        (None, Some(Value::Array(a))) if !a.is_empty() => a
            .iter()
            .map(|v| v.as_u64().and_then(|n| u32::try_from(n).ok()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| CmdError::BadParams("`clips` must be clip ids".into()))?,
        _ => return Err(CmdError::BadParams("give `clip` or `clips`".into())),
    };
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let mut moved = 0;
    for id in &ids {
        let clip = board
            .clips
            .iter_mut()
            .find(|c| c.id == ClipId(*id))
            .ok_or_else(|| CmdError::BadParams(format!("no clip #{id}")))?;
        if clip.shot_type == ShotType::Ttl || clip.region() == region {
            continue;
        }
        clip.region = (region != Region::A).then_some(region);
        moved += 1;
    }
    if moved > 0 {
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    let letter = region.letter();
    Ok(json!({
        "region": letter.to_ascii_lowercase(),
        "clips": ids,
        "changed": moved > 0,
        "message": format!("{moved} clip{} in region {letter}", if moved == 1 { "" } else { "s" }),
    }))
}
