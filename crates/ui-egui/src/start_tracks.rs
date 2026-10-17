//! Start storyboard v2, column 1 · Pick a track (S2a; mock-up `docs/design/start-storyboard-v2.dc.html`).
//!
//! Draws only. Every number comes from the track's cue map through `storyboard.tracks`: tempo band,
//! a 28-bar energy graph of the sections' energy, the section strip, drop times and the styles whose
//! BPM range fits. Search, tempo filter and sort are view state. ▶ runs `track.preview` (from the
//! first drop); a click on a section of the selected track's strip previews from that section's
//! start (Paul, 2026-10-16: sample the chorus or the verse); the music folder row runs
//! `storyboard.musicFolder`.

use crate::Navigator;
use crate::storyboard_view::{ACCENT, SELECT};
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::{Value, json};

/// Selected row fill and the Analysis text colour (design tokens `surface-selected`, `text-ai`).
const ROW_SELECTED: Color32 = Color32::from_rgb(0x1f, 0x33, 0x50);
const TEXT_AI: Color32 = Color32::from_rgb(0x6f, 0xa3, 0xdc);
const BARS: Color32 = Color32::from_gray(0x5a);
const BARS_PREVIEW: Color32 = Color32::from_rgb(0x5b, 0x8f, 0xc9);

/// Tempo filter (design: chips with counts).
pub(crate) const BANDS: [(&str, &str); 5] =
    [("all", "All"), ("slow", "Slow <100"), ("mid", "Mid <118"), ("upbeat", "Upbeat <135"), ("fast", "Fast 135+")];

/// How the list is sorted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Sort {
    /// By title.
    #[default]
    Name,
    /// By tempo.
    Bpm,
    /// By length.
    Length,
}

/// Column 1 view state.
#[derive(Debug, Default)]
pub(crate) struct TrackFilter {
    pub(crate) query: String,
    pub(crate) band: Option<&'static str>,
    pub(crate) sort: Sort,
}

/// Section tint by label: Chorus (the drop) red, Build blue, the rest grey.
pub(crate) fn section_tint(label: &str) -> (Color32, Color32) {
    match label {
        "Chorus" | "Drop" => (Color32::from_rgba_unmultiplied(229, 36, 59, 56), Color32::from_rgb(0xff, 0x8a, 0x96)),
        "Build" => (Color32::from_rgba_unmultiplied(77, 163, 255, 41), Color32::from_rgb(0xa8, 0xd0, 0xff)),
        _ => (Color32::from_rgba_unmultiplied(255, 255, 255, 13), Color32::from_gray(0x9a)),
    }
}

