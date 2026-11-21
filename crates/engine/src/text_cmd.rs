//! Text clip commands: switch a clip's text between Card and Overlay, and set its theme,
//! animation and overlay position (`docs/storyboard-plan.md`, step TC1).
//!
//! The rules live in `assetcraft_storyboard::text`; these commands parse params, try the edit on
//! a copy (so a refusal or a no-op leaves no undo step), then record one undo step.

use crate::storyboard::{clip_id, no_board};
use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{
    BoxAlign, DesignPick, MAX_TEXT_MS, MIN_TEXT_MS, TextAnimation, TextBox, TextEffect, TextExit, TextKind,
    TextPosition, TextTheme, ThemeChoice, ThemeId, set_text_area, set_text_design, set_text_kind, set_text_style,
    set_text_timing,
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
    let theme = theme_choice(s, p)?;
    let animation = parse_or_theme(p, "animation", TextAnimation::from_id, "none|fade|rise|zoom|typewriter")?;
    let position = parse(p, "position", TextPosition::from_id, "lowerThirdLeft|lowerThirdRight|center|top|bottom")?;
    let effect = parse_or_theme(p, "effect", TextEffect::from_id, "none|push|drift|pulse")?;
    let exit = parse_or_theme(p, "exit", TextExit::from_id, "none|fade|sink|zoom|erase")?;
    let duration = text_duration(p)?;
    // `design`: a built-in's id or `none`; `"theme"` goes back to the theme pack's own.
    let design = match p.get("design") {
        None => None,
        Some(v) => match v.as_str() {
            Some("theme") => Some(None),
            Some(id) => Some(Some(DesignPick::from_id(id).ok_or_else(|| {
                CmdError::BadParams(format!("`design` {id:?} isn't one of theme|none|banner-laser-trace|banner-stack"))
            })?)),
            None => {
                return Err(CmdError::BadParams("`design` must be theme|none|banner-laser-trace|banner-stack".into()));
            }
        },
    };
    if theme.is_none()
        && design.is_none()
        && animation.is_none()
        && position.is_none()
        && effect.is_none()
        && exit.is_none()
        && duration.is_none()
    {
        return Err(CmdError::BadParams(
            "give at least one of `theme`, `design`, `animation`, `duration`, `effect`, `exit`, `position`".into(),
        ));
    }
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let bad = |e: assetcraft_storyboard::TextError| CmdError::BadParams(e.to_string());
    let styled = set_text_style(&mut board, id, theme, animation, position).map_err(bad)?;
    let timed = set_text_timing(&mut board, id, duration, effect, exit).map_err(bad)?;
    let designed = match design {
        Some(d) => set_text_design(&mut board, id, d).map_err(bad)?,
        None => false,
    };
    let changed = styled || timed || designed;
    let board_clip = board.clip(id).cloned();
    let style = board_clip.as_ref().map(|c| c.text_style).unwrap_or_default();
    if changed {
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    // What the text does (the clip's choices over its theme's), and which parts follow the theme.
    let t = style.timing_with(s.user_looks());
    let from_theme: Vec<&str> = [
        ("animation", style.animation.is_none()),
        ("duration", style.duration_ms.is_none()),
        ("effect", style.effect.is_none()),
        ("exit", style.exit.is_none()),
    ]
    .into_iter()
    .filter_map(|(k, inherit)| inherit.then_some(k))
    .collect();
    Ok(json!({
        "clip": id.0,
        "changed": changed,
        "theme": style.theme_id.map_or_else(|| style.theme.id().to_string(), |u| u.as_str().to_string()),
        "fallback": style.theme.id(),
        "missingTheme": style.missing_theme(s.user_looks()).map(|u| u.as_str().to_string()),
        "design": style.design.map_or("theme", DesignPick::id),
        "drawnBy": board_clip.as_ref().and_then(|c| s.clip_design(c)).map(|(name, role)| json!({ "name": name, "role": role })),
        "animation": t.animation.id(),
        "position": style.position.id(),
        "duration": t.duration_ms.map(|ms| f64::from(ms) / 1000.0),
        "effect": t.effect.id(),
        "exit": t.exit.id(),
        "fromTheme": from_theme,
    }))
}

/// `storyboard.text.area {clip, align?, width?, height?, y?, center?, reset?}` (R-T19): where a
/// clip's title sits and its size (its `area`), in percent of the frame. Starts from the clip's
/// area, or `from` (the built-in banners' box); `center` puts it dead centre; `reset` lets the
/// title fall where it does by itself. Values are clamped so the area stays inside the title-safe area. One undo step.
pub(crate) fn sb_text_area(s: &mut Session, p: &Value) -> CmdResult {
    let id = clip_id(p.get("clip").unwrap_or(&Value::Null))?;
    let flag = |key: &str| -> Result<bool, CmdError> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(false),
            Some(v) => v.as_bool().ok_or_else(|| CmdError::BadParams(format!("`{key}` must be true or false"))),
        }
    };
    let percent = |key: &str| -> Result<Option<u16>, CmdError> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_f64()
                .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
                .map(|v| Some((v * 10.0).round() as u16))
                .ok_or_else(|| CmdError::BadParams(format!("`{key}` must be a percent of the frame, 0–100"))),
        }
    };
    let align = parse(p, "align", BoxAlign::from_id, "left|center|right")?;
    let (width, height, y) = (percent("width")?, percent("height")?, percent("y")?);
    let (center, reset) = (flag("center")?, flag("reset")?);
    if reset && (align.is_some() || width.is_some() || height.is_some() || y.is_some() || center) {
        return Err(CmdError::BadParams("`reset` lets the title fall where it does by itself; give it alone".into()));
    }
    if !reset && !center && align.is_none() && width.is_none() && height.is_none() && y.is_none() {
        return Err(CmdError::BadParams(
            "give at least one of `align`, `width`, `height`, `y`, `center`, `reset`".into(),
        ));
    }
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let current = board.clip(id).and_then(|c| c.text_style.area);
    let area = if reset {
        None
    } else {
        let mut b = current.unwrap_or(TextBox::BANNER);
        if center {
            b = b.centered();
        }
        b.align = align.unwrap_or(b.align);
        b.width = width.unwrap_or(b.width);
        b.height = height.unwrap_or(b.height);
        b.y = y.unwrap_or(b.y);
        Some(b)
    };
    let changed = set_text_area(&mut board, id, area).map_err(|e| CmdError::BadParams(e.to_string()))?;
    let area = board.clip(id).and_then(|c| c.text_style.area);
    if changed {
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    Ok(json!({
        "clip": id.0,
        "changed": changed,
        "area": area.map(area_json),
        "from": area_json(TextBox::BANNER),
    }))
}

