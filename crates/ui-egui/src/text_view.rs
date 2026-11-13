//! Text clips on screen: Cards and Overlays painted with their theme and intro animation, and
//! the kind chip that switches a clip between them.
//!
//! Draws only. Looks and animation frames come from `assetcraft_storyboard::text`; a click on
//! the chip (Shot map tile, Strip clip) or on the Inspector's Card | Overlay buttons runs
//! `storyboard.text.kind`, and the Inspector's pickers run `storyboard.text.style`.

use crate::Navigator;
use crate::storyboard_view::rgb;
use assetcraft_storyboard::{
    AnimFrame, Backdrop, Clip, DecorRect, MAX_TEXT_MS, MIN_TEXT_MS, TextAnimation, TextEffect, TextExit, TextKind,
    TextPosition, TextStyle, TextTheme,
};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

/// Kind chip size.
const CHIP_W: f32 = 52.0;
const CHIP_H: f32 = 14.0;

/// Paint a clip's text into `frame` (16:9), `elapsed` seconds into the clip (`f64::INFINITY` =
/// at rest). A Card fills the frame with its theme's background first; an Overlay paints only
/// the text, over whatever is already there. Clips without text paint nothing.
pub(crate) fn paint_text(painter: &Painter, frame: Rect, clip: &Clip, elapsed: f64) {
    paint_text_frame(painter, frame, clip, clip.text_style.frame_at(elapsed, clip.duration.as_secs_f64()));
}

/// [`paint_text`] at a given animation frame (the Inspector's preview runs its own clock).
fn paint_text_frame(painter: &Painter, frame: Rect, clip: &Clip, anim: AnimFrame) {
    let (Some(kind), Some(words)) = (clip.text_kind(), clip.text()) else { return };
    let style = clip.text_style;
    let look = style.theme.look();
    let painter = painter.with_clip_rect(frame.intersect(painter.clip_rect()));
    if kind == TextKind::Card {
        // Over a background asset the theme colour becomes a scrim, so the image shows through.
        if clip.background().is_some() {
            painter.rect_filled(frame, 0, rgb(look.background).gamma_multiply(0.35));
        } else {
            paint_backdrop(&painter, frame, &look.backdrop);
            for r in assetcraft_storyboard::card_decor(&look, frame.width(), frame.height()) {
                paint_decor(&painter, frame.min, &r, 1.0);
            }
        }
    }
    // Past its duration (and its out) the words are gone; a Card keeps its background.
    if words.trim().is_empty() || anim.is_hidden() {
        return;
    }
    let size = match kind {
        TextKind::Card => look.card_size,
        TextKind::Overlay => look.overlay_size,
    };
    let height = (frame.height() * size * anim.scale).clamp(6.0, 120.0);
    let shown = shown_text(words, look.uppercase, &anim);
    let color = rgb(look.text).gamma_multiply(anim.alpha);
    let wrap = frame.width() * if kind == TextKind::Card { 0.8 } else { 0.6 };
    let galley = painter.layout_job(job(&shown, height, look.tracking * height, color, wrap));
    let full = painter.layout_job(job(
        &shown_text(words, look.uppercase, &AnimFrame::REST),
        height,
        look.tracking * height,
        color,
        wrap,
    ));
    // Place by the finished text's size so a typewriter doesn't drift as it types.
    let block = full.size();
    let (ax, ay, align) = match kind {
        TextKind::Card => (0.5, 0.5, 0),
        TextKind::Overlay => style.position.anchor(),
    };
    let anchor = pos2(frame.left() + frame.width() * ax, frame.top() + frame.height() * ay);
    let left = match align {
        -1 => anchor.x,
        1 => anchor.x - block.x,
        _ => anchor.x - block.x / 2.0,
    };
    let top = anchor.y - block.y / 2.0 + anim.offset_y * frame.height();
    let at = pos2(left, top);
    if kind == TextKind::Overlay {
        let pad = vec2(height * 0.45, height * 0.25);
        let plate = Rect::from_min_size(at, block).expand2(pad);
        match look.plate {
            Some((fill, alpha)) => {
                let a = (f32::from(alpha) * anim.alpha).round().clamp(0.0, 255.0) as u8;
                let c = rgb(fill);
                painter.rect_filled(plate, 2, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a));
            }
            None => {
                let shadow = Color32::from_black_alpha((160.0 * anim.alpha) as u8);
                let g = painter.layout_job(job(&shown, height, look.tracking * height, shadow, wrap));
                painter.galley(at + vec2(1.0, 1.5), g, shadow);
            }
        }
    }
    let pad = [height * 0.45, height * 0.25];
    for r in assetcraft_storyboard::text_decor(&look, kind, [left, top, block.x, block.y], frame.height(), pad) {
        paint_decor(&painter, Pos2::ZERO, &r, anim.alpha);
    }
    painter.galley(at, galley, color);
}

