//! Text clips on screen: the kind chip that switches a clip between Card and Overlay, and the
//! Inspector's TEXT section. Titles themselves are drawn by the compositor through the title
//! service, in the Viewer and in the Inspector's preview alike (L3, TF-4).
//!
//! Draws only. Looks and animation frames come from `assetcraft_storyboard::text`; a click on
//! the chip (Shot map tile, Strip clip) or on the Inspector's Card | Overlay buttons runs
//! `storyboard.text.kind`, and the Inspector's pickers run `storyboard.text.style`.

use crate::Navigator;
use crate::storyboard_view::rgb;
use assetcraft_storyboard::{
    Backdrop, BoxAlign, Clip, DesignPick, Look, MAX_TEXT_MS, MIN_TEXT_MS, TextAnimation, TextBox, TextEffect, TextExit,
    TextKind, TextPosition, TextTheme, UserLooks,
};
use egui::{Align2, Color32, FontId, Painter, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

/// Kind chip size.
const CHIP_W: f32 = 52.0;
const CHIP_H: f32 = 14.0;

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
    let mut edit_design: Option<(&str, serde_json::Value)> = None;
    // R-T18: the design's own text fields (a banner's Caption), when a design draws the clip.
    let fields = nav.session.design_fields(clip);
    let mut field_edit: Option<serde_json::Value> = None;
    // The user's loaded themes (TP2a-2): picked by id, listed after the built-ins.
    let looks = nav.session.user_looks().clone();
    let yours: Vec<(String, String, Look)> = nav
        .session
        .loaded_themes()
        .iter()
        .map(|u| (u.theme.id.clone(), u.theme.name.clone(), u.theme.look()))
        .collect();
    let picked_user = style.theme_id.map(|u| u.as_str().to_string());
    let drawn_by = nav.session.clip_design(clip);
    let design_problem = nav.session.design_problem(clip);
    // The theme pack's own design for this clip's role, whatever the clip picked.
    let pack_design = {
        let mut own = clip.clone();
        own.text_style.design = None;
        nav.session.clip_design(&own).map(|(name, _)| name)
    };
    let selected = match &picked_user {
        Some(uid) => yours
            .iter()
            .find(|(i, ..)| i == uid)
            .map_or_else(|| format!("{uid} (not loaded)"), |(_, name, _)| name.clone()),
        None => style.theme.name().to_string(),
    };
    // A built-in design (Laser Trace, Stack) or the pack's own is drawing: it brings its own in,
    // effect and out, so those rows rest (Paul, 2026-11-20: "unable to make any changes").
    let designed = drawn_by.is_some() && design_problem.is_none();
    // The notes wrap inside the value column; unwrapped, a long one widened the Inspector past
    // the window and it vanished (Paul, 2026-11-20).
    let note_w = (ui.available_width() - NOTE_INDENT).max(80.0);
    // The title's area (L5): where it starts before it has one, and a slider held right now.
    let natural = TextBox::natural(kind, style.position, drawn_by.is_some());
    let mut area_ui =
        AreaUi { drag: nav.state.storyboard.area_drag.filter(|(c, _)| *c == id).map(|(_, b)| b), commit: None };
    egui::Grid::new(("sb-text-style", id)).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Theme");
        egui::ComboBox::from_id_salt(("sb-theme", id)).selected_text(selected).show_ui(ui, |ui| {
            for t in TextTheme::ALL {
                let on = picked_user.is_none() && style.theme == t;
                let row = ui.horizontal(|ui| {
                    theme_swatch(ui, &t.look(), t.accent());
                    ui.selectable_label(on, t.name())
                });
                if row.inner.clicked() && !on {
                    style_edit = Some(json!({ "clip": id, "theme": t.id() }));
                }
            }
            if !yours.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Yours").small().weak());
            }
            for (uid, name, look) in &yours {
                let on = picked_user.as_deref() == Some(uid.as_str());
                let row = ui.horizontal(|ui| {
                    theme_swatch(ui, look, look.background);
                    ui.selectable_label(on, name)
                });
                if row.inner.clicked() && !on {
                    style_edit = Some(json!({ "clip": id, "theme": uid }));
                }
            }
        });
        ui.end_row();
        if let Some(missing) = style.missing_theme(&looks) {
            ui.label("");
            note(
                ui,
                note_w,
                egui::RichText::new(format!(
                    "“{}” isn't loaded: drawing {}. Import it (File › Import Theme…) or pick another.",
                    missing.as_str(),
                    style.theme.name()
                ))
                .small()
                .color(crate::theme::WARN),
            );
            ui.end_row();
        }
        // TP2b-2: which design it draws through.
        ui.label("Design")
            .on_hover_text("A motion design the text draws through: its theme pack's own, a built-in banner, or none");
        let pack_own = pack_design
            .as_deref()
            .map_or_else(|| "Theme's own (none in this theme)".to_owned(), |n| format!("Theme's own: {n}"));
        let shown = style.design.map_or(pack_own.clone(), |d| d.name().to_owned());
        egui::ComboBox::from_id_salt(("sb-design", id)).selected_text(shown).show_ui(ui, |ui| {
            if ui.selectable_label(style.design.is_none(), pack_own.as_str()).clicked() && style.design.is_some() {
                style_edit = Some(json!({ "clip": id, "design": "theme" }));
            }
            for d in DesignPick::ALL {
                if ui.selectable_label(style.design == Some(d), d.name()).clicked() && style.design != Some(d) {
                    style_edit = Some(json!({ "clip": id, "design": d.id() }));
                }
            }
        });
        ui.end_row();
        if let Some(why) = &design_problem {
            ui.label("");
            note(
                ui,
                note_w,
                egui::RichText::new(format!("The design couldn't draw ({why}); the theme's words show instead."))
                    .small()
                    .color(crate::theme::WARN),
            );
            ui.end_row();
        }
        // R-T7: a design brings its own moves and case.
        if let Some((name, role)) = drawn_by.as_ref().filter(|_| design_problem.is_none()) {
            ui.label("");
            let whose = if style.design.is_some() { "the built-in".to_owned() } else { format!("the pack's {role}") };
            note(
                ui,
                note_w,
                egui::RichText::new(format!(
                    "Drawn through {whose} design “{name}”: its own in, effect, out and capitals apply; \
                     Duration sets how long it's up."
                ))
                .small()
                .weak(),
            );
            ui.end_row();
        }
        // TP4c: the pack's own design opens in EffectCraft (its edit copy); a reload picks up the save.
        if let (Some(uid), Some(_), None) = (&picked_user, &pack_design, style.design) {
            ui.label("");
            let installed = nav.session.app_program("effectcraft").is_some();
            let tip = if installed {
                "Opens this design's edit copy (designs/<name>.ecproj) in EffectCraft, making it from the \
                 template the first time. Save there, then File › Reload Themes."
            } else {
                "EffectCraft isn't installed here (or set ASSETCRAFT_EFFECTCRAFT_EXE to its program, then press F5)"
            };
            let role = assetcraft_animatic::design_role(kind);
            ui.horizontal(|ui| {
                let button = ui.add_enabled(installed, egui::Button::new("Edit in EffectCraft…").small());
                if button.on_hover_text(tip).on_disabled_hover_text(tip).clicked() {
                    edit_design = Some(("storyboard.editDesign", json!({ "theme": uid, "role": role })));
                }
                // TP4c-2: the edit is drawing (its copy is newer); keep it in the pack's template.
                if nav.session.design_edit_copy(uid, role).is_some()
                    && ui
                        .small_button("Save into Template")
                        .on_hover_text(
                            "Writes your edit into the pack's design template, so the pack carries it \
                             when it's shared or imported. The edit copy stays for more editing.",
                        )
                        .clicked()
                {
                    edit_design = Some(("storyboard.saveDesign", json!({ "theme": uid, "role": role })));
                }
            });
            ui.end_row();
        }
        for (name, designer, own) in &fields {
            ui.label(name.as_str())
                .on_hover_text("One of the design's own text fields; the clip's words fill the headline");
            // Kept to the value column: a full-width field widens the Inspector (R-T17).
            ui.scope(|ui| {
                ui.set_max_width(note_w);
                ui.horizontal(|ui| {
                    let hint = if designer.trim().is_empty() { "(empty)".to_owned() } else { designer.clone() };
                    let reset =
                        own.is_some() && ui.small_button("↺").on_hover_text("Back to the design's words").clicked();
                    let current = own.clone().unwrap_or_default();
                    let key = egui::Id::new(("sb-design-field", id, name.as_str()));
                    if reset {
                        field_edit = Some(json!({ "clip": id, "control": name, "reset": true }));
                    } else if let Some(words) = crate::inset_view::commit_field(ui, key, &current, false, &hint) {
                        field_edit = Some(json!({ "clip": id, "control": name, "text": words }));
                    }
                });
            });
            ui.end_row();
        }
        let theme_moves = style.look_with(&looks).motion;
        let rest = "The design brings its own; pick the theme's own design (or None) to set this";
        ui.label("In").on_hover_text("How the text arrives");
        let pick = resting(ui, designed, rest, |ui| {
            move_combo(
                ui,
                ("sb-anim", id),
                style.animation,
                theme_moves.animation,
                &TextAnimation::ALL,
                TextAnimation::name,
                TextAnimation::id,
            )
        });
        if let Some(v) = pick {
            style_edit = Some(json!({ "clip": id, "animation": v }));
        }
        ui.end_row();
        ui.label("Duration").on_hover_text("How long the text stays up from the clip's start, its in and out included");
        if let Some(d) = duration_row(ui, clip, &looks) {
            style_edit = Some(json!({ "clip": id, "duration": d }));
        }
        ui.end_row();
        ui.label("Effect").on_hover_text("What the text does while it is up");
        let pick = resting(ui, designed, rest, |ui| {
            move_combo(
                ui,
                ("sb-effect", id),
                style.effect,
                theme_moves.effect,
                &TextEffect::ALL,
                TextEffect::name,
                TextEffect::id,
            )
        });
        if let Some(v) = pick {
            style_edit = Some(json!({ "clip": id, "effect": v }));
        }
        ui.end_row();
        ui.label("Out").on_hover_text("How the text leaves at the end of its duration");
        let pick = resting(ui, designed, rest, |ui| {
            move_combo(ui, ("sb-exit", id), style.exit, theme_moves.exit, &TextExit::ALL, TextExit::name, TextExit::id)
        });
        if let Some(v) = pick {
            style_edit = Some(json!({ "clip": id, "exit": v }));
        }
        ui.end_row();
        // A starting place for an overlay that has no area yet; once it has one (or on a card) the
        // Area rows below place it, so the row isn't shown (TF-5: greyed out, it confused).
        if kind == TextKind::Overlay && style.area.is_none() {
            ui.label("Position").on_hover_text("Where the overlay starts; the Area rows move it from there");
            egui::ComboBox::from_id_salt(("sb-pos", id)).selected_text(style.position.name()).show_ui(ui, |ui| {
                for p in TextPosition::ALL {
                    if ui.selectable_label(style.position == p, p.name()).clicked() && style.position != p {
                        style_edit = Some(json!({ "clip": id, "position": p.id() }));
                    }
                }
            });
            ui.end_row();
        }
        let safe = nav.session.storyboard().map_or(TextBox::FILM_SAFE, |b| b.frame_spec().safe_area());
        area_rows(ui, &mut area_ui, id, (style.area, natural), safe);
    });
    if let Some(p) = style_edit {
        nav.run("storyboard.text.style", p);
    }
    match area_ui.commit.take() {
        Some(p) => {
            nav.state.storyboard.area_drag = None;
            nav.run("storyboard.text.area", p);
        }
        None => nav.state.storyboard.area_drag = area_ui.drag.map(|b| (id, b)),
    }
    if let Some(p) = field_edit {
        nav.run("storyboard.text.designField", p);
    }
    if let Some((command, p)) = edit_design {
        nav.run(command, p);
    }
    // Preview (TF-4): the title as the Viewer and the export draw it — the title service's
    // frames, design and area included — in the board's frame (F2): a vertical post previews
    // tall, held to a sensible height.
    let ratio = nav.session.storyboard().map_or(16.0 / 9.0, |b| b.frame_spec().ratio() as f32).clamp(0.1, 10.0);
    let w = ui.available_width().min(268.0);
    let h = (w / ratio).min(220.0);
    let (frame, _) = ui.allocate_exact_size(vec2(h * ratio, h), Sense::hover());
    ui.painter().rect_filled(frame, 2, Color32::from_rgb(0x2b, 0x31, 0x3b));
    if kind == TextKind::Overlay {
        ui.painter().text(
            frame.center(),
            Align2::CENTER_CENTER,
            "shot",
            FontId::proportional(12.0),
            Color32::from_gray(0x6a),
        );
    }
    // A slider held shows its box here too, as in the Viewer.
    let held = nav.state.storyboard.area_drag.filter(|(at, _)| *at == id).map(|(_, area)| {
        let mut c = clip.clone();
        c.text_style.area = Some(area);
        c
    });
    let shown_clip = held.as_ref().unwrap_or(clip);
    let clip_secs = clip.duration.as_secs_f64();
    let looks = nav.session.user_looks().clone();
    let timing = style.timing_with(&looks);
    let timed = timing.duration_ms.is_some() || timing.exit != TextExit::None || timing.effect != TextEffect::None;
    let slot = crate::strip_view::preview_slot(clip);
    if !timed {
        // Replays the in, then holds for a second.
        let intro = timing.animation.seconds();
        let cycle = intro + 1.0;
        let elapsed = if intro > 0.0 { ui.input(|i| i.time) % cycle } else { f64::INFINITY };
        let at = if elapsed < intro { elapsed } else { f64::INFINITY };
        crate::strip_view::show_title_as(nav, ui, frame, shown_clip, at, slot);
        // Repaint every frame only during the in; during the hold, wake once for the next replay.
        if elapsed < intro {
            ui.ctx().request_repaint();
        } else if elapsed.is_finite() {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((cycle - elapsed).max(0.01)));
        }
        return;
    }
    // In, effect, out, a beat of nothing, again, on the clip's own clock. A title up longer than
    // the preview's 5 s plays its first half, then skips to its last, so the out comes round;
    // the caption says the real numbers.
    let shown = timing.shown_secs(clip_secs);
    let p = shown.clamp(0.5, PREVIEW_MAX_SECS);
    let t = ui.input(|i| i.time) % (p + PREVIEW_GAP_SECS);
    let at = if t >= p {
        shown + 1.0
    } else if shown <= p || t < p / 2.0 {
        t
    } else {
        shown - (p - t)
    };
    crate::strip_view::show_title_as(nav, ui, frame, shown_clip, at, slot);
    ui.ctx().request_repaint();
    let caption = if timing.duration_ms.is_some() && shown < clip_secs {
        format!("Up {shown:.1} s of the clip's {clip_secs:.1} s, then gone")
    } else {
        format!("Up for the whole clip ({clip_secs:.1} s)")
    };
    ui.weak(caption);
}

