//! The Cut as a FilmCraft project (`.fcproj`), its in-app titles as FilmCraft graphic clips you
//! can edit there (FC-2; D-SF8, decided 2026-11-27: write the project file; spikes in
//! `docs/sibling-apis/filmcraft-graphic-clips.md`).
//!
//! `storyboard.writeFcproj {dryRun?, path?, overwrite?}`:
//!
//! 1. The Cut as OTIO ([`crate::otio_out::document_with`]) — shots, the song through its edit,
//!    scene markers — with every in-app title left out of V1 (an overlay's shot stays; a card is
//!    its own picture or a gap), read by FilmCraft's own importer (`filmcraft_interchange`).
//! 2. Each in-app title added as a graphic clip built from the compositor's own layout of it
//!    ([`assetcraft_animatic::layout`], FC-1): a card on V1 where it sits, an overlay on V2 over
//!    its shot. Its layers, bottom to top: a card's background (the theme's gradient) and decor,
//!    an overlay's plate, the decor that moves with the words (an accent bar, a card's rules),
//!    then the words — the same face, its em size, the same tracking, broken into the same lines
//!    at the same places, in capitals when the theme sets them (kept as typed, FilmCraft's
//!    All Caps on), the theme's shadow, outline or glow as FilmCraft's own. The in and out are
//!    keyframes sampled from the clip's own timing (opacity, the rise, the zoom). Its marker
//!    carries the key a later sync finds it by.
//! 3. A title an EffectCraft design draws goes as its render (SF1b-1), merged with its shot.
//!
//! What FilmCraft can't take as it is is listed in `notes` (a backdrop's grain or more than two
//! stops, a typewriter, the OTIO's own notes). `dryRun` returns the plan and writes nothing; a
//! real run needs every design-drawn title rendered and writes the file atomically
//! (`<name>.fcproj` beside the storyboard; a path of the user's needs `overwrite` to replace one).

use crate::export_job::write_atomic;
use crate::otio_out::{document_with, title_key};
use crate::title_render::{RenderPlan, cut_spans};
use crate::{CmdError, CmdResult, Session};
use assetcraft_animatic::Compositor;
use assetcraft_animatic::layout::{LayoutBackground, LayoutRect, TitleLayout};
use assetcraft_storyboard::{AnimFrame, Clip, Renderer, TICKS_PER_SECOND, TextFinish, TextKind};
use filmcraft_geom::Vec2;
use filmcraft_project::graphic::{ensure_layer_uids, new_shape_layer, new_text_layer};
use filmcraft_project::{EffectInstance, ItemKind, Label, Marker, MarkerId, MarkerKind, Param, ParamValue, TrackKind};
use filmcraft_time::{FrameRate, Tick, TimeRange};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

/// Everything the writer worked out.
#[derive(Debug)]
pub struct FcprojPlan {
    /// The file.
    pub path: PathBuf,
    /// The project file's bytes.
    pub bytes: Vec<u8>,
    /// Titles that went as graphic clips (clip ids).
    pub graphics: Vec<u32>,
    /// Titles that went as their render (clip ids).
    pub rendered: Vec<u32>,
    /// Design-drawn titles not rendered yet (clip ids).
    pub unrendered: Vec<u32>,
    /// What FilmCraft won't get, in words.
    pub notes: Vec<String>,
}

fn rgba(hex: u32, alpha: f32) -> ParamValue {
    let c = |shift: u32| ((hex >> shift) & 0xff) as f32 / 255.0;
    ParamValue::Color([c(16), c(8), c(0), alpha.clamp(0.0, 1.0)])
}

fn set(e: &mut EffectInstance, id: &str, v: ParamValue) {
    match e.param_mut(id) {
        Some(p) => p.value = v,
        None => {
            e.params.insert(id.to_owned(), Param::new(v));
        }
    }
}

fn name(e: &mut EffectInstance, text: &str) {
    set(e, "name", ParamValue::Text(text.to_owned()));
}

/// A rectangle layer over `r`.
fn rectangle(r: &LayoutRect, label: &str) -> EffectInstance {
    let mut e = new_shape_layer(
        0,
        Vec2::new(f64::from(r.x + r.w / 2.0), f64::from(r.y + r.h / 2.0)),
        Vec2::new(f64::from(r.w), f64::from(r.h)),
        Vec::new(),
    );
    set(&mut e, "fill_color", rgba(r.color, 1.0));
    set(&mut e, "opacity", ParamValue::Float(f64::from(r.alpha * 100.0)));
    name(&mut e, label);
    e
}

