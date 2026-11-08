//! `storyboard.inset` / `storyboard.insetClear` (SR2a, `docs/storyboard-plan.md` § Sections): a
//! section's inset lane — the user's talking video, b-roll, an interview, a graphic — in a corner
//! over the section's clips. One undo step each; a refused change leaves none.

use crate::storyboard::no_board;
use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{AssetId, Corner, Inset, InsetKind, InsetSize, Limits, SceneId, Storyboard, Ticks};
use serde_json::{Value, json};

/// `scene` as a scene number.
pub(crate) fn scene_id(v: &Value) -> Result<SceneId, CmdError> {
    v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .map(SceneId)
        .ok_or_else(|| CmdError::BadParams("`scene` must be a scene number".into()))
}

fn kind(v: &Value) -> Result<InsetKind, CmdError> {
    match v.as_str() {
        Some("video") => Ok(InsetKind::Video),
        Some("graphic") => Ok(InsetKind::Graphic),
        _ => Err(CmdError::BadParams("`kind` must be video or graphic".into())),
    }
}

fn corner(v: &Value) -> Result<Corner, CmdError> {
    match v.as_str() {
        Some("topLeft" | "TL") => Ok(Corner::TopLeft),
        Some("topRight" | "TR") => Ok(Corner::TopRight),
        Some("bottomLeft" | "BL") => Ok(Corner::BottomLeft),
        Some("bottomRight" | "BR") => Ok(Corner::BottomRight),
        _ => Err(CmdError::BadParams("`corner` must be topLeft, topRight, bottomLeft or bottomRight".into())),
    }
}

fn size(v: &Value) -> Result<InsetSize, CmdError> {
    match v.as_str() {
        Some("small" | "S") => Ok(InsetSize::Small),
        Some("medium" | "M") => Ok(InsetSize::Medium),
        Some("large" | "L") => Ok(InsetSize::Large),
        _ => Err(CmdError::BadParams("`size` must be small, medium or large".into())),
    }
}

/// A whole number of dB within `range`.
fn level(p: &Value, name: &str, range: (i8, i8)) -> Result<Option<i8>, CmdError> {
    match p.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .and_then(|n| i8::try_from(n).ok())
            .filter(|n| (range.0..=range.1).contains(n))
            .map(Some)
            .ok_or_else(|| CmdError::BadParams(format!("`{name}` must be {}..={} dB", range.0, range.1))),
    }
}

fn flag(p: &Value, name: &str) -> Result<Option<bool>, CmdError> {
    match p.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(CmdError::BadParams(format!("`{name}` must be true or false"))),
    }
}

/// The scene the inset belongs to; refused for a scene with nothing but title cards.
fn section(board: &Storyboard, id: SceneId) -> Result<usize, CmdError> {
    let index = board
        .scenes
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| CmdError::BadParams(format!("no scene {}", id.0)))?;
    if board.inset_span(id).next().is_none() {
        return Err(CmdError::Unavailable(format!(
            "scene {} has only title cards: an inset needs a shot to sit over",
            id.0
        )));
    }
    Ok(index)
}

/// Video extensions an inset treats as a video (its length can size the section).
const VIDEO_EXT: [&str; 7] = ["mp4", "m4v", "mov", "mkv", "webm", "avi", "3gp"];

