//! Start storyboard (design v2, `docs/design/start-storyboard-v2.dc.html`): 1 · track (left), 2 ·
//! preset (centre: six style cards with their tempo range and a ◆ when the track fits, the shape
//! controls, then a card per length with its clip-structure strip), 3 · create (right).
//!
//! Draws only. The music list is `storyboard.tracks` (the music folder plus the folder open in
//! the Organizer; `storyboard.musicFolder` sets the folder), each card's strip is
//! `storyboard.preview`, and Create runs `storyboard.generate` or `storyboard.new {template}` (then `storyboard.autofill` when
//! asked) or `storyboard.new`. Shown whenever Storyboard mode has no board, and from the mode
//! bar's **＋ New**.
//!
//! Until the 20 music-timed preset files land (track analysis A7–A8), a preset is a pacing
//! profile at one of the lengths the track fits.

use crate::Navigator;
use crate::storyboard_view::{ACCENT, INSPECTOR_W, SELECT, rgb, section};
use assetcraft_engine::PreviewMode;
use assetcraft_storyboard::{BUNDLED, Pacing, STEPS_RANGE, SceneTransition, Shape, ShotType, Titles};
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Music list width.
const MUSIC_W: f32 = 340.0;
/// Length card size.
const CARD: egui::Vec2 = vec2(300.0, 104.0);
/// Preset card height.
const PRESET_H: f32 = 58.0;
/// Cut pace steps, long holds to rapid (mock-up `PACE` 2, 1.4, 1, 0.7, 0.5 × shot length): the
/// generator's `density`.
pub(crate) const PACE_DENSITY: [f32; 5] = [0.5, 0.71, 1.0, 1.43, 2.0];
/// The preset as designed.
const PACE_PRESET: usize = 2;

/// What the Start screen will build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Style {
    /// Generated from a track at this pacing.
    Pacing(Pacing),
    /// A bundled template (`assetcraft_storyboard::BUNDLED` index): its skeleton, with the
    /// song as the track when one is picked (ST3b).
    Template(usize),
    /// The 24-clip handoff sample.
    Sample,
    /// One title card, no track.
    Blank,
}

/// Start screen state kept by the window (not application state).
#[derive(Debug)]
pub(crate) struct StartUi {
    /// Shown over an open storyboard (the mode bar's ＋ New).
    pub(crate) open: bool,
    style: Style,
    /// The chosen track's cue map.
    pub(crate) track: Option<String>,
    /// Chosen length (`None` = the whole track).
    length: Option<u32>,
    pub(crate) autofill: bool,
    name: String,
    folder_text: String,
    /// `storyboard.tracks`, fetched when the screen first shows and after a folder change.
    library: Option<Value>,
    /// Cut pace step (index into [`PACE_DENSITY`]).
    pace: usize,
    /// Shape overrides on top of the style (cleared when a preset is picked).
    shape: Shape,
    /// `storyboard.preview` results by `cue|pacing|seconds|density|shape`.
    pub(crate) previews: HashMap<String, Option<Value>>,
    /// The timeline's waveform: `(audio path, track.peaks answer)` once ready or failed.
    pub(crate) waveform: Option<(String, Value)>,
    /// A scrub drag held on the timeline: where, in cut seconds.
    pub(crate) scrub: Option<f64>,
    /// The timeline's paused playhead, in cut seconds.
    pub(crate) cursor: f64,
    /// Track column search, tempo filter and sort.
    pub(crate) filter: crate::start_tracks::TrackFilter,
    /// Ask "Analyse all N audio files in this folder?" (set when a music folder is picked).
    pub(crate) ask_analyse: bool,
    /// The analyser's `(done + failed, batches)` last drawn, to refresh the list and notify once.
    pub(crate) seen_analyse: (usize, u64),
    /// How many of the chosen template's repeating section (points, steps); `None` = its default.
    pub(crate) steps: Option<usize>,
    /// How long each of them runs, title card included (SR2; `STEP_SECS` by default).
    pub(crate) step_secs: u32,
}

impl Default for StartUi {
    fn default() -> StartUi {
        StartUi {
            open: false,
            style: Style::Pacing(Pacing::Trailer),
            track: None,
            length: Some(60),
            autofill: true,
            name: String::new(),
            folder_text: String::new(),
            library: None,
            pace: PACE_PRESET,
            step_secs: assetcraft_storyboard::STEP_SECS as u32,
            shape: Shape::default(),
            previews: HashMap::new(),
            waveform: None,
            scrub: None,
            cursor: 0.0,
            filter: crate::start_tracks::TrackFilter::default(),
            ask_analyse: false,
            seen_analyse: (0, 0),
            steps: None,
        }
    }
}

/// The music folder field's id (tests focus it).
pub(crate) fn folder_field_id() -> egui::Id {
    egui::Id::new("sb-start-music-folder")
}

impl StartUi {
    /// Every track `storyboard.tracks` listed.
    pub(crate) fn tracks_all(&self) -> Vec<Value> {
        self.library.as_ref().and_then(|l| l["tracks"].as_array().cloned()).unwrap_or_default()
    }

    /// The music folder field's text.
    pub(crate) fn folder_text_mut(&mut self) -> &mut String {
        &mut self.folder_text
    }