/// An area slider: its label, its key, its value, its range and its hint.
type AreaRow = (&'static str, &'static str, f32, (f32, f32), &'static str);

/// The area rows' state for one pass: a slider held (the box shown live) and what to commit.
struct AreaUi {
    drag: Option<TextBox>,
    commit: Option<serde_json::Value>,
}

/// The title's area (L5; Paul, 2026-11-20: "alignment left|right|center, width, height,
/// vertical placement … and there should be a button for dead center"): Left / Center / Right,
/// **Dead center** and **Reset** (a row of their own), then Width, Height and Vertical sliders in percent of
/// the frame. A slider shows its box live in the Viewer while held and commits once on release
/// (one undo step); clicks commit at once. Before the title has an area, the sliders start from
/// where it falls by itself (`natural`). Everything keeps inside the board's safe area `safe`
/// (TF-2: the social guide on a vertical frame): Width and Height stop at its size, and Vertical
/// runs from its top (0 %) to its bottom (100 %).
fn area_rows(ui: &mut Ui, a: &mut AreaUi, id: u32, (area, natural): (Option<TextBox>, TextBox), safe: [f32; 4]) {
    let shown = a.drag.or(area).unwrap_or(natural).clamped_in(safe);
    ui.label("Area").on_hover_text("Where the title sits and its size, inside the safe area");
    ui.horizontal(|ui| {
        for al in BoxAlign::ALL {
            let on = area.is_some_and(|b| b.align == al && b.x.is_none());
            if ui.selectable_label(on, al.name()).on_hover_text("Snap to this side of the safe area").clicked() && !on {
                a.commit = Some(json!({ "clip": id, "align": al.id() }));
            }
        }
    });
    ui.end_row();
    // On a row of their own: beside the three sides they widened the Inspector past its width.
    ui.label("");
    ui.horizontal(|ui| {
        if ui.button("Dead center").on_hover_text("The middle of the frame, at the same size").clicked() {
            a.commit = Some(json!({ "clip": id, "center": true }));
        }
        if ui
            .add_enabled(area.is_some(), egui::Button::new("Reset"))
            .on_hover_text("Back to where the title falls by itself")
            .on_disabled_hover_text("The title falls where it does by itself")
            .clicked()
        {
            a.commit = Some(json!({ "clip": id, "reset": true }));
        }
    });
    ui.end_row();
    let pct = |v: u16| f32::from(v) / 10.0;
    // The biggest the safe area allows, and Vertical as a share of where the middle can go.
    let big = TextBox { width: u16::MAX, height: u16::MAX, ..shown }.clamped_in(safe);
    let (top, bottom) = shown.y_range(safe);
    let span = f32::from(bottom.saturating_sub(top)).max(1.0);
    let vertical = if bottom > top { f32::from(shown.y.saturating_sub(top)) / span * 100.0 } else { 50.0 };
    // Horizontal the same way across (TF-5): from its left edge on the safe area's left (0 %) to
    // its right edge on the safe area's right (100 %); Left / Center / Right are its snaps.
    let (left, right) = shown.x_range(safe);
    let xspan = f32::from(right.saturating_sub(left)).max(1.0);
    let horizontal = if right > left { f32::from(shown.x_in(safe).saturating_sub(left)) / xspan * 100.0 } else { 50.0 };
    let rows: [AreaRow; 4] = [
        ("Width", "width", pct(shown.width), (pct(TextBox::WIDTH.0), pct(big.width)), "Of the frame's width"),
        ("Height", "height", pct(shown.height), (pct(TextBox::HEIGHT.0), pct(big.height)), "Of the frame's height"),
        (
            "Horizontal",
            "x",
            horizontal,
            (0.0, 100.0),
            "0 % puts its left edge on the safe area's left, 100 % its right edge on the safe area's right",
        ),
        (
            "Vertical",
            "y",
            vertical,
            (0.0, 100.0),
            "0 % puts its top on the safe area's top, 100 % its bottom on the safe area's bottom",
        ),
    ];
    for (label, key, value, (lo, hi), tip) in rows {
        ui.label(label).on_hover_text(tip);
        let mut v = value;
        let r = ui.add(egui::Slider::new(&mut v, lo..=hi).suffix(" %").max_decimals(1));
        // Only the user's hand moves it: a slider can nudge a value onto its own grid on the
        // first frame and call that a change, which would commit an area nobody asked for.
        let by_hand = r.dragged() || r.drag_stopped() || r.has_focus() || r.clicked();
        let moved = r.changed() && by_hand && (v - value).abs() >= 0.05;
        if moved {
            let pm = (v * 10.0).round().clamp(0.0, 1000.0) as u16;
            let mut b = shown;
            match key {
                "width" => b.width = pm,
                "height" => b.height = pm,
                "x" => b.x = Some(left + (v.clamp(0.0, 100.0) / 100.0 * xspan).round() as u16),
                _ => b.y = top + (v.clamp(0.0, 100.0) / 100.0 * span).round() as u16,
            }
            a.drag = Some(b.clamped_in(safe));
        }
        if r.drag_stopped() || (moved && !r.dragged()) {
            let b = a.drag.unwrap_or(shown);
            let mut commit = json!({
                "clip": id,
                "width": f64::from(b.width) / 10.0,
                "height": f64::from(b.height) / 10.0,
                "y": f64::from(b.y) / 10.0,
            });
            // Slid across: its `x`; otherwise still snapped to its side.
            match b.x {
                Some(x) => commit["x"] = json!(f64::from(x) / 10.0),
                None => commit["align"] = json!(b.align.id()),
            }
            a.commit = Some(commit);
        }
        ui.end_row();
    }
}

/// The width of the Inspector's label column and the gap after it: what a note in the value
/// column leaves of the section's width.
const NOTE_INDENT: f32 = 72.0;

/// A note in the value column, wrapped to `width`.
fn note(ui: &mut Ui, width: f32, text: egui::RichText) {
    ui.scope(|ui| {
        ui.set_max_width(width);
        ui.add(egui::Label::new(text).wrap());
    });
}

/// A row that rests (drawn disabled, with `why` on hover) while `resting`.
fn resting<R>(ui: &mut Ui, resting: bool, why: &str, add: impl FnOnce(&mut Ui) -> Option<R>) -> Option<R> {
    let r = ui.add_enabled_ui(!resting, add);
    if resting {
        r.response.on_disabled_hover_text(why);
        return None;
    }
    r.inner
}

/// The preview's longest run, in seconds, and the pause before it replays.
const PREVIEW_MAX_SECS: f64 = 5.0;
const PREVIEW_GAP_SECS: f64 = 0.6;

/// A move's menu (themes 1g): first "Theme (…)" — follow the theme — then every move. The
/// shown value says which: "Zoom · theme" or "Fade". Returns the param to send: a move's id,
/// or `"theme"`.
fn move_combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    salt: (&str, u32),
    own: Option<T>,
    theme: T,
    all: &[T],
    name: fn(T) -> &'static str,
    id: fn(T) -> &'static str,
) -> Option<&'static str> {
    let mut out = None;
    let shown = match own {
        Some(v) => name(v).to_owned(),
        None => format!("{} · theme", name(theme)),
    };
    egui::ComboBox::from_id_salt(salt).selected_text(shown).show_ui(ui, |ui| {
        let follow = format!("Theme ({})", name(theme));
        if ui.selectable_label(own.is_none(), follow).on_hover_text("Follow the theme's move").clicked()
            && own.is_some()
        {
            out = Some("theme");
        }
        ui.separator();
        for &v in all {
            if ui.selectable_label(own == Some(v), name(v)).clicked() && own != Some(v) {
                out = Some(id(v));
            }
        }
    });
    out
}

