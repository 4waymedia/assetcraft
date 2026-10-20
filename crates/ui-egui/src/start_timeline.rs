//! Start storyboard v2, the full-width timeline (S2d-1/2; Paul, 2026-10-16: "the full audio is
//! placed in the bottom track, and the storyboard is in the track above it").
//!
//! Draws only. Two lanes on one ruler in **cut time** (Paul, 2026-10-16: "not show the cut out
//! audio area... all clips together... the seamless run"), fit to the window's width:
//! - **Storyboard:** what Create would build now (`storyboard.preview` for the chosen track,
//!   preset, cut pace, shape and length): clips back to back by shot type, title cards and text
//!   overlays marked, fades and flashes at clip edges, § at each scene.
//! - **Audio:** the music the cut plays: the kept parts of the track (`edit.segments`) butted
//!   together, each with its waveform (`track.peaks`), section bands and ◆ drops; a splice mark
//!   where the music jumps (crossfaded), the fade-out as a ramp. At full length this is the whole
//!   track.
//!
//! The timeline plays the **cut** (Paul, 2026-10-17, "two modes"): ▶ / Space play the song
//! through the chosen length's edit from the playhead (`track.preview {cut}`), a click plays the
//! cut from there, and dragging there or dragging the playhead scrubs (the cut plays from where
//! the drag ends). Everything here is in cut time, so the playhead runs straight across the
//! splices. The track column's ▶ and section clicks play the **song** instead; starting either
//! stops the other. Ruler, playhead and scrub reading are the shared [`crate::timebar`] ones.

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
const WAVE: Color32 = Color32::from_rgb(0x5b, 0x8f, 0xc9);
const SPLICE: Color32 = Color32::from_rgb(0xe0, 0xa0, 0x40);

/// What the lanes need, gathered by the Start screen.
pub(crate) struct Lanes<'a> {
    /// The selected track (`storyboard.tracks` row).
    pub track: &'a Value,
    /// `storyboard.preview` for the chosen preset and length (`None` = can't build).
    pub preview: Option<&'a Value>,
    /// `track.peaks` once ready.
    pub peaks: Option<&'a Value>,
    /// Cut seconds a playing Cut preview of this track is at.
    pub heard: Option<f64>,
    /// While a scrub drag is held: where it is, in cut seconds (drawn instead of `heard`).
    pub scrub: Option<f64>,
    /// Where the paused playhead rests, in cut seconds (0 for a newly picked track or length).
    pub cursor: f64,
    /// A Cut preview of this track is playing or loading (▶ shows ⏸).
    pub playing: bool,
}

/// What the user did on the timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Action {
    /// ▶ / ⏸.
    Toggle,
    /// A click or drag at `cut` seconds.
    Scrub { cut: f64, how: crate::timebar::Scrub },
}

/// Cut time → track time through the edit's segments (`[[start, end]]` in track seconds, played
/// back to back). Past the end: the end of the last segment.
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

/// Each segment as `(track start, track end, cut offset)`.
fn placed(segments: &[(f64, f64)]) -> Vec<(f64, f64, f64)> {
    let mut off = 0.0;
    segments
        .iter()
        .map(|&(a, b)| {
            let at = off;
            off += (b - a).max(0.0);
            (a, b, at)
        })
        .collect()
}

fn segments(preview: Option<&Value>, total: f64) -> Vec<(f64, f64)> {
    let segs: Vec<(f64, f64)> = preview
        .and_then(|p| p["edit"]["segments"].as_array())
        .map(|a| a.iter().filter_map(|s| Some((s[0].as_f64()?, s[1].as_f64()?))).filter(|(a, b)| b > a).collect())
        .unwrap_or_default();
    if segs.is_empty() { vec![(0.0, total)] } else { segs }
}

fn ink_on(fill: Color32) -> Color32 {
    let l = 0.299 * f32::from(fill.r()) + 0.587 * f32::from(fill.g()) + 0.114 * f32::from(fill.b());
    if l > 150.0 { Color32::from_gray(0x16) } else { Color32::from_gray(0xd8) }
}

