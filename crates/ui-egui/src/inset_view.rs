//! The Inspector's INSET section (SR2c, `docs/storyboard-plan.md` § Sections): the selected
//! clip's section and its inset lane — the file (from the Organizer's selection, or a drop on
//! the Strip's INSET row), video or graphic, the corner, the size, Swap, Mix audio, Clear.
//! Every control is one `storyboard.inset` / `storyboard.insetClear` (one undo step each).

use crate::Navigator;
use assetcraft_storyboard::{Clip, Corner, InsetKind, InsetSize, Storyboard};
use egui::{RichText, Ui};
use serde_json::json;
use std::path::Path;

/// The INSET section for `clip`'s section. Nothing for a title card's scene without shots.
pub(crate) fn inspector_section(nav: &mut Navigator, ui: &mut Ui, board: &Storyboard, clip: &Clip) {
    let scene = clip.scene_id;
    if board.inset_span(scene).next().is_none() {
        return;
    }
    let inset = board.scenes.iter().find(|s| s.id == scene).and_then(|s| s.inset.clone());
    let title = board.scene_title(scene).map_or_else(|| format!("Scene {}", scene.0), str::to_owned);
    crate::storyboard_view::section(ui, "INSET");
    ui.weak(format!("Over {title}'s shots, in a corner"));
    let focused = crate::storyboard_view::focused_file(nav);
    let mut edit: Option<serde_json::Value> = None;
    match inset.as_ref().and_then(|i| i.asset.as_ref()) {
        Some(a) => {
            crate::storyboard_view::hero_frame(nav, ui, &a.0);
            ui.label(RichText::new(assetcraft_fs::display_name(Path::new(&a.0))).small()).on_hover_text(a.0.as_str());
        }
        None => {
            ui.weak("Empty slot: drop b-roll, an interview, a host or a graphic on the Strip's INSET row, or");
        }
    }
    let label = if inset.as_ref().is_some_and(|i| !i.is_empty()) {
        "Replace with Organizer selection"
    } else {
        "Use Organizer selection"
    };
    if ui
        .add_enabled(focused.is_some(), egui::Button::new(label))
        .on_hover_text("A video's own length sizes the section (MP4 / MOV)")
        .clicked()
        && let Some(p) = &focused
    {
        edit = Some(json!({ "scene": scene.0, "asset": p.to_string_lossy() }));
    }
    let i = inset.clone().unwrap_or_else(|| assetcraft_storyboard::Inset::slot(InsetKind::Video));
    if let Some(note) = note_field(ui, "inspector", scene.0, i.note.as_deref()) {
        edit = Some(json!({ "scene": scene.0, "note": note }));
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Kind");
        for (k, name, tip) in [
            (InsetKind::Video, "Video", "The host, b-roll, an interview: its audio can be mixed in"),
            (InsetKind::Graphic, "Graphic", "A still: a diagram, a part, a label"),
        ] {
            if ui.selectable_label(i.kind == k, name).on_hover_text(tip).clicked() && i.kind != k {
                edit =
                    Some(json!({ "scene": scene.0, "kind": if k == InsetKind::Video { "video" } else { "graphic" } }));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Corner");
        for c in Corner::ALL {
            if ui.selectable_label(i.corner == c, c.code()).clicked() && i.corner != c {
                edit = Some(json!({ "scene": scene.0, "corner": c.code() }));
            }
        }
        ui.add_space(8.0);
        ui.label("Size");
        for (sz, code) in [(InsetSize::Small, "S"), (InsetSize::Medium, "M"), (InsetSize::Large, "L")] {
            let tip = format!("{:.0} % of the frame's width", sz.share() * 100.0);
            if ui.selectable_label(i.size == sz, code).on_hover_text(tip).clicked() && i.size != sz {
                edit = Some(json!({ "scene": scene.0, "size": code }));
            }
        }
    });
    ui.horizontal(|ui| {
        let mut swap = i.swap;
        if ui
            .checkbox(&mut swap, "Swap")
            .on_hover_text("The inset fills the frame and the section's shots sit in the corner")
            .changed()
        {
            edit = Some(json!({ "scene": scene.0, "swap": swap }));
        }
        let mut mix = i.mix_audio;
        if ui
            .add_enabled(i.kind == InsetKind::Video, egui::Checkbox::new(&mut mix, "Mix audio in"))
            .on_hover_text("Play the video's sound under the section; the music ducks while it does")
            .changed()
        {
            edit = Some(json!({ "scene": scene.0, "mixAudio": mix }));
        }
        if inset.as_ref().is_some_and(|i| !i.is_empty())
            && ui.button("Clear").on_hover_text("Empty the slot; the options stay (Ctrl+Z undoes)").clicked()
        {
            nav.run("storyboard.insetClear", json!({ "scene": scene.0 }));
        }
    });
    if i.kind == InsetKind::Video {
        ui.add_enabled_ui(i.mix_audio, |ui| {
            if let Some(v) =
                level_slider(ui, ("music", scene.0), "Music", i.music_db, assetcraft_storyboard::MUSIC_DB_RANGE)
            {
                edit = Some(json!({ "scene": scene.0, "musicDb": v }));
            }
            if let Some(v) =
                level_slider(ui, ("voice", scene.0), "Voice", i.voice_db, assetcraft_storyboard::VOICE_DB_RANGE)
            {
                edit = Some(json!({ "scene": scene.0, "voiceDb": v }));
            }
        });
    }
    if let Some(params) = edit {
        nav.run("storyboard.inset", params);
    }
    ui.add_space(6.0);
}

/// A text field that commits once: its text lives in egui temp memory while it has focus, and
/// comes back when focus leaves (Enter on a single line, a click away, Tab) if it changed — so
/// one edit is one command, one undo step. `multiline`: Enter makes a new line and Ctrl+Enter
/// or a click away saves. Returns the trimmed new text.
pub(crate) fn commit_field(ui: &mut Ui, id: egui::Id, current: &str, multiline: bool, hint: &str) -> Option<String> {
    let mut text: String = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_owned());
    // Read Ctrl+Enter before the field: a multi-line field consumes Enter.
    let focused = ui.memory(|m| m.has_focus(id.with("edit")));
    let ctrl_enter =
        multiline && focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));
    let edit = if multiline {
        egui::TextEdit::multiline(&mut text).desired_rows(2)
    } else {
        egui::TextEdit::singleline(&mut text)
    };
    let r = ui.add(edit.id(id.with("edit")).hint_text(hint).desired_width(f32::INFINITY));
    if r.has_focus() && !ctrl_enter {
        ui.data_mut(|d| d.insert_temp(id, text));
        return None;
    }
    if ctrl_enter {
        r.surrender_focus();
    }
    ui.data_mut(|d| d.remove::<String>(id));
    let next = text.trim().to_owned();
    ((r.lost_focus() || ctrl_enter) && next != current.trim()).then_some(next)
}