    /// Forget the chosen track (the music folder changed).
    pub(crate) fn clear_track(&mut self) {
        self.track = None;
    }

    /// What's typed in the music folder field (tests).
    #[cfg(test)]
    pub(crate) fn folder_text(&self) -> &str {
        &self.folder_text
    }

    /// Choose a track (its cue map), a preset, a cut pace step and shape overrides (tests).
    #[cfg(test)]
    pub(crate) fn choose(&mut self, cue_map: &str, pacing: Pacing, seconds: Option<u32>, pace: usize, shape: Shape) {
        self.pace = pace.min(PACE_DENSITY.len() - 1);
        self.shape = shape;
        self.track = Some(cue_map.to_owned());
        self.style = Style::Pacing(pacing);
        self.length = seconds;
    }

    /// Choose a bundled template, its count and a track (tests).
    #[cfg(test)]
    pub(crate) fn choose_template(
        &mut self,
        index: usize,
        steps: Option<usize>,
        cue_map: Option<&str>,
        step_secs: u32,
    ) {
        self.style = Style::Template(index);
        self.steps = steps;
        self.track = cue_map.map(str::to_owned);
        self.step_secs = step_secs;
    }

    /// The generator's `density` for the cut pace step.
    fn density(&self) -> f32 {
        PACE_DENSITY.get(self.pace).copied().unwrap_or(1.0)
    }

    /// Pick a preset: its own shape and pace come back (the mock-up's `applyPreset`).
    fn apply_preset(&mut self, pacing: Pacing) {
        self.style = Style::Pacing(pacing);
        self.shape = Shape::default();
        self.pace = PACE_PRESET;
    }

    /// Look for tracks again next time the screen shows (the open folder may have changed).
    pub(crate) fn library_refresh(&mut self) {
        self.library = None;
    }

    /// Whether the track list has been read since the last refresh.
    pub(crate) fn library_loaded(&self) -> bool {
        self.library.is_some()
    }
}

fn blurb(style: Style) -> &'static str {
    match style {
        Style::Pacing(p) => p.description(),
        Style::Template(i) => BUNDLED.get(i).map_or("", |t| t.description),
        Style::Sample => "The 24-clip Hollow Saint teaser from the design",
        Style::Blank => "One title card, no track; build it by hand",
    }
}

fn style_name(style: Style) -> &'static str {
    match style {
        Style::Pacing(p) => p.name(),
        Style::Template(i) => BUNDLED.get(i).map_or("Template", |t| t.name),
        Style::Sample => "Sample",
        Style::Blank => "Blank",
    }
}

fn length_label(secs: Option<u32>, track_secs: f64) -> String {
    match secs {
        None => format!("Full length · {}:{:02}", (track_secs / 60.0) as u32, (track_secs % 60.0) as u32),
        Some(s) => format!("{}:{:02}", s / 60, s % 60),
    }
}

pub(crate) fn fill_for(code: &str) -> Color32 {
    ShotType::ALL
        .into_iter()
        .find(|t| t.code() == code)
        .and_then(ShotType::fill_rgb)
        .map(rgb)
        .unwrap_or(Color32::from_gray(0x6b))
}

/// The Start screen.
pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    if nav.state.storyboard.start.library.is_none() {
        let lib = nav.run("storyboard.tracks", json!({}));
        let start = &mut nav.state.storyboard.start;
        start.library = Some(lib.unwrap_or(Value::Null));
        if start.track.is_none() {
            start.track = first_analysed(start.library.as_ref());
        }
        if start.folder_text.is_empty() {
            start.folder_text = nav.session.music_folder().map(|p| p.display().to_string()).unwrap_or_default();
        }
    }
    // The song makes the cuts, so it comes first: 1 · song (left), 2 · preset (centre), Create (right).
    // Keep music (a preview, or a Cut playing behind the screen) in step.
    let _ = crate::strip_view::playhead(nav, ui.ctx());
    // The track makes the cuts, so it comes first: 1 · track (left), 2 · preset (centre), 3 · create (right).
    // Full width under the three columns: the storyboard over the whole track (S2d).
    egui::Panel::bottom("sb-start-timeline")
        .exact_size(crate::start_timeline::TIMELINE_H)
        .resizable(false)
        .show(ui, |ui| timeline(nav, ui));
    egui::Panel::left("sb-start-music").exact_size(MUSIC_W).resizable(false).show(ui, |ui| {
        let chosen = nav.state.storyboard.start.track.clone();
        if let Some(crate::start_tracks::Pick::Track(c)) = crate::start_tracks::column(nav, ui, chosen.as_deref()) {
            pick_track(nav, c);
        }
    });
    egui::Panel::right("sb-start-summary").exact_size(INSPECTOR_W).resizable(false).show(ui, |ui| summary(nav, ui));
    egui::CentralPanel::default().show(ui, |ui| {
        styles(nav, ui);
        cards(nav, ui);
    });
}

/// Whether a Cut preview of `audio` is playing or loading.
fn cut_playing(nav: &Navigator, audio: &str) -> bool {
    nav.session.preview_mode() == Some(PreviewMode::Cut)
        && nav.session.preview_state().is_some_and(|(p, _)| p.to_string_lossy() == audio)
}