pub(crate) fn mmss(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The analysed tracks after search, tempo filter and sort; and the not-analysed ones.
pub(crate) fn visible(all: &[Value], f: &TrackFilter) -> (Vec<Value>, Vec<Value>) {
    let q = f.query.trim().to_lowercase();
    let (mut analysed, pending): (Vec<Value>, Vec<Value>) = all
        .iter()
        .filter(|t| q.is_empty() || t["title"].as_str().unwrap_or("").to_lowercase().contains(&q))
        .cloned()
        .partition(|t| t["analysed"] == true);
    analysed.retain(|t| f.band.is_none_or(|b| t["band"].as_str() == Some(b)));
    let num = |t: &Value, k: &str| t[k].as_f64().unwrap_or(0.0);
    match f.sort {
        Sort::Name => analysed.sort_by_key(|t| t["title"].as_str().unwrap_or("").to_lowercase()),
        Sort::Bpm => analysed.sort_by(|a, b| num(a, "bpm").total_cmp(&num(b, "bpm"))),
        Sort::Length => analysed.sort_by(|a, b| num(a, "seconds").total_cmp(&num(b, "seconds"))),
    }
    (analysed, pending)
}

/// What the column asks the Start screen to do.
pub(crate) enum Pick {
    /// Select this track (its cue map).
    Track(String),
}

/// Draw column 1. `chosen` is the selected track's cue map.
pub(crate) fn column(nav: &mut Navigator, ui: &mut Ui, chosen: Option<&str>) -> Option<Pick> {
    let all = nav.state.storyboard.start.tracks_all();
    let mut pick = None;
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Start storyboard").size(16.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            crate::storyboard_view::section(ui, "1 · PICK A TRACK");
        });
    });
    ui.add_space(4.0);
    folder_row(nav, ui);
    ui.add_space(4.0);
    let filter = &mut nav.state.storyboard.start.filter;
    ui.add(
        egui::TextEdit::singleline(&mut filter.query)
            .id(egui::Id::new("sb-start-search"))
            .hint_text("Search tracks")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (id, label) in BANDS {
            let n =
                all.iter().filter(|t| t["analysed"] == true && (id == "all" || t["band"].as_str() == Some(id))).count();
            let on = filter.band.unwrap_or("all") == id;
            let text = RichText::new(format!("{label}  {n}")).small();
            if ui.selectable_label(on, text).clicked() {
                filter.band = if id == "all" { None } else { Some(id) };
            }
        }
    });
    let (list, pending) = visible(&all, filter);
    ui.horizontal(|ui| {
        crate::storyboard_view::section(ui, "SORT");
        ui.spacing_mut().item_spacing.x = 0.0;
        for (s, label) in [(Sort::Name, "Name"), (Sort::Bpm, "BPM"), (Sort::Length, "Length")] {
            let on = filter.sort == s;
            let text =
                if on { RichText::new(label).color(Color32::WHITE).small() } else { RichText::new(label).small() };
            let fill = if on { ACCENT } else { ui.visuals().widgets.inactive.bg_fill };
            if ui.add(egui::Button::new(text).fill(fill)).clicked() {
                filter.sort = s;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("{} shown", list.len())).monospace().small().weak());
        });
    });
    ui.separator();
    let previewing = nav.session.preview_state().map(|(p, loading)| (p.to_string_lossy().into_owned(), loading));
    let footer_h = 28.0;
    let list_h = (ui.available_height() - footer_h).max(80.0);
    let mut preview: Option<Value> = None;
    egui::ScrollArea::vertical().id_salt("sb-start-tracks").max_height(list_h).auto_shrink([false, false]).show(
        ui,
        |ui| {
            for t in &list {
                let cue = t["cueMap"].as_str().unwrap_or("").to_owned();
                let selected = chosen == Some(cue.as_str());
                let audio = t["audio"].as_str().unwrap_or("").to_owned();
                let state = previewing.as_ref().filter(|(p, _)| *p == audio).map(|(_, loading)| *loading);
                let at = nav.session.preview_heard(&audio);
                let (clicked, play, seek) = row(ui, t, selected, state, at);
                if let Some(from) = seek {
                    preview = Some(json!({ "path": audio, "from": from }));
                } else if play {
                    preview = Some(match state {
                        Some(_) => json!({ "path": null }),
                        None => json!({ "path": audio, "from": t["drops"][0].as_f64().unwrap_or(0.0) }),
                    });
                } else if clicked && !selected {
                    pick = Some(Pick::Track(cue));
                }
            }
            for t in &pending {
                ui.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.label(RichText::new(t["title"].as_str().unwrap_or("?")).color(Color32::from_gray(0x6a)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("not analysed").monospace().small().color(Color32::from_gray(0x6a)))
                            .on_hover_text(
                                "No cue map beside this file yet; the built-in analyser (A2–A4) will add one",
                            );
                    });
                });
            }
        },
    );
    if let Some(p) = preview {
        nav.run("track.preview", p);
    }
    // Loading, or the playhead moving across the selected track's sections.
    if previewing.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
    let analysed = all.iter().filter(|t| t["analysed"] == true).count();
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{analysed} tracks · {} not analysed", all.len() - analysed))
                .monospace()
                .small()
                .weak(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Preview volume (Paul, 2026-10-16: a quick sample at a low volume). Saved; the Cut stays at full level.
            let mut v = nav.session.preview_volume();
            ui.label(RichText::new(format!("{:>3.0}%", v * 100.0)).monospace().small());
            let slider = egui::Slider::new(&mut v, 0.0..=1.0).show_value(false);
            let r = ui
                .add_sized(vec2(90.0, 16.0), slider)
                .on_hover_text("Preview volume (Watch your Cut plays at full level)");
            if r.changed() {
                nav.run("track.previewVolume", json!({ "volume": v }));
            }
            ui.label(RichText::new("Preview").small().weak());
        });
    });
    pick
}