/// What sits under each layer's own opacity and place, for the keyframes.
struct Layer {
    effect: EffectInstance,
    opacity: f64,
    position: Option<Vec2>,
    moves: bool,
}

/// The graphic clip's layers for `l`, bottom to top, and the notes for what they can't carry.
fn layers(l: &TitleLayout, notes: &mut Vec<String>, label: &str) -> Vec<Layer> {
    let (fw, fh) = (l.width as f32, l.height as f32);
    let mut out = Vec::new();
    let mut push = |effect: EffectInstance, moves: bool| {
        let opacity = effect.f64_at("opacity", Tick::ZERO);
        let position = effect.param("position").map(|p| p.vec2_at(Tick::ZERO));
        out.push(Layer { effect, opacity, position, moves });
    };
    match &l.background {
        Some(LayoutBackground::Backdrop { backdrop, decor }) => {
            let n = usize::from(backdrop.count.clamp(1, 4));
            let mut bg = rectangle(
                &LayoutRect { x: 0.0, y: 0.0, w: fw, h: fh, color: backdrop.stops[0], alpha: 1.0 },
                "Background",
            );
            if n > 1 && !backdrop.radial {
                set(&mut bg, "fill_kind", ParamValue::Choice(1));
                set(&mut bg, "gradient_start", rgba(backdrop.stops[0], 1.0));
                set(&mut bg, "gradient_end", rgba(backdrop.stops[n - 1], 1.0));
                set(&mut bg, "gradient_angle", ParamValue::Float(f64::from(backdrop.angle)));
            }
            if n > 2 || backdrop.radial {
                notes.push(format!("{label}: its background keeps its first and last colours, as a straight blend"));
            }
            if backdrop.grain > 0.0 || backdrop.vignette > 0.0 {
                notes.push(format!("{label}: its background's grain and vignette aren't carried"));
            }
            push(bg, false);
            for (i, r) in decor.iter().enumerate() {
                push(rectangle(r, &format!("Card decor {}", i + 1)), false);
            }
        }
        Some(LayoutBackground::Scrim { color, alpha }) => {
            push(rectangle(&LayoutRect { x: 0.0, y: 0.0, w: fw, h: fh, color: *color, alpha: *alpha }, "Scrim"), false);
        }
        None => {}
    }
    if let Some(p) = &l.plate {
        push(rectangle(p, "Plate"), true);
    }
    for (i, r) in l.decor.iter().enumerate() {
        push(rectangle(r, if i == 0 { "Accent" } else { "Decor" }), true);
    }
    let w = &l.words;
    let Some(first) = w.lines.first() else { return out };
    // Point text, one line per line as drawn; each line aligned on the block's edge or centre, as
    // the compositor aligns it.
    let [left, top, bw, bh] = w.block;
    let x = match w.align {
        ..=-1 => left,
        0 => left + bw / 2.0,
        1.. => left + bw,
    };
    let text: Vec<&str> = w.lines.iter().map(|line| line.typed.as_str()).collect();
    let m = w.metrics;
    let em = f64::from(m.em.max(1.0));
    // Anchored on the block's middle, so the theme's push or zoom grows it from there as the
    // compositor does (it re-sets the words around their middle at each size).
    let lift = top + bh / 2.0 - first.baseline;
    let mut words = new_text_layer(&text.join("\n"), Vec2::new(f64::from(x), f64::from(first.baseline + lift)), em);
    for (k, v) in [
        ("font", ParamValue::Text(w.family.clone())),
        ("font_style", ParamValue::Text("Regular".into())),
        (
            "align",
            ParamValue::Choice(match w.align {
                ..=-1 => 0,
                0 => 1,
                1.. => 2,
            }),
        ),
        ("caps", ParamValue::Choice(u32::from(w.uppercase))),
        ("fill_color", rgba(w.color, 1.0)),
        ("tracking", ParamValue::Float(f64::from(w.tracking) / em * 1000.0)),
        // The compositor sets no kerning or ligatures, and spaces lines 1.2 text heights apart.
        ("kerning", ParamValue::Bool(false)),
        ("ligatures", ParamValue::Bool(false)),
        ("leading", ParamValue::Float(f64::from(m.line_height - (m.ascent + m.descent + m.line_gap)))),
        ("anchor", ParamValue::Vec2(Vec2::new(0.0, f64::from(lift)))),
    ] {
        set(&mut words, k, v);
    }
    name(&mut words, "Words");
    // The finish as FilmCraft's own: a drop shadow, an outline (stroke), a glow (a soft shadow).
    match w.finish {
        TextFinish::Outline => {
            if let Some(p) = w.passes.first() {
                set(&mut words, "stroke", ParamValue::Bool(true));
                set(&mut words, "stroke_color", rgba(p.color, 1.0));
                set(&mut words, "stroke_width", ParamValue::Float(f64::from(p.dx.hypot(p.dy))));
            }
        }
        TextFinish::Glow => {
            if let Some(p) = w.passes.first() {
                set(&mut words, "shadow", ParamValue::Bool(true));
                set(&mut words, "shadow_color", rgba(p.color, 1.0));
                set(&mut words, "shadow_distance", ParamValue::Float(0.0));
                set(&mut words, "shadow_blur", ParamValue::Float(f64::from(p.dx.hypot(p.dy))));
                set(&mut words, "shadow_opacity", ParamValue::Float(60.0));
            }
        }
        TextFinish::Shadow | TextFinish::None => {
            // The passes' weighted offset and their combined cover.
            let total: f32 = w.passes.iter().map(|p| p.alpha).sum();
            if total > 0.0 {
                let dx = w.passes.iter().map(|p| p.dx * p.alpha).sum::<f32>() / total;
                let dy = w.passes.iter().map(|p| p.dy * p.alpha).sum::<f32>() / total;
                let cover = 1.0 - w.passes.iter().fold(1.0, |k, p| k * (1.0 - p.alpha.clamp(0.0, 1.0)));
                set(&mut words, "shadow", ParamValue::Bool(true));
                set(&mut words, "shadow_color", rgba(w.passes[0].color, 1.0));
                set(&mut words, "shadow_angle", ParamValue::Float(f64::from(dy.atan2(dx).to_degrees() + 90.0)));
                set(&mut words, "shadow_distance", ParamValue::Float(f64::from(dx.hypot(dy))));
                set(&mut words, "shadow_blur", ParamValue::Float(0.0));
                set(&mut words, "shadow_size", ParamValue::Float(0.0));
                set(&mut words, "shadow_opacity", ParamValue::Float(f64::from(cover * 100.0)));
            }
        }
    }
    push(words, true);
    out
}

