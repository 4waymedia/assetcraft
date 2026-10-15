//! Orbit preview of a selected 3D model in the Preview panel.
//!
//! Drag to orbit, scroll to zoom, double-click to reset. The camera is view state that lives
//! here (it changes nothing in the session), so it is not an engine command. Loading and
//! rendering run on a `ModelView` thread from `assetcraft-thumbs`; this module only turns input
//! into camera changes, asks for frames, and draws the newest one. Until the first frame
//! arrives (or if the model can't be loaded) the ordinary thumbnail is drawn instead.

use crate::Navigator;
use crate::content::draw_thumb;
use crate::thumbnails::{PRIORITY_PREVIEW, preview_side};
use crate::widgets::fit_at_most;
use assetcraft_fs::{Entry, EntryKind};
use assetcraft_probe::Format;
use assetcraft_thumbs::model::{ModelState, ModelView, View, is_model};
use egui::{Align2, Color32, ColorImage, FontId, Rect, Response, TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// Radians of orbit per point dragged.
const ORBIT_PER_POINT: f32 = 0.01;
/// Zoom factor per point scrolled (as an exponent).
const ZOOM_PER_POINT: f32 = 0.004;
/// Longest preview frame side in pixels.
const MAX_FRAME_SIDE: f32 = 1024.0;
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
    requested: Option<(View, [u32; 2], bool)>,
    texture: Option<TextureHandle>,
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

    /// Whether the loader has been started (for tests).
    #[cfg(test)]
    pub(crate) fn is_open(&self) -> bool {
        self.model.is_some()
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
        });
    }
    let ctx = ui.ctx().clone();
    let scroll = if response.hovered() { ui.input(|i| i.smooth_scroll_delta.y) } else { 0.0 };
    let services = nav.session.services();
    let failed = 'model: {
        let Some(preview) = nav.state.model.as_mut() else { return };
        let drag = if response.dragged() { response.drag_delta() } else { Vec2::ZERO };
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
        }
        let ready = matches!(model.state(), ModelState::Ready { .. });
        let size = frame_size(rect, ctx.pixels_per_point());
        let wanted = (preview.camera, size, response.dragged());
        if ready && preview.requested != Some(wanted) {
            model.request(wanted.0, size[0], size[1], wanted.2);
            preview.requested = Some(wanted);
        }
        if model.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        matches!(model.state(), ModelState::Failed(_))
    };
    let texture = nav.state.model.as_ref().and_then(|m| m.texture.as_ref()).map(|t| (t.id(), t.size_vec2()));
    match texture {
        Some((id, size)) if !failed => {
            let target = fit_at_most(rect, size / ctx.pixels_per_point(), 1.0);
            ui.painter().image(id, target, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            ui.painter().text(
                rect.center_bottom() - vec2(0.0, 4.0),
                Align2::CENTER_BOTTOM,
                "Drag to orbit · scroll to zoom · double-click to reset",
                FontId::proportional(11.0),
                ui.visuals().weak_text_color(),
            );
        }
        _ => {
            let side = preview_side(rect.width().max(rect.height()) * ctx.pixels_per_point());
            draw_thumb(nav, ui, entry, rect, side, PRIORITY_PREVIEW);
        }
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
    fn frame_size_is_capped_and_keeps_the_aspect() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        assert_eq!(frame_size(r, 1.0), [800, 400]);
        assert_eq!(frame_size(r, 2.0), [1024, 512]);
        assert_eq!(frame_size(Rect::from_min_size(pos2(0.0, 0.0), vec2(2.0, 2.0)), 1.0), [16, 16]);
    }
}