/// Play the selected track's cut from `from` cut seconds (`track.preview {cut}`), and rest the
/// playhead there. Stops a Song preview (one preview at a time).
pub(crate) fn play_cut(nav: &mut Navigator, from: f64) {
    let Some(track) = selected_track(nav) else { return };
    let (Some(audio), Some(cue)) = (track["audio"].as_str(), track["cueMap"].as_str()) else { return };
    let start = &mut nav.state.storyboard.start;
    start.cursor = from.max(0.0);
    let cut = json!({ "cueMap": cue, "seconds": start.length });
    nav.run("track.preview", json!({ "path": audio, "from": from.max(0.0), "cut": cut }));
}

/// ▶ / ⏸ on the timeline, and Space on the Start screen: play the selected track's cut from the
/// playhead, or pause it there.
pub(crate) fn toggle_play(nav: &mut Navigator) {
    let Some(audio) = selected_track(nav).and_then(|t| t["audio"].as_str().map(str::to_owned)) else { return };
    if cut_playing(nav, &audio) {
        if let Some(at) = nav.session.preview_heard(&audio, PreviewMode::Cut) {
            nav.state.storyboard.start.cursor = at;
        }
        nav.run("track.preview", json!({ "path": null }));
        return;
    }
    // From the end: start over.
    let end = nav.state.storyboard.start.length.map(f64::from);
    let from = nav.state.storyboard.start.cursor;
    play_cut(nav, if end.is_some_and(|e| from >= e - 0.1) { 0.0 } else { from });
}

/// Choose a length. The timeline shows a new cut, so its playhead goes back to 0:00 and a Cut
/// preview of the old length stops.
pub(crate) fn set_length(nav: &mut Navigator, seconds: Option<u32>) {
    if nav.session.preview_mode() == Some(PreviewMode::Cut) {
        nav.run("track.preview", json!({ "path": null }));
    }
    let start = &mut nav.state.storyboard.start;
    start.length = seconds;
    start.cursor = 0.0;
    start.scrub = None;
}

/// Choose a step length for a template (SR2). The timeline shows a new cut, so the playhead
/// goes back and a Cut preview stops, as [`set_length`].
pub(crate) fn set_step_secs(nav: &mut Navigator, secs: u32) {
    if nav.session.preview_mode() == Some(PreviewMode::Cut) {
        nav.run("track.preview", json!({ "path": null }));
    }
    let start = &mut nav.state.storyboard.start;
    start.step_secs = secs;
    start.cursor = 0.0;
    start.scrub = None;
}

fn mmss_f(secs: f64) -> String {
    format!("{}:{:02}", (secs / 60.0) as u32, (secs % 60.0).round() as u32)
}

/// Choose the track whose cue map is `cue`. A preview of another song stops (Paul, 2026-10-17:
/// "when the user clicks another song we need to stop the preview of the previous song"); a held
/// scrub ends.
pub(crate) fn pick_track(nav: &mut Navigator, cue: String) {
    let audio = tracks(nav).into_iter().find(|t| t["cueMap"].as_str() == Some(cue.as_str()));
    let audio = audio.and_then(|t| t["audio"].as_str().map(str::to_owned)).unwrap_or_default();
    if nav.session.preview_state().is_some_and(|(p, _)| p.to_string_lossy() != audio) {
        nav.run("track.preview", json!({ "path": null }));
    }
    let start = &mut nav.state.storyboard.start;
    if start.track.as_deref() != Some(cue.as_str()) {
        // A newly picked track's timeline starts at 0:00 (Paul, 2026-10-17).
        start.cursor = 0.0;
    }
    start.scrub = None;
    start.track = Some(cue);
    if !matches!(start.style, Style::Pacing(_) | Style::Template(_)) {
        start.apply_preset(Pacing::Trailer);
    }
}

/// The timeline under the columns: the chosen track and what Create would build from it.
fn timeline(nav: &mut Navigator, ui: &mut Ui) {
    let Some(track) = selected_track(nav) else {
        crate::start_timeline::show(ui, None);
        return;
    };
    let audio = track["audio"].as_str().unwrap_or_default().to_owned();
    let preview = match (nav.state.storyboard.start.style, track["cueMap"].as_str()) {
        (Style::Pacing(p), Some(cue)) => {
            let secs = nav.state.storyboard.start.length;
            preview(nav, cue, p, secs)
        }
        (Style::Template(i), Some(cue)) => template_preview(nav, cue, i),
        _ => None,
    };
    let peaks = crate::start_timeline::peaks(nav, &audio);
    if peaks.is_none() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
    }
    let heard = nav.session.preview_heard(&audio, PreviewMode::Cut);
    let playing = cut_playing(nav, &audio);
    let start = &nav.state.storyboard.start;
    let lanes = crate::start_timeline::Lanes {
        track: &track,
        preview: preview.as_ref(),
        peaks: peaks.as_ref(),
        heard,
        scrub: start.scrub,
        cursor: start.cursor,
        playing,
    };
    match crate::start_timeline::show(ui, Some(lanes)) {
        Some(crate::start_timeline::Action::Toggle) => toggle_play(nav),
        Some(crate::start_timeline::Action::Scrub { cut, how }) => {
            use crate::timebar::Scrub;
            match how {
                // A drag holds the playhead (silent) and the cut plays from where it is let go.
                Scrub::Start => {
                    nav.state.storyboard.start.scrub = Some(cut);
                    if nav.session.preview_state().is_some() {
                        nav.run("track.preview", json!({ "path": null }));
                    }
                }
                Scrub::Move => nav.state.storyboard.start.scrub = Some(cut),
                Scrub::Click | Scrub::Stop => {
                    nav.state.storyboard.start.scrub = None;
                    play_cut(nav, cut);
                }
            }
        }
        None => {}
    }
    if heard.is_some() || playing {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
}