/// Key the title's layers from the clip's own in and out (GC-10): opacity × the title's alpha,
/// lifted by its offset, the words zoomed by its scale; a key wherever the
/// title's motion changes, so a hold is two keys and a move one key a frame.
fn animate(
    layers: &mut [Layer],
    clip: &Clip,
    looks: &assetcraft_storyboard::UserLooks,
    rate: u32,
    frames: u64,
    h: f32,
) -> bool {
    let fps = f64::from(rate.max(1));
    let secs = clip.duration.as_secs_f64();
    let at = |f: u64| clip.text_style.frame_at_with(looks, f as f64 / fps, secs);
    let anim: Vec<AnimFrame> = (0..frames).map(at).collect();
    if anim.iter().all(|a| *a == AnimFrame::REST) {
        return false;
    }
    let tick = TICKS_PER_SECOND / u64::from(rate.max(1));
    let mut types = false;
    for (f, a) in anim.iter().enumerate() {
        let before = f.checked_sub(1).and_then(|i| anim.get(i));
        let after = anim.get(f + 1);
        if before == Some(a) && after == Some(a) {
            continue;
        }
        types |= a.reveal < 1.0 && !a.is_hidden();
        let t = Tick(i64::try_from(f as u64 * tick).unwrap_or(i64::MAX));
        let alpha = if a.is_hidden() { 0.0 } else { f64::from(a.alpha) };
        // A card's background and its decor stay as they are: only the title moves and fades.
        for layer in layers.iter_mut().filter(|l| l.moves) {
            if let Some(p) = layer.effect.param_mut("opacity") {
                p.put_keyframe(t, ParamValue::Float(layer.opacity * alpha));
            }
            if let Some(base) = layer.position
                && let Some(p) = layer.effect.param_mut("position")
            {
                p.put_keyframe(t, ParamValue::Vec2(Vec2::new(base.x, base.y + f64::from(a.offset_y * h))));
            }
            if layer.effect.effect == filmcraft_project::graphic::TEXT_LAYER
                && let Some(p) = layer.effect.param_mut("scale")
            {
                p.put_keyframe(t, ParamValue::Float(f64::from(a.scale * 100.0)));
            }
        }
    }
    types
}