/// `storyboard.title {clip}` (L1): the clip's title as layers — area, background, plate, text,
/// effects — and who draws it. Read only.
pub(crate) fn sb_title(s: &mut Session, p: &Value) -> CmdResult {
    let id = clip_id(p.get("clip").unwrap_or(&Value::Null))?;
    let board = s.storyboard().ok_or_else(no_board)?;
    let clip = board.clip(id).ok_or_else(|| CmdError::BadParams(format!("no clip #{}", id.0)))?;
    let designs = s.theme_designs();
    let title = assetcraft_animatic::title_of(&designs, s.user_looks(), clip)
        .ok_or_else(|| CmdError::BadParams(format!("clip #{} has no text (only title cards and overlays do)", id.0)))?;
    let mut out = serde_json::to_value(&title).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    if let Some(o) = out.as_object_mut() {
        o.insert("clip".into(), json!(id.0));
        o.insert("renderer".into(), json!(title.renderer()));
        o.insert("area".into(), title.area.map_or(Value::Null, area_json));
    }
    Ok(out)
}

/// An area in percent of the frame, as the command takes it.
fn area_json(b: TextBox) -> Value {
    let pct = |v: u16| f64::from(v) / 10.0;
    json!({ "align": b.align.id(), "width": pct(b.width), "height": pct(b.height), "y": pct(b.y) })
}

/// `theme`: a built-in's id, or a loaded user theme's (TP2a-2).
fn theme_choice(s: &Session, p: &Value) -> Result<Option<ThemeChoice>, CmdError> {
    match p.get("theme") {
        None => Ok(None),
        Some(v) => theme_named(s, "theme", v).map(Some),
    }
}

/// `titleTheme` on `storyboard.generate` / `storyboard.new` (Start screen, Paul 2026-11-20): the
/// theme every title card and text overlay of the new board takes; absent or null keeps the
/// preset's own.
pub(crate) fn title_theme_param(s: &Session, p: &Value) -> Result<Option<ThemeChoice>, CmdError> {
    match p.get("titleTheme") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => theme_named(s, "titleTheme", v).map(Some),
    }
}

/// Give every text clip of `board` the theme `choice` (its animations and place kept); how many.
pub(crate) fn apply_title_theme(board: &mut assetcraft_storyboard::Storyboard, choice: ThemeChoice) -> usize {
    let ids: Vec<_> = board.clips.iter().filter(|c| c.text_kind().is_some()).map(|c| c.id).collect();
    ids.into_iter().filter(|id| set_text_style(board, *id, Some(choice), None, None).is_ok()).count()
}

/// Theme id `v` (param `key`): a built-in's, or a loaded user theme's.
fn theme_named(s: &Session, key: &str, v: &Value) -> Result<ThemeChoice, CmdError> {
    let id = v.as_str().ok_or_else(|| CmdError::BadParams(format!("`{key}` must be a theme id")))?;
    if let Some(t) = TextTheme::from_id(id) {
        return Ok(ThemeChoice::Builtin(t));
    }
    if let Some(u) = ThemeId::new(id).filter(|u| s.user_looks().contains(*u)) {
        return Ok(ThemeChoice::User(u));
    }
    let mut known: Vec<String> = TextTheme::ALL.iter().map(|t| t.id().to_string()).collect();
    known.extend(s.loaded_themes().iter().map(|u| u.theme.id.clone()));
    Err(CmdError::BadParams(format!("`{key}` {id:?} isn't a loaded theme ({})", known.join("|"))))
}

/// A move: one of `ids`, or `"theme"` to follow the theme again (`Some(None)`).
fn parse_or_theme<T>(
    p: &Value,
    key: &str,
    from_id: fn(&str) -> Option<T>,
    ids: &str,
) -> Result<Option<Option<T>>, CmdError> {
    if p.get(key).and_then(Value::as_str) == Some("theme") {
        return Ok(Some(None));
    }
    Ok(parse(p, key, from_id, &format!("{ids}|theme"))?.map(Some))
}

/// `duration`: seconds the text stays up (0.5–600), `0` for until the clip ends, or
/// `"theme"` for the theme's.
fn text_duration(p: &Value) -> Result<Option<Option<u32>>, CmdError> {
    let Some(v) = p.get("duration").filter(|v| !v.is_null()) else { return Ok(None) };
    if v.as_str() == Some("theme") {
        return Ok(Some(None));
    }
    let secs = v.as_f64().filter(|s| s.is_finite()).ok_or_else(|| {
        CmdError::BadParams("`duration` must be seconds (0.5–600), 0 for until the clip ends, or \"theme\"".into())
    })?;
    if secs == 0.0 {
        return Ok(Some(Some(0)));
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
