//! Text clip commands: switch a clip's text between Card and Overlay, and set its theme,
//! animation and overlay position (`docs/storyboard-plan.md`, step TC1).
//!
//! The rules live in `assetcraft_storyboard::text`; these commands parse params, try the edit on
//! a copy (so a refusal or a no-op leaves no undo step), then record one undo step.

use crate::storyboard::{clip_id, no_board};
use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{TextAnimation, TextKind, TextPosition, TextTheme, set_text_kind, set_text_style};
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
    if theme.is_none() && animation.is_none() && position.is_none() {
        return Err(CmdError::BadParams("give at least one of `theme`, `animation`, `position`".into()));
    }
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let changed =
        set_text_style(&mut board, id, theme, animation, position).map_err(|e| CmdError::BadParams(e.to_string()))?;
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
    }))
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
