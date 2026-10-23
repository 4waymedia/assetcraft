//! 3D models (GLB, glTF, OBJ): loading through [`Services`] and an interactive orbit preview.
//!
//! [`load`] reads a model file within [`MAX_FILE_BYTES`] and parses it with `assetcraft-mesh`
//! under its default [`Limits`] (2 M vertices / triangles, 128 MiB of buffers). A `.gltf`'s
//! external buffers are read relative to the file through `services.read_range`: only
//! regular files inside the model's folder (no `..`, no absolute paths, no symlink anywhere
//! between the model's folder and the buffer), each at most its declared `byteLength`.
//!
//! [`decode_textures`] turns the mesh's encoded images into [`Texture`]s with the `image` crate
//! under the thumbnail decode limits, shrunk to [`TEXTURE_MAX_SIDE`]; an image that won't
//! decode leaves its material untextured, so the geometry always draws.
//!
//! [`ModelView`] keeps one loaded [`Model`] on its own thread and renders the newest requested
//! [`View`] with the CPU rasterizer, so the UI thread only uploads finished images. Requests
//! made while a render runs are coalesced to the latest one.

use crate::{MAX_FILE_BYTES, ThumbError};
use assetcraft_fs::{EntryKind, Services};
use assetcraft_probe::{Category, Format, Probe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

pub use assetcraft_mesh::{
    Background, DisplayMode, Image, Lighting, Limits, Mesh, MeshError, RenderOptions, Texture, View,
};
use image::ImageReader;
use std::io::Cursor;

/// Above this many triangles, interactive (`fast`) preview frames draw every n-th triangle so
/// orbiting stays responsive; the frame after the drag ends draws them all.
pub const PREVIEW_MAX_TRIANGLES: usize = 500_000;

/// Largest preview frame side.
pub const PREVIEW_MAX_SIDE: u32 = 2048;

/// Decoded textures are shrunk to this side: 4 MiB each instead of 64 MiB for a 4096² map,
/// and more than enough for a preview panel (opportunity: full size in the full-window view).
pub const TEXTURE_MAX_SIDE: u32 = 1024;

/// A mesh with its decoded textures.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// The geometry and materials.
    pub mesh: Mesh,
    /// `textures[i]` is the decoded `mesh.images[i]`, or `None` when it would not decode.
    pub textures: Vec<Option<Texture>>,
}

impl Model {
    /// Bytes held, for memory budgets.
    pub fn cost(&self) -> usize {
        self.mesh.cost() + self.textures.iter().flatten().map(|t| t.rgba.len()).sum::<usize>()
    }
}

/// Decode the images of `mesh` that some material draws (base colour maps; the
/// metallic-roughness map waits for V1's environment term), format sniffed from the bytes,
/// not the declared MIME type, within the thumbnail limits, each shrunk to
/// [`TEXTURE_MAX_SIDE`]. Images that fail (unknown format, damaged, over the limits) and
/// images nothing draws are `None`.
pub fn decode_textures(mesh: &Mesh) -> Vec<Option<Texture>> {
    mesh.images
        .iter()
        .enumerate()
        .map(|(i, image)| {
            let drawn = mesh.materials.iter().any(|m| m.base_color_image == Some(i as u32));
            if !drawn {
                return None;
            }
            let reader = ImageReader::new(Cursor::new(image.bytes.as_slice())).with_guessed_format().ok()?;
            let mut reader = reader;
            reader.limits(crate::decoders::image_limits());
            let decoded = reader.decode().ok()?;
            let small = if decoded.width().max(decoded.height()) > TEXTURE_MAX_SIDE {
                decoded.thumbnail(TEXTURE_MAX_SIDE, TEXTURE_MAX_SIDE)
            } else {
                decoded
            };
            let rgba = small.to_rgba8();
            Texture::new(rgba.width(), rgba.height(), rgba.into_raw())
        })
        .collect()
}

/// [`load`] plus [`decode_textures`].
pub fn load_model(services: &dyn Services, probe: &Probe) -> Result<Model, ThumbError> {
    let mesh = load(services, probe)?;
    let textures = decode_textures(&mesh);
    Ok(Model { mesh, textures })
}

/// `true` for the formats [`load`] reads.
pub fn is_model(format: Format) -> bool {
    format.category() == Category::Model3d
}

impl From<MeshError> for ThumbError {
    fn from(e: MeshError) -> Self {
        match e {
            MeshError::TooLarge => ThumbError::TooLarge,
            other => ThumbError::Decode(other.to_string()),
        }
    }
}