/// The Duration row: the theme's, the whole clip, or seconds. A drag commits once, when it
/// ends (one undo step); a typed value commits when it lands. Returns the `duration` param:
/// `"theme"`, `0` (whole clip) or seconds.
fn duration_row(ui: &mut Ui, clip: &Clip, looks: &UserLooks) -> Option<serde_json::Value> {
    let clip_secs = clip.duration.as_secs_f64();
    let own = clip.text_style.duration_ms;
    let theme = clip.text_style.look_with(looks).motion.duration_ms;
    let max = clip_secs.clamp(MIN_SECS, f64::from(MAX_TEXT_MS) / 1000.0);
    let label =
        |ms: Option<u32>| ms.map_or_else(|| "Whole clip".to_owned(), |ms| format!("{:.1} s", f64::from(ms) / 1000.0));
    let mut out = None;
    ui.horizontal(|ui| {
        let shown = match own {
            None => format!("{} · theme", label(theme)),
            Some(0) => "Whole clip".to_owned(),
            Some(_) => "Set time".to_owned(),
        };
        egui::ComboBox::from_id_salt(("sb-duration", clip.id.0)).selected_text(shown).show_ui(ui, |ui| {
            if ui.selectable_label(own.is_none(), format!("Theme ({})", label(theme))).clicked() && own.is_some() {
                out = Some(json!("theme"));
            }
            ui.separator();
            if ui.selectable_label(own == Some(0), "Whole clip").on_hover_text("Up until the clip ends").clicked()
                && own != Some(0)
            {
                out = Some(json!(0));
            }
            if ui.selectable_label(own.is_some_and(|ms| ms > 0), "Set time").clicked() && !own.is_some_and(|ms| ms > 0)
            {
                // Start from three seconds (or the clip, if shorter).
                out = Some(json!((max.min(3.0) * 10.0).round() / 10.0));
            }
        });
        if let Some(ms) = own.filter(|ms| *ms > 0) {
            let secs = f64::from(ms) / 1000.0;
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
                    out = Some(json!(v));
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
fn theme_swatch(ui: &mut Ui, look: &Look, accent: u32) {
    let (rect, _) = ui.allocate_exact_size(vec2(30.0, 16.0), Sense::hover());
    let p = ui.painter();
    paint_backdrop(p, rect, &look.backdrop);
    p.rect_stroke(rect, 2, Stroke::new(1.0, Color32::from_gray(0x55)), StrokeKind::Inside);
    let bar =
        Rect::from_min_max(pos2(rect.left() + 2.0, rect.bottom() - 4.0), pos2(rect.right() - 2.0, rect.bottom() - 2.0));
    p.rect_filled(bar, 0, rgb(look.accent.unwrap_or(accent)));
    let aa = if look.uppercase { "AA" } else { "Aa" };
    p.text(rect.center() - vec2(0.0, 1.5), Align2::CENTER_CENTER, aa, FontId::proportional(10.0), rgb(look.text));
}