/// `storyboard.inset {scene, asset?, kind?, corner?, size?, mixAudio?, swap?, seconds?, fit?}`:
/// set or change a section's inset. Omitted fields keep their value (an empty slot is made
/// first when the section has none); `asset: null` empties the slot; `seconds` stretches the
/// section's shots (pro rata, title cards kept) to the recording's length; without `seconds`,
/// a new video file sizes the section to its own length when the container says it (`fit`,
/// on by default; MP4 / MOV today) — "the section takes its recording's length". `kind` is
/// read off the extension when not given.
pub(crate) fn sb_inset(s: &mut Session, p: &Value) -> CmdResult {
    let id = scene_id(p.get("scene").unwrap_or(&Value::Null))?;
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let index = section(&board, id)?;
    let limits = Limits::default();
    let mut inset = board.scenes[index].inset.clone().unwrap_or_else(|| Inset::slot(InsetKind::Video));
    if let Some(v) = p.get("kind") {
        inset.kind = kind(v)?;
    }
    match p.get("asset") {
        None => {}
        Some(Value::Null) => inset.asset = None,
        Some(v) => {
            let a = v
                .as_str()
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .ok_or_else(|| CmdError::BadParams("`asset` must be a path, or null to empty the slot".into()))?;
            if a.len() > limits.max_string {
                return Err(CmdError::BadParams("`asset` is too long".into()));
            }
            inset.asset = Some(AssetId(a.to_owned()));
        }
    }
    if let Some(v) = p.get("corner") {
        inset.corner = corner(v)?;
    }
    if let Some(v) = p.get("size") {
        inset.size = size(v)?;
    }
    if let Some(b) = flag(p, "mixAudio")? {
        inset.mix_audio = b;
    }
    if let Some(b) = flag(p, "swap")? {
        inset.swap = b;
    }
    if let Some(v) = level(p, "musicDb", assetcraft_storyboard::MUSIC_DB_RANGE)? {
        inset.music_db = v;
    }
    if let Some(v) = level(p, "voiceDb", assetcraft_storyboard::VOICE_DB_RANGE)? {
        inset.voice_db = v;
    }
    match p.get("note") {
        None => {}
        Some(Value::Null) => inset.note = None,
        Some(Value::String(n)) => {
            let n = n.trim();
            if n.len() > limits.max_string {
                return Err(CmdError::BadParams("`note` is too long".into()));
            }
            inset.note = (!n.is_empty()).then(|| n.to_owned());
        }
        Some(_) => return Err(CmdError::BadParams("`note` must be text, or null to clear it".into())),
    }
    // A new file: its kind from the extension unless given, and a video's length when asked.
    let new_file = matches!(p.get("asset"), Some(Value::String(_)));
    if new_file && p.get("kind").is_none() {
        inset.kind = match inset.asset.as_ref().map(|a| is_video(&a.0)) {
            Some(true) => InsetKind::Video,
            _ => InsetKind::Graphic,
        };
    }
    let fit = flag(p, "fit")?.unwrap_or(true);
    let mut asset_secs: Option<f64> = None;
    let stretched = match p.get("seconds") {
        None | Some(Value::Null) if new_file && fit && inset.kind == InsetKind::Video => {
            let secs = inset
                .asset
                .as_ref()
                .and_then(|a| assetcraft_probe::media_duration(s.services().as_ref(), std::path::Path::new(&a.0)));
            asset_secs = secs;
            match secs {
                Some(v) if v >= crate::MIN_CLIP_SECS => Some(stretch_section(&mut board, id, v, &limits)?),
                _ => None,
            }
        }
        None | Some(Value::Null) => None,
        Some(v) => {
            let secs = v
                .as_f64()
                .filter(|v| v.is_finite() && *v >= crate::MIN_CLIP_SECS)
                .ok_or_else(|| CmdError::BadParams("`seconds` must be a length of at least 0.5 s".into()))?;
            Some(stretch_section(&mut board, id, secs, &limits)?)
        }
    };
    let changed = board.scenes[index].inset.as_ref() != Some(&inset) || stretched.is_some();
    if changed {
        board.scenes[index].inset = Some(inset.clone());
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    let board = s.boards.board.as_ref().ok_or_else(no_board)?;
    let span: f64 = board.inset_span(id).map(|c| c.duration.as_secs_f64()).sum();
    Ok(json!({
        "scene": id.0,
        "changed": changed,
        "inset": inset_json(&inset),
        "spanSeconds": span,
        "assetSeconds": asset_secs,
        "total": board.total().timecode(),
        "message": match (&inset.asset, stretched) {
            (Some(a), Some(_)) => format!("Inset on scene {}: {} · section now {span:.1} s", id.0, name_of(a)),
            (Some(a), None) if new_file && fit && inset.kind == InsetKind::Video => {
                format!("Inset on scene {}: {} (length unknown: the section keeps {span:.1} s)", id.0, name_of(a))
            }
            (Some(a), None) => format!("Inset on scene {}: {}", id.0, name_of(a)),
            (None, _) => format!("Empty inset slot on scene {}", id.0),
        },
    }))
}

/// `storyboard.insetClear {scene, remove?}`: empty the slot, or drop the lane with `remove`.
pub(crate) fn sb_inset_clear(s: &mut Session, p: &Value) -> CmdResult {
    let id = scene_id(p.get("scene").unwrap_or(&Value::Null))?;
    let remove = flag(p, "remove")?.unwrap_or(false);
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let index = board
        .scenes
        .iter()
        .position(|sc| sc.id == id)
        .ok_or_else(|| CmdError::BadParams(format!("no scene {}", id.0)))?;
    let after = match (&board.scenes[index].inset, remove) {
        (None, _) => None,
        (Some(_), true) => None,
        (Some(i), false) => Some(Inset { asset: None, ..i.clone() }),
    };
    let changed = board.scenes[index].inset != after;
    if changed {
        board.scenes[index].inset = after.clone();
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    Ok(json!({
        "scene": id.0,
        "changed": changed,
        "inset": after.as_ref().map(inset_json),
        "message": if remove { format!("Inset lane removed from scene {}", id.0) } else { format!("Inset slot on scene {} emptied", id.0) },
    }))
}

/// Stretch (or shrink) a section's shots to `secs` in all, pro rata, title cards kept; the
/// last shot takes the rounding. Returns the new span in seconds.
fn stretch_section(board: &mut Storyboard, scene: SceneId, secs: f64, limits: &Limits) -> Result<f64, CmdError> {
    let want = Ticks::from_secs_f64(secs).ok_or_else(|| CmdError::BadParams("`seconds` is out of range".into()))?;
    if want > limits.max_duration {
        return Err(CmdError::BadParams("a section can't be longer than 6 hours".into()));
    }
    let shots: Vec<usize> = board
        .clips
        .iter()
        .enumerate()
        .filter(|(_, c)| c.scene_id == scene && c.shot_type != assetcraft_storyboard::ShotType::Ttl)
        .map(|(i, _)| i)
        .collect();
    let min = Ticks::from_secs_f64(crate::MIN_CLIP_SECS).unwrap_or(Ticks(1));
    let floor = min.0.saturating_mul(shots.len() as u64);
    if want.0 < floor {
        return Err(CmdError::BadParams(format!(
            "{} shot{} need at least {:.1} s",
            shots.len(),
            if shots.len() == 1 { "" } else { "s" },
            Ticks(floor).as_secs_f64()
        )));
    }
    let before: u64 = shots.iter().map(|&i| board.clips[i].duration.0).sum();
    let n = shots.len() as u64;
    // Pro rata shares (u128 keeps the product exact), the rounding to the last shot, then every
    // shot at least the minimum, taken from the longest.
    let mut shares: Vec<u64> = shots
        .iter()
        .map(|&i| {
            if before == 0 {
                want.0 / n
            } else {
                let exact = (u128::from(want.0) * u128::from(board.clips[i].duration.0)) / u128::from(before);
                u64::try_from(exact).unwrap_or(u64::MAX)
            }
        })
        .collect();
    let given: u64 = shares.iter().sum();
    if let Some(last) = shares.last_mut() {
        *last += want.0.saturating_sub(given);
    }
    for k in 0..shares.len() {
        if shares[k] < min.0 {
            let short = min.0 - shares[k];
            let longest = (0..shares.len()).filter(|&j| j != k).max_by_key(|&j| shares[j]);
            if let Some(j) = longest.filter(|&j| shares[j] >= min.0 + short) {
                shares[j] -= short;
                shares[k] = min.0;
            }
        }
    }
    for (&i, share) in shots.iter().zip(shares) {
        board.clips[i].duration = Ticks(share);
    }
    let rest: u64 = board.clips.iter().map(|c| c.duration.0).sum();
    if Ticks(rest) > limits.max_duration {
        return Err(CmdError::BadParams("the Cut would be longer than the limit (6 hours)".into()));
    }
    Ok(want.as_secs_f64())
}

/// A video by its extension.
fn is_video(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| VIDEO_EXT.contains(&e.as_str()))
}

fn name_of(a: &AssetId) -> String {
    std::path::Path::new(&a.0).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| a.0.clone())
}

