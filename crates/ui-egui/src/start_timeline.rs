//! Start storyboard v2, the full-width timeline (S2d-1/2; Paul, 2026-10-16: "the full audio is
//! placed in the bottom track, and the storyboard is in the track above it").
//!
//! Draws only. Two lanes on one ruler in **track time**, fit to the window's width:
//! - **Storyboard:** what Create would build now (`storyboard.preview` for the chosen track,
//!   preset, cut pace, shape and length): clips by shot type, title cards and text overlays
//!   marked, fades and flashes at clip edges, § at each scene. A cut version's clips sit over the
//!   music they play with; the part of the song the edit skips is a gap.
//! - **Audio:** the whole track: waveform (`track.peaks`), section bands, ◆ drops, the skipped
//!   part dimmed. A click previews from there (`track.preview {from}`); the playhead is
//!   `Session::preview_position`.

use crate::Navigator;
use crate::start_tracks::{mmss, section_tint};
use crate::start_view::fill_for;
use crate::storyboard_view::SELECT;
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::{Value, json};

/// Panel height.
pub(crate) const TIMELINE_H: f32 = 178.0;
const RULER_H: f32 = 16.0;
const BOARD_H: f32 = 46.0;
const AUDIO_H: f32 = 56.0;
const LABEL_W: f32 = 74.0;
const DIM: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 150);
const WAVE: Color32 = Color32::from_rgb(0x5b, 0x8f, 0xc9);

/// What the lanes need, gathered by the Start screen.
pub(crate) struct Lanes<'a> {
    /// The selected track (`storyboard.tracks` row).
    pub track: &'a Value,
    /// `storyboard.preview` for the chosen preset and length (`None` = can't build).
    pub preview: Option<&'a Value>,
    /// `track.peaks` once ready.
    pub peaks: Option<&'a Value>,
    /// Seconds into this track a playing preview is at.
    pub heard: Option<f64>,
}

/// Board time → track time through the edit's segments (`[[start, end]]` in track seconds,
/// played back to back). Past the end: the end of the last segment.
pub(crate) fn to_track(segments: &[(f64, f64)], t: f64) -> f64 {
    let mut off = 0.0;
    for &(a, b) in segments {
        let len = (b - a).max(0.0);
        if t < off + len {
            return a + (t - off).max(0.0);
        }
        off += len;
    }
    segments.last().map_or(t, |s| s.1)
}

/// The track ranges a board span `[s, e)` plays (split where the edit splices).
pub(crate) fn track_ranges(segments: &[(f64, f64)], s: f64, e: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut off = 0.0;
    for &(a, b) in segments {
        let len = (b - a).max(0.0);
        let (lo, hi) = (s.max(off), e.min(off + len));
        if hi > lo {
            out.push((a + lo - off, a + hi - off));
        }
        off += len;
    }
    out
}

fn segments(preview: Option<&Value>) -> Vec<(f64, f64)> {
    preview
        .and_then(|p| p["edit"]["segments"].as_array())
        .map(|a| a.iter().filter_map(|s| Some((s[0].as_f64()?, s[1].as_f64()?))).collect())
        .unwrap_or_default()
}

/// A tick step (seconds) that keeps labels at least `min_px` apart.
fn tick_step(px_per_sec: f32, min_px: f32) -> f64 {
    [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0]
        .into_iter()
        .find(|s| (*s as f32) * px_per_sec >= min_px)
        .unwrap_or(600.0)
}

fn ink_on(fill: Color32) -> Color32 {
    let l = 0.299 * f32::from(fill.r()) + 0.587 * f32::from(fill.g()) + 0.114 * f32::from(fill.b());
    if l > 150.0 { Color32::from_gray(0x16) } else { Color32::from_gray(0xd8) }
}