/// A shot's size in pixels: its header (pictures, most videos), else the video's first stream.
fn picture_size(s: &Session, file: &std::path::Path) -> Option<(u32, u32)> {
    if let Some(d) = assetcraft_probe::probe(s.services.as_ref(), file).ok().and_then(|p| p.dimensions)
        && d.width > 0
        && d.height > 0
    {
        return Some((d.width, d.height));
    }
    let reader = assetcraft_media::VideoReader::open(file).ok()?;
    Some(reader.size()).filter(|(w, h)| *w > 0 && *h > 0)
}

/// `<folder>/<name>.fcproj` beside the saved storyboard.
fn default_path(s: &Session) -> Result<PathBuf, CmdError> {
    let document = s
        .document()
        .ok_or_else(|| CmdError::Unavailable("save the storyboard first: the project goes beside it".into()))?;
    let dir = document.parent().ok_or_else(|| CmdError::Unavailable("the storyboard has no folder".into()))?;
    Ok(dir.join(format!("{}.fcproj", crate::document_cmd::document_name(document))))
}

/// The project for the open storyboard, its titles as `titles` plans them.
pub fn build(s: &Session, titles: &RenderPlan, path: PathBuf) -> Result<FcprojPlan, CmdError> {
    let board = s.boards.board.as_ref().ok_or_else(crate::storyboard::no_board)?;
    let rate = titles.fps.max(1);
    let (w, h) = (titles.width, titles.height);
    // Who draws each title: the in-app ones become graphic clips.
    let mut compositor = Compositor::new(Arc::clone(&s.services), w, h, s.model.render_look(), s.decode_options())
        .map_err(|e| CmdError::Unavailable(e.to_string()))?
        .with_crop(board.frame.as_ref())
        .with_user_looks(s.user_looks().clone())
        .with_designs(s.theme_designs());
    let mut layouts = Vec::new();
    let (mut graphics, mut rendered, mut unrendered) = (Vec::new(), Vec::new(), Vec::new());
    for t in &titles.titles {
        let Some(clip) = board.clips.get(t.index) else { continue };
        match (t.renderer, compositor.title_layout(clip)) {
            (Renderer::InApp, Some(l)) => {
                graphics.push(t.clip);
                layouts.push((t.index, l));
            }
            _ if t.reuse => rendered.push(t.clip),
            _ => unrendered.push(t.clip),
        }
    }
    let indices: Vec<usize> = layouts.iter().map(|(i, _)| *i).collect();
    let otio = document_with(board, titles, path.with_extension("otio"), &indices);
    let mut notes = otio.notes.clone();
    let text = serde_json::to_vec(&otio.otio).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    let (imported, report) = filmcraft_interchange::import(&text, filmcraft_interchange::Format::Otio, None)
        .map_err(|e| CmdError::Unavailable(format!("FilmCraft's importer refused the Cut: {e}")))?;
    notes.extend(report.entries.iter().map(|e| format!("FilmCraft: {e:?}")));
    let mut project = imported.project;
    project.name = board.name.clone();
    let seq_id = *imported
        .sequences
        .first()
        .ok_or_else(|| CmdError::Unavailable("FilmCraft's importer made no sequence".into()))?;
    let frame_rate = FrameRate::new(i64::from(rate), 1);
    let graphic =
        project.add_item("Graphic", Label::Rose, ItemKind::Graphic { width: w, height: h, rate: frame_rate }, None);
    let tick = TICKS_PER_SECOND / u64::from(rate);
    let ticks = |frames: u64| Tick(i64::try_from(frames.saturating_mul(tick)).unwrap_or(i64::MAX));
    let spans = cut_spans(board, rate);
    let mut items = Vec::new();
    for (index, layout) in &layouts {
        let (Some(clip), Some(span)) = (board.clips.get(*index), spans.get(*index)) else { continue };
        let label = format!("#{:02} {}", index + 1, clip.shot_type.code());
        let mut ls = layers(layout, &mut notes, &label);
        if animate(&mut ls, clip, s.user_looks(), rate, span.frames, h as f32) {
            notes.push(format!("{label}: it types in here; FilmCraft shows the words whole"));
        }
        let dur = ticks(span.frames);
        let mut item = project
            .make_track_item(graphic, TrackKind::Video, ticks(span.first), TimeRange::new(Tick::ZERO, dur), frame_rate)
            .ok_or_else(|| CmdError::Unavailable("FilmCraft wouldn't make a graphic clip".into()))?;
        let words = clip.text().unwrap_or_default();
        item.name = format!("{label} · {}", words.lines().next().unwrap_or("").trim());
        item.effects.extend(ls.into_iter().map(|l| l.effect));
        ensure_layer_uids(&mut item.effects);
        item.markers.push(Marker {
            url: String::new(),
            author: String::new(),
            custom_color: None,
            id: MarkerId(project.alloc_id()),
            start: Tick::ZERO,
            duration: dur,
            name: words.lines().next().unwrap_or("").trim().to_owned(),
            comment: title_key(clip),
            kind: MarkerKind::Comment,
            color: Label::Purple,
        });
        items.push((layout.kind, item));
    }
    let spare = [project.alloc_id(), project.alloc_id()];
    let seq = project.sequence_mut(seq_id).ok_or_else(|| CmdError::Unavailable("the sequence went missing".into()))?;
    // OTIO carries no frame size: the board's, and its rate.
    seq.settings.width = w;
    seq.settings.height = h;
    seq.settings.frame_rate = frame_rate;
    seq.settings.preset = format!("AssetCraft {w} × {h} {rate} fps");
    // Shots framed as the board frames them (GC-12): FilmCraft fits each to the frame, then the
    // clip's Motion scales it on to the board's fill and moves it to its snap.
    let spec = board.frame_spec();
    let at_start: std::collections::HashMap<i64, usize> =
        spans.iter().enumerate().map(|(i, span)| (ticks(span.first).0, i)).collect();
    for track in &mut seq.video_tracks {
        for item in &mut track.items {
            item.scale_to_frame = true;
            let Some(index) = at_start.get(&item.start.0) else { continue };
            let Some(clip) = board.clips.get(*index) else { continue };
            if indices.contains(index) && clip.text_kind() == Some(TextKind::Card) {
                continue;
            }
            let Some(file) = clip.asset_ids.first().map(|a| std::path::PathBuf::from(&a.0)) else { continue };
            if titles.titles.iter().any(|t| t.index == *index && !indices.contains(index)) {
                continue; // a rendered title is the frame's own size
            }
            let Some((pw, ph)) = picture_size(s, &file) else {
                notes.push(format!("#{:02}: its size couldn't be read, so FilmCraft fits it whole", index + 1));
                continue;
            };
            let Some(motion) = item.effect_mut("motion") else { continue };
            let (fw, fh) = (f64::from(w), f64::from(h));
            let [x, y, pw_frac, ph_frac] = spec.place(pw as f32, ph as f32).map(f64::from);
            let placed_w = pw_frac * fw;
            let fit = (fw / f64::from(pw)).min(fh / f64::from(ph));
            let scale = placed_w / f64::from(pw) / fit * 100.0;
            set(motion, "scale", ParamValue::Float(scale));
            set(motion, "position", ParamValue::Vec2(Vec2::new((x + pw_frac / 2.0) * fw, (y + ph_frac / 2.0) * fh)));
        }
    }
    for (kind, item) in items {
        // A card where it sits on V1 (its gap there); an overlay on V2 over its shot.
        let want = usize::from(kind == TextKind::Overlay);
        while seq.video_tracks.len() <= want {
            let n = seq.video_tracks.len();
            let id = filmcraft_project::TrackId(spare.get(n).copied().unwrap_or(spare[1]));
            seq.video_tracks.push(filmcraft_project::Track::new(id, TrackKind::Video, format!("Video {}", n + 1)));
        }
        seq.video_tracks[want].items.push(item);
        seq.video_tracks[want].sort();
    }
    seq.check().map_err(|e| CmdError::Unavailable(format!("the sequence doesn't hold together: {e}")))?;
    let bytes = filmcraft_format::encode(&project, true);
    Ok(FcprojPlan { path, bytes, graphics, rendered, unrendered, notes })
}

