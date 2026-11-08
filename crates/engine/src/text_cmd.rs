//! Text clip commands: switch a clip's text between Card and Overlay, and set its theme,
//! animation and overlay position (`docs/storyboard-plan.md`, step TC1).
//!
//! The rules live in `assetcraft_storyboard::text`; these commands parse params, try the edit on
//! a copy (so a refusal or a no-op leaves no undo step), then record one undo step.

use crate::storyboard::{clip_id, no_board};
use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{
    MAX_TEXT_MS, MIN_TEXT_MS, TextAnimation, TextEffect, TextExit, TextKind, TextPosition, TextTheme, set_text_kind,
    set_text_style, set_text_timing,
};
use serde_json::{Value, json};

/// `storyboard.text.kind {clip, kind: card|overlay}`.
pub(crate) fn sb_text_kind(s: &mut Session, p: &Value) -> CmdResult {
    let id = clip_id(p.get("clip").unwrap_or(&Value::Null))?;
    let kind = p
        .get("kind")
        .and_then(Value::as_str)
        .and_then(TextKind::from_id)
        .ok_or_else(|| CmdError::BadParams("`kind` must be card or overlay".into()))?;
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let change = set_text_kind(&mut board, id, kind).map_err(|e| CmdError::BadParams(e.to_string()))?;
    let number = board.index_of(id).map(|i| i + 1).unwrap_or(0);
    if !change.changed() {
        return Ok(json!({
            "clip": id.0,
            "kind": kind.id(),
            "changed": false,
            "message": format!("Clip #{number:02} is already a {}", kind.name().to_lowercase()),
        }));
    }
    s.boards.checkpoint();
    s.boards.board = Some(board);
    s.boards.selected = Some(id);
    let mut message = format!("Clip #{number:02}: {} → {}", change.from.name(), change.to.name());
    match change.to {
        TextKind::Overlay => message.push_str(" (the shot under it needs an image)"),
        TextKind::Card if change.removed_assets > 0 => {
            message.push_str(&format!(
                " (removed {} image{}; Ctrl+Z restores it)",
                change.removed_assets,
                plural(change.removed_assets)
            ));
        }
        TextKind::Card => {
            if board_background(s, id) {
                message.push_str(" (its shot stays as the card's background)");
            }
        }
    }
    Ok(json!({
        "clip": id.0,
        "kind": kind.id(),
        "changed": true,
        "removedAssets": change.removed_assets,
        "message": message,
    }))
}

