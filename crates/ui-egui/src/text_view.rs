//! Text clips on screen: Cards and Overlays painted with their theme and intro animation, and
//! the kind chip that switches a clip between them.
//!
//! Draws only. Looks and animation frames come from `assetcraft_storyboard::text`; a click on
//! the chip (Shot map tile, Strip clip) or on the Inspector's Card | Overlay buttons runs
//! `storyboard.text.kind`, and the Inspector's pickers run `storyboard.text.style`.

use crate::Navigator;
use crate::storyboard_view::rgb;
use assetcraft_storyboard::{AnimFrame, Clip, TextAnimation, TextKind, TextPosition, TextTheme};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Painter, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

/// Kind chip size.
const CHIP_W: f32 = 52.0;
const CHIP_H: f32 = 14.0;

/// Paint a clip's text into `frame` (16:9), `elapsed` seconds into the clip (`f64::INFINITY` =
/// at rest). A Card fills the frame with its theme's background first; an Overlay paints only
/// the text, over whatever is already there. Clips without text paint nothing.
pub(crate) fn paint_text(painter: &Painter, frame: Rect, clip: &Clip, elapsed: f64) {
    let (Some(kind), Some(words)) = (clip.text_kind(), clip.text()) else { return };
    let style = clip.text_style;
    let look = style.theme.look();
    let anim = style.animation.frame(elapsed);
    let painter = painter.with_clip_rect(frame.intersect(painter.clip_rect()));
    if kind == TextKind::Card {
        painter.rect_filled(frame, 0, rgb(look.background));
    }
    if words.trim().is_empty() {
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
    painter.galley(at, galley, color);
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
    let Some(kind) = clip.text_kind() else { return };
    let style = clip.text_style;
    crate::storyboard_view::section(ui, "TEXT");
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
                if ui.selectable_label(style.theme == t, t.name()).clicked() && style.theme != t {
                    style_edit = Some(json!({ "clip": id, "theme": t.id() }));
                }
            }
        });
        ui.end_row();
        ui.label("Animation");
        egui::ComboBox::from_id_salt(("sb-anim", id)).selected_text(style.animation.name()).show_ui(ui, |ui| {
            for a in TextAnimation::ALL {
                if ui.selectable_label(style.animation == a, a.name()).clicked() && style.animation != a {
                    style_edit = Some(json!({ "clip": id, "animation": a.id() }));
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
    let intro = style.animation.seconds();
    let cycle = intro + 1.0;
    let elapsed = if intro > 0.0 { ui.input(|i| i.time) % cycle } else { f64::INFINITY };
    paint_text(painter, frame, clip, elapsed);
    // Repaint every frame only during the intro; during the hold, wake once for the next replay.
    if elapsed < intro {
        ui.ctx().request_repaint();
    } else if elapsed.is_finite() {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((cycle - elapsed).max(0.01)));
    }
}
