//! The Cut as an OpenTimelineIO file for FilmCraft (SF1b-2; D-SF7; the shape FilmCraft's own
//! importer read as written in SF1b-0, `docs/sibling-apis/filmcraft-otio-import.md`).
//!
//! `storyboard.writeOtio {dryRun?, path?, overwrite?}`:
//!
//! - **V1:** every clip in Cut order, back to back in whole frames ([`cut_spans`]): a title card
//!   or an overlay (merged with its shot) as its rendered `.mp4` (`storyboard.renderTitles`), a
//!   shot as its own file (a video from its in-point), a clip with no picture as a gap.
//! - **A1:** the song through the board's audio edit: each kept stretch of the file a clip, at
//!   the song's level (`metadata.filmcraft.gain_db`).
//! - **Markers:** each title's words on its clip (purple), its comment carrying the key a later
//!   sync finds it by — `assetcraft clip <id> · <theme> · <design>` — since FilmCraft keeps no
//!   other app's metadata (OT-5); each scene on the sequence (blue).
//! - Times are whole frames at the board's rate: each clip from its start, to the nearest frame,
//!   to the next one's, so FilmCraft's timeline is the Cut's frames with no gap or overlap (it
//!   rounds to frames itself: an exact length more than 1 ms off its frames is dropped,
//!   `crates/interchange/src/otio.rs:697-700` @ `960a661`). A source's in-point also goes as
//!   FilmCraft's exact ticks (`metadata.filmcraft.source_in_ticks`; its tick and ours are both
//!   254,016,000,000 a second).
//!
//! What the file can't carry yet is listed in `notes` (a still's pan, an inset, a split or
//! framed layout, a transition between clips, the song's crossfades and fade) so the dry run says
//! plainly what FilmCraft will be missing. `dryRun` returns the plan and the document and writes
//! nothing; otherwise every title must be rendered first, and the file is written atomically
//! (`<name>.otio` beside the storyboard unless `path` says otherwise; a different existing file
//! only with `overwrite`).

use crate::export_job::write_atomic;
use crate::title_render::{RenderPlan, cut_spans, frames_of};
use crate::{CmdError, CmdResult, Session};
use assetcraft_storyboard::{Clip, Storyboard, TICKS_PER_SECOND, TextKind, Ticks, Transition};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The file's own version, in `metadata.assetcraft`.
pub const OTIO_VERSION: u32 = 1;

/// A `RationalTime.1` of `frames` at `rate`.
fn rt(frames: u64, rate: u32) -> Value {
    json!({ "OTIO_SCHEMA": "RationalTime.1", "rate": rate, "value": frames })
}

/// A `TimeRange.1` from `start` for `frames`, at `rate`.
fn tr(start: u64, frames: u64, rate: u32) -> Value {
    json!({ "OTIO_SCHEMA": "TimeRange.1", "start_time": rt(start, rate), "duration": rt(frames, rate) })
}

/// `path` as a `file://` URL: forward slashes, a drive letter after `file:///`, and every byte
/// but letters, digits, `-._~/:` percent-encoded (spaces as `%20`; FilmCraft decodes them, OT-3).
pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::with_capacity(text.len() + 8);
    out.push_str(if text.starts_with('/') { "file://" } else { "file:///" });
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// An `ExternalReference.1` to `path` (its length left for FilmCraft to read from the file).
fn external(path: &Path) -> Value {
    json!({
        "OTIO_SCHEMA": "ExternalReference.1",
        "name": path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        "target_url": file_url(path),
        "available_range": Value::Null,
        "metadata": {},
    })
}

/// A `Marker.2`.
fn marker(name: &str, color: &str, comment: &str, (start, frames): (u64, u64), rate: u32, ticks: (i64, i64)) -> Value {
    json!({
        "OTIO_SCHEMA": "Marker.2",
        "name": name,
        "color": color,
        "comment": comment,
        "marked_range": tr(start, frames, rate),
        "metadata": { "filmcraft": { "start_ticks": ticks.0, "duration_ticks": ticks.1 } },
    })
}