/// `storyboard.writeFcproj {dryRun?, path?, overwrite?}`.
pub(crate) fn sb_write_fcproj(s: &mut Session, p: &Value) -> CmdResult {
    let flag = |name: &str| p.get(name).and_then(Value::as_bool).unwrap_or(false);
    let titles = crate::title_render::plan(s)?;
    let given = p.get("path").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty());
    let path = match given {
        Some(text) if text.len() <= 4096 => {
            let path = PathBuf::from(text);
            if !path.is_absolute() {
                return Err(CmdError::BadParams("`path` must be absolute".into()));
            }
            if path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() != Some("fcproj") {
                return Err(CmdError::BadParams("`path` must end with .fcproj".into()));
            }
            path
        }
        Some(_) => return Err(CmdError::BadParams("`path` is too long".into())),
        None => default_path(s)?,
    };
    if s.boards.board.as_ref().is_none_or(|b| b.clips.is_empty()) {
        return Err(CmdError::Unavailable("the storyboard has no clips".into()));
    }
    let plan = build(s, &titles, path)?;
    let summary = json!({
        "path": plan.path.to_string_lossy(),
        "graphics": plan.graphics,
        "rendered": plan.rendered,
        "unrendered": plan.unrendered,
        "problems": titles.problems.iter().map(|x| json!({ "clip": x.clip, "what": x.what, "why": x.why })).collect::<Vec<_>>(),
        "notes": plan.notes,
        "bytes": plan.bytes.len(),
    });
    if flag("dryRun") {
        let mut out = summary;
        out["dryRun"] = json!(true);
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
            "{} title(s) drawn by a design aren't rendered yet: run storyboard.renderTitles first",
            plan.unrendered.len()
        )));
    }
    if given.is_some() && plan.path.exists() && !flag("overwrite") {
        return Err(CmdError::Unavailable(format!(
            "{} exists: pass `overwrite: true` to replace it",
            plan.path.display()
        )));
    }
    let bytes = write_atomic(&plan.path, |out| {
        out.write_all(&plan.bytes).map_err(|e| assetcraft_video::VideoError::Encode(e.to_string()))?;
        Ok(plan.bytes.len() as u64)
    })
    .map_err(CmdError::Unavailable)?;
    let mut out = summary;
    out["written"] = json!(true);
    out["bytes"] = json!(bytes);
    out["message"] = json!(format!(
        "Wrote {} for FilmCraft: {} title(s) you can edit there, {} as their render",
        plan.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        plan.graphics.len(),
        plan.rendered.len()
    ));
    Ok(out)
}

