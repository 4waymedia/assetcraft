//! The storyboard setup (frame-settings.md F4; Paul, 2026-11-17, D1: "Dimension, Framerate,
//! Default Crop, Template Preset"): the Cut's frame — a film, square or social preset or a
//! custom size, the frame rate, and how shots of another shape are cropped in.
//!
//! The same controls serve two places. File › Storyboard Setup… changes the open storyboard
//! (`storyboard.frame`, one undo step each); the Start screen's Frame row sets the frame new
//! storyboards start with (`storyboard.defaultFrame`, kept in prefs). The template preset stays
//! the Start screen's own picker. The UI only draws: every change is a command.

use crate::Navigator;
use assetcraft_storyboard::{FRAME_PRESETS, FRAME_RATES, Fill, FrameSpec, Snap};
use egui::{RichText, Ui};
use serde_json::{Value, json};

/// Which frame the controls change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    /// The open storyboard's (`storyboard.frame`).
    Board,
    /// The workspace default for new storyboards (`storyboard.defaultFrame`).
    Default,
}

impl Target {
    fn command(self) -> &'static str {
        match self {
            Target::Board => "storyboard.frame",
            Target::Default => "storyboard.defaultFrame",
        }
    }
}

/// The frame the controls show for `target`.
pub(crate) fn current(nav: &Navigator, target: Target) -> FrameSpec {
    match target {
        Target::Board => nav.session.storyboard().map(|b| b.frame_spec()).unwrap_or_default(),
        Target::Default => nav.session.default_frame(),
    }
}

/// The Dimension, Frame rate and Crop rows, as a grid of `columns` 2. Each change runs the
/// target's command at once.
pub(crate) fn controls(nav: &mut Navigator, ui: &mut Ui, target: Target, id: &str) {
    let frame = current(nav, target);
    let mut change: Option<Value> = None;
    egui::Grid::new(("frame-setup", id)).num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Dimension");
        ui.vertical(|ui| {
            let name = frame
                .preset
                .as_deref()
                .and_then(assetcraft_storyboard::frame_preset)
                .map_or_else(|| "Custom".to_owned(), |p| p.name.to_owned());
            egui::ComboBox::from_id_salt(("frame-preset", id)).selected_text(name).width(220.0).show_ui(ui, |ui| {
                let mut group = "";
                for p in FRAME_PRESETS {
                    if p.group != group {
                        group = p.group;
                        ui.label(RichText::new(group).small().weak());
                    }
                    let on = frame.preset.as_deref() == Some(p.id);
                    let label = format!("{}   {} × {}", p.name, p.width, p.height);
                    if ui.selectable_label(on, label).on_hover_text(p.for_).clicked() && !on {
                        change = Some(json!({ "preset": p.id }));
                    }
                }
            });
            // The size itself: typing a value makes it custom.
            ui.horizontal(|ui| {
                let (mut w, mut h) = (frame.width, frame.height);
                let (lo, hi) = assetcraft_storyboard::FRAME_SIDE_RANGE;
                let rw = ui.add(egui::DragValue::new(&mut w).range(lo..=hi).speed(2.0).suffix(" px"));
                ui.weak("×");
                let rh = ui.add(egui::DragValue::new(&mut h).range(lo..=hi).speed(2.0).suffix(" px"));
                ui.weak(frame.ratio_label());
                let even = |v: u32| v - v % 2;
                let done = |r: &egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
                if (done(&rw) || done(&rh)) && (even(w), even(h)) != (frame.width, frame.height) {
                    change = Some(json!({ "width": even(w), "height": even(h) }));
                }
            });
        });
        ui.end_row();
        ui.label("Frame rate");
        ui.horizontal(|ui| {
            for fps in FRAME_RATES {
                if ui.selectable_label(frame.fps == fps, fps.to_string()).clicked() && frame.fps != fps {
                    change = Some(json!({ "fps": fps }));
                }
            }
            ui.weak("fps");
        });
        ui.end_row();
        ui.label("Crop");
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                for (fill, tip) in [
                    (Fill::Vertical, "The shot's height fills the frame: a wide shot in a tall frame loses its sides"),
                    (
                        Fill::Horizontal,
                        "The shot's width fills the frame: a wide shot in a tall frame keeps its sides, with bands",
                    ),
                ] {
                    if ui.selectable_label(frame.fill == fill, fill.label()).on_hover_text(tip).clicked()
                        && frame.fill != fill
                    {
                        change = Some(json!({ "fill": fill }));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.weak("Snap to");
                for snap in Snap::ALL {
                    if ui.selectable_label(frame.snap == snap, snap.label()).clicked() && frame.snap != snap {
                        change = Some(json!({ "snap": snap }));
                    }
                }
            });
        });
        ui.end_row();
    });
    if let Some(p) = change {
        nav.run(target.command(), p);
    }
}

