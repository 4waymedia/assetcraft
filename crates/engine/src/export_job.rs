//! Export the Cut as a video (SF5-4; route D-SF6: pure Rust, `crates/animatic` draws the frames,
//! `crates/video` encodes H.264 + AAC-LC and writes MP4 / MOV).
//!
//! - `storyboard.exportVideo {path, size?, fps?, quality?, overwrite?, dryRun?, wait?}` checks
//!   everything it can before starting (an open storyboard with clips, an absolute `.mp4` /
//!   `.mov` path, a different existing file only with `overwrite`, the size / fps / quality
//!   ranges), then renders on one background thread: every frame of the Cut in order through
//!   the compositor into the encoder, then the song through the board's audio edit, then the
//!   file — written as `.<name>.<pid>.tmp` beside the target and renamed over it, so a cancel
//!   or a crash never leaves a half file under the real name. `dryRun` returns the plan (frames,
//!   seconds, size, an estimate of the bytes) and writes nothing. `wait` blocks until the job
//!   ends (the CLI and tests); the app polls.
//! - `storyboard.exportStatus {}` → the job's progress; `storyboard.exportCancel {}` stops it
//!   at the next frame and removes the temp file.
//!
//! One export at a time. Assets that fail to decode are reported in the result (the clip shows
//! empty, as in the Viewer) rather than stopping the export.

use crate::{CmdError, CmdResult, Session};
use assetcraft_animatic::Compositor;
use assetcraft_audio::program::Program;
use assetcraft_storyboard::{Storyboard, TICKS_PER_SECOND, Ticks};
use assetcraft_video::{AudioTrack, Container, DEFAULT_AUDIO_BITRATE, DEFAULT_QP, Settings, VideoEncoder};
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Output sizes by name (`size`), 16:9.
pub const SIZES: [(&str, u32, u32); 3] = [("720p", 1280, 720), ("1080p", 1920, 1080), ("4k", 3840, 2160)];
/// Frame rates accepted (`fps`).
pub const FRAME_RATES: [u32; 5] = [24, 25, 30, 50, 60];
/// Quantizer range accepted (`quality`; lower is better and bigger).
pub const QUALITY_RANGE: (u8, u8) = (16, 36);
/// Longest Cut exported, in seconds (the encoder's frame cap at 60 fps is higher).
pub const MAX_SECONDS: f64 = 4.0 * 60.0 * 60.0;
/// Rough bytes per second of 1080p24 at QP 24 before any export has run. The spike's grainy
/// plate gave 1.1 MB/s; Paul's first real Cut (2026-11-06: stills, cards, slow moves) came out
/// 8× smaller than that, so this starts low and [`Exporter::rate`] learns from each export.
const ESTIMATE_BYTES_PER_SEC_1080P24: f64 = 150_000.0;
/// Audio is encoded at this rate at most (AAC-LC's common ceiling).
const MAX_AUDIO_RATE: u32 = 48_000;

/// Where an export is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportStage {
    /// No export has run.
    Idle,
    /// Drawing and encoding frames.
    Rendering,
    /// Decoding and encoding the song.
    Audio,
    /// Writing the file.
    Writing,
    /// The file is there.
    Done,
    /// Stopped by `storyboard.exportCancel`.
    Cancelled,
    /// Stopped by an error (`message` says why).
    Failed,
}

/// The job's progress, as the UI and `storyboard.exportStatus` see it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    /// Where it is.
    pub stage: ExportStage,
    /// Frames encoded so far.
    pub frames_done: usize,
    /// Frames in all.
    pub frames_total: usize,
    /// The file being written.
    pub path: Option<PathBuf>,
    /// Bytes written, once done.
    pub bytes: Option<u64>,
    /// Seconds of the export's wall-clock time so far / in all.
    pub elapsed_secs: f64,
    /// Assets that would not decode (`path: reason`), reported once done.
    pub problems: Vec<String>,
    /// What to tell the user (an error, or "Exported …").
    pub message: String,
}