/// The section under `x` on a strip: `(label, start seconds)`.
pub(crate) fn section_at(sections: &[Value], total: f64, strip: Rect, x: f32) -> Option<(String, f64)> {
    if strip.width() <= 0.0 || total <= 0.0 {
        return None;
    }
    let secs = f64::from((x - strip.left()) / strip.width()) * total;
    sections
        .iter()
        .find(|s| s["start"].as_f64().unwrap_or(0.0) <= secs && secs < s["end"].as_f64().unwrap_or(0.0))
        .map(|s| (s["label"].as_str().unwrap_or("").to_owned(), s["start"].as_f64().unwrap_or(0.0)))
}

/// One track row (design: two lines, the selected one expanded). `at` is where a playing preview
/// of this track is. Returns (row clicked, ▶ clicked, preview from these seconds).
fn row(ui: &mut Ui, t: &Value, selected: bool, preview: Option<bool>, at: Option<f64>) -> (bool, bool, Option<f64>) {
    let width = ui.available_width();
    // The expanded row grows with its Analysis line.
    let analysis = format!("Analysis: {}", t["analysis"].as_str().unwrap_or(""));
    let analysis_galley =
        selected.then(|| ui.painter().layout(analysis, FontId::proportional(11.0), TEXT_AI, (width - 50.0).max(40.0)));
    let height = match &analysis_galley {
        Some(g) => 74.0 + g.size().y + 18.0,
        None => 46.0,
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0, ROW_SELECTED);
    } else if response.hovered() {
        painter.rect_filled(rect, 0, Color32::from_white_alpha(6));
    }
    painter.line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, Color32::from_gray(0x2a)));
    let x0 = rect.left() + 10.0;
    let y1 = rect.top() + 14.0;
    // ▶ preview.
    let play_rect = Rect::from_center_size(pos2(x0 + 10.0, y1), vec2(20.0, 20.0));
    let play = ui.interact(play_rect, ui.id().with(("sb-preview", t["audio"].as_str().unwrap_or(""))), Sense::click());
    let (glyph, fill, ink) = match preview {
        Some(true) => ("…", ACCENT, Color32::WHITE),
        Some(false) => ("⏸", ACCENT, Color32::WHITE),
        None => ("▶", Color32::from_gray(0x33), Color32::from_gray(0xaa)),
    };
    painter.circle_filled(play_rect.center(), 10.0, fill);
    painter.circle_stroke(play_rect.center(), 10.0, Stroke::new(1.0, Color32::from_gray(0x4a)));
    painter.text(play_rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(9.0), ink);
    let tip = if preview.is_some() { "Stop the preview" } else { "Preview from the first drop" };
    let play = play.on_hover_text(tip);
    // Title, length, BPM.
    let ink = if selected { Color32::WHITE } else { Color32::from_gray(0xdd) };
    let title = t["title"].as_str().unwrap_or("?");
    let right = rect.right() - 12.0;
    let bpm = format!("{:.1} BPM", t["bpm"].as_f64().unwrap_or(0.0));
    painter.text(pos2(right, y1), Align2::RIGHT_CENTER, &bpm, FontId::monospace(11.0), Color32::from_gray(0xcc));
    painter.text(
        pos2(right - 76.0, y1),
        Align2::RIGHT_CENTER,
        mmss(t["seconds"].as_f64().unwrap_or(0.0)),
        FontId::monospace(11.0),
        Color32::from_gray(0x8a),
    );
    let title_clip =
        painter.with_clip_rect(Rect::from_min_max(pos2(x0 + 28.0, rect.top()), pos2(right - 120.0, rect.top() + 28.0)));
    title_clip.text(pos2(x0 + 28.0, y1), Align2::LEFT_CENTER, title, FontId::proportional(13.0), ink);
    // Energy graph, band, suits.
    let y2 = rect.top() + 34.0;
    let graph = Rect::from_min_size(pos2(x0 + 28.0, y2 - 8.0), vec2(96.0, 16.0));
    let bars = t["energy"].as_array().cloned().unwrap_or_default();
    let n = bars.len().max(1) as f32;
    let bar_color = if selected {
        SELECT
    } else if preview.is_some() {
        BARS_PREVIEW
    } else {
        BARS
    };
    for (i, e) in bars.iter().enumerate() {
        let h = (0.15 + 0.85 * e.as_f64().unwrap_or(0.0) as f32) * graph.height();
        let w = graph.width() / n;
        let r = Rect::from_min_max(
            pos2(graph.left() + i as f32 * w, graph.bottom() - h),
            pos2(graph.left() + (i as f32 + 1.0) * w - 1.0, graph.bottom()),
        );
        painter.rect_filled(r, 0, bar_color);
    }
    let band = t["band"].as_str().unwrap_or("").to_uppercase();
    painter.text(
        pos2(graph.right() + 8.0, y2),
        Align2::LEFT_CENTER,
        &band,
        FontId::monospace(10.0),
        Color32::from_gray(0xaa),
    );
    let suits: Vec<&str> =
        t["suits"].as_array().map(|a| a.iter().filter_map(Value::as_str).take(2).collect()).unwrap_or_default();
    if !suits.is_empty() {
        let clip = painter
            .with_clip_rect(Rect::from_min_max(pos2(graph.right() + 64.0, rect.top()), pos2(right, rect.bottom())));
        clip.text(
            pos2(graph.right() + 64.0, y2),
            Align2::LEFT_CENTER,
            format!("Suits {}", suits.join(", ")),
            FontId::proportional(11.0),
            Color32::from_gray(0x8a),
        );
    }
    // Expanded: section strip (click a section to hear it), Analysis, artist · licence.
    let mut seek = None;
    if selected {
        let strip = Rect::from_min_max(pos2(x0 + 28.0, rect.top() + 50.0), pos2(right, rect.top() + 66.0));
        let total = t["seconds"].as_f64().unwrap_or(1.0).max(0.1);
        let sections = t["sections"].as_array().cloned().unwrap_or_default();
        let hit = ui.interact(strip, ui.id().with(("sb-sections", t["audio"].as_str().unwrap_or(""))), Sense::click());
        if let Some(pos) = hit.hover_pos()
            && let Some((label, start)) = section_at(&sections, total, strip, pos.x)
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            let hit = hit.clone().on_hover_text(format!("Preview from the {label} ({})", mmss(start)));
            if hit.clicked() {
                seek = Some(start);
            }
        }
        for s in &sections {
            let a = s["start"].as_f64().unwrap_or(0.0) / total;
            let b = s["end"].as_f64().unwrap_or(0.0) / total;
            let r = Rect::from_min_max(
                pos2(strip.left() + a as f32 * strip.width(), strip.top()),
                pos2((strip.left() + b as f32 * strip.width() - 1.0).max(strip.left()), strip.bottom()),
            );
            let label = s["label"].as_str().unwrap_or("");
            let (bg, fg) = section_tint(label);
            painter.rect_filled(r, 1, bg);
            if (b - a) > 0.1 {
                let clip = painter.with_clip_rect(r);
                clip.text(
                    r.left_center() + vec2(4.0, 0.0),
                    Align2::LEFT_CENTER,
                    label.to_uppercase(),
                    FontId::proportional(9.0),
                    fg,
                );
            }
        }
        if let Some(at) = at {
            let x = strip.left() + (at / total).clamp(0.0, 1.0) as f32 * strip.width();
            painter.line_segment(
                [pos2(x, strip.top() - 2.0), pos2(x, strip.bottom() + 2.0)],
                Stroke::new(2.0, Color32::WHITE),
            );
        }
        let mut g_h = 0.0;
        if let Some(g) = analysis_galley {
            g_h = g.size().y;
            painter.galley(pos2(x0 + 28.0, rect.top() + 70.0), g, TEXT_AI);
        }
        let credit = format!("{} · {}", t["artist"].as_str().unwrap_or(""), t["license"].as_str().unwrap_or(""));
        painter.text(
            pos2(x0 + 28.0, rect.top() + 72.0 + g_h),
            Align2::LEFT_TOP,
            credit,
            FontId::monospace(10.0),
            Color32::from_gray(0x7a),
        );
    }
    if selected {
        painter.rect_stroke(
            rect,
            0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(77, 163, 255, 60)),
            StrokeKind::Inside,
        );
    }
    // A click on the strip is a seek, not a row click.
    (response.clicked() && seek.is_none(), play.clicked(), seek)
}