/// A clip's name in FilmCraft: `#NN CODE`, then its words for a title.
fn clip_name(index: usize, clip: &Clip) -> String {
    let head = format!("#{:02} {}", index + 1, clip.shot_type.code());
    match clip.text() {
        Some(words) if clip.text_kind() == Some(TextKind::Card) => format!("TITLE · {}", first_line(words)),
        Some(words) => format!("{head} · {}", first_line(words)),
        None => head,
    }
}

fn first_line(words: &str) -> &str {
    words.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("")
}

/// The key a later sync finds a title by, in its marker's comment (OT-5).
pub fn title_key(clip: &Clip) -> String {
    let style = &clip.text_style;
    let theme = style.theme_id.as_ref().map_or_else(|| style.theme.id().to_string(), |t| t.as_str().to_string());
    let design = style.design.map_or("theme", |d| d.id());
    format!("assetcraft clip {} · {theme} · {design}", clip.id.0)
}

/// Everything the writer worked out.
#[derive(Debug, Clone)]
pub struct OtioPlan {
    /// The file.
    pub path: PathBuf,
    /// The document.
    pub otio: Value,
    /// V1's clips (gaps included).
    pub video: usize,
    /// A1's clips.
    pub audio: usize,
    /// Markers on clips and on the sequence.
    pub markers: usize,
    /// Titles whose `.mp4` isn't rendered yet.
    pub unrendered: Vec<u32>,
    /// What FilmCraft won't get, in words.
    pub notes: Vec<String>,
}

/// The document for `board`, its titles as `titles` plans them.
pub fn document(board: &Storyboard, titles: &RenderPlan, path: PathBuf) -> OtioPlan {
    document_with(board, titles, path, &[])
}