/// File › Storyboard Setup…: open the window.
pub(crate) fn open(nav: &mut Navigator) {
    if nav.session.storyboard().is_some() {
        nav.state.setup_open = true;
    }
}

/// The Setup window for the open storyboard, while it is open.
pub(crate) fn window(nav: &mut Navigator, ctx: &egui::Context) {
    if !nav.state.setup_open {
        return;
    }
    if nav.session.storyboard().is_none() {
        nav.state.setup_open = false;
        return;
    }
    let mut open = true;
    egui::Window::new("Storyboard Setup")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(380.0);
            controls(nav, ui, Target::Board, "board");
            ui.separator();
            layout_controls(nav, ui);
            ui.add_space(6.0);
            ui.weak("Each change is one undo step. The Viewer and the export follow the frame.");
            ui.separator();
            ui.horizontal(|ui| {
                let same = current(nav, Target::Board) == current(nav, Target::Default);
                if ui
                    .add_enabled(!same, egui::Button::new("Use for New Storyboards"))
                    .on_hover_text("New storyboards start with this frame")
                    .on_disabled_hover_text("New storyboards already start with this frame")
                    .clicked()
                {
                    nav.run("storyboard.defaultFrame", json!({ "fromBoard": true }));
                }
                if ui.button("Done").clicked() {
                    nav.state.setup_open = false;
                }
            });
        });
    if !open {
        nav.state.setup_open = false;
    }
}