fn first_analysed(lib: Option<&Value>) -> Option<String> {
    lib?["tracks"].as_array()?.iter().find(|t| t["analysed"] == true)?["cueMap"].as_str().map(str::to_owned)
}

fn tracks(nav: &Navigator) -> Vec<Value> {
    nav.state.storyboard.start.library.as_ref().and_then(|l| l["tracks"].as_array().cloned()).unwrap_or_default()
}

fn selected_track(nav: &Navigator) -> Option<Value> {
    let cue = nav.state.storyboard.start.track.as_deref()?;
    tracks(nav).into_iter().find(|t| t["cueMap"].as_str() == Some(cue))
}

fn styles(nav: &mut Navigator, ui: &mut Ui) {
    ui.add_space(8.0);
    let track = selected_track(nav);
    ui.horizontal(|ui| {
        section(ui, "2 · PICK A PRESET");
        if let Some(t) = &track {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} · {:.0} BPM",
                        t["title"].as_str().unwrap_or("Track"),
                        t["bpm"].as_f64().unwrap_or(0.0)
                    ))
                    .monospace()
                    .small()
                    .color(ui.visuals().weak_text_color()),
                );
            });
        }
    });
    preset_cards(nav, ui, track.as_ref().and_then(|t| t["bpm"].as_f64()));
    ui.add_space(4.0);
    template_cards(nav, ui);
    let style = nav.state.storyboard.start.style;
    ui.label(RichText::new(blurb(style)).small().color(ui.visuals().weak_text_color()));
    ui.add_space(6.0);
    if let Style::Pacing(pacing) = style {
        shape_controls(nav, ui, pacing, track.as_ref());
        ui.add_space(6.0);
        legend(ui);
        ui.add_space(8.0);
    }
    if let Style::Template(i) = style {
        steps_control(nav, ui, i, track.is_some());
        ui.add_space(8.0);
    }
}

/// The six style cards in one row: name, ◆ when the track's tempo fits, tempo range, fit label.
fn preset_cards(nav: &mut Navigator, ui: &mut Ui, bpm: Option<f64>) {
    let gap = 8.0;
    let n = Pacing::ALL.len() as f32;
    let w = ((ui.available_width() - gap * (n - 1.0)) / n).max(80.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for pacing in Pacing::ALL {
            let chosen = nav.state.storyboard.start.style == Style::Pacing(pacing);
            let fits = bpm.is_some_and(|b| pacing.suits(b));
            let (rect, response) = ui.allocate_exact_size(vec2(w, PRESET_H), Sense::click());
            card_frame(ui, rect, chosen);
            let p = ui.painter();
            let ink = if chosen { Color32::WHITE } else { Color32::from_gray(0xdd) };
            p.text(rect.min + vec2(10.0, 8.0), Align2::LEFT_TOP, pacing.name(), FontId::proportional(13.0), ink);
            if fits {
                p.text(rect.right_top() + vec2(-10.0, 8.0), Align2::RIGHT_TOP, "◆", FontId::proportional(11.0), SELECT);
            }
            let (lo, hi) = pacing.bpm_range();
            p.text(
                rect.min + vec2(10.0, 26.0),
                Align2::LEFT_TOP,
                format!("{lo:.0}–{hi:.0} BPM"),
                FontId::monospace(10.0),
                Color32::from_gray(0xaa),
            );
            if bpm.is_some() {
                let (label, color) =
                    if fits { ("Matches tempo", SELECT) } else { ("Off tempo", Color32::from_gray(0x77)) };
                p.text(rect.min + vec2(10.0, 40.0), Align2::LEFT_TOP, label, FontId::proportional(10.0), color);
            }
            if response.on_hover_text(pacing.description()).clicked() && !chosen {
                nav.state.storyboard.start.apply_preset(pacing);
            }
        }
    });
}