/// One decor shape, offset by `origin` (card decor is laid out from the frame's corner; text
/// decor is already in screen space).
fn paint_decor(painter: &Painter, origin: Pos2, r: &DecorRect, alpha: f32) {
    let rect = Rect::from_min_size(origin + vec2(r.x, r.y), vec2(r.w, r.h));
    painter.rect_filled(rect, 0, rgb(r.color).gamma_multiply((r.alpha * alpha).clamp(0.0, 1.0)));
}

/// A card's backdrop in the Viewer: a grid of vertex colours from the same
/// [`Backdrop::color_at`] the export uses (gradient and vignette; grain is export-only — at
/// Viewer sizes it reads as noise on the thumbnail, not film).
pub(crate) fn paint_backdrop(painter: &Painter, rect: Rect, b: &Backdrop) {
    if b.count <= 1 && b.vignette <= 0.0 {
        painter.rect_filled(rect, 0, rgb(b.stops[0]));
        return;
    }
    const COLS: u32 = 24;
    const ROWS: u32 = 14;
    let mut mesh = egui::Mesh::default();
    for r in 0..=ROWS {
        for c in 0..=COLS {
            let (u, v) = (c as f32 / COLS as f32, r as f32 / ROWS as f32);
            let [cr, cg, cb] = b.color_at(u, v).map(|x| x.round().clamp(0.0, 255.0) as u8);
            mesh.colored_vertex(rect.lerp_inside(vec2(u, v)), Color32::from_rgb(cr, cg, cb));
        }
    }
    let w = COLS + 1;
    for r in 0..ROWS {
        for c in 0..COLS {
            let i = r * w + c;
            mesh.add_triangle(i, i + 1, i + w);
            mesh.add_triangle(i + 1, i + w + 1, i + w);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn shown_text(words: &str, uppercase: bool, anim: &AnimFrame) -> String {
    let text = if uppercase { words.to_uppercase() } else { words.to_owned() };
    let n = anim.visible_chars(text.chars().count());
    text.chars().take(n).collect()
}

fn job(text: &str, height: f32, tracking: f32, color: Color32, wrap: f32) -> LayoutJob {
    let format = TextFormat {
        font_id: FontId::proportional(height),
        color,
        extra_letter_spacing: tracking,
        ..TextFormat::default()
    };
    let mut job = LayoutJob::single_section(text.to_owned(), format);
    job.wrap.max_width = wrap.max(10.0);
    job
}

/// The kind chip in the top-right corner of `rect`: `CARD ⇄` or `OVERLAY ⇄`. Returns the kind
/// to switch to when clicked. Register it after the clip's own response so it wins the click.
pub(crate) fn kind_chip(ui: &Ui, rect: Rect, clip: &Clip, id: egui::Id) -> Option<TextKind> {
    let kind = clip.text_kind()?;
    if rect.width() < CHIP_W + 8.0 || rect.height() < CHIP_H + 8.0 {
        return None;
    }
    let chip = Rect::from_min_size(pos2(rect.right() - CHIP_W - 4.0, rect.top() + 4.0), vec2(CHIP_W, CHIP_H));
    let response = ui.interact(chip, id, Sense::click());
    let painter = ui.painter();
    let fill = if response.hovered() { Color32::from_black_alpha(230) } else { Color32::from_black_alpha(170) };
    painter.rect_filled(chip, 3, fill);
    if response.hovered() {
        painter.rect_stroke(chip, 3, Stroke::new(1.0, Color32::WHITE), StrokeKind::Inside);
    }
    let label = match kind {
        TextKind::Card => "CARD ⇄",
        TextKind::Overlay => "OVERLAY ⇄",
    };
    painter.text(chip.center(), Align2::CENTER_CENTER, label, FontId::monospace(8.5), Color32::WHITE);
    let other = kind.other();
    let tip = match other {
        TextKind::Overlay => "Text clip: Card. Click to show the text as an overlay on a shot",
        TextKind::Card => "Text clip: Overlay. Click to make it a card of its own",
    };
    response.on_hover_text(tip).clicked().then_some(other)
}

/// Run the switch a chip asked for.
pub(crate) fn switch_kind(nav: &mut Navigator, clip: &Clip, to: TextKind) {
    nav.run("storyboard.text.kind", json!({ "clip": clip.id.0, "kind": to.id() }));
}

/// The Inspector's TEXT section: Card | Overlay, theme, animation, position, and a preview
/// that replays the intro.
pub(crate) fn inspector_section(nav: &mut Navigator, ui: &mut Ui, clip: &Clip) {
    let Some(kind) = clip.text_kind() else {
        // A plain shot: one field puts text over it (it becomes an Overlay).
        crate::storyboard_view::section(ui, "TEXT");
        let id = egui::Id::new(("sb-clip-text", clip.id.0));
        if let Some(t) = crate::inset_view::commit_field(ui, id, "", true, "Add text over this shot…")
            && !t.is_empty()
        {
            nav.run("storyboard.text.set", json!({ "clip": clip.id.0, "text": t }));
        }
        return;
    };
    let style = clip.text_style;
    crate::storyboard_view::section(ui, "TEXT");
    // The words on screen (Paul, 2026-11-08: "I am unable to change the text: 'The goal'").
    // Enter for a new line (two-line titles); Ctrl+Enter or clicking away saves.
    let id = egui::Id::new(("sb-clip-text", clip.id.0));
    let hint = match kind {
        TextKind::Card => "Title text (Ctrl+Enter saves)",
        TextKind::Overlay => "Text over the shot; empty removes it (Ctrl+Enter saves)",
    };
    if let Some(t) = crate::inset_view::commit_field(ui, id, clip.text().unwrap_or(""), true, hint) {
        nav.run("storyboard.text.set", json!({ "clip": clip.id.0, "text": t }));
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Type");
        for k in TextKind::ALL {
            let tip = match k {
                TextKind::Card => "A clip of its own: the theme's background with the text animated in",
                TextKind::Overlay => "Text over the clip's shot, at a position, in the theme's style",
            };
            if ui.selectable_label(kind == k, k.name()).on_hover_text(tip).clicked() && kind != k {
                switch_kind(nav, clip, k);
            }
        }
    });
    let id = clip.id.0;
    let mut style_edit: Option<serde_json::Value> = None;
    egui::Grid::new(("sb-text-style", id)).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Theme");
        egui::ComboBox::from_id_salt(("sb-theme", id)).selected_text(style.theme.name()).show_ui(ui, |ui| {
            for t in TextTheme::ALL {
                let row = ui.horizontal(|ui| {
                    theme_swatch(ui, t);
                    ui.selectable_label(style.theme == t, t.name())
                });
                if row.inner.clicked() && style.theme != t {
                    style_edit = Some(json!({ "clip": id, "theme": t.id() }));
                }
            }
        });
        ui.end_row();
        ui.label("In").on_hover_text("How the text arrives");
        egui::ComboBox::from_id_salt(("sb-anim", id)).selected_text(style.animation.name()).show_ui(ui, |ui| {
            for a in TextAnimation::ALL {
                if ui.selectable_label(style.animation == a, a.name()).clicked() && style.animation != a {
                    style_edit = Some(json!({ "clip": id, "animation": a.id() }));
                }
            }
        });
        ui.end_row();
        ui.label("Duration").on_hover_text("How long the text stays up from the clip's start, its in and out included");
        if let Some(d) = duration_row(ui, clip) {
            style_edit = Some(json!({ "clip": id, "duration": d }));
        }
        ui.end_row();
        ui.label("Effect").on_hover_text("What the text does while it is up");
        egui::ComboBox::from_id_salt(("sb-effect", id)).selected_text(style.effect.name()).show_ui(ui, |ui| {
            for e in TextEffect::ALL {
                if ui.selectable_label(style.effect == e, e.name()).clicked() && style.effect != e {
                    style_edit = Some(json!({ "clip": id, "effect": e.id() }));
                }
            }
        });
        ui.end_row();
        ui.label("Out").on_hover_text("How the text leaves at the end of its duration");
        egui::ComboBox::from_id_salt(("sb-exit", id)).selected_text(style.exit.name()).show_ui(ui, |ui| {
            for e in TextExit::ALL {
                if ui.selectable_label(style.exit == e, e.name()).clicked() && style.exit != e {
                    style_edit = Some(json!({ "clip": id, "exit": e.id() }));
                }
            }
        });
        ui.end_row();
        ui.label("Position");
        ui.add_enabled_ui(kind == TextKind::Overlay, |ui| {
            egui::ComboBox::from_id_salt(("sb-pos", id))
                .selected_text(style.position.name())
                .show_ui(ui, |ui| {
                    for p in TextPosition::ALL {
                        if ui.selectable_label(style.position == p, p.name()).clicked() && style.position != p {
                            style_edit = Some(json!({ "clip": id, "position": p.id() }));
                        }
                    }
                })
                .response
                .on_disabled_hover_text("Cards centre their text; switch to Overlay to place it");
        });
        ui.end_row();
    });
    if let Some(p) = style_edit {
        nav.run("storyboard.text.style", p);
    }
    // Preview: replays the intro, then holds for a second.
    let w = ui.available_width().min(268.0);
    let (frame, _) = ui.allocate_exact_size(vec2(w, w * 9.0 / 16.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(frame, 2, Color32::from_rgb(0x2b, 0x31, 0x3b));
    if kind == TextKind::Overlay {
        painter.text(
            frame.center(),
            Align2::CENTER_CENTER,
            "shot",
            FontId::proportional(12.0),
            Color32::from_gray(0x6a),
        );
    }
    let clip_secs = clip.duration.as_secs_f64();
    let timed = style.duration_ms.is_some() || style.exit != TextExit::None || style.effect != TextEffect::None;
    if !timed {
        // Replays the in, then holds for a second.
        let intro = style.animation.seconds();
        let cycle = intro + 1.0;
        let elapsed = if intro > 0.0 { ui.input(|i| i.time) % cycle } else { f64::INFINITY };
        paint_text(painter, frame, clip, elapsed);
        // Repaint every frame only during the in; during the hold, wake once for the next replay.
        if elapsed < intro {
            ui.ctx().request_repaint();
        } else if elapsed.is_finite() {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((cycle - elapsed).max(0.01)));
        }
        return;
    }
    // In, effect, out, a beat of nothing, again. A long duration is shortened to 5 s here so
    // the out comes round; the caption says the real numbers.
    let shown = style.shown_secs(clip_secs);
    let p = shown.clamp(0.5, PREVIEW_MAX_SECS);
    let preview = TextStyle { duration_ms: Some((p * 1000.0) as u32), ..style };
    let elapsed = ui.input(|i| i.time) % (p + PREVIEW_GAP_SECS);
    paint_text_frame(painter, frame, clip, preview.frame_at(elapsed, p));
    ui.ctx().request_repaint();
    let caption = if style.duration_ms.is_some() && shown < clip_secs {
        format!("Up {shown:.1} s of the clip's {clip_secs:.1} s, then gone")
    } else {
        format!("Up for the whole clip ({clip_secs:.1} s)")
    };
    ui.weak(caption);
}

/// The preview's longest run, in seconds, and the pause before it replays.
const PREVIEW_MAX_SECS: f64 = 5.0;
const PREVIEW_GAP_SECS: f64 = 0.6;

/// The Duration row: "Whole clip", or seconds. A drag commits once, when it ends (one undo
/// step); a typed value commits when it lands. Returns the `duration` param (0 = whole clip).
fn duration_row(ui: &mut Ui, clip: &Clip) -> Option<f64> {
    let clip_secs = clip.duration.as_secs_f64();
    let current = clip.text_style.duration_ms.map(|ms| f64::from(ms) / 1000.0);
    let max = clip_secs.clamp(MIN_SECS, f64::from(MAX_TEXT_MS) / 1000.0);
    let mut out = None;
    ui.horizontal(|ui| {
        let mut whole = current.is_none();
        if ui.checkbox(&mut whole, "Whole clip").on_hover_text("Up until the clip ends").changed() {
            // Unticked: start from three seconds (or the clip, if shorter).
            out = Some(if whole { 0.0 } else { (max.min(3.0) * 10.0).round() / 10.0 });
        }
        if let Some(secs) = current {
            let key = egui::Id::new(("sb-text-duration", clip.id.0));
            let mut v: f64 = ui.data(|d| d.get_temp::<f64>(key)).unwrap_or(secs);
            let r =
                ui.add(egui::DragValue::new(&mut v).range(MIN_SECS..=max).speed(0.05).fixed_decimals(1).suffix(" s"));
            if r.dragged() {
                ui.data_mut(|d| d.insert_temp(key, v));
            } else {
                ui.data_mut(|d| d.remove::<f64>(key));
                let v = (v * 10.0).round() / 10.0;
                if (r.drag_stopped() || r.changed()) && (v - secs).abs() > 1e-6 {
                    out = Some(v);
                }
            }
        }
    });
    out
}

/// Shortest duration, in seconds.
const MIN_SECS: f64 = MIN_TEXT_MS as f64 / 1000.0;

/// A theme's colours in a chip: its background, an "Aa" in its text colour, an accent bar —
/// so eight theme names read at a glance in the picker.
fn theme_swatch(ui: &mut Ui, t: TextTheme) {
    let look = t.look();
    let (rect, _) = ui.allocate_exact_size(vec2(30.0, 16.0), Sense::hover());
    let p = ui.painter();
    paint_backdrop(p, rect, &look.backdrop);
    p.rect_stroke(rect, 2, Stroke::new(1.0, Color32::from_gray(0x55)), StrokeKind::Inside);
    let bar =
        Rect::from_min_max(pos2(rect.left() + 2.0, rect.bottom() - 4.0), pos2(rect.right() - 2.0, rect.bottom() - 2.0));
    p.rect_filled(bar, 0, rgb(look.accent.unwrap_or(t.accent())));
    let aa = if look.uppercase { "AA" } else { "Aa" };
    p.text(rect.center() - vec2(0.0, 1.5), Align2::CENTER_CENTER, aa, FontId::proportional(10.0), rgb(look.text));
}