/// The slot's note (what belongs there, in the user's words): a [`commit_field`]. Returns
/// `Some(new)` (`None` inside = cleared).
/// `place` keeps the Inspector's field and the slot menu's apart: one widget id per place, or
/// egui sees one field in two layers (focus jumps, the menu closes).
pub(crate) fn note_field(ui: &mut Ui, place: &str, scene: u32, current: Option<&str>) -> Option<Option<String>> {
    commit_field(
        ui,
        egui::Id::new(("sb-inset-note", place, scene)),
        current.unwrap_or(""),
        false,
        "Note: what goes here (e.g. B-roll: close on the hands)",
    )
    .map(|n| (!n.is_empty()).then_some(n))
}

/// A dB slider that commits once (SR4): the value follows the drag in egui's temp memory and
/// comes back when the drag ends or a typed / clicked value lands — one `storyboard.inset`,
/// one undo step.
pub(crate) fn level_slider(ui: &mut Ui, key: (&str, u32), label: &str, value: i8, range: (i8, i8)) -> Option<i8> {
    let id = egui::Id::new(("sb-inset-level", key.0, key.1));
    let mut v: i32 = ui.data(|d| d.get_temp::<i32>(id)).unwrap_or(i32::from(value));
    let tip = match label {
        "Music" => "How loud the music is under this section while the video's sound plays",
        _ => "How loud the video's own sound is",
    };
    let r = ui
        .horizontal(|ui| {
            ui.label(RichText::new(label).small());
            ui.add(egui::Slider::new(&mut v, i32::from(range.0)..=i32::from(range.1)).suffix(" dB")).on_hover_text(tip)
        })
        .inner;
    if r.dragged() {
        ui.data_mut(|d| d.insert_temp(id, v));
        return None;
    }
    ui.data_mut(|d| d.remove::<i32>(id));
    let done = r.drag_stopped() || (r.changed() && !r.dragged());
    let v = i8::try_from(v).ok()?;
    (done && v != value).then_some(v)
}