/// [`document`], with the clips at `graphics` (Cut indices) left for graphic clips (FC-2): such
/// a clip goes as what is under its title — an overlay's shot, a card's own picture, else a gap
/// — and gets no title marker (the graphic clip carries it).
pub fn document_with(board: &Storyboard, titles: &RenderPlan, path: PathBuf, graphics: &[usize]) -> OtioPlan {
    let rate = titles.fps.max(1);
    let tick = TICKS_PER_SECOND / u64::from(rate);
    let spans = cut_spans(board, rate);
    let ticks_of = |frames: u64| i64::try_from(frames.saturating_mul(tick)).unwrap_or(i64::MAX);
    let mut notes = Vec::new();
    let mut unrendered = Vec::new();
    let mut markers = 0usize;
    let mut v1 = Vec::with_capacity(board.clips.len());
    for (index, clip) in board.clips.iter().enumerate() {
        let Some(span) = spans.get(index).copied() else { continue };
        let name = clip_name(index, clip);
        let mut fc = json!({});
        let title = titles.titles.iter().find(|t| t.index == index).filter(|_| !graphics.contains(&index));
        let (reference, start) = match (title, clip.asset_ids.first()) {
            (Some(t), _) => {
                if !t.reuse {
                    unrendered.push(t.clip);
                }
                (Some(external(&t.path)), 0)
            }
            (None, Some(asset)) => {
                let file = Path::new(asset.0.as_str());
                if assetcraft_media::is_video(file) {
                    let at = frames_of(clip.source_in, rate);
                    fc["source_in_ticks"] = json!(i64::try_from(clip.source_in.0).unwrap_or(i64::MAX));
                    (Some(external(file)), at)
                } else {
                    if clip.motion.is_some() {
                        notes.push(format!(
                            "{name}: a still with a pan or zoom goes as the still (its move isn't carried)"
                        ));
                    }
                    (Some(external(file)), 0)
                }
            }
            (None, None) => (None, 0),
        };
        if clip.transition_in != Transition::Cut || clip.transition_out != Transition::Cut {
            notes.push(format!("{name}: its transition isn't carried (a cut in FilmCraft)"));
        }
        let mut clip_markers = Vec::new();
        if let (Some(_), Some(words)) = (title, clip.text()) {
            clip_markers.push(marker(
                first_line(words),
                "PURPLE",
                &title_key(clip),
                (0, span.frames),
                rate,
                (0, ticks_of(span.frames)),
            ));
            markers += 1;
        }
        let item = match reference {
            Some(r) => json!({
                "OTIO_SCHEMA": "Clip.2",
                "name": name,
                "source_range": tr(start, span.frames, rate),
                "media_references": { "DEFAULT_MEDIA": r },
                "active_media_reference_key": "DEFAULT_MEDIA",
                "markers": clip_markers,
                "effects": [],
                "enabled": true,
                "metadata": { "filmcraft": fc },
            }),
            None => json!({
                "OTIO_SCHEMA": "Gap.1",
                "name": format!("{name} · no picture"),
                "source_range": tr(0, span.frames, rate),
                "effects": [],
                "markers": [],
                "metadata": {},
            }),
        };
        v1.push(item);
    }
    // Scenes, on the sequence: from a scene's first clip to its last.
    let mut stack_markers = Vec::new();
    for scene in &board.scenes {
        let mine: Vec<usize> =
            board.clips.iter().enumerate().filter(|(_, c)| c.scene_id == scene.id).map(|(i, _)| i).collect();
        let (Some(a), Some(b)) = (mine.first().and_then(|i| spans.get(*i)), mine.last().and_then(|i| spans.get(*i)))
        else {
            continue;
        };
        let frames = (b.first + b.frames).saturating_sub(a.first);
        let words = board.scene_title(scene.id).map(first_line).unwrap_or("");
        let name = if words.is_empty() { format!("Scene {}", scene.id.0) } else { format!("Scene · {words}") };
        let comment = scene.song_section.clone().unwrap_or_default();
        let ticks = (ticks_of(a.first), ticks_of(frames));
        stack_markers.push(marker(&name, "BLUE", &comment, (a.first, frames), rate, ticks));
        markers += 1;
        if scene.inset.is_some() {
            notes.push(format!("{name}: its inset isn't carried yet"));
        }
    }
    if board.layout.is_some() {
        notes.push("the board's split or framed layout isn't carried yet: shots go full frame".into());
    }
    // The song, through its edit.
    let mut a1 = Vec::new();
    if let Some(track) = board.track.as_ref().filter(|t| Path::new(&t.id.0).is_absolute()) {
        let file = Path::new(&track.id.0);
        let total = spans.last().map_or(0, |s| s.first + s.frames);
        let stretches: Vec<(Ticks, Ticks)> = match &track.edit {
            Some(edit) if edit.sample_rate > 0 && !edit.segments.is_empty() => {
                let to_ticks = |f: u64| {
                    Ticks(
                        u64::try_from(u128::from(f) * u128::from(TICKS_PER_SECOND) / u128::from(edit.sample_rate))
                            .unwrap_or(u64::MAX),
                    )
                };
                if edit.crossfade_frames > 0 {
                    notes.push("the song's crossfades become cuts".into());
                }
                if edit.fade_out.is_some() {
                    notes.push("the song's fade-out isn't carried".into());
                }
                edit.segments.iter().filter(|s| s[1] > s[0]).map(|s| (to_ticks(s[0]), to_ticks(s[1] - s[0]))).collect()
            }
            _ => vec![(Ticks::ZERO, track.duration)],
        };
        let mut at = 0u64;
        for (from, length) in stretches {
            if at >= total {
                break;
            }
            let frames = frames_of(length, rate).min(total - at).max(1);
            a1.push(json!({
                "OTIO_SCHEMA": "Clip.2",
                "name": track.name,
                "source_range": tr(frames_of(from, rate), frames, rate),
                "media_references": { "DEFAULT_MEDIA": external(file) },
                "active_media_reference_key": "DEFAULT_MEDIA",
                "markers": [],
                "effects": [],
                "enabled": true,
                "metadata": { "filmcraft": {
                    "gain_db": f64::from(track.level_db),
                    "source_in_ticks": i64::try_from(from.0).unwrap_or(i64::MAX),
                } },
            }));
            at += frames;
        }
        if !track.volume.is_empty() {
            notes.push("the song's volume line isn't carried (its level is)".into());
        }
    }
    let (video, audio) = (v1.len(), a1.len());
    let otio = json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": board.name,
        "global_start_time": rt(0, rate),
        "metadata": { "assetcraft": {
            "version": OTIO_VERSION,
            "board": board.name,
            "width": titles.width,
            "height": titles.height,
            "fps": rate,
        } },
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "name": "tracks",
            "markers": stack_markers,
            "effects": [],
            "metadata": {},
            "children": [
                { "OTIO_SCHEMA": "Track.1", "name": "V1", "kind": "Video", "children": v1, "markers": [], "effects": [], "metadata": {} },
                { "OTIO_SCHEMA": "Track.1", "name": "A1", "kind": "Audio", "children": a1, "markers": [], "effects": [], "metadata": {} },
            ],
        },
    });
    OtioPlan { path, otio, video, audio, markers, unrendered, notes }
}