/// Music folder row: path field (Enter sets), Browse…, Set, ↻.
fn folder_row(nav: &mut Navigator, ui: &mut Ui) {
    let mut set: Option<Value> = None;
    ui.horizontal(|ui| {
        let start = &mut nav.state.storyboard.start;
        let field = ui.add(
            egui::TextEdit::singleline(start.folder_text_mut())
                .id(crate::start_view::folder_field_id())
                .desired_width((ui.available_width() - 150.0).max(80.0))
                .hint_text("Music folder: type, paste or Browse…")
                .font(egui::TextStyle::Monospace),
        );
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.small_button("Browse…").on_hover_text("Pick the music folder").clicked() {
            let mut dialog = rfd::FileDialog::new().set_title("Music folder");
            if let Some(current) = nav.session.music_folder() {
                dialog = dialog.set_directory(current);
            }
            if let Some(dir) = dialog.pick_folder() {
                let d = dir.to_string_lossy().into_owned();
                *nav.state.storyboard.start.folder_text_mut() = d.clone();
                set = Some(json!({ "path": d }));
            }
        }
        if ui.small_button("Set").clicked() || entered {
            let t = nav.state.storyboard.start.folder_text_mut().trim().to_owned();
            set = Some(if t.is_empty() { json!({ "path": null }) } else { json!({ "path": t }) });
        }
        if ui.small_button("↻").on_hover_text("Look for tracks again").clicked() {
            nav.state.storyboard.start.library_refresh();
        }
    });
    if let Some(params) = set
        && nav.run("storyboard.musicFolder", params).is_some()
    {
        nav.state.storyboard.start.library_refresh();
        nav.state.storyboard.start.clear_track();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(title: &str, bpm: f64, secs: f64, band: &str) -> Value {
        json!({ "title": title, "bpm": bpm, "seconds": secs, "band": band, "analysed": true })
    }

    #[test]
    fn search_filter_and_sort() {
        let all = vec![
            t("Vanguard", 136.0, 234.0, "fast"),
            t("Amberlight", 129.2, 282.0, "upbeat"),
            t("Titan", 103.4, 134.0, "mid"),
            json!({ "title": "loose", "analysed": false }),
        ];
        let names = |v: &[Value]| v.iter().map(|x| x["title"].as_str().unwrap_or("").to_owned()).collect::<Vec<_>>();
        let mut f = TrackFilter::default();
        let (list, pending) = visible(&all, &f);
        assert_eq!(names(&list), ["Amberlight", "Titan", "Vanguard"]);
        assert_eq!(pending.len(), 1);
        f.sort = Sort::Bpm;
        assert_eq!(names(&visible(&all, &f).0), ["Titan", "Amberlight", "Vanguard"]);
        f.sort = Sort::Length;
        assert_eq!(names(&visible(&all, &f).0), ["Titan", "Vanguard", "Amberlight"]);
        f.band = Some("fast");
        assert_eq!(names(&visible(&all, &f).0), ["Vanguard"]);
        f.band = None;
        f.query = "TI".into();
        assert_eq!(names(&visible(&all, &f).0), ["Titan"]);
    }
}