impl Default for ExportProgress {
    fn default() -> Self {
        Self {
            stage: ExportStage::Idle,
            frames_done: 0,
            frames_total: 0,
            path: None,
            bytes: None,
            elapsed_secs: 0.0,
            problems: Vec::new(),
            message: String::new(),
        }
    }
}

impl ExportProgress {
    /// Whether a job is running.
    pub fn running(&self) -> bool {
        matches!(self.stage, ExportStage::Rendering | ExportStage::Audio | ExportStage::Writing)
    }

    /// `0..=1` through the frames (audio and writing count as the last few percent).
    pub fn fraction(&self) -> f32 {
        match self.stage {
            ExportStage::Rendering if self.frames_total > 0 => {
                0.95 * self.frames_done as f32 / self.frames_total as f32
            }
            ExportStage::Audio => 0.96,
            ExportStage::Writing => 0.98,
            ExportStage::Done => 1.0,
            _ => 0.0,
        }
    }
}

/// The background exporter: one job at a time.
#[derive(Debug, Default)]
pub(crate) struct Exporter {
    progress: Arc<Mutex<ExportProgress>>,
    cancel: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Video bytes per second of 1080p24-equivalent at QP 24, measured on the last export that
    /// finished, so the next estimate reflects this user's pictures.
    rate: Arc<Mutex<Option<f64>>>,
}

