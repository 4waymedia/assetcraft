//! Video frames from media files (P1; Paul, 2026-11-15: "mp4 videos do not display/play in the
//! preview, so I cannot properly edit the timeline"; spike S3, `docs/design/cut-editing.md` §4.2).
//!
//! Layer 2: reads files and runs threads, one per core but one (P2). [`VideoFrames`] decodes
//! through FilmCraft's own
//! codecs (`filmcraft-codecs`, pinned by tag in the workspace), opening a file as FilmCraft does
//! (`FileReader` + `open_reader` with the codecs' openers), so AssetCraft plays what FilmCraft
//! plays. A seek decodes from the previous keyframe (up to a GOP), so the caller never waits:
//!
//! - [`VideoFrames::frame`] answers at once — the frame if it is cached, otherwise the nearest
//!   cached frame of that file while the worker decodes the asked one ([`Lookup`]);
//! - the newest request wins, so a fast scrub decodes where the playhead is, not everywhere it
//!   has been;
//! - frames are shrunk to the asked size (the sources return full size whatever is asked) and
//!   converted to RGBA on the worker, then kept in a cache ([`CACHE_BYTES`]);
//! - while playing, [`VideoFrames::ahead`] queues the coming seconds in chunks that the other
//!   workers decode in parallel, each with its own opened copy of the file (P2), so the frames
//!   are ready before the playhead gets there.
//!
//! Disk input is hostile: a file that won't open or decode is reported once, as [`Lookup::Failed`].

#![forbid(unsafe_code)]

use filmcraft_media::{FrameRequest, SharedSource};
use filmcraft_time::Tick;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

/// How many decoded frames' bytes the cache keeps (RGBA, preview-sized): a few seconds ahead of
/// the playhead at Viewer size (P2).
pub const CACHE_BYTES: usize = 384 * 1024 * 1024;
/// How many files each worker keeps open.
const OPEN_FILES: usize = 4;
/// Read-ahead is queued in chunks this long, so idle workers decode them in parallel (P2).
pub const CHUNK_SECS: f64 = 1.0;
/// A chunk decoded this recently isn't queued again.
const DONE_FOR: std::time::Duration = std::time::Duration::from_secs(5);

/// A decoded frame, preview-sized RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` bytes, row by row.
    pub rgba: Vec<u8>,
    /// The frame's number in the file (its time × the frame rate).
    pub index: u64,
}

/// The answer to [`VideoFrames::frame`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// The asked frame.
    Ready(Arc<Frame>),
    /// Being decoded; the nearest frame of this file so far, if any, to show meanwhile.
    Pending(Option<Arc<Frame>>),
    /// The file can't be played, and why.
    Failed(String),
}

/// Extensions [`VideoFrames`] opens (what `filmcraft-codecs` reads).
pub const VIDEO_EXTENSIONS: &[&str] =
    &["mp4", "m4v", "mov", "mkv", "webm", "avi", "mts", "m2ts", "ts", "mxf", "mpg", "mpeg"];

/// Whether `path` names a video [`VideoFrames`] can play (by its extension).
pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.iter().any(|v| v.eq_ignore_ascii_case(e)))
}

/// One frame asked for now.
struct Want {
    path: PathBuf,
    secs: f64,
    side: u32,
}

/// A stretch of read-ahead: chunk `chunk` (of [`CHUNK_SECS`]) of `path`.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Chunk {
    path: PathBuf,
    chunk: u64,
    side: u32,
}

#[derive(Default)]
struct State {
    want: Option<Want>,
    ahead: VecDeque<Chunk>,
    /// Bumped when read-ahead is dropped (paused, scrubbing) or moves to another file: running
    /// chunks of an older epoch stop after their current frame.
    epoch: u64,
    ahead_path: Option<PathBuf>,
    busy: Vec<Chunk>,
    done: HashMap<Chunk, std::time::Instant>,
    /// `(path, frame index, size asked, frame)`, oldest first.
    cache: VecDeque<(PathBuf, u64, u32, Arc<Frame>)>,
    bytes: usize,
    /// Frames a second and the last frame's index, once a file is open.
    fps: HashMap<PathBuf, (f64, u64)>,
    failed: HashMap<PathBuf, String>,
    stop: bool,
}

