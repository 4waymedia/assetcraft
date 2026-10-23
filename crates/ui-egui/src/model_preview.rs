//! Orbit preview of a selected 3D model in the Preview panel.
//!
//! Drag to orbit, scroll to zoom, double-click to reset. The camera is view state that lives
//! here (it changes nothing in the session), so it is not an engine command; the look (display
//! mode, background, exposure, turntable) is the session's `view.model` setting, shown in a
//! small bar over the preview and in View → 3D Preview. Loading and
//! rendering run on a `ModelView` thread from `assetcraft-thumbs`; this module only turns input
//! into camera changes, asks for frames, and draws the newest one. Until the first frame
//! arrives (or if the model can't be loaded) the ordinary thumbnail is drawn instead.
//!
//! While dragging, frames are "fast": the model view renders them smaller (and coarser) so
//! each fits its time budget on this machine, and they are drawn scaled up to the panel; the
//! frame after the drag ends is full size.

use crate::Navigator;
use crate::content::draw_thumb;
use crate::thumbnails::{PRIORITY_PREVIEW, preview_side};
use crate::widgets::fit_at_most;
use assetcraft_engine::{ModelBackground, ModelDisplay, ModelPrefs};
use assetcraft_fs::{Entry, EntryKind};
use assetcraft_probe::Format;
use assetcraft_thumbs::model::{Background, DisplayMode, Lighting, ModelState, ModelView, RenderOptions, View, is_model};
use egui::{
    Align2, Color32, ColorImage, FontId, Id, Order, Rect, Response, TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2,
};
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// Radians of orbit per point dragged.
const ORBIT_PER_POINT: f32 = 0.01;
/// Zoom factor per point scrolled (as an exponent).
const ZOOM_PER_POINT: f32 = 0.004;
/// Longest preview frame side in pixels.
const MAX_FRAME_SIDE: f32 = 1024.0;
/// How far a fast (drag) frame may be scaled up to fill the panel (`1 / MIN_DRAG_SCALE`).
const MAX_UPSCALE: f32 = 4.0;
/// Turntable speed: one turn every 12 s.
const TURNTABLE_RADIANS_PER_SECOND: f32 = std::f32::consts::TAU / 12.0;
/// Height of the controls bar over the preview, in points.
const BAR_HEIGHT: f32 = 24.0;
/// How long a model must stay selected before it is loaded. Arrowing through a folder of big
/// models then starts no loads at all (each `ModelView` is its own thread), and a load whose
/// selection moved on is cancelled when its view is dropped.
pub(crate) const OPEN_AFTER: Duration = Duration::from_millis(150);

/// The model being previewed and its camera.
pub(crate) struct ModelPreview {
    path: PathBuf,
    modified: Option<SystemTime>,
    /// When this model was selected; the view opens [`OPEN_AFTER`] later.
    since: Instant,
    model: Option<ModelView>,
    camera: View,
    requested: Option<(View, [u32; 2], bool, ModelPrefs)>,
    texture: Option<TextureHandle>,
    /// The shown frame is a fast one, drawn at a fraction of the requested size: scale it up.
    fast_frame: bool,
    /// Where the preview was last drawn (for tests that simulate pointer input).
    rect: Rect,
}

impl ModelPreview {
    /// The current camera (for tests).
    #[cfg(test)]
    pub(crate) fn camera(&self) -> View {
        self.camera
    }

    /// Whether a rendered frame is being shown (for tests).
    #[cfg(test)]
    pub(crate) fn has_frame(&self) -> bool {
        self.texture.is_some()
    }

    /// Whether the shown frame is a fast (drag) one (for tests).
    #[cfg(test)]
    pub(crate) fn shows_fast_frame(&self) -> bool {
        self.fast_frame
    }

    /// Whether the loader has been started (for tests).
    #[cfg(test)]
    pub(crate) fn is_open(&self) -> bool {
        self.model.is_some()
    }

    /// The look the last requested frame was asked for (for tests).
    #[cfg(test)]
    pub(crate) fn requested_prefs(&self) -> Option<ModelPrefs> {
        self.requested.map(|r| r.3)
    }
}