/// `<folder>/<name>.otio` beside the saved storyboard.
fn default_path(s: &Session) -> Result<PathBuf, CmdError> {
    let document = s.document().ok_or_else(|| {
        CmdError::Unavailable("save the storyboard first: its titles go in a folder beside it".into())
    })?;
    let dir = document.parent().ok_or_else(|| CmdError::Unavailable("the storyboard has no folder".into()))?;
    Ok(dir.join(format!("{}.otio", crate::document_cmd::document_name(document))))
}

/// `storyboard.writeOtio {dryRun?, path?, overwrite?}`.
pub(crate) fn sb_write_otio(s: &mut Session, p: &Value) -> CmdResult {
    let flag = |name: &str| p.get(name).and_then(Value::as_bool).unwrap_or(false);
    let titles = crate::title_render::plan(s)?;
    let path = match p.get("path").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()) {
        Some(text) if text.len() <= 4096 => {
            let path = PathBuf::from(text);
            if !path.is_absolute() {
                return Err(CmdError::BadParams("`path` must be absolute".into()));
            }
            if path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() != Some("otio") {
                return Err(CmdError::BadParams("`path` must end with .otio".into()));
            }
            path
        }
        Some(_) => return Err(CmdError::BadParams("`path` is too long".into())),
        None => default_path(s)?,
    };
    let board = s.boards.board.as_ref().ok_or_else(crate::storyboard::no_board)?;
    if board.clips.is_empty() {
        return Err(CmdError::Unavailable("the storyboard has no clips".into()));
    }
    let plan = document(board, &titles, path);
    let summary = json!({
        "path": plan.path.to_string_lossy(),
        "fps": titles.fps,
        "video": plan.video,
        "audio": plan.audio,
        "markers": plan.markers,
        "unrendered": plan.unrendered,
        "problems": titles.problems.iter().map(|x| json!({ "clip": x.clip, "what": x.what, "why": x.why })).collect::<Vec<_>>(),
        "notes": plan.notes,
    });
    if flag("dryRun") {
        let mut out = summary;
        out["dryRun"] = json!(true);
        out["otio"] = plan.otio;
        return Ok(out);
    }
    if !titles.problems.is_empty() {
        return Err(CmdError::Unavailable(format!(
            "{} title problem(s) first (storyboard.renderTitles {{dryRun: true}} lists them)",
            titles.problems.len()
        )));
    }
    if !plan.unrendered.is_empty() {
        return Err(CmdError::Unavailable(format!(
            "{} title(s) aren't rendered yet: run storyboard.renderTitles first",
            plan.unrendered.len()
        )));
    }
    if plan.path.exists() && !flag("overwrite") && p.get("path").is_some() {
        return Err(CmdError::Unavailable(format!(
            "{} exists: pass `overwrite: true` to replace it",
            plan.path.display()
        )));
    }
    let text = serde_json::to_string_pretty(&plan.otio).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    let bytes = write_atomic(&plan.path, |out| {
        out.write_all(text.as_bytes()).map_err(|e| assetcraft_video::VideoError::Encode(e.to_string()))?;
        Ok(text.len() as u64)
    })
    .map_err(CmdError::Unavailable)?;
    let mut out = summary;
    out["written"] = json!(true);
    out["bytes"] = json!(bytes);
    out["message"] = json!(format!(
        "Wrote {} for FilmCraft: {} clip(s), the song in {} part(s), {} marker(s)",
        plan.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        plan.video,
        plan.audio,
        plan.markers
    ));
    Ok(out)
}