impl State {
    fn cached(&self, path: &Path, index: u64, side: u32) -> Option<Arc<Frame>> {
        self.cache.iter().rev().find(|(p, i, sz, _)| p == path && *i == index && *sz >= side).map(|c| Arc::clone(&c.3))
    }

    fn store(&mut self, path: PathBuf, side: u32, frame: Frame) {
        if self.cached(&path, frame.index, side).is_some() {
            return;
        }
        self.bytes += frame.rgba.len();
        let index = frame.index;
        self.cache.push_back((path, index, side, Arc::new(frame)));
        while self.bytes > CACHE_BYTES
            && let Some((.., old)) = self.cache.pop_front()
        {
            self.bytes = self.bytes.saturating_sub(old.rgba.len());
        }
    }
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Video frames decoded on background threads (see the module docs): one per core but one, the
/// first always on the frame asked for now, the others reading ahead.
pub struct VideoFrames {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for VideoFrames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.shared.lock();
        f.debug_struct("VideoFrames")
            .field("workers", &self.workers.len())
            .field("cached", &s.cache.len())
            .field("failed", &s.failed.len())
            .finish()
    }
}

impl Default for VideoFrames {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode workers for this machine: every core but one (the UI's), at least one (P2; Paul,
/// 2026-11-15: "Utilize full cores available").
pub fn worker_count() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1)).max(1)
}

impl VideoFrames {
    /// A frame service with [`worker_count`] decode threads started.
    pub fn new() -> Self {
        Self::with_workers(worker_count())
    }

    /// A frame service with `n` decode threads (at least one).
    pub fn with_workers(n: usize) -> Self {
        let shared = Arc::new(Shared { state: Mutex::new(State::default()), wake: Condvar::new() });
        let workers = (0..n.max(1))
            .filter_map(|k| {
                let for_worker = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name(format!("video-frames-{k}"))
                    .spawn(move || work(&for_worker, k == 0))
                    .ok()
            })
            .collect();
        Self { shared, workers }
    }

    /// How many decode threads are running.
    pub fn workers(&self) -> usize {
        self.workers.len()
    }

    /// The frame of `path` shown at `secs` into the file, at most `max_side` pixels a side;
    /// never waits (see [`Lookup`]).
    pub fn frame(&self, path: &Path, secs: f64, max_side: u32) -> Lookup {
        if self.workers.is_empty() {
            return Lookup::Failed("the video decoder could not start".into());
        }
        let secs = clean(secs);
        let side = max_side.clamp(16, 4096);
        let mut s = self.shared.lock();
        if let Some(why) = s.failed.get(path) {
            return Lookup::Failed(why.clone());
        }
        let index = s.fps.get(path).map(|(fps, last)| frame_index(secs, *fps).min(*last));
        if let Some(index) = index
            && let Some(f) = s.cached(path, index, side)
        {
            return Lookup::Ready(f);
        }
        // A stand-in while it decodes: the nearest frame at or before the one asked (so playback
        // never shows a later frame and then steps back). Never a later one: a read-ahead chunk
        // can land before the frame asked for (Paul's Windows run, 2026-11-17: `[0, 0, 12, 0, …]`),
        // so with nothing earlier at hand the caller keeps what it shows. None until the file's
        // rate is known.
        let near = index.and_then(|want| {
            s.cache
                .iter()
                .filter(|(p, i, sz, _)| p == path && *sz >= side && *i <= want)
                .max_by_key(|(_, i, ..)| *i)
                .map(|c| Arc::clone(&c.3))
        });
        s.want = Some(Want { path: path.to_path_buf(), secs, side });
        drop(s);
        self.shared.wake.notify_all();
        Lookup::Pending(near)
    }

    /// Read ahead (P2, while playing): decode `seconds` of `path` from `from`, in
    /// [`CHUNK_SECS`] chunks the idle workers take in parallel, the nearest first. Replaces the
    /// last read-ahead (the playhead moved on); cheap to call every frame.
    pub fn ahead(&self, path: &Path, from: f64, seconds: f64, max_side: u32) {
        let (from, seconds) = (clean(from), clean(seconds).min(30.0));
        let side = max_side.clamp(16, 4096);
        let first = (from / CHUNK_SECS).floor() as u64;
        let last = ((from + seconds) / CHUNK_SECS).ceil() as u64;
        let mut s = self.shared.lock();
        if s.failed.contains_key(path) {
            return;
        }
        let now = std::time::Instant::now();
        s.done.retain(|_, at| now.duration_since(*at) < DONE_FOR);
        if s.ahead_path.as_deref() != Some(path) {
            s.epoch += 1;
            s.ahead.clear();
            s.ahead_path = Some(path.to_path_buf());
        }
        s.ahead.clear();
        for chunk in first..last.max(first + 1) {
            let c = Chunk { path: path.to_path_buf(), chunk, side };
            if !s.done.contains_key(&c) && !s.busy.contains(&c) {
                s.ahead.push_back(c);
            }
        }
        drop(s);
        self.shared.wake.notify_all();
    }