/// The template cards under the style cards (ST3b): Review · Tutorial · 5-minute instructions.
/// A template is a skeleton of scenes and shots; with a track picked, the song becomes its
/// track and the clips are re-cut to fit.
fn template_cards(nav: &mut Navigator, ui: &mut Ui) {
    let gap = 8.0;
    let n = BUNDLED.len() as f32;
    let w = ((ui.available_width() - gap * (n - 1.0)) / n).max(80.0);
    ui.label(RichText::new("TEMPLATES").small().color(ui.visuals().weak_text_color()));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (i, t) in BUNDLED.iter().enumerate() {
            let chosen = nav.state.storyboard.start.style == Style::Template(i);
            let (rect, response) = ui.allocate_exact_size(vec2(w, PRESET_H), Sense::click());
            card_frame(ui, rect, chosen);
            let p = ui.painter();
            let ink = if chosen { Color32::WHITE } else { Color32::from_gray(0xdd) };
            p.text(rect.min + vec2(10.0, 8.0), Align2::LEFT_TOP, t.name, FontId::proportional(13.0), ink);
            let steps = nav.state.storyboard.start.steps.filter(|_| chosen).unwrap_or(t.steps);
            p.text(
                rect.min + vec2(10.0, 26.0),
                Align2::LEFT_TOP,
                format!("{steps} {}", t.steps_name),
                FontId::monospace(10.0),
                Color32::from_gray(0xaa),
            );
            p.text(
                rect.min + vec2(10.0, 40.0),
                Align2::LEFT_TOP,
                "Skeleton · song optional",
                FontId::proportional(10.0),
                Color32::from_gray(0x77),
            );
            if response.on_hover_text(t.description).clicked() && !chosen {
                let start = &mut nav.state.storyboard.start;
                start.style = Style::Template(i);
                start.steps = None;
            }
        }
    });
}

/// Step lengths offered, in seconds (Paul, 2026-11-06: "a 'title' clip every 30 seconds").
pub(crate) const STEP_CHOICES: [u32; 4] = [20, 30, 45, 60];

/// The chosen template at its steps and step length: its clip count and its length in all.
fn template_length(index: usize, steps: Option<usize>, step_secs: u32, credits: bool) -> Option<(usize, f64)> {
    let t = BUNDLED.get(index)?;
    let mut board =
        assetcraft_storyboard::bundled_template_sized(t.id, steps.unwrap_or(t.steps), f64::from(step_secs))?;
    if credits {
        board.append_credits(&assetcraft_storyboard::credits_template());
    }
    Some((board.clips.len(), board.total().as_secs_f64()))
}

/// How many points or steps the chosen template gets (Paul, 2026-11-06: "an easy way to set
/// the 5 or 3").
fn steps_control(nav: &mut Navigator, ui: &mut Ui, index: usize, has_track: bool) {
    let Some(t) = BUNDLED.get(index) else { return };
    let mut steps = nav.state.storyboard.start.steps.unwrap_or(t.steps);
    ui.horizontal(|ui| {
        ui.label(format!("Number of {}", t.steps_name));
        if ui.add_enabled(steps > STEPS_RANGE.0, egui::Button::new("−").small()).clicked() {
            steps -= 1;
        }
        ui.add(egui::DragValue::new(&mut steps).range(STEPS_RANGE.0..=STEPS_RANGE.1).speed(0.1));
        if ui.add_enabled(steps < STEPS_RANGE.1, egui::Button::new("+").small()).clicked() {
            steps += 1;
        }
        ui.weak(if has_track {
            "a title card opens each; the shots sit on the song's beat"
        } else {
            "a title card opens each; add a song for the beat and the music"
        });
    });
    nav.state.storyboard.start.steps = Some(steps.clamp(STEPS_RANGE.0, STEPS_RANGE.1));
}