/// An inset as the state and the CLI show it.
pub(crate) fn inset_json(i: &Inset) -> Value {
    json!({
        "kind": match i.kind { InsetKind::Video => "video", InsetKind::Graphic => "graphic" },
        "asset": i.asset.as_ref().map(|a| a.0.clone()),
        "corner": match i.corner {
            Corner::TopLeft => "topLeft",
            Corner::TopRight => "topRight",
            Corner::BottomLeft => "bottomLeft",
            Corner::BottomRight => "bottomRight",
        },
        "size": match i.size { InsetSize::Small => "small", InsetSize::Medium => "medium", InsetSize::Large => "large" },
        "mixAudio": i.mix_audio,
        "swap": i.swap,
        "musicDb": i.music_db,
        "voiceDb": i.voice_db,
        "note": i.note,
    })
}

/// `inset: {kind?, corner?, size?, mixAudio?, addTo?: every | first}` on `storyboard.new
/// {template}` / `storyboard.preview {template}` (SR2d, the Start screen's INSET panel): every
/// section scene (or only the first) gets an empty slot with these options, ready for a drop.
/// Opening and closing scenes are left alone. Absent → nothing changes.
pub(crate) fn apply_defaults(board: &mut Storyboard, p: &Value) -> Result<(), CmdError> {
    let Some(d) = p.get("inset").filter(|v| !v.is_null()) else { return Ok(()) };
    if !d.is_object() {
        return Err(CmdError::BadParams("`inset` must be an object".into()));
    }
    let mut slot = Inset::slot(InsetKind::Video);
    if let Some(v) = d.get("kind") {
        slot.kind = kind(v)?;
    }
    if let Some(v) = d.get("corner") {
        slot.corner = corner(v)?;
    }
    if let Some(v) = d.get("size") {
        slot.size = size(v)?;
    }
    if let Some(b) = flag(d, "mixAudio")? {
        slot.mix_audio = b;
    }
    let first_only = match d.get("addTo").and_then(Value::as_str) {
        None | Some("every") => false,
        Some("first") => true,
        Some(_) => return Err(CmdError::BadParams("`inset.addTo` must be every or first".into())),
    };
    let ids: Vec<SceneId> = board
        .inset_scenes()
        .filter(|sc| sc.part == assetcraft_storyboard::ScenePart::Section)
        .map(|sc| sc.id)
        .collect();
    for (i, id) in ids.into_iter().enumerate() {
        if first_only && i > 0 {
            break;
        }
        if let Some(sc) = board.scenes.iter_mut().find(|sc| sc.id == id) {
            sc.inset = Some(slot.clone());
        }
    }
    Ok(())
}