/// The timeline panel. Returns a time (track seconds) to preview from, when the audio lane is clicked.
pub(crate) fn show(ui: &mut Ui, lanes: Option<Lanes<'_>>) -> Option<f64> {
    let Some(l) = lanes else {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.weak("Pick a track: its full audio and the storyboard it makes line up here.");
        });
        return None;
    };
    let total = l.track["seconds"].as_f64().unwrap_or(0.0);
    if total <= 0.0 {
        return None;
    }
    // Header.
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("TIMELINE").small().color(ui.visuals().weak_text_color()));
        let what = match l.preview {
            Some(p) => format!(
                "{} · {} clips · {} of {}",
                l.track["title"].as_str().unwrap_or("Track"),
                p["count"],
                p["length"].as_str().unwrap_or(""),
                mmss(total)
            ),
            None => format!("{} · {}", l.track["title"].as_str().unwrap_or("Track"), mmss(total)),
        };
        ui.label(RichText::new(what).monospace().small());
    });
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, RULER_H + BOARD_H + AUDIO_H + 16.0), Sense::hover());
    let lane_x = rect.left() + LABEL_W;
    let lane_w = (rect.width() - LABEL_W - 8.0).max(10.0);
    let pps = lane_w / total as f32;
    let x_of = |t: f64| lane_x + (t.clamp(0.0, total) as f32) * pps;
    let ruler = Rect::from_min_size(rect.min, vec2(rect.width(), RULER_H));
    let board = Rect::from_min_size(pos2(lane_x, ruler.bottom() + 10.0), vec2(lane_w, BOARD_H));
    let audio = Rect::from_min_size(pos2(lane_x, board.bottom() + 6.0), vec2(lane_w, AUDIO_H));
    let painter = ui.painter_at(rect);
    let label = |r: Rect, text: &str| {
        painter.text(
            pos2(rect.left() + 4.0, r.center().y),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(10.0),
            Color32::from_gray(0x8a),
        );
    };
    label(board, "STORYBOARD");
    label(audio, "AUDIO");
    // Ruler.
    let step = tick_step(pps, 56.0);
    let mut t = 0.0;
    while t <= total {
        let x = x_of(t);
        painter.line_segment(
            [pos2(x, ruler.bottom() - 5.0), pos2(x, ruler.bottom())],
            Stroke::new(1.0, Color32::from_gray(0x55)),
        );
        painter.text(
            pos2(x + 3.0, ruler.top()),
            Align2::LEFT_TOP,
            mmss(t),
            FontId::monospace(9.0),
            Color32::from_gray(0x8a),
        );
        t += step;
    }
    let segs = segments(l.preview);
    // Audio lane: sections, waveform, the skipped part dimmed, drops.
    painter.rect_filled(audio, 2, Color32::from_gray(0x1b));
    for s in l.track["sections"].as_array().into_iter().flatten() {
        let (a, b) = (s["start"].as_f64().unwrap_or(0.0), s["end"].as_f64().unwrap_or(0.0));
        let r = Rect::from_min_max(pos2(x_of(a), audio.top()), pos2(x_of(b) - 1.0, audio.bottom()));
        let name = s["label"].as_str().unwrap_or("");
        let (bg, fg) = section_tint(name);
        painter.rect_filled(r, 1, bg);
        if r.width() > 40.0 {
            painter.with_clip_rect(r).text(
                r.left_top() + vec2(4.0, 2.0),
                Align2::LEFT_TOP,
                name.to_uppercase(),
                FontId::proportional(9.0),
                fg,
            );
        }
    }
    if let Some(peaks) = l.peaks.and_then(|p| p["peaks"].as_array()).filter(|p| !p.is_empty()) {
        // Peaks span the decoded file, which can differ a little from the cue map's length.
        let wave_secs = l.peaks.and_then(|p| p["seconds"].as_f64()).filter(|s| *s > 0.0).unwrap_or(total);
        let pairs: Vec<(f64, f64)> =
            peaks.iter().map(|p| (p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0))).collect();
        let n = pairs.len();
        // Normalised, so a quiet master still reads.
        let loudest = pairs.iter().fold(0.0f64, |m, (lo, hi)| m.max(-lo).max(*hi)).max(1e-3);
        let mid = audio.center().y + 4.0;
        let half = audio.height() * 0.4 / loudest as f32;
        let cols = (x_of(wave_secs) - lane_x).max(1.0) as usize;
        for c in 0..cols {
            let i0 = (c * n / cols).min(n - 1);
            let i1 = ((c + 1) * n / cols).clamp(i0 + 1, n);
            let (lo, hi) = pairs[i0..i1].iter().fold((0.0f64, 0.0f64), |(lo, hi), (a, b)| (lo.min(*a), hi.max(*b)));
            let x = lane_x + c as f32 + 0.5;
            painter.line_segment(
                [pos2(x, mid - hi as f32 * half), pos2(x, mid - lo as f32 * half + 1.0)],
                Stroke::new(1.0, WAVE),
            );
        }
    } else {
        let msg = match l.peaks {
            Some(p) => format!("No waveform: {}", p["error"].as_str().unwrap_or("can't read the audio")),
            None => "Reading the waveform…".to_owned(),
        };
        painter.text(audio.center(), Align2::CENTER_CENTER, msg, FontId::proportional(10.0), Color32::from_gray(0x77));
    }
    if !segs.is_empty() {
        let mut at = 0.0;
        for &(a, b) in &segs {
            if a > at {
                painter.rect_filled(
                    Rect::from_min_max(pos2(x_of(at), audio.top()), pos2(x_of(a), audio.bottom())),
                    0,
                    DIM,
                );
            }
            at = b;
        }
        if at < total {
            painter.rect_filled(
                Rect::from_min_max(pos2(x_of(at), audio.top()), pos2(x_of(total), audio.bottom())),
                0,
                DIM,
            );
        }
        // The fade-out: a ramp down to the end of the edit.
        if let Some(f) = l.preview.and_then(|p| p["edit"]["fadeOut"].as_array()) {
            let (s, n) = (f[0].as_f64().unwrap_or(0.0), f[1].as_f64().unwrap_or(0.0));
            let (x0, x1) = (x_of(to_track(&segs, s)), x_of(to_track(&segs, s + n)));
            painter.line_segment(
                [pos2(x0, audio.top() + 2.0), pos2(x1, audio.bottom() - 2.0)],
                Stroke::new(1.5, Color32::from_gray(0xcc)),
            );
        }
    }
    for d in l.track["drops"].as_array().into_iter().flatten().filter_map(Value::as_f64) {
        painter.text(
            pos2(x_of(d), audio.bottom() - 2.0),
            Align2::CENTER_BOTTOM,
            "◆",
            FontId::proportional(10.0),
            SELECT,
        );
    }
    // Storyboard lane.
    painter.rect_filled(board, 2, Color32::from_gray(0x1b));
    let clips = l.preview.and_then(|p| p["clips"].as_array()).cloned().unwrap_or_default();
    let mut last_scene = None;
    for c in &clips {
        let (s, d) = (c["start"].as_f64().unwrap_or(0.0), c["seconds"].as_f64().unwrap_or(0.0));
        let code = c["shot"].as_str().unwrap_or("");
        let fill = fill_for(code);
        let scene = c["scene"].as_u64();
        let new_scene = last_scene.is_some() && scene != last_scene;
        let first = last_scene.is_none();
        last_scene = scene;
        for (i, (a, b)) in track_ranges(&segs, s, s + d).into_iter().enumerate() {
            let r = Rect::from_min_max(
                pos2(x_of(a), board.top()),
                pos2((x_of(b) - 1.0).max(x_of(a) + 1.0), board.bottom()),
            );
            if code == "INS" {
                // Insert: dashed, as in the legend and the Shot map.
                crate::storyboard_view::dashed_rect(ui, r, Color32::from_gray(0x8a));
            } else {
                painter.rect_filled(r, 1, fill);
                painter.rect_stroke(r, 1, Stroke::new(1.0, Color32::from_gray(0x3a)), StrokeKind::Inside);
            }
            if i > 0 {
                continue;
            }
            let ink = if code == "INS" { Color32::from_gray(0xaa) } else { ink_on(fill) };
            if r.width() > 24.0 {
                painter.with_clip_rect(r).text(
                    r.left_top() + vec2(3.0, 3.0),
                    Align2::LEFT_TOP,
                    code,
                    FontId::monospace(9.0),
                    ink,
                );
            }
            match c["text"].as_str() {
                Some("overlay") => {
                    painter.text(
                        r.left_bottom() + vec2(3.0, -3.0),
                        Align2::LEFT_BOTTOM,
                        "T",
                        FontId::proportional(11.0),
                        Color32::WHITE,
                    );
                }
                Some("card") if r.width() > 14.0 => {
                    painter.text(r.center(), Align2::CENTER_CENTER, "T", FontId::proportional(13.0), Color32::WHITE);
                }
                _ => {}
            }
            // Transitions at the clip's edges: fades as a wedge, a flash as a white bar.
            let wedge = |x: f32, dir: f32| {
                let w = 6.0f32.min(r.width() / 2.0) * dir;
                painter.add(egui::Shape::convex_polygon(
                    vec![pos2(x, r.top()), pos2(x + w, r.center().y), pos2(x, r.bottom())],
                    Color32::from_black_alpha(170),
                    Stroke::NONE,
                ));
            };
            match c["in"].as_str() {
                Some("Fade" | "Fade in") => wedge(r.left(), 1.0),
                Some("Flash") => {
                    painter.rect_filled(Rect::from_min_size(r.left_top(), vec2(2.0, r.height())), 0, Color32::WHITE);
                }
                _ => {}
            }
            if c["out"].as_str() == Some("Fade out") {
                wedge(r.right(), -1.0);
            }
            if new_scene || first {
                // § on the lane's top edge, clear of the ruler's labels.
                painter.line_segment(
                    [pos2(r.left(), board.top() - 3.0), pos2(r.left(), board.bottom())],
                    Stroke::new(1.5, SELECT),
                );
                painter.text(
                    pos2(r.left() + 2.0, board.top() - 1.0),
                    Align2::LEFT_BOTTOM,
                    "§",
                    FontId::proportional(9.0),
                    SELECT,
                );
            }
        }
    }
    if clips.is_empty() {
        painter.text(
            board.center(),
            Align2::CENTER_CENTER,
            "Can't build at this length",
            FontId::proportional(10.0),
            Color32::from_gray(0x77),
        );
    }
    // Playhead and click-to-preview.
    if let Some(at) = l.heard {
        let x = x_of(at);
        painter.line_segment([pos2(x, board.top()), pos2(x, audio.bottom())], Stroke::new(2.0, Color32::WHITE));
    }
    let hit = ui.interact(audio, ui.id().with("sb-timeline-audio"), Sense::click());
    let mut seek = None;
    if let Some(pos) = hit.hover_pos() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        let t = f64::from((pos.x - lane_x) / pps).clamp(0.0, total);
        let hit = hit.clone().on_hover_text(format!("Preview from {}", mmss(t)));
        if hit.clicked() {
            seek = Some(t);
        }
    }
    seek
}

/// The waveform of `audio` (`track.peaks`), kept on the Start screen once ready or failed;
/// `None` while it decodes.
pub(crate) fn peaks(nav: &mut Navigator, audio: &str) -> Option<Value> {
    if let Some((p, v)) = &nav.state.storyboard.start.waveform
        && p == audio
    {
        return Some(v.clone());
    }
    let v = nav.session.execute("track.peaks", &json!({ "path": audio, "buckets": 2048 })).ok()?;
    if v["status"] == "loading" {
        return None;
    }
    // Ready or failed: kept, so it isn't asked every frame.
    nav.state.storyboard.start.waveform = Some((audio.to_owned(), v.clone()));
    Some(v)
}