/// The renderer options for the session's `view.model` settings (size fields are placeholders:
/// the model view sets them per frame).
pub(crate) fn look(prefs: ModelPrefs) -> RenderOptions {
    RenderOptions {
        mode: match prefs.mode {
            ModelDisplay::Shaded => DisplayMode::Shaded,
            ModelDisplay::Wireframe => DisplayMode::Wireframe,
            ModelDisplay::Normals => DisplayMode::Normals,
            ModelDisplay::Uv => DisplayMode::UvChecker,
        },
        background: match prefs.background.rgb() {
            Some(rgb) => Background::Solid(rgb),
            None => Background::Transparent,
        },
        lighting: Lighting { exposure: prefs.exposure_factor(), ..Lighting::STUDIO },
        ..RenderOptions::new(16, 16)
    }
}

/// `true` when `entry` should get the orbit preview (decided by extension, so it works before
/// the background probe arrives; a file that isn't really a model fails to load and falls back
/// to the thumbnail).
pub(crate) fn wants_orbit(entry: &Entry) -> bool {
    entry.kind == EntryKind::File && entry.extension().and_then(|e| Format::from_extension(&e)).is_some_and(is_model)
}

/// The camera after one frame of input: drag orbits, scroll zooms, double-click resets.
pub(crate) fn next_camera(camera: View, drag: Vec2, scroll: f32, reset: bool) -> View {
    if reset {
        return View::default();
    }
    let mut camera = camera;
    if drag != Vec2::ZERO {
        camera = camera.orbit(-drag.x * ORBIT_PER_POINT, drag.y * ORBIT_PER_POINT);
    }
    if scroll != 0.0 {
        camera = camera.zoomed((scroll * ZOOM_PER_POINT).exp());
    }
    camera
}

/// Frame size in pixels for a panel area, longest side at most [`MAX_FRAME_SIDE`].
fn frame_size(rect: Rect, pixels_per_point: f32) -> [u32; 2] {
    let px = rect.size() * pixels_per_point;
    let scale = (MAX_FRAME_SIDE / px.x.max(px.y).max(1.0)).min(1.0);
    [
        (px.x * scale).round().clamp(16.0, MAX_FRAME_SIDE) as u32,
        (px.y * scale).round().clamp(16.0, MAX_FRAME_SIDE) as u32,
    ]
}