fn lock(p: &Mutex<ExportProgress>) -> std::sync::MutexGuard<'_, ExportProgress> {
    p.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Exporter {
    fn progress(&self) -> ExportProgress {
        lock(&self.progress).clone()
    }

    fn running(&self) -> bool {
        lock(&self.progress).running()
    }

    /// The learned rate, if an export has finished.
    fn rate(&self) -> Option<f64> {
        *self.rate.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Session {
    /// The export job's progress (idle when none has run).
    pub fn export_progress(&self) -> ExportProgress {
        self.exporter.progress()
    }

    pub(crate) fn exporter_running(&self) -> bool {
        self.exporter.running()
    }
}

/// What one export does, checked before it starts.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportPlan {
    /// Bytes per second of 1080p24 at QP 24 the estimate uses (learned, or the default).
    pub rate: f64,
    /// The file.
    pub path: PathBuf,
    /// `.mp4` or `.mov`, from the extension.
    pub container: Container,
    /// Frame size, rate and quantizer.
    pub settings: Settings,
    /// Frames to render.
    pub frames: usize,
    /// Seconds of the Cut.
    pub seconds: f64,
    /// Whether the board has a song to lay under it.
    pub audio: bool,
}

impl ExportPlan {
    /// How the settings scale the 1080p24 / QP 24 rate: pixels, the square root of the frame
    /// rate (frames repeat more at higher rates), a QP step of 6 halving the size.
    fn scale(&self) -> f64 {
        let px = f64::from(self.settings.width) * f64::from(self.settings.height) / (1920.0 * 1080.0);
        let rate = f64::from(self.settings.fps) / 24.0;
        let qp = 2f64.powf((24.0 - f64::from(self.settings.qp)) / 6.0);
        px * rate.sqrt() * qp
    }

    /// A rough file size: the learned (or default) rate scaled by the settings, plus the song.
    pub fn estimated_bytes(&self) -> u64 {
        (self.rate * self.scale() * self.seconds + if self.audio { 20_000.0 * self.seconds } else { 0.0 }) as u64
    }

    fn json(&self) -> Value {
        json!({
            "path": self.path.to_string_lossy(),
            "container": self.container.extension(),
            "width": self.settings.width,
            "height": self.settings.height,
            "fps": self.settings.fps,
            "quality": self.settings.qp,
            "frames": self.frames,
            "seconds": self.seconds,
            "audio": self.audio,
            "estimatedBytes": self.estimated_bytes(),
        })
    }
}

/// Read and check the parameters against the open board.
pub(crate) fn plan(s: &Session, p: &Value) -> Result<ExportPlan, CmdError> {
    let board = s.boards.board.as_ref().ok_or_else(crate::storyboard::no_board)?;
    if board.clips.is_empty() {
        return Err(CmdError::Unavailable("the storyboard has no clips to export".into()));
    }
    let text = p
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty() && t.len() <= 4096)
        .ok_or_else(|| CmdError::BadParams("need `path` (a .mp4 or .mov file)".into()))?;
    let path = PathBuf::from(text);
    if !path.is_absolute() {
        return Err(CmdError::BadParams("`path` must be absolute".into()));
    }
    let container = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("mp4") => Container::Mp4,
        Some("mov") => Container::Mov,
        _ => return Err(CmdError::BadParams("`path` must end with .mp4 or .mov".into())),
    };
    let (width, height) = match p.get("size") {
        None | Some(Value::Null) => (1920, 1080),
        Some(Value::String(name)) => SIZES
            .iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, w, h)| (*w, *h))
            .ok_or_else(|| CmdError::BadParams("`size` must be 720p, 1080p or 4k, or {width, height}".into()))?,
        Some(Value::Object(o)) => {
            let side = |k: &str| o.get(k).and_then(Value::as_u64).and_then(|v| u32::try_from(v).ok());
            match (side("width"), side("height")) {
                (Some(w), Some(h)) => (w, h),
                _ => return Err(CmdError::BadParams("`size` needs `width` and `height`".into())),
            }
        }
        Some(_) => return Err(CmdError::BadParams("`size` must be a name or {width, height}".into())),
    };
    let fps = match p.get("fps") {
        None | Some(Value::Null) => 24,
        Some(v) => v
            .as_u64()
            .and_then(|f| u32::try_from(f).ok())
            .filter(|f| FRAME_RATES.contains(f))
            .ok_or_else(|| CmdError::BadParams("`fps` must be 24, 25, 30, 50 or 60".into()))?,
    };
    let qp = match p.get("quality") {
        None | Some(Value::Null) => DEFAULT_QP,
        Some(v) => v
            .as_u64()
            .and_then(|q| u8::try_from(q).ok())
            .filter(|q| (QUALITY_RANGE.0..=QUALITY_RANGE.1).contains(q))
            .ok_or_else(|| {
                CmdError::BadParams(format!(
                    "`quality` must be {}..={} (lower is better)",
                    QUALITY_RANGE.0, QUALITY_RANGE.1
                ))
            })?,
    };
    let settings = Settings { width, height, fps, qp };
    settings.check().map_err(|e| CmdError::BadParams(e.to_string()))?;
    let seconds = board.total().as_secs_f64();
    if seconds <= 0.0 {
        return Err(CmdError::Unavailable("the Cut has no length".into()));
    }
    if seconds > MAX_SECONDS {
        return Err(CmdError::BadParams(format!("the Cut is longer than {} hours", MAX_SECONDS / 3600.0)));
    }
    let frames = (seconds * f64::from(fps)).ceil() as usize;
    let audio = board.track.as_ref().is_some_and(|t| Path::new(&t.id.0).is_absolute()) || !voices(board).is_empty();
    let rate = s.exporter.rate().unwrap_or(ESTIMATE_BYTES_PER_SEC_1080P24);
    Ok(ExportPlan { rate, path, container, settings, frames, seconds, audio })
}

fn flag(p: &Value, name: &str) -> bool {
    p.get(name).and_then(Value::as_bool).unwrap_or(false)
}