/// The timeline panel. Returns what the user did on it, if anything.
pub(crate) fn show(ui: &mut Ui, lanes: Option<Lanes<'_>>) -> Option<Action> {
    let Some(l) = lanes else {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.weak("Pick a track: the storyboard it makes and the music it plays line up here.");
        });
        return None;
    };
    let total = l.track["seconds"].as_f64().unwrap_or(0.0);
    if total <= 0.0 {
        return None;
    }
    let segs = segments(l.preview, total);
    let pieces = placed(&segs);
    let clips = l.preview.and_then(|p| p["clips"].as_array()).cloned().unwrap_or_default();
    let clips_end =
        clips.last().map_or(0.0, |c| c["start"].as_f64().unwrap_or(0.0) + c["seconds"].as_f64().unwrap_or(0.0));
    let cut_len = pieces.last().map_or(total, |(a, b, o)| o + (b - a)).max(clips_end).max(0.1);
    // Header: transport, timecode, what plays.
    let mut action = None;
    let at = l.scrub.or(l.heard).unwrap_or(l.cursor).clamp(0.0, cut_len);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let (icon, tip) =
            if l.playing { ("⏸", "Pause (Space)") } else { ("▶", "Play the cut from the playhead (Space)") };
        if ui.small_button(icon).on_hover_text(tip).clicked() {
            action = Some(Action::Toggle);
        }
        ui.label(RichText::new(format!("{} / {}", mmss(at), mmss(cut_len))).monospace().small());
        ui.label(RichText::new("TIMELINE").small().color(ui.visuals().weak_text_color()));
        let title = l.track["title"].as_str().unwrap_or("Track");
        let what = match l.preview {
            Some(p) if segs.len() > 1 || (segs[0].1 - total).abs() > 0.5 => format!(
                "{title} · {} clips · {} cut from {} ({} splice{})",
                p["count"],
                p["length"].as_str().unwrap_or(""),
                mmss(total),
                segs.len() - 1,
                if segs.len() == 2 { "" } else { "s" }
            ),
            Some(p) => format!("{title} · {} clips · {}", p["count"], p["length"].as_str().unwrap_or("")),
            None => format!("{title} · {}", mmss(total)),
        };
        ui.label(RichText::new(what).monospace().small());
    });
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, RULER_H + BOARD_H + AUDIO_H + 16.0), Sense::hover());
    let lane_x = rect.left() + LABEL_W;
    let lane_w = (rect.width() - LABEL_W - 8.0).max(10.0);
    let pps = lane_w / cut_len as f32;
    let x_of = |t: f64| lane_x + (t.clamp(0.0, cut_len) as f32) * pps;
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
    // Ruler (cut time).
    crate::timebar::paint_ruler(&painter, ruler, pps, cut_len, x_of, Color32::from_gray(0x8a));
    // Audio lane: each kept piece of the track, butted together.
    painter.rect_filled(audio, 2, Color32::from_gray(0x1b));
    let peaks: Vec<(f64, f64)> = l
        .peaks
        .and_then(|p| p["peaks"].as_array())
        .map(|a| a.iter().map(|p| (p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0))).collect())
        .unwrap_or_default();
    let wave_secs = l.peaks.and_then(|p| p["seconds"].as_f64()).filter(|s| *s > 0.0).unwrap_or(total);
    // Normalised to the loudest moment of the whole track, so pieces compare.
    let loudest = peaks.iter().fold(0.0f64, |m, (lo, hi)| m.max(-lo).max(*hi)).max(1e-3);
    let mid = audio.center().y + 4.0;
    let half = audio.height() * 0.4 / loudest as f32;
    for &(a, b, off) in &pieces {
        let (x0, x1) = (x_of(off), x_of(off + (b - a)));
        // Section bands inside this piece.
        for s in l.track["sections"].as_array().into_iter().flatten() {
            let (sa, sb) = (s["start"].as_f64().unwrap_or(0.0).max(a), s["end"].as_f64().unwrap_or(0.0).min(b));
            if sb <= sa {
                continue;
            }
            let r = Rect::from_min_max(
                pos2(x_of(off + sa - a), audio.top()),
                pos2(x_of(off + sb - a) - 1.0, audio.bottom()),
            );
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
        // Waveform: one column per pixel, read at the track time under it.
        if !peaks.is_empty() {
            let n = peaks.len();
            let mut x = x0;
            while x < x1 {
                let (t0, t1) = (a + f64::from((x - x0) / pps), a + f64::from((x + 1.0 - x0) / pps));
                if t0 < wave_secs {
                    let i0 = ((t0 / wave_secs * n as f64) as usize).min(n - 1);
                    let i1 = ((t1 / wave_secs * n as f64).ceil() as usize).clamp(i0 + 1, n);
                    let (lo, hi) =
                        peaks[i0..i1].iter().fold((0.0f64, 0.0f64), |(lo, hi), (p, q)| (lo.min(*p), hi.max(*q)));
                    painter.line_segment(
                        [pos2(x + 0.5, mid - hi as f32 * half), pos2(x + 0.5, mid - lo as f32 * half + 1.0)],
                        Stroke::new(1.0, WAVE),
                    );
                }
                x += 1.0;
            }
        }
        // Drops inside this piece.
        for d in l.track["drops"].as_array().into_iter().flatten().filter_map(Value::as_f64) {
            if d >= a && d < b {
                painter.text(
                    pos2(x_of(off + d - a), audio.bottom() - 2.0),
                    Align2::CENTER_BOTTOM,
                    "◆",
                    FontId::proportional(10.0),
                    SELECT,
                );
            }
        }
        // Where the music jumps: a splice mark and the track time it jumps to.
        if off > 0.0 {
            painter.line_segment([pos2(x0, board.top()), pos2(x0, audio.bottom())], Stroke::new(1.0, SPLICE));
            painter.text(
                pos2(x0 + 3.0, audio.bottom() - 2.0),
                Align2::LEFT_BOTTOM,
                format!("→ {}", mmss(a)),
                FontId::monospace(9.0),
                SPLICE,
            );
        }
    }
    if peaks.is_empty() {
        let msg = match l.peaks {
            Some(p) => format!("No waveform: {}", p["error"].as_str().unwrap_or("can't read the audio")),
            None => "Reading the waveform…".to_owned(),
        };
        painter.text(audio.center(), Align2::CENTER_CENTER, msg, FontId::proportional(10.0), Color32::from_gray(0x77));
    }
    // The fade-out (cut time): a ramp down to the end.
    if let Some(f) = l.preview.and_then(|p| p["edit"]["fadeOut"].as_array()) {
        let (s, n) = (f[0].as_f64().unwrap_or(0.0), f[1].as_f64().unwrap_or(0.0));
        painter.line_segment(
            [pos2(x_of(s), audio.top() + 2.0), pos2(x_of(s + n), audio.bottom() - 2.0)],
            Stroke::new(1.5, Color32::from_gray(0xcc)),
        );
    }
    // Storyboard lane: the clips back to back.
    painter.rect_filled(board, 2, Color32::from_gray(0x1b));
    let mut last_scene = None;
    for c in &clips {
        let (s, d) = (c["start"].as_f64().unwrap_or(0.0), c["seconds"].as_f64().unwrap_or(0.0));
        let code = c["shot"].as_str().unwrap_or("");
        let fill = fill_for(code);
        let scene = c["scene"].as_u64();
        let new_scene = last_scene != scene;
        last_scene = scene;
        let r = Rect::from_min_max(
            pos2(x_of(s), board.top()),
            pos2((x_of(s + d) - 1.0).max(x_of(s) + 1.0), board.bottom()),
        );
        if code == "INS" {
            // Insert: dashed, as in the legend and the Shot map.
            crate::storyboard_view::dashed_rect(ui, r, Color32::from_gray(0x8a));
        } else {
            painter.rect_filled(r, 1, fill);
            painter.rect_stroke(r, 1, Stroke::new(1.0, Color32::from_gray(0x3a)), StrokeKind::Inside);
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
        if new_scene {
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
    if clips.is_empty() {
        painter.text(
            board.center(),
            Align2::CENTER_CENTER,
            "Can't build at this length",
            FontId::proportional(10.0),
            Color32::from_gray(0x77),
        );
    }
    // Click or drag anywhere on the ruler and both lanes (cut time).
    let cut_at = |x: f32| f64::from((x - lane_x) / pps).clamp(0.0, cut_len);
    let lanes_rect = Rect::from_min_max(pos2(lane_x, rect.top()), pos2(lane_x + lane_w, audio.bottom()));
    let tip = match ui.input(|i| i.pointer.hover_pos()).filter(|p| lanes_rect.contains(*p)) {
        Some(p) => {
            let t = cut_at(p.x);
            format!("Click to play the cut from {} (song {}), or drag to scrub", mmss(t), mmss(to_track(&segs, t)))
        }
        None => String::new(),
    };
    let mut scrub = crate::timebar::scrub_area(ui, lanes_rect, ui.id().with("sb-timeline-scrub"), &tip);
    // The playhead is always shown: at the cursor when paused, following the cut when playing.
    let x = x_of(at);
    crate::timebar::paint_playhead(&painter, x, board.top(), audio.bottom(), Color32::WHITE);
    let id = ui.id().with("sb-timeline-playhead");
    if let Some(grab) = crate::timebar::playhead_handle(ui, x, board.top(), audio.bottom(), id) {
        scrub = Some(grab);
    }
    action.or_else(|| scrub.map(|(x, how)| Action::Scrub { cut: cut_at(x), how }))
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