/// Load the probed model file as a mesh.
pub fn load(services: &dyn Services, probe: &Probe) -> Result<Mesh, ThumbError> {
    load_unless(services, probe, &AtomicBool::new(false))
}

/// [`load`], giving up (with a `Decode("cancelled")` error) if `cancel` is set before the file
/// is read or before it is parsed.
fn load_unless(services: &dyn Services, probe: &Probe, cancel: &AtomicBool) -> Result<Mesh, ThumbError> {
    let cancelled = || ThumbError::Decode("cancelled".into());
    if !is_model(probe.format) {
        return Err(ThumbError::Unsupported(probe.format));
    }
    if cancel.load(Ordering::Acquire) {
        return Err(cancelled());
    }
    let bytes = crate::decoders::read_file(services, probe, MAX_FILE_BYTES)?;
    if cancel.load(Ordering::Acquire) {
        return Err(cancelled());
    }
    let limits = Limits::default();
    let folder = probe.path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut resolve = |rel: &str, len: usize| external_buffer(services, &folder, rel, len);
    let mesh = match probe.format {
        Format::Glb => assetcraft_mesh::parse_glb(&bytes, &limits, &mut resolve)?,
        Format::Gltf => assetcraft_mesh::parse_gltf(&bytes, &limits, &mut resolve)?,
        _ => assetcraft_mesh::parse_obj(&bytes, &limits)?,
    };
    Ok(mesh)
}

/// Probe `path` and [`load`] it.
pub fn load_path(services: &dyn Services, path: &Path) -> Result<Mesh, ThumbError> {
    let probe = assetcraft_probe::probe(services, path).map_err(ThumbError::Io)?;
    load(services, &probe)
}

/// Read `rel` (already checked by the parser: relative, no `..`) next to the model. Every
/// folder between the model's folder and the buffer must be a real folder, not a symlink (or
/// junction) that leads elsewhere, and the buffer itself a regular file.
fn external_buffer(services: &dyn Services, folder: &Path, rel: &str, len: usize) -> Result<Vec<u8>, String> {
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    let mut path = folder.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        path.push(part);
        if i + 1 < parts.len() {
            let dir = services.entry(&path).map_err(|e| e.to_string())?;
            if dir.kind != EntryKind::Dir {
                return Err(format!("{rel}: {part} is not a folder (links are not followed)"));
            }
        }
    }
    let entry = services.entry(&path).map_err(|e| e.to_string())?;
    if entry.kind != EntryKind::File {
        return Err(format!("{rel} is not a regular file"));
    }
    if entry.size < len as u64 {
        return Err(format!("{rel} is shorter than its byteLength"));
    }
    services.read_range(&path, 0, len).map_err(|e| e.to_string())
}

/// Threads a preview frame rasterizes on: every core up to [`PREVIEW_MAX_THREADS`] (the preview
/// is the one thing the user is waiting on; thumbnails stay on their pool's workers).
pub fn preview_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, PREVIEW_MAX_THREADS)
}

/// Most threads one preview frame uses.
pub const PREVIEW_MAX_THREADS: usize = 16;

/// How long a drag (fast) frame may take. Drag frames are rendered smaller (and, at the floor,
/// with fewer triangles) until they fit, so orbiting stays fluid on one slow core; the frame
/// after the drag ends is full size and quality.
pub const DRAG_BUDGET: Duration = Duration::from_millis(25);

/// Smallest drag-frame scale (a quarter of the panel's pixels per side).
pub const MIN_DRAG_SCALE: f32 = 0.25;

/// Most triangles skipped per drawn one at the scale floor.
pub const MAX_DRAG_STRIDE: usize = 8;

/// Picks the size and triangle stride of drag frames so each one fits a time budget on this
/// machine. Pure: feed it what the last frame took and it moves for the next one. Shrinks at
/// once (one slow frame is enough), grows back damped (no oscillation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragQuality {
    /// Side scale of drag frames, [`MIN_DRAG_SCALE`]..=1.
    pub scale: f32,
    /// Every `stride`-th triangle is drawn; > 1 only once `scale` sits at the floor.
    pub stride: usize,
}

impl Default for DragQuality {
    fn default() -> Self {
        Self { scale: 1.0, stride: 1 }
    }
}