    /// Stop reading ahead (paused, or scrubbing).
    pub fn stop_ahead(&self) {
        let mut s = self.shared.lock();
        if s.ahead_path.is_some() {
            s.epoch += 1;
            s.ahead.clear();
            s.ahead_path = None;
        }
    }
}

impl Drop for VideoFrames {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_all();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

fn clean(secs: f64) -> f64 {
    if secs.is_finite() { secs.max(0.0) } else { 0.0 }
}

fn frame_index(secs: f64, fps: f64) -> u64 {
    (secs * fps).floor().max(0.0) as u64
}

enum Job {
    Now(Want),
    Ahead(Chunk, u64),
}

/// A worker: the first takes the frame asked for now before anything else; every worker reads
/// ahead when there is nothing asked. Each keeps its own open files (its own decoder), so workers
/// decode different stretches at once.
fn work(shared: &Shared, first: bool) {
    let mut open: Vec<(PathBuf, SharedSource)> = Vec::new();
    loop {
        let job = {
            let mut s = shared.lock();
            loop {
                if s.stop {
                    return;
                }
                if first && let Some(w) = s.want.take() {
                    break Job::Now(w);
                }
                if let Some(c) = s.ahead.pop_front() {
                    s.busy.push(c.clone());
                    break Job::Ahead(c, s.epoch);
                }
                // With one worker it serves the asked frame too; others leave it to the first.
                s = shared.wake.wait(s).unwrap_or_else(PoisonError::into_inner);
            }
        };
        match job {
            Job::Now(want) => {
                let result = source(&mut open, &want.path).and_then(|src| {
                    let fps = fps_of(&src)?;
                    let last = last_frame(&src, fps);
                    shared.lock().fps.insert(want.path.clone(), (fps, last));
                    decode(&src, frame_index(want.secs, fps).min(last), fps, want.side)
                });
                let mut s = shared.lock();
                match result {
                    Ok(frame) => s.store(want.path, want.side, frame),
                    Err(why) => {
                        s.failed.insert(want.path, why);
                    }
                }
            }
            Job::Ahead(c, epoch) => {
                let result = source(&mut open, &c.path).and_then(|src| {
                    let fps = fps_of(&src)?;
                    let last = last_frame(&src, fps);
                    shared.lock().fps.insert(c.path.clone(), (fps, last));
                    // Nothing past the video's end: those would be its last frame again under
                    // indexes it doesn't have.
                    let from = (c.chunk as f64 * CHUNK_SECS * fps).ceil() as u64;
                    let to = (((c.chunk + 1) as f64 * CHUNK_SECS * fps).ceil() as u64).min(last.saturating_add(1));
                    for index in from..to {
                        {
                            let s = shared.lock();
                            // Dropped (paused, scrubbing, another file) or shutting down: stop here.
                            if s.stop || s.epoch != epoch {
                                break;
                            }
                            if s.cached(&c.path, index, c.side).is_some() {
                                continue;
                            }
                        }
                        let frame = decode(&src, index, fps, c.side)?;
                        shared.lock().store(c.path.clone(), c.side, frame);
                    }
                    Ok(())
                });
                let mut s = shared.lock();
                s.busy.retain(|b| *b != c);
                match result {
                    Ok(()) if s.epoch == epoch => {
                        s.done.insert(c, std::time::Instant::now());
                    }
                    Ok(()) => {}
                    Err(why) => {
                        s.failed.insert(c.path, why);
                    }
                }
            }
        }
    }
}

/// `path` from this worker's open files, opened if need be (the oldest closed past
/// [`OPEN_FILES`]).
fn source(open: &mut Vec<(PathBuf, SharedSource)>, path: &Path) -> Result<SharedSource, String> {
    if let Some((_, src)) = open.iter().find(|(p, _)| p == path) {
        return Ok(Arc::clone(src));
    }
    let src = open_file(path)?;
    if open.len() >= OPEN_FILES {
        open.remove(0);
    }
    open.push((path.to_path_buf(), Arc::clone(&src)));
    Ok(src)
}

fn fps_of(src: &SharedSource) -> Result<f64, String> {
    src.info()
        .video
        .as_ref()
        .map(|v| v.frame_rate.as_f64())
        .filter(|f| f.is_finite() && *f > 0.0)
        .ok_or_else(|| "the file has no video".to_string())
}

/// The index of the last frame of `src` at `fps`, from its length (unbounded without one).
fn last_frame(src: &SharedSource, fps: f64) -> u64 {
    let secs = src.info().duration.seconds();
    if secs.is_finite() && secs > 0.0 { ((secs * fps).ceil() as u64).saturating_sub(1) } else { u64::MAX }
}

/// Frame `index` of `src`, shrunk to `side` and converted to RGBA.
fn decode(src: &SharedSource, index: u64, fps: f64, side: u32) -> Result<Frame, String> {
    // The middle of the frame's span, so rounding never lands on the one before.
    let at = Tick::from_seconds_f64((index as f64 + 0.5) / fps);
    let frame = src.video_frame(FrameRequest::full(at)).map_err(|e| e.to_string())?;
    let longest = frame.width.max(frame.height).max(1);
    let n = longest.div_ceil(side).max(1) as usize;
    let small = if n > 1 { frame.box_decimated(n).unwrap_or_else(|| (*frame).clone()) } else { (*frame).clone() };
    let rgba = small.to_rgba8();
    if rgba.len() != small.width as usize * small.height as usize * 4 {
        return Err("the decoder gave a frame of the wrong size".into());
    }
    Ok(Frame { width: small.width, height: small.height, rgba, index })
}

/// How long the video at `path` runs, in seconds, from its header (C6: a slip's limit). Opens
/// the file on the calling thread; quick, as no frame is decoded.
pub fn seconds(path: &Path) -> Result<f64, String> {
    let src = open_file(path)?;
    fps_of(&src)?;
    let secs = src.info().duration.seconds();
    if secs.is_finite() && secs > 0.0 { Ok(secs) } else { Err("the video has no length".into()) }
}

/// One video's frames read on the calling thread, in any order (P3: the export draws the Cut
/// frame by frame and waits for each). A time past the end is the last frame; the last frame
/// read is kept, so an export at a higher rate than the video doesn't decode a frame twice.
pub struct VideoReader {
    src: SharedSource,
    fps: f64,
    last: u64,
    held: Option<(u64, u32, Arc<Frame>)>,
}

impl std::fmt::Debug for VideoReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoReader").field("fps", &self.fps).field("last", &self.last).finish()
    }
}