/// `storyboard.voiceover {asset?: path | null, musicDb?, voiceDb?}` (SR4b): set, change or
/// remove (`asset: null`) the narration under the whole Cut. Its levels are an inset's: the
/// song at `musicDb` while it speaks, the voice at `voiceDb`. One undo step; unchanged → none.
pub(crate) fn sb_voiceover(s: &mut Session, p: &Value) -> CmdResult {
    let mut board = s.boards.board.clone().ok_or_else(no_board)?;
    let limits = Limits::default();
    let mut vo = board.voiceover.clone();
    match p.get("asset") {
        None => {}
        Some(Value::Null) => vo = None,
        Some(v) => {
            let a =
                v.as_str().map(str::trim).filter(|a| !a.is_empty() && a.len() <= limits.max_string).ok_or_else(
                    || CmdError::BadParams("`asset` must be a path, or null to remove the voiceover".into()),
                )?;
            let keep = vo.take();
            let mut next = assetcraft_storyboard::Voiceover::new(AssetId(a.to_owned()));
            if let Some(k) = keep {
                next.music_db = k.music_db;
                next.voice_db = k.voice_db;
            }
            vo = Some(next);
        }
    }
    let music = level(p, "musicDb", assetcraft_storyboard::MUSIC_DB_RANGE)?;
    let voice = level(p, "voiceDb", assetcraft_storyboard::VOICE_DB_RANGE)?;
    if music.is_some() || voice.is_some() {
        let v = vo.as_mut().ok_or_else(|| CmdError::Unavailable("add a voiceover first".into()))?;
        if let Some(m) = music {
            v.music_db = m;
        }
        if let Some(x) = voice {
            v.voice_db = x;
        }
    }
    let changed = board.voiceover != vo;
    if changed {
        board.voiceover = vo.clone();
        s.boards.checkpoint();
        s.boards.board = Some(board);
    }
    Ok(json!({
        "changed": changed,
        "voiceover": vo.as_ref().map(|v| json!({ "asset": v.asset.0, "musicDb": v.music_db, "voiceDb": v.voice_db })),
        "message": match &vo {
            Some(v) => format!("Voiceover: {} (the song ducks to {} dB while it speaks)", name_of(&v.asset), v.music_db),
            None => "No voiceover".to_owned(),
        },
    }))
}