/// The stage's layout theme for the open storyboard (F5, L1): Full, or Split stacked or side
/// by side, and region A's share. Each choice is one `storyboard.layout`.
pub(crate) fn layout_controls(nav: &mut Navigator, ui: &mut Ui) {
    use assetcraft_storyboard::{BAND_MAX, Layout, SPLIT_SHARES, TextTheme};
    let current = nav.session.storyboard().and_then(|b| b.layout.clone());
    let mut change: Option<Value> = None;
    let framed = matches!(current, Some(Layout::Framed { .. }));
    egui::Grid::new("layout-setup").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Layout");
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let (split, stack) = match current {
                    Some(Layout::Split { stack, .. }) => (true, stack),
                    _ => (false, true),
                };
                let full = current.is_none();
                if ui.selectable_label(full, "Full").on_hover_text("One shot fills the frame").clicked() && !full {
                    change = Some(json!({ "kind": "full" }));
                }
                if ui
                    .selectable_label(split && stack, "Split ▤")
                    .on_hover_text("Two regions, top (A) and bottom (B): a talking head over b-roll, a reaction")
                    .clicked()
                    && !(split && stack)
                {
                    change = Some(json!({ "kind": "split", "stack": true }));
                }
                if ui
                    .selectable_label(split && !stack, "Split ◫")
                    .on_hover_text(
                        "Two regions side by side, left (A) and right (B): before and after, presenter and subject",
                    )
                    .clicked()
                    && !(split && !stack)
                {
                    change = Some(json!({ "kind": "split", "stack": false }));
                }
                if ui
                    .selectable_label(framed, "Framed ▭")
                    .on_hover_text("The shots between a headline band and a caption band: a feed post (4:5 or 1:1)")
                    .clicked()
                    && !framed
                {
                    change = Some(json!({ "kind": "framed" }));
                }
            });
            if let Some(Layout::Split { share, .. }) = current {
                ui.horizontal(|ui| {
                    ui.weak("Region A");
                    for s in SPLIT_SHARES {
                        if ui.selectable_label(share == s, format!("{s}/{}", 100 - s)).clicked() && share != s {
                            change = Some(json!({ "kind": "split", "share": s }));
                        }
                    }
                });
                ui.weak(
                    "Each clip plays in region A or B (the Inspector's Region); the other region holds its last shot.",
                );
            }
            if let Some(Layout::Framed { top, bottom, headline, caption, theme }) = &current {
                // The words are edited in a draft and sent when the field is left or Enter is pressed.
                let draft = nav.state.post_draft.get_or_insert_with(|| [headline.clone(), caption.clone()]);
                let mut sent = None;
                for (i, (key, hint)) in [("headline", "Headline"), ("caption", "Caption")].into_iter().enumerate() {
                    let r = ui.add(egui::TextEdit::singleline(&mut draft[i]).hint_text(hint).desired_width(260.0));
                    let saved = if i == 0 { headline } else { caption };
                    if r.lost_focus() && draft[i] != *saved {
                        sent = Some(json!({ "kind": "framed", key: draft[i].clone() }));
                    }
                }
                if sent.is_some() {
                    change = sent;
                }
                ui.horizontal(|ui| {
                    let (mut t, mut b) = (*top, *bottom);
                    ui.weak("Bands");
                    let rt = ui.add(egui::DragValue::new(&mut t).range(0..=BAND_MAX).suffix(" % top"));
                    let rb = ui.add(egui::DragValue::new(&mut b).range(0..=BAND_MAX).suffix(" % bottom"));
                    let done = |r: &egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
                    if (done(&rt) || done(&rb)) && (t, b) != (*top, *bottom) {
                        change = Some(json!({ "kind": "framed", "top": t, "bottom": b }));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.weak("Colours");
                    for th in TextTheme::ALL {
                        if ui.selectable_label(*theme == th, th.name()).clicked() && *theme != th {
                            change = Some(json!({ "kind": "framed", "theme": th }));
                        }
                    }
                });
            }
        });
        ui.end_row();
    });
    if let Some(p) = change {
        nav.run("storyboard.layout", p);
        // The words are read back from the board next frame.
        nav.state.post_draft = None;
    }
}

/// The Inspector's Region row on a split stage (F5, L1): A or B for the selected clip, one
/// `storyboard.region` each. Nothing on a full stage or for a title card.
pub(crate) fn region_field(
    nav: &mut Navigator,
    ui: &mut Ui,
    board: &assetcraft_storyboard::Storyboard,
    clip: &assetcraft_storyboard::Clip,
) {
    use assetcraft_storyboard::{Layout, Region, ShotType};
    let Some(Layout::Split { stack, .. }) = board.layout else { return };
    if clip.shot_type == ShotType::Ttl {
        return;
    }
    ui.label(RichText::new("Region").size(10.0).color(ui.visuals().weak_text_color()));
    let now = clip.region();
    let mut pick = None;
    ui.horizontal(|ui| {
        for (r, name) in if stack {
            [(Region::A, "A · top"), (Region::B, "B · bottom")]
        } else {
            [(Region::A, "A · left"), (Region::B, "B · right")]
        } {
            if ui.selectable_label(now == r, name).clicked() && now != r {
                pick = Some(r);
            }
        }
    });
    if let Some(r) = pick {
        nav.run("storyboard.region", json!({ "clip": clip.id.0, "region": r.letter().to_ascii_lowercase() }));
    }
    ui.add_space(4.0);
}