/// Cut pace, Transitions, Title cards and Shape (the mock-up's controls card). Changes are
/// overrides on top of the style; Reset to preset clears them.
fn shape_controls(nav: &mut Navigator, ui: &mut Ui, pacing: Pacing, track: Option<&Value>) {
    let readout = track.and_then(|t| {
        let cue = t["cueMap"].as_str()?.to_owned();
        let secs = nav.state.storyboard.start.length;
        preview(nav, &cue, pacing, secs)
    });
    let weak = ui.visuals().weak_text_color();
    let resolved = nav.state.storyboard.start.shape.resolve(pacing);
    egui::Frame::group(ui.style()).fill(Color32::from_gray(0x25)).inner_margin(12.0).show(ui, |ui| {
        ui.columns(2, |cols| {
            // Cut pace.
            let start = &mut nav.state.storyboard.start;
            cols[0].horizontal(|ui| {
                section_label(ui, "CUT PACE");
                if let Some(v) = &readout {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("avg {}s · {} beats", v["avgSeconds"], v["avgBeats"]))
                                .monospace()
                                .small(),
                        );
                    });
                }
            });
            cols[0].spacing_mut().slider_width = cols[0].available_width();
            let slider = egui::Slider::new(&mut start.pace, 0..=PACE_DENSITY.len() - 1).show_value(false);
            cols[0]
                .add_sized(vec2(cols[0].available_width(), 18.0), slider)
                .on_hover_text("Scales every shot's length (titles keep theirs); the cards update as you drag");
            cols[0].horizontal(|ui| {
                ui.label(RichText::new("Long holds").small().color(weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Rapid").small().color(weak));
                    ui.add_space((ui.available_width() / 2.0 - 20.0).max(0.0));
                    ui.label(RichText::new("Preset").small().color(weak));
                });
            });
            // Transitions.
            section_label(&mut cols[1], "TRANSITIONS");
            cols[1].horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for t in SceneTransition::ALL {
                    let on = resolved.transition == t;
                    let text = if on { RichText::new(t.name()).color(Color32::WHITE) } else { RichText::new(t.name()) };
                    let fill = if on { ACCENT } else { ui.visuals().widgets.inactive.bg_fill };
                    if ui.add(egui::Button::new(text).fill(fill)).clicked() {
                        start.shape.transition = Some(t);
                    }
                }
            });
            cols[1].add_space(4.0);
            // Title cards.
            section_label(&mut cols[0], "TITLE CARDS");
            cols[0].horizontal_wrapped(|ui| {
                for t in Titles::ALL {
                    if ui.selectable_label(resolved.titles == t, t.name()).clicked() {
                        start.shape.titles = Some(t);
                    }
                }
            });
            // Shape.
            cols[1].horizontal(|ui| {
                section_label(ui, "SHAPE");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let changed = !start.shape.is_default() || start.pace != PACE_PRESET;
                    if ui.add_enabled(changed, egui::Button::new("Reset to preset").small()).clicked() {
                        start.shape = Shape::default();
                        start.pace = PACE_PRESET;
                    }
                });
            });
            let toggle = |ui: &mut Ui, value: bool, label: &str, hint: &str| -> Option<bool> {
                let mut v = value;
                ui.checkbox(&mut v, label).on_hover_text(hint).changed().then_some(v)
            };
            if let Some(v) =
                toggle(&mut cols[1], resolved.tighten, "Tighten into drops", "Shots shorten toward each drop")
            {
                start.shape.tighten = Some(v);
            }
            if let Some(v) =
                toggle(&mut cols[1], resolved.end_card, "End title card", "A title card over the final bars")
            {
                start.shape.end_card = Some(v);
            }
            if let Some(v) = toggle(
                &mut cols[1],
                resolved.overlays,
                "Text overlays",
                "Scene titles as text over a shot instead of cards",
            ) {
                start.shape.overlays = Some(v);
            }
        });
    });
}

fn section_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(ui.visuals().weak_text_color()));
}

/// Key for the clip strips: the Shot map's shot-type colours.
fn legend(ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for t in ShotType::ALL {
            let (r, _) = ui.allocate_exact_size(vec2(14.0, 10.0), Sense::hover());
            match t.fill_rgb() {
                Some(c) => {
                    ui.painter().rect_filled(r, 2, rgb(c));
                    ui.painter().rect_stroke(r, 2, Stroke::new(1.0, Color32::from_gray(0x55)), StrokeKind::Inside);
                }
                None => crate::storyboard_view::dashed_rect(ui, r, Color32::from_gray(0x8a)),
            }
            ui.label(RichText::new(t.name()).small());
            ui.add_space(8.0);
        }
    });
}

/// Fetch (once) and return the preview for a card.
fn preview(nav: &mut Navigator, cue: &str, pacing: Pacing, secs: Option<u32>) -> Option<Value> {
    let start = &nav.state.storyboard.start;
    let (density, shape) = (start.density(), start.shape);
    let shape_json = serde_json::to_value(shape).unwrap_or(Value::Null);
    let key = format!("{cue}|{}|{secs:?}|{density:.2}|{shape_json}", pacing.id());
    if let Some(v) = nav.state.storyboard.start.previews.get(&key) {
        return v.clone();
    }
    let v = nav
        .session
        .execute(
            "storyboard.preview",
            &json!({ "cueMap": cue, "pacing": pacing.id(), "seconds": secs, "density": density, "shape": shape_json }),
        )
        .ok();
    nav.state.storyboard.start.previews.insert(key, v.clone());
    v
}

/// `storyboard.preview {template}` for the chosen template, steps, credits and length, cached
/// as [`preview`] is (Paul, 2026-11-06: "When I click on a template… I want to see the cuts in
/// the storyboard, and the inset section").
fn template_preview(nav: &mut Navigator, cue: &str, index: usize) -> Option<Value> {
    let t = BUNDLED.get(index)?;
    let steps = nav.state.storyboard.start.steps.unwrap_or(t.steps);
    let step_secs = nav.state.storyboard.start.step_secs;
    let credits = nav.session.credits_on();
    let key = format!("{cue}|template:{}|{steps}|{step_secs}|{credits}", t.id);
    if let Some(v) = nav.state.storyboard.start.previews.get(&key) {
        return v.clone();
    }
    let v = nav
        .session
        .execute(
            "storyboard.preview",
            &json!({ "cueMap": cue, "template": t.id, "steps": steps, "stepSeconds": step_secs, "credits": credits }),
        )
        .ok();
    nav.state.storyboard.start.previews.insert(key, v.clone());
    v
}