/// Draw the orbit preview for `entry` into `rect`, handling `response`'s input.
pub(crate) fn draw(nav: &mut Navigator, ui: &Ui, entry: &Entry, rect: Rect, response: &Response) {
    let same = nav.state.model.as_ref().is_some_and(|m| m.path == entry.path && m.modified == entry.modified);
    if !same {
        // Replacing the old preview drops (and so cancels) its view.
        nav.state.model = Some(ModelPreview {
            path: entry.path.clone(),
            modified: entry.modified,
            since: Instant::now(),
            model: None,
            camera: View::default(),
            requested: None,
            texture: None,
            fast_frame: false,
            rect,
        });
    }
    let ctx = ui.ctx().clone();
    let scroll = if response.hovered() { ui.input(|i| i.smooth_scroll_delta.y) } else { 0.0 };
    let services = nav.session.services();
    let prefs = nav.session.model();
    let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
    let failed = 'model: {
        let Some(preview) = nav.state.model.as_mut() else { return };
        preview.rect = rect;
        let mut drag = if response.dragged() { response.drag_delta() } else { Vec2::ZERO };
        let turning = prefs.turntable && !response.dragged();
        if turning {
            // The turntable is a slow drag the clock makes: budgeted frames, like a real one.
            drag.x += TURNTABLE_RADIANS_PER_SECOND * dt / ORBIT_PER_POINT;
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        preview.camera = next_camera(preview.camera, drag, scroll, response.double_clicked());
        let waited = preview.since.elapsed();
        if preview.model.is_none() && waited >= OPEN_AFTER {
            preview.model = Some(ModelView::open(services, preview.path.clone()));
        }
        let Some(model) = preview.model.as_mut() else {
            ctx.request_repaint_after(OPEN_AFTER.saturating_sub(waited));
            break 'model false;
        };
        if let Some(frame) = model.poll() {
            let image = ColorImage::from_rgba_unmultiplied(
                [frame.image.width as usize, frame.image.height as usize],
                &frame.image.rgba,
            );
            match &mut preview.texture {
                Some(texture) => texture.set(image, TextureOptions::LINEAR),
                None => {
                    preview.texture = Some(ctx.load_texture("assetcraft-model-preview", image, TextureOptions::LINEAR))
                }
            }
            preview.fast_frame = frame.fast;
        }
        let ready = matches!(model.state(), ModelState::Ready { .. });
        let size = frame_size(rect, ctx.pixels_per_point());
        let wanted = (preview.camera, size, response.dragged() || turning, prefs);
        if ready && preview.requested != Some(wanted) {
            model.request_look(wanted.0, size[0], size[1], wanted.2, &look(prefs));
            preview.requested = Some(wanted);
        }
        if model.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        matches!(model.state(), ModelState::Failed(_))
    };
    let texture =
        nav.state.model.as_ref().and_then(|m| m.texture.as_ref().map(|t| (t.id(), t.size_vec2(), m.fast_frame)));
    match texture {
        Some((id, size, fast)) if !failed => {
            // A fast frame is a scaled-down render of this rect: let it scale back up.
            let target = fit_at_most(rect, size / ctx.pixels_per_point(), if fast { MAX_UPSCALE } else { 1.0 });
            ui.painter().image(id, target, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            ui.painter().text(
                rect.center_bottom() - vec2(0.0, 4.0),
                Align2::CENTER_BOTTOM,
                "Drag to orbit · scroll to zoom · double-click to reset",
                FontId::proportional(11.0),
                ui.visuals().weak_text_color(),
            );
            controls_bar(nav, &ctx, rect, prefs);
        }
        _ => {
            let side = preview_side(rect.width().max(rect.height()) * ctx.pixels_per_point());
            draw_thumb(nav, ui, entry, rect, side, PRIORITY_PREVIEW);
        }
    }
}

/// The bar over the preview: display mode, background, turntable, exposure, reset camera.
/// Every control runs `view.model`; the camera reset is view state. An `Area` (like the
/// full-window overlay) so its buttons take clicks before the orbit surface under them.
fn controls_bar(nav: &mut Navigator, ctx: &egui::Context, rect: Rect, prefs: ModelPrefs) {
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), BAR_HEIGHT));
    let mut actions: Vec<serde_json::Value> = Vec::new();
    let mut reset = false;
    egui::Area::new(Id::new("assetcraft-model-bar")).order(Order::Middle).fixed_pos(bar.min).show(ctx, |ui| {
        ui.set_max_width(bar.width());
        ui.painter().rect_filled(bar, 4.0, Color32::from_black_alpha(110));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (mode, label) in ModelDisplay::ALL {
                let short = match mode {
                    ModelDisplay::Shaded => "Shaded",
                    ModelDisplay::Wireframe => "Wire",
                    ModelDisplay::Normals => "Normals",
                    ModelDisplay::Uv => "UV",
                };
                if ui.selectable_label(prefs.mode == mode, short).on_hover_text(label).clicked() {
                    actions.push(json!({ "mode": mode.name() }));
                }
            }
            ui.separator();
            let all = ModelBackground::ALL;
            let at = all.iter().position(|(b, _)| *b == prefs.background).unwrap_or(0);
            let (next_background, _) = all[(at + 1) % all.len()];
            let bg_label = all[at].1;
            if ui.button(format!("BG: {bg_label}")).on_hover_text("Background (click to cycle)").clicked() {
                actions.push(json!({ "background": next_background.name() }));
            }
            if ui.selectable_label(prefs.turntable, "⟳").on_hover_text("Turntable").clicked() {
                actions.push(json!({ "turntable": !prefs.turntable }));
            }
            let (lo, hi) = assetcraft_engine::prefs::EXPOSURE_RANGE;
            let mut exposure = prefs.exposure;
            let slider = egui::Slider::new(&mut exposure, lo..=hi).show_value(false).logarithmic(true);
            if ui.add_sized(vec2(70.0, 16.0), slider).on_hover_text(format!("Exposure {}%", prefs.exposure)).changed() {
                actions.push(json!({ "exposure": exposure }));
            }
            if ui.button("Reset").on_hover_text("Reset the camera").clicked() {
                reset = true;
            }
        });
    });
    for params in actions {
        nav.run("view.model", params);
    }
    if reset && let Some(preview) = nav.state.model.as_mut() {
        preview.camera = View::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_moves_the_camera_and_double_click_resets_it() {
        let start = View::default();
        let dragged = next_camera(start, vec2(100.0, -50.0), 0.0, false);
        assert!(dragged.yaw < start.yaw && dragged.pitch < start.pitch);
        let zoomed = next_camera(dragged, Vec2::ZERO, 200.0, false);
        assert!(zoomed.zoom > 1.5 && zoomed.yaw == dragged.yaw);
        assert_eq!(next_camera(zoomed, vec2(5.0, 5.0), 10.0, true), View::default());
        assert_eq!(next_camera(start, Vec2::ZERO, 0.0, false), start);
        let wild = next_camera(start, vec2(f32::MAX, f32::MAX), f32::MAX, false);
        assert!(wild.yaw.is_finite() && wild.pitch.is_finite() && wild.zoom <= View::ZOOM_RANGE.1);
    }

    #[test]
    fn selecting_a_model_shows_a_rendered_orbit_frame_headless() {
        use assetcraft_engine::{Prefs, RecordingLauncher, Session};
        use assetcraft_fs::RealFs;
        use serde_json::json;
        use std::sync::Arc;
        let models = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/props/models");
        let session = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
        let mut nav = Navigator::with_one_worker(session);
        assert!(nav.open_folder(&models));
        nav.run("select.click", json!({ "path": models.join("soul_orb_icosphere.glb") }));
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !nav.state.model.as_ref().is_some_and(ModelPreview::has_frame) && std::time::Instant::now() < deadline {
            let input = egui::RawInput { screen_rect: Some(screen), ..egui::RawInput::default() };
            let mut output = ctx.run_ui(input, |ui| nav.ui(ui));
            output.textures_delta.clear();
            std::thread::sleep(Duration::from_millis(10));
        }
        let preview = nav.state.model.as_ref().expect("a model preview for the selected GLB");
        assert!(preview.has_frame(), "a rendered frame arrived");
        assert_eq!(preview.camera(), View::default());
        assert!(nav.message().is_none(), "{:?}", nav.message());
    }

    #[test]
    fn a_model_loads_only_once_it_stays_selected() {
        use assetcraft_engine::{Prefs, RecordingLauncher, Session};
        use assetcraft_fs::RealFs;
        use serde_json::json;
        use std::sync::Arc;
        let models = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/props/models");
        let session = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
        let mut nav = Navigator::with_one_worker(session);
        assert!(nav.open_folder(&models));
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));
        let frame = |nav: &mut Navigator| {
            let input = egui::RawInput { screen_rect: Some(screen), ..egui::RawInput::default() };
            let mut output = ctx.run_ui(input, |ui| nav.ui(ui));
            output.textures_delta.clear();
        };
        // Arrowing through models: each is shown for one frame, none is loaded.
        for name in ["soul_orb_icosphere.glb", "scythe_ring.gltf", "tombstone.obj"] {
            nav.run("select.click", json!({ "path": models.join(name) }));
            frame(&mut nav);
            let preview = nav.state.model.as_ref().expect("a preview for the selected model");
            assert_eq!(preview.path, models.join(name));
            assert!(!preview.is_open(), "{name} was loaded before the selection settled");
        }
        std::thread::sleep(OPEN_AFTER + Duration::from_millis(20));
        frame(&mut nav);
        assert!(nav.state.model.as_ref().is_some_and(ModelPreview::is_open), "the settled model loads");
    }

    #[test]
    fn dragging_shows_fast_frames_and_releasing_a_full_one() {
        use assetcraft_engine::{Prefs, RecordingLauncher, Session};
        use assetcraft_fs::RealFs;
        use serde_json::json;
        use std::sync::Arc;
        let models = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/props/models");
        let session = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
        let mut nav = Navigator::with_one_worker(session);
        assert!(nav.open_folder(&models));
        nav.run("select.click", json!({ "path": models.join("rune_crate_textured.glb") }));
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));
        let run = |nav: &mut Navigator, events: Vec<egui::Event>| {
            let input = egui::RawInput { screen_rect: Some(screen), events, ..egui::RawInput::default() };
            let mut output = ctx.run_ui(input, |ui| nav.ui(ui));
            output.textures_delta.clear();
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !nav.state.model.as_ref().is_some_and(ModelPreview::has_frame) && std::time::Instant::now() < deadline {
            run(&mut nav, Vec::new());
            std::thread::sleep(Duration::from_millis(10));
        }
        let preview = nav.state.model.as_ref().expect("a preview");
        assert!(preview.has_frame() && !preview.shows_fast_frame(), "the first frame is a full one");
        // Press in the preview panel and drag: the frames that arrive are fast ones.
        let press = egui::Event::PointerButton {
            pos: preview_center(&nav),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        run(&mut nav, vec![press]);
        let mut saw_fast = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let mut step = 0.0;
        while !saw_fast && std::time::Instant::now() < deadline {
            step += 6.0;
            run(&mut nav, vec![egui::Event::PointerMoved(preview_center(&nav) + vec2(step, 0.0))]);
            saw_fast = nav.state.model.as_ref().is_some_and(ModelPreview::shows_fast_frame);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(saw_fast, "a fast frame was shown while dragging");
        let release = egui::Event::PointerButton {
            pos: preview_center(&nav) + vec2(step, 0.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        };
        run(&mut nav, vec![release]);
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while nav.state.model.as_ref().is_some_and(ModelPreview::shows_fast_frame)
            && std::time::Instant::now() < deadline
        {
            run(&mut nav, Vec::new());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!nav.state.model.as_ref().unwrap().shows_fast_frame(), "a full frame follows the release");
        assert_ne!(nav.state.model.as_ref().unwrap().camera(), View::default(), "the drag orbited");
    }

    /// The centre of the preview's image area in the headless layout.
    fn preview_center(nav: &Navigator) -> egui::Pos2 {
        nav.state.model.as_ref().map(|m| m.rect.center()).unwrap_or(pos2(1000.0, 400.0))
    }

    #[test]
    fn look_maps_the_settings_and_changing_them_re_requests_a_frame() {
        use assetcraft_engine::{ModelBackground, ModelDisplay, ModelPrefs, Prefs, RecordingLauncher, Session};
        use assetcraft_fs::RealFs;
        use serde_json::json;
        use std::sync::Arc;
        let l = look(ModelPrefs {
            mode: ModelDisplay::Uv,
            background: ModelBackground::Grey,
            exposure: 200,
            turntable: false,
        });
        assert_eq!(l.mode, DisplayMode::UvChecker);
        assert_eq!(l.background, Background::Solid([77, 77, 80]));
        assert!((l.lighting.exposure - 2.0).abs() < 1e-6);
        assert_eq!(look(ModelPrefs::default()).background, Background::Transparent);

        let models = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/props/models");
        let session = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
        let mut nav = Navigator::with_one_worker(session);
        assert!(nav.open_folder(&models));
        nav.run("select.click", json!({ "path": models.join("rune_crate_textured.glb") }));
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));
        let run = |nav: &mut Navigator, time: f64| {
            let input = egui::RawInput { screen_rect: Some(screen), time: Some(time), ..egui::RawInput::default() };
            let mut output = ctx.run_ui(input, |ui| nav.ui(ui));
            output.textures_delta.clear();
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let mut t = 0.0;
        while !nav.state.model.as_ref().is_some_and(ModelPreview::has_frame) && std::time::Instant::now() < deadline {
            t += 0.016;
            run(&mut nav, t);
            std::thread::sleep(Duration::from_millis(10));
        }
        let before = nav.state.model.as_ref().unwrap().requested_prefs().unwrap();
        assert_eq!(before.mode, ModelDisplay::Shaded);
        // A setting change through the command is picked up by the next frame request.
        nav.run("view.model", json!({ "mode": "wireframe" }));
        t += 0.016;
        run(&mut nav, t);
        assert_eq!(nav.state.model.as_ref().unwrap().requested_prefs().unwrap().mode, ModelDisplay::Wireframe);
        // The turntable advances the camera on the clock, without any pointer input.
        let yaw = nav.state.model.as_ref().unwrap().camera().yaw;
        nav.run("view.model", json!({ "turntable": true }));
        for _ in 0..5 {
            t += 0.1;
            run(&mut nav, t);
        }
        let turned = nav.state.model.as_ref().unwrap().camera().yaw;
        assert!((turned - yaw).abs() > 0.05, "yaw moved {yaw} -> {turned}");
        nav.run("view.model", json!({ "turntable": false }));
        t += 0.1;
        run(&mut nav, t);
        let stopped = nav.state.model.as_ref().unwrap().camera().yaw;
        t += 0.1;
        run(&mut nav, t);
        assert_eq!(nav.state.model.as_ref().unwrap().camera().yaw, stopped, "off: the camera holds");
    }

    #[test]
    fn frame_size_is_capped_and_keeps_the_aspect() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        assert_eq!(frame_size(r, 1.0), [800, 400]);
        assert_eq!(frame_size(r, 2.0), [1024, 512]);
        assert_eq!(frame_size(Rect::from_min_size(pos2(0.0, 0.0), vec2(2.0, 2.0)), 1.0), [16, 16]);
    }
}