/// `storyboard.exportVideo`.
pub(crate) fn sb_export_video(s: &mut Session, p: &Value) -> CmdResult {
    let plan = plan(s, p)?;
    if flag(p, "dryRun") {
        let mut out = plan.json();
        if let Value::Object(map) = &mut out {
            map.insert("dryRun".into(), json!(true));
            map.insert(
                "message".into(),
                json!(format!(
                    "Would export {} frames ({:.1} s) to {}",
                    plan.frames,
                    plan.seconds,
                    plan.path.display()
                )),
            );
        }
        return Ok(out);
    }
    if s.exporter.running() {
        return Err(CmdError::Unavailable("an export is already running".into()));
    }
    if !flag(p, "overwrite") && std::fs::symlink_metadata(&plan.path).is_ok() {
        return Err(CmdError::Unavailable(format!(
            "{} exists; pass `overwrite: true` to replace it",
            plan.path.display()
        )));
    }
    let Some(board) = s.boards.board.clone() else { return Err(crate::storyboard::no_board()) };
    let job = Job {
        board,
        plan: plan.clone(),
        services: Arc::clone(&s.services),
        look: s.model.render_look(),
        decode: s.decode_options(),
        progress: Arc::clone(&s.exporter.progress),
        cancel: Arc::clone(&s.exporter.cancel),
        rate: Arc::clone(&s.exporter.rate),
    };
    s.exporter.cancel.store(false, Ordering::SeqCst);
    *lock(&s.exporter.progress) = ExportProgress {
        stage: ExportStage::Rendering,
        frames_total: plan.frames,
        path: Some(plan.path.clone()),
        message: format!("Exporting {} frames…", plan.frames),
        ..ExportProgress::default()
    };
    if let Some(old) = s.exporter.thread.take() {
        let _ = old.join();
    }
    let handle = std::thread::Builder::new().name("assetcraft-export".into()).spawn(move || job.run());
    match handle {
        Ok(h) => s.exporter.thread = Some(h),
        Err(e) => {
            let mut pr = lock(&s.exporter.progress);
            pr.stage = ExportStage::Failed;
            pr.message = format!("could not start the export: {e}");
            return Err(CmdError::Unavailable(pr.message.clone()));
        }
    }
    if flag(p, "wait")
        && let Some(h) = s.exporter.thread.take()
    {
        let _ = h.join();
    }
    let progress = s.exporter.progress();
    let mut out = plan.json();
    if let Value::Object(map) = &mut out {
        map.insert("started".into(), json!(true));
        map.insert("status".into(), serde_json::to_value(&progress).unwrap_or(Value::Null));
        map.insert("message".into(), json!(progress.message));
    }
    if progress.stage == ExportStage::Failed {
        return Err(CmdError::Unavailable(progress.message));
    }
    Ok(out)
}

/// `storyboard.exportStatus`.
pub(crate) fn sb_export_status(s: &mut Session, _p: &Value) -> CmdResult {
    let progress = s.exporter.progress();
    if !progress.running()
        && let Some(h) = s.exporter.thread.take()
    {
        let _ = h.join();
    }
    serde_json::to_value(&progress).map_err(|e| CmdError::Unavailable(e.to_string()))
}

/// `storyboard.exportCancel`.
pub(crate) fn sb_export_cancel(s: &mut Session, _p: &Value) -> CmdResult {
    if !s.exporter.running() {
        return Err(CmdError::Unavailable("no export is running".into()));
    }
    s.exporter.cancel.store(true, Ordering::SeqCst);
    Ok(json!({ "cancelled": true, "message": "Stopping the export…" }))
}

/// The `export` field of `storyboard.state`.
pub(crate) fn status_json(s: &Session) -> Value {
    serde_json::to_value(s.exporter.progress()).unwrap_or(Value::Null)
}

/// Everything the worker thread needs; the session is not touched.
struct Job {
    board: Storyboard,
    plan: ExportPlan,
    services: Arc<dyn assetcraft_fs::Services>,
    look: assetcraft_mesh::RenderOptions,
    decode: assetcraft_thumbs::DecodeOptions,
    progress: Arc<Mutex<ExportProgress>>,
    cancel: Arc<AtomicBool>,
    rate: Arc<Mutex<Option<f64>>>,
}

enum Stop {
    Cancelled,
    Failed(String),
}