fn cards(nav: &mut Navigator, ui: &mut Ui) {
    let style = nav.state.storyboard.start.style;
    let Style::Pacing(pacing) = style else {
        let (title, meta) = match style {
            Style::Sample => ("Hollow Saint teaser", "24 clips · 5 scenes · stand-in 92 BPM track"),
            Style::Template(i) => (
                BUNDLED.get(i).map_or("Template", |t| t.name),
                if nav.state.storyboard.start.track.is_some() {
                    "the template's shots, re-cut to the song"
                } else {
                    "the template's shots and cards, no track yet"
                },
            ),
            _ => ("Blank storyboard", "1 title card · add a track later"),
        };
        if let Style::Template(i) = style {
            // A template's length comes from its steps, not the song (SR2, Paul: "I set 10
            // steps… a 'title' clip every 30 seconds… 12 title clips, and then the chorus
            // music between each title"): STEP LENGTH chips, and the total they make.
            let steps = nav.state.storyboard.start.steps;
            let credits = nav.session.credits_on();
            let chosen_secs = nav.state.storyboard.start.step_secs;
            ui.horizontal(|ui| {
                section_label(ui, "STEP LENGTH");
                ui.add_space(6.0);
                for secs in STEP_CHOICES {
                    let chosen = chosen_secs == secs;
                    let label =
                        if secs >= 60 { format!("{}:{:02}", secs / 60, secs % 60) } else { format!("{secs} s") };
                    if ui.selectable_label(chosen, label).clicked() && !chosen {
                        set_step_secs(nav, secs);
                    }
                }
                if let Some((clips, total)) = template_length(i, steps, chosen_secs, credits) {
                    let n = steps.unwrap_or(BUNDLED.get(i).map_or(0, |t| t.steps));
                    let what = BUNDLED.get(i).map_or("steps", |t| t.steps_name);
                    ui.weak(format!("{n} {what} · {clips} clips · {}", mmss_f(total)));
                }
            });
            // What the song does under it (SR3): "Music: the chorus looped".
            if let Some(cue) = nav.state.storyboard.start.track.clone()
                && let Some(p) = template_preview(nav, &cue, i)
                && let Some(c) = p["fitChanges"].as_str()
            {
                ui.label(RichText::new(c).small().color(ui.visuals().weak_text_color()));
            }
            ui.add_space(4.0);
        }
        let (rect, _) = ui.allocate_exact_size(CARD, Sense::hover());
        card_frame(ui, rect, true);
        ui.painter().text(
            rect.min + vec2(12.0, 12.0),
            Align2::LEFT_TOP,
            title,
            FontId::proportional(15.0),
            Color32::WHITE,
        );
        ui.painter().text(
            rect.min + vec2(12.0, 36.0),
            Align2::LEFT_TOP,
            meta,
            FontId::proportional(12.0),
            Color32::GRAY,
        );
        return;
    };
    let Some(track) = selected_track(nav) else {
        ui.add_space(20.0);
        ui.weak("Pick a track on the left: its beats and sections make the cuts and clips.");
        return;
    };
    let cue = track["cueMap"].as_str().unwrap_or_default().to_owned();
    let track_secs = track["seconds"].as_f64().unwrap_or(0.0);
    let lengths: Vec<Option<u32>> = track["lengths"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_u64().and_then(|n| u32::try_from(n).ok())).collect())
        .unwrap_or_else(|| vec![None]);
    // The chosen length must be one this track fits.
    if !lengths.contains(&nav.state.storyboard.start.length) {
        nav.state.storyboard.start.length =
            lengths.iter().rev().find(|l| l.is_some_and(|s| s <= 60)).copied().flatten();
    }
    // Lengths as chips (S2c, Paul 2026-10-16: the timeline below shows the structure full width).
    ui.horizontal(|ui| {
        section_label(ui, "LENGTH");
        ui.add_space(6.0);
        for secs in &lengths {
            let p = preview(nav, &cue, pacing, *secs);
            let chosen = nav.state.storyboard.start.length == *secs;
            let text = match &p {
                Some(v) => format!("{} · {} clips", length_label(*secs, track_secs), v["count"]),
                None => format!("{} · can't build", length_label(*secs, track_secs)),
            };
            if ui.selectable_label(chosen, text).clicked() && !chosen {
                set_length(nav, *secs);
            }
        }
    });
    let chosen = nav.state.storyboard.start.length;
    if let Some(v) = preview(nav, &cue, pacing, chosen) {
        ui.label(
            RichText::new(format!(
                "{} clips · avg {}s · {} scenes · {} titles · {}",
                v["count"],
                v["avgSeconds"],
                v["scenes"],
                v["titleCards"],
                v["length"].as_str().unwrap_or("")
            ))
            .small(),
        );
        if let Some(c) = v["fitChanges"].as_str() {
            ui.label(RichText::new(c).small().color(ui.visuals().weak_text_color()));
        }
    }
}

fn card_frame(ui: &Ui, rect: Rect, chosen: bool) {
    let p = ui.painter();
    p.rect_filled(rect, 6, Color32::from_gray(0x25));
    let stroke = if chosen { Stroke::new(2.0, SELECT) } else { Stroke::new(1.0, Color32::from_gray(0x3d)) };
    p.rect_stroke(rect, 6, stroke, StrokeKind::Inside);
}