/// `storyboard.sendToFilmCraft {open?: bool}` (SF1b-3; File › Send to FilmCraft): the whole
/// hand-off as one command. A title a design draws that isn't rendered yet is rendered first, on
/// the export thread (`{stage: "rendering"}`: send again when `storyboard.exportStatus` says
/// done — the app does that by itself); then the project is written beside the storyboard
/// (`storyboard.writeFcproj`) and, unless `open: false`, opened in FilmCraft (`file.openIn`). A
/// FilmCraft that isn't installed leaves the project written and says where it is.
pub(crate) fn sb_send_to_filmcraft(s: &mut Session, p: &Value) -> CmdResult {
    let open = p.get("open").and_then(Value::as_bool).unwrap_or(true);
    let titles = crate::title_render::plan(s)?;
    if !titles.problems.is_empty() {
        let list: Vec<String> = titles.problems.iter().map(|x| format!("clip {}: {}", x.clip, x.why)).collect();
        return Err(CmdError::Unavailable(format!(
            "{} problem(s) to fix before sending, so nothing goes missing: {}",
            list.len(),
            list.join("; ")
        )));
    }
    if s.exporter.running() {
        return Err(CmdError::Unavailable("an export is running: wait for it, or press Esc to stop it".into()));
    }
    let board = s.boards.board.as_ref().ok_or_else(crate::storyboard::no_board)?;
    let designed = titles
        .titles
        .iter()
        .filter(|t| t.renderer == Renderer::EffectCraft && !t.reuse && board.clips.get(t.index).is_some())
        .count();
    if designed > 0 {
        crate::title_render::sb_render_titles(s, &json!({ "designed": true }))?;
        return Ok(json!({
            "stage": "rendering",
            "render": designed,
            "message": format!("Rendering {designed} title(s) drawn by a design first…"),
        }));
    }
    let written = sb_write_fcproj(s, &json!({}))?;
    let path = written["path"].as_str().unwrap_or_default().to_owned();
    let mut out = written;
    out["stage"] = json!("written");
    if !open {
        return Ok(out);
    }
    match s.execute("file.openIn", &json!({ "app": "filmcraft", "paths": [path.clone()] })) {
        Ok(_) => {
            out["stage"] = json!("opened");
            out["message"] = json!(format!("Sent to FilmCraft: {}", out["message"].as_str().unwrap_or_default()));
        }
        Err(e) => {
            out["message"] = json!(format!("Wrote {path}; FilmCraft didn't open it ({e})"));
        }
    }
    Ok(out)
}