impl Job {
    fn run(self) {
        let started = std::time::Instant::now();
        let outcome = self.render(started);
        let mut pr = lock(&self.progress);
        pr.elapsed_secs = started.elapsed().as_secs_f64();
        match outcome {
            Ok((bytes, problems)) => {
                // Learn this user's rate for the next estimate (video bytes only).
                let audio_bytes = if self.plan.audio { 20_000.0 * self.plan.seconds } else { 0.0 };
                let scale = self.plan.scale() * self.plan.seconds;
                if scale > 0.0 && bytes as f64 > audio_bytes {
                    *self.rate.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some((bytes as f64 - audio_bytes) / scale);
                }
                pr.stage = ExportStage::Done;
                pr.bytes = Some(bytes);
                pr.frames_done = pr.frames_total;
                pr.problems = problems;
                let note = if pr.problems.is_empty() {
                    String::new()
                } else {
                    format!(" ({} asset(s) could not be drawn)", pr.problems.len())
                };
                pr.message =
                    format!("Exported {} ({:.1} MiB){note}", self.plan.path.display(), bytes as f64 / 1048576.0);
            }
            Err(Stop::Cancelled) => {
                pr.stage = ExportStage::Cancelled;
                pr.message = "Export cancelled".into();
            }
            Err(Stop::Failed(why)) => {
                pr.stage = ExportStage::Failed;
                pr.message = why;
            }
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    fn render(&self, started: std::time::Instant) -> Result<(u64, Vec<String>), Stop> {
        let s = self.plan.settings;
        let mut compositor = Compositor::new(Arc::clone(&self.services), s.width, s.height, self.look, self.decode)
            .map_err(|e| Stop::Failed(e.to_string()))?;
        let mut encoder = VideoEncoder::new(s).map_err(|e| Stop::Failed(e.to_string()))?;
        // Exact for every rate in `FRAME_RATES` (the tick rate is divisible by 24, 25, 30, 50, 60).
        let tick = Ticks(TICKS_PER_SECOND / u64::from(s.fps.max(1)));
        for i in 0..self.plan.frames {
            if self.cancelled() {
                return Err(Stop::Cancelled);
            }
            let at = Ticks(tick.0.saturating_mul(i as u64));
            let frame = compositor.frame(&self.board, at);
            encoder.push_rgba(&frame.data).map_err(|e| Stop::Failed(e.to_string()))?;
            let mut pr = lock(&self.progress);
            pr.frames_done = i + 1;
            pr.elapsed_secs = started.elapsed().as_secs_f64();
        }
        let video = encoder.finish().map_err(|e| Stop::Failed(e.to_string()))?;
        if self.cancelled() {
            return Err(Stop::Cancelled);
        }
        lock(&self.progress).stage = ExportStage::Audio;
        let (audio, sound_problems) = self.audio()?;
        if self.cancelled() {
            return Err(Stop::Cancelled);
        }
        lock(&self.progress).stage = ExportStage::Writing;
        let bytes = write_atomic(&self.plan.path, |out| {
            assetcraft_video::mux(&video, audio.as_ref(), self.plan.container, out)
        })
        .map_err(Stop::Failed)?;
        let mut problems: Vec<String> =
            compositor.problems().into_iter().map(|p| format!("{}: {}", p.path.display(), p.reason)).collect();
        problems.extend(sound_problems);
        Ok((bytes, problems))
    }

    /// The Cut's sound, as long as the Cut: the song through the board's audio edit, lowered
    /// under every section whose inset video has "Mix audio in" to that section's Music level,
    /// with the video's own sound over it at its Voice level (SR4). `None` with neither.
    fn audio(&self) -> Result<(Option<AudioTrack>, Vec<String>), Stop> {
        let music = match self.board.track.as_ref().filter(|t| Path::new(&t.id.0).is_absolute()) {
            Some(track) => {
                let path = Path::new(&track.id.0);
                let decoded = assetcraft_audio::load(self.services.as_ref(), path)
                    .map_err(|e| Stop::Failed(format!("{}: {e}", path.display())))?;
                let edit = track.edit.as_ref().map(|e| assetcraft_audio::program::Edit {
                    rate: e.sample_rate,
                    segments: e.segments.iter().map(|[a, b]| (*a, *b)).collect(),
                    crossfade: e.crossfade_frames,
                    fade_out: e.fade_out.map(|[s, n]| (s, n)),
                    length: e.length_frames,
                });
                Some(Program::new(Arc::new(decoded), edit.as_ref()))
            }
            None => None,
        };
        let voices = voices(&self.board);
        if music.is_none() && voices.is_empty() {
            return Ok((None, Vec::new()));
        }
        let mut problems = Vec::new();
        let rate = music.as_ref().map_or(48_000, Program::rate).clamp(8_000, MAX_AUDIO_RATE);
        let frames = (self.plan.seconds * f64::from(rate)).round() as usize;
        let mut pcm = vec![0.0f32; frames * 2];
        if let Some(program) = &music {
            program.render(0.0, rate, &mut pcm, 2);
            // Each section ducks to its own level: one pass per distinct level keeps it simple.
            for v in &voices {
                assetcraft_audio::mix::duck(
                    &mut pcm,
                    2,
                    rate,
                    &[(v.start, v.length)],
                    assetcraft_audio::mix::db(f64::from(v.music_db)),
                );
            }
        }
        for v in &voices {
            // A video whose sound won't decode is reported, not fatal: the Cut still exports.
            match assetcraft_audio::load(self.services.as_ref(), Path::new(&v.path)) {
                Ok(sound) => assetcraft_audio::mix::add_voice(
                    &mut pcm,
                    2,
                    rate,
                    &sound,
                    v.start,
                    v.length,
                    assetcraft_audio::mix::db(f64::from(v.voice_db)),
                ),
                Err(e) => problems.push(format!("{}: no sound mixed in ({e})", v.path)),
            }
        }
        assetcraft_video::encode_audio(&pcm, 2, rate, DEFAULT_AUDIO_BITRATE)
            .map(|a| (Some(a), problems))
            .map_err(|e| Stop::Failed(e.to_string()))
    }
}

/// Write through `fill` to `.<name>.<pid>.tmp` beside `path`, flush, rename over `path`.
fn write_atomic(
    path: &Path,
    fill: impl FnOnce(&mut dyn Write) -> Result<u64, assetcraft_video::VideoError>,
) -> Result<u64, String> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).ok_or_else(|| "no folder".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(|e| e.to_string())?;
        let mut out = std::io::BufWriter::new(file);
        let bytes = fill(&mut out).map_err(|e| e.to_string())?;
        let file = out.into_inner().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        Ok(bytes)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(|e: String| format!("could not write {}: {e}", path.display()))
}

/// A section's inset sound to mix in (SR4): the file, where its section's shots run in the Cut,
/// and the two levels. Shared with Watch your Cut (`audio_out`).
pub(crate) struct Voice {
    pub(crate) path: String,
    pub(crate) start: f64,
    pub(crate) length: f64,
    pub(crate) music_db: i8,
    pub(crate) voice_db: i8,
}

/// Every section whose inset is a video with "Mix audio in" and a rooted path (`has_root`, as
/// the track check in `audio_out`: on Windows a path like `/clips/a.mp4` has a root but is not
/// "absolute").
pub(crate) fn voices(board: &Storyboard) -> Vec<Voice> {
    board
        .scenes
        .iter()
        .filter_map(|sc| {
            let i = sc.inset.as_ref()?;
            let a = i.asset.as_ref()?;
            if !i.mix_audio || i.kind != assetcraft_storyboard::InsetKind::Video || !Path::new(&a.0).has_root() {
                return None;
            }
            let (start, length) = board.inset_window(sc.id)?;
            Some(Voice {
                path: a.0.clone(),
                start: start.as_secs_f64(),
                length: length.as_secs_f64(),
                music_db: i.music_db,
                voice_db: i.voice_db,
            })
        })
        .collect()
}