/// `storyboard.text.style {clip, theme?, animation?, position?}`.
pub(crate) fn sb_text_style(s: &mut Session, p: &Value) -> CmdResult {
    let id = clip_id(p.get("clip").unwrap_or(&Value::Null))?;
    let theme = parse(p, "theme", TextTheme::from_id, "bold|minimal|cinematic|paper")?;
    let animation = parse(p, "animation", TextAnimation::from_id, "none|fade|rise|zoom|typewriter")?;
    let position = parse(p, "position", TextPosition::from_id, "lowerThirdLeft|lowerThirdRight|center|top|bottom")?;
    let effect = parse(p, "effect", TextEffect::from_id, "none|push|drift|pulse")?;
    let exit = parse(p, "exit", TextExit::from_id, "none|fade|sink|zoom|erase")?;
    let duration = text_duration(p)?;
    if theme.is_none()
        && animation.is_none()
        && position.is_none()
        && effect.is_none()
        && exit.is_none()
        && duration.is_none()
    {
        return Err(CmdError::BadParams(
            "give at least one of `theme`, `animation`, `duration`, `effect`, `exit`, `position`".into(),
        ));
    }
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let bad = |e: assetcraft_storyboard::TextError| CmdError::BadParams(e.to_string());
    let styled = set_text_style(&mut board, id, theme, animation, position).map_err(bad)?;
    let timed = set_text_timing(&mut board, id, duration, effect, exit).map_err(bad)?;
    let changed = styled || timed;
    let style = board.clip(id).map(|c| c.text_style).unwrap_or_default();
    if changed {
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    Ok(json!({
        "clip": id.0,
        "changed": changed,
        "theme": style.theme.id(),
        "animation": style.animation.id(),
        "position": style.position.id(),
        "duration": style.duration_ms.map(|ms| f64::from(ms) / 1000.0),
        "effect": style.effect.id(),
        "exit": style.exit.id(),
    }))
}

/// `duration`: seconds the text stays up (0.5–600), or `0` for until the clip ends.
fn text_duration(p: &Value) -> Result<Option<Option<u32>>, CmdError> {
    let Some(v) = p.get("duration").filter(|v| !v.is_null()) else { return Ok(None) };
    let secs = v.as_f64().filter(|s| s.is_finite()).ok_or_else(|| {
        CmdError::BadParams("`duration` must be seconds (0.5–600), or 0 for until the clip ends".into())
    })?;
    if secs == 0.0 {
        return Ok(Some(None));
    }
    let ms = (secs * 1000.0).round();
    if ms < f64::from(MIN_TEXT_MS) || ms > f64::from(MAX_TEXT_MS) {
        return Err(CmdError::BadParams(format!("`duration` {secs} s is outside 0.5–600 s (0 = until the clip ends)")));
    }
    Ok(Some(Some(ms as u32)))
}

fn parse<T>(p: &Value, key: &str, from_id: fn(&str) -> Option<T>, ids: &str) -> Result<Option<T>, CmdError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .and_then(from_id)
            .map(Some)
            .ok_or_else(|| CmdError::BadParams(format!("`{key}` must be one of {ids}"))),
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Whether clip `id` of the open board is a title card with a background.
fn board_background(s: &Session, id: assetcraft_storyboard::ClipId) -> bool {
    s.storyboard().and_then(|b| b.clip(id)).is_some_and(|c| c.background().is_some())
}

/// Longest clip text accepted, in characters (a card holds a few lines, not a page).
pub const MAX_CLIP_TEXT: usize = 500;

/// `storyboard.text.set {clip, text?, description?}`: what a clip shows and what it is.
/// `text` is the words on screen — a title card's text, an overlay's text; on a plain shot it
/// adds an overlay (`""` on an overlay removes it). `description` is the shot note; on a title
/// card it is the same thing as `text`. Line breaks are kept (two-line titles); each line is
/// trimmed. One undo step; unchanged → none.
pub(crate) fn sb_text_set(s: &mut Session, p: &Value) -> CmdResult {
    let id = clip_id(p.get("clip").unwrap_or(&Value::Null))?;
    let field = |name: &str| -> Result<Option<String>, CmdError> {
        match p.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(t)) => {
                let t: String = t.lines().map(str::trim).collect::<Vec<_>>().join("\n").trim().to_owned();
                if t.chars().count() > MAX_CLIP_TEXT || t.chars().any(|c| c.is_control() && c != '\n') {
                    return Err(CmdError::BadParams(format!(
                        "`{name}` must be at most {MAX_CLIP_TEXT} characters, without control characters"
                    )));
                }
                Ok(Some(t))
            }
            Some(_) => Err(CmdError::BadParams(format!("`{name}` must be text"))),
        }
    };
    let (text, description) = (field("text")?, field("description")?);
    if text.is_none() && description.is_none() {
        return Err(CmdError::BadParams("give `text` or `description`".into()));
    }
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let index = board.index_of(id).ok_or_else(|| CmdError::BadParams(format!("no clip #{}", id.0)))?;
    let clip = &mut board.clips[index];
    let before = clip.clone();
    let card = clip.shot_type == assetcraft_storyboard::ShotType::Ttl;
    if let Some(t) = text {
        if card {
            clip.description = t;
        } else if t.is_empty() {
            clip.overlay = None;
        } else {
            clip.overlay = Some(t);
        }
    }
    if let Some(d) = description {
        clip.description = d;
    }
    let changed = *clip != before;
    let shown = clip.text().unwrap_or("").to_owned();
    let desc = clip.description.clone();
    if changed {
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    Ok(json!({
        "clip": id.0,
        "changed": changed,
        "text": shown,
        "description": desc,
        "message": format!("Clip #{:02}: {}", index + 1, if shown.is_empty() { desc.as_str() } else { shown.as_str() }),
    }))
}