impl DragQuality {
    /// The last drag frame at this quality took `took`; adjust for the next one to fit `budget`.
    pub fn observe(&mut self, took: Duration, budget: Duration) {
        if took.is_zero() || budget.is_zero() {
            return; // no information
        }
        let ratio = budget.as_secs_f32() / took.as_secs_f32();
        if !ratio.is_finite() || ratio <= 0.0 {
            return;
        }
        if ratio < 1.0 {
            if self.scale > MIN_DRAG_SCALE {
                // Pixels (and time) go with scale², so the side scale moves by the square root.
                self.scale = (self.scale * ratio.sqrt()).clamp(MIN_DRAG_SCALE, 1.0);
            } else {
                self.stride = ((self.stride as f32 / ratio).ceil() as usize).clamp(1, MAX_DRAG_STRIDE);
            }
        } else if ratio > 1.3 {
            if self.stride > 1 {
                self.stride -= 1;
            } else {
                let target = (self.scale * ratio.sqrt()).min(1.0);
                self.scale += (target - self.scale) * 0.5;
            }
        }
    }

    /// `width × height` scaled, each side at least 16 px.
    pub fn size(&self, width: u32, height: u32) -> (u32, u32) {
        let s = |v: u32| ((v as f32 * self.scale).round() as u32).clamp(16, v.max(16));
        (s(width), s(height))
    }
}

/// Render `model` from `view` at `width × height`. `fast` frames skip supersampling and, above
/// [`PREVIEW_MAX_TRIANGLES`], draw a decimated subset. `look` sets the display mode, lighting
/// and background (the size and quality fields are overwritten).
pub fn render_view(
    model: &Model,
    view: View,
    width: u32,
    height: u32,
    fast: bool,
    look: &RenderOptions,
) -> Result<Image, ThumbError> {
    let mut opts = RenderOptions {
        width: width.clamp(1, PREVIEW_MAX_SIDE),
        height: height.clamp(1, PREVIEW_MAX_SIDE),
        supersample: 2,
        stride: 1,
        threads: preview_threads(),
        ..*look
    };
    if fast {
        opts.supersample = 1;
        opts.stride = model.mesh.triangle_count().div_ceil(PREVIEW_MAX_TRIANGLES).max(1);
    }
    Ok(assetcraft_mesh::render_with(&model.mesh, &model.textures, view, &opts)?)
}

/// [`render_view`] for a drag frame at `quality`: `width × height` scaled down and every
/// `quality.stride`-th triangle (or the size-based stride, whichever is coarser).
pub fn render_drag(
    model: &Model,
    view: View,
    width: u32,
    height: u32,
    quality: DragQuality,
    look: &RenderOptions,
) -> Result<Image, ThumbError> {
    let (w, h) = quality.size(width, height);
    let opts = RenderOptions {
        width: w.clamp(1, PREVIEW_MAX_SIDE),
        height: h.clamp(1, PREVIEW_MAX_SIDE),
        supersample: 1,
        stride: model.mesh.triangle_count().div_ceil(PREVIEW_MAX_TRIANGLES).max(quality.stride).max(1),
        threads: preview_threads(),
        ..*look
    };
    Ok(assetcraft_mesh::render_with(&model.mesh, &model.textures, view, &opts)?)
}

/// Where a [`ModelView`] is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelState {
    /// Reading and parsing the file.
    Loading,
    /// Loaded; frames can be requested.
    Ready {
        /// Triangles in the mesh.
        triangles: usize,
        /// Textures that decoded, out of the images the file has.
        textures: (usize, usize),
    },
    /// The file could not be loaded (the UI falls back to the thumbnail).
    Failed(ThumbError),
}

/// One finished preview frame.
#[derive(Debug, Clone)]
pub struct Frame {
    /// The camera it was drawn from.
    pub view: View,
    /// Whether it was a fast (interactive) frame: drawn at `scale` of the requested size, to
    /// be scaled up when shown.
    pub fast: bool,
    /// Side scale against the requested size (1 for full frames).
    pub scale: f32,
    /// The pixels.
    pub image: Image,
}

struct Job {
    seq: u64,
    view: View,
    size: (u32, u32),
    fast: bool,
    look: RenderOptions,
}

enum Event {
    Loaded(Result<(usize, (usize, usize)), ThumbError>),
    Frame(u64, Result<Frame, ThumbError>),
}

/// A loaded model on a background thread, rendering on request.
pub struct ModelView {
    /// Set on drop: a load still running stops before its next read or parse.
    cancel: Arc<AtomicBool>,
    jobs: Sender<Job>,
    events: Receiver<Event>,
    state: ModelState,
    requested: u64,
    finished: u64,
}