fn summary(nav: &mut Navigator, ui: &mut Ui) {
    let style = nav.state.storyboard.start.style;
    ui.add_space(8.0);
    if nav.session.storyboard().is_some() && ui.button("Cancel").on_hover_text("Back to the open storyboard").clicked()
    {
        nav.state.storyboard.start.open = false;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        section(ui, "3 · CREATE");
        if let (Style::Pacing(p), Some(t)) = (style, selected_track(nav)) {
            let secs = t["seconds"].as_f64().unwrap_or(0.0);
            let len = length_label(nav.state.storyboard.start.length, secs);
            ui.label(RichText::new(t["title"].as_str().unwrap_or("")).strong());
            ui.weak(format!("{} · {len}", p.name()));
            ui.add_space(6.0);
        }
        section(ui, "NAME");
        ui.add(
            egui::TextEdit::singleline(&mut nav.state.storyboard.start.name)
                .id(egui::Id::new("sb-start-name"))
                .hint_text("Track · Style"),
        );
        ui.add_space(8.0);
        // Pacings and templates both come out as empty shots; the open folder fills them
        // (Paul, 2026-11-06: "when I pick a song, I still want the storyboard to fill").
        if matches!(style, Style::Pacing(_) | Style::Template(_)) {
            let files = nav.session.items().iter().filter(|e| e.kind == assetcraft_fs::EntryKind::File).count();
            ui.checkbox(
                &mut nav.state.storyboard.start.autofill,
                format!("Auto-fill shots from the open folder ({files} files)"),
            )
            .on_hover_text("Places the Organizer's files into the empty clips in order (Quick fill's Auto-fill rest)");
            ui.add_space(12.0);
        }
        // The credits clip (Paul, 2026-11-06): one checkbox, remembered; the clip is the
        // user's own template once edited.
        let mut credits = nav.session.credits_on();
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut credits, "Add the credits clip at the end")
                .on_hover_text("A title card with the storyboard's name, the song's credit and \"Made with AssetCraft\" — edit it to make it yours")
                .changed()
            {
                nav.run("storyboard.credits", json!({ "on": credits }));
            }
            if ui.small_button("Edit…").on_hover_text("Open the credits clip as a storyboard; File › Save keeps your version").clicked() {
                nav.run("storyboard.credits", json!({ "edit": true }));
                nav.state.storyboard.start.open = false;
            }
        });
        ui.add_space(12.0);
        let ready = match style {
            Style::Pacing(_) => selected_track(nav).is_some(),
            _ => true,
        };
        if let Style::Template(_) = style {
            ui.weak(if selected_track(nav).is_some() {
                "With the picked song as the track"
            } else {
                "No song picked: pick one on the left to fit the cut to it, or add a track later"
            });
            ui.add_space(6.0);
        }
        let create = egui::Button::new(RichText::new("Create storyboard").color(Color32::WHITE).strong()).fill(ACCENT);
        if ui.add_enabled(ready, create.min_size(vec2(200.0, 32.0))).clicked() {
            create_board(nav);
        }
        ui.add_space(24.0);
        section(ui, "OR START WITHOUT A TRACK");
        for other in [Style::Sample, Style::Blank] {
            if ui.selectable_label(style == other, style_name(other)).on_hover_text(blurb(other)).clicked() {
                nav.state.storyboard.start.style = other;
            }
        }
    });
}

pub(crate) fn create_board(nav: &mut Navigator) {
    let start = &nav.state.storyboard.start;
    let name = start.name.trim().to_owned();
    let (style, track, length, autofill, density, shape) =
        (start.style, start.track.clone(), start.length, start.autofill, start.density(), start.shape);
    let credits = nav.session.credits_on();
    // Auto-fill only when there is something to fill with: an empty Organizer isn't an error.
    let files = nav.session.items().iter().filter(|e| e.kind == assetcraft_fs::EntryKind::File).count();
    let autofill = autofill && files > 0;
    let ok = match style {
        Style::Pacing(p) => {
            let Some(cue) = track else { return };
            let mut params = json!({
                "cueMap": cue, "pacing": p.id(), "seconds": length, "density": density,
                "shape": serde_json::to_value(shape).unwrap_or(json!({})), "credits": credits,
            });
            if !name.is_empty() {
                params["name"] = json!(name);
            }
            let made = nav.run("storyboard.generate", params).is_some();
            if made && autofill {
                nav.run("storyboard.autofill", json!({}));
            }
            made
        }
        Style::Template(i) => {
            let Some(t) = BUNDLED.get(i) else { return };
            let mut params = json!({ "template": t.id, "credits": credits });
            if let Some(steps) = nav.state.storyboard.start.steps {
                params["steps"] = json!(steps);
            }
            params["stepSeconds"] = json!(nav.state.storyboard.start.step_secs);
            if let Some(cue) = track {
                params["cueMap"] = json!(cue);
            }
            if !name.is_empty() {
                params["name"] = json!(name);
            }
            let made = nav.run("storyboard.new", params).is_some();
            if made && autofill {
                nav.run("storyboard.autofill", json!({}));
            }
            made
        }
        Style::Sample => nav.run("storyboard.new", json!({ "sample": true, "credits": credits })).is_some(),
        Style::Blank => {
            let mut params = json!({ "credits": credits });
            if !name.is_empty() {
                params["name"] = json!(name);
            }
            nav.run("storyboard.new", params).is_some()
        }
    };
    if ok {
        let start = &mut nav.state.storyboard.start;
        start.open = false;
        start.name.clear();
    }
}