impl VideoReader {
    /// Open the video at `path`.
    pub fn open(path: &Path) -> Result<Self, String> {
        let src = open_file(path)?;
        let fps = fps_of(&src)?;
        let last = last_frame(&src, fps);
        Ok(Self { src, fps, last, held: None })
    }

    /// The frame `secs` into the video (clamped to its start and end), its longest side at
    /// most about `side`.
    pub fn frame(&mut self, secs: f64, side: u32) -> Result<Arc<Frame>, String> {
        let index = frame_index(if secs.is_finite() { secs } else { 0.0 }, self.fps).min(self.last);
        if let Some((i, s, f)) = &self.held
            && *i == index
            && *s == side
        {
            return Ok(Arc::clone(f));
        }
        let f = Arc::new(decode(&self.src, index, self.fps, side.max(1))?);
        self.held = Some((index, side, Arc::clone(&f)));
        Ok(f)
    }
}

/// Open `path` as FilmCraft does: a pooled file reader and the codecs' openers.
fn open_file(path: &Path) -> Result<SharedSource, String> {
    let reader = filmcraft_media::reader::FileReader::open(path).map_err(|e| format!("can't read the file: {e}"))?;
    let reader: filmcraft_media::SharedReader = Arc::new(reader);
    let name = path.to_string_lossy();
    filmcraft_media::reader::open_reader(
        &name,
        reader,
        &filmcraft_codecs::reader_openers(),
        &filmcraft_codecs::openers(),
    )
    .map_err(|e| format!("can't play this file: {e}"))
}

#[cfg(test)]
mod tests;