impl ModelView {
    /// Start loading `path` on a new thread. Dropping the view ends the thread: a load that
    /// hasn't read or parsed the file yet stops before doing so; a parse or render already
    /// running finishes first.
    pub fn open(services: Arc<dyn Services>, path: PathBuf) -> Self {
        let (jobs, job_rx) = mpsc::channel::<Job>();
        let (event_tx, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&cancel);
        let spawned = std::thread::Builder::new().name("assetcraft-model".into()).spawn(move || {
            let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if stop.load(Ordering::Acquire) {
                    return Err(ThumbError::Decode("cancelled".into()));
                }
                let probe = assetcraft_probe::probe(services.as_ref(), &path).map_err(ThumbError::Io)?;
                let mesh = load_unless(services.as_ref(), &probe, &stop)?;
                if stop.load(Ordering::Acquire) {
                    return Err(ThumbError::Decode("cancelled".into()));
                }
                let textures = decode_textures(&mesh);
                Ok(Model { mesh, textures })
            }))
            .unwrap_or_else(|_| Err(ThumbError::Decode("model loader panicked".into())));
            let model = match loaded {
                Ok(model) => {
                    let decoded = model.textures.iter().flatten().count();
                    let _ = event_tx
                        .send(Event::Loaded(Ok((model.mesh.triangle_count(), (decoded, model.mesh.images.len())))));
                    model
                }
                Err(e) => {
                    let _ = event_tx.send(Event::Loaded(Err(e)));
                    return;
                }
            };
            let mut quality = DragQuality::default();
            while let Ok(mut job) = job_rx.recv() {
                loop {
                    match job_rx.try_recv() {
                        Ok(newer) => job = newer,
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
                let Job { seq, view, size, fast, look } = job;
                let started = Instant::now();
                let scale = if fast { quality.scale } else { 1.0 };
                let frame = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if fast {
                        render_drag(&model, view, size.0, size.1, quality, &look)
                    } else {
                        render_view(&model, view, size.0, size.1, false, &look)
                    }
                }))
                .unwrap_or_else(|_| Err(ThumbError::Decode("model renderer panicked".into())))
                .map(|image| Frame { view, fast, scale, image });
                if fast {
                    quality.observe(started.elapsed(), DRAG_BUDGET);
                }
                if event_tx.send(Event::Frame(seq, frame)).is_err() {
                    return;
                }
            }
        });
        let state = match spawned {
            Ok(_) => ModelState::Loading,
            Err(e) => ModelState::Failed(ThumbError::Decode(format!("could not start the model thread: {e}"))),
        };
        Self { cancel, jobs, events, state, requested: 0, finished: 0 }
    }

    /// Ask for a frame of `width × height` from `view` with the default look (coalesced with
    /// earlier requests that haven't started).
    pub fn request(&mut self, view: View, width: u32, height: u32, fast: bool) {
        self.request_look(view, width, height, fast, &RenderOptions::new(width, height));
    }

    /// [`ModelView::request`] with a display mode, lighting and background.
    pub fn request_look(&mut self, view: View, width: u32, height: u32, fast: bool, look: &RenderOptions) {
        self.requested += 1;
        let job = Job { seq: self.requested, view, size: (width, height), fast, look: *look };
        if self.jobs.send(job).is_err() {
            self.finished = self.requested;
        }
    }

    /// Collect finished work; returns the newest frame, if one arrived.
    pub fn poll(&mut self) -> Option<Frame> {
        let mut newest = None;
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Loaded(Ok((triangles, textures))) => self.state = ModelState::Ready { triangles, textures },
                Event::Loaded(Err(e)) => {
                    self.state = ModelState::Failed(e);
                    self.finished = self.requested;
                }
                Event::Frame(seq, frame) => {
                    self.finished = self.finished.max(seq);
                    match frame {
                        Ok(frame) => newest = Some(frame),
                        Err(e) => self.state = ModelState::Failed(e),
                    }
                }
            }
        }
        if matches!(self.state, ModelState::Failed(_)) {
            self.finished = self.requested;
        }
        newest
    }

    /// The load state as of the last [`ModelView::poll`].
    pub fn state(&self) -> &ModelState {
        &self.state
    }

    /// `true` while loading or while a requested frame hasn't arrived.
    pub fn is_busy(&self) -> bool {
        self.state == ModelState::Loading || self.finished < self.requested
    }
}

impl Drop for ModelView {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
