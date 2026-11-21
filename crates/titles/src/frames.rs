//! Design frames for the Viewer (theme-packs.md TP2b): rendered on background threads, kept in
//! a size-capped cache, read ahead while playing, so a design (Stack's drop costs 40–75 ms a
//! frame at 640 × 360, R-T5) never stalls the UI. Mirrors `assetcraft_media::VideoFrames`: a
//! lookup never waits; while a frame renders it hands back the nearest one at or before it.
//!
//! The service renders any [`FrameJob`]: a design filled one way ([`DesignJob`]), or a whole
//! title drawn by the compositor (L3, `assetcraft_animatic::TitleJob`).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

use crate::{Design, Fill};

/// Frames a second the Viewer's design frames are cut at.
pub const FPS: f64 = 30.0;
/// How many frames past the one asked are queued (a third of a second).
pub const AHEAD: u32 = 10;
/// The cache's cap, in bytes of pixels.
pub const CACHE_BYTES: usize = 128 << 20;
/// Queued renders at most (older read-ahead goes first).
const MAX_QUEUE: usize = 96;

/// One design filled one way: what a clip draws. `key` names it in the cache (the caller's
/// hash of the design's file, its role and the fill).
#[derive(Debug, Clone)]
pub struct DesignJob {
    /// The cache key.
    pub key: u64,
    /// The design.
    pub design: Arc<Design>,
    /// Its fill.
    pub fill: Fill,
}

/// Something the service renders a frame at a time: its cache key, its last frame, and one
/// frame `width` pixels wide. Called from the render threads.
pub trait FrameJob: Send + Sync {
    /// The cache key: two jobs with one key draw the same frames.
    fn key(&self) -> u64;
    /// The last frame number (frames past it hold it; [`REST`] stands apart).
    fn last_index(&self) -> u32;
    /// Frame `index` (at [`FPS`]; [`REST`]: at rest), `width` pixels wide; `Err` when it can't
    /// be drawn at all.
    fn render(&self, index: u32, width: u32) -> Result<DesignFrame, String>;
}

/// The frame number of a title at rest (paused on it, not playing).
pub const REST: u32 = u32::MAX;

impl FrameJob for DesignJob {
    fn key(&self) -> u64 {
        self.key
    }
    fn last_index(&self) -> u32 {
        frame_index(&self.design, f64::MAX)
    }
    fn render(&self, index: u32, width: u32) -> Result<DesignFrame, String> {
        let (dw, _) = self.design.size();
        let scale = f64::from(width) / f64::from(dw.max(1));
        let f = self.design.render(&self.fill, f64::from(index) / FPS, scale).map_err(|e| e.to_string())?;
        Ok(DesignFrame { index, width: f.width, height: f.height, rgba: f.rgba, problem: None })
    }
}

/// A rendered frame: frame `index` (at [`FPS`]) at `width` pixels wide, straight RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignFrame {
    /// The frame number at [`FPS`].
    pub index: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` bytes.
    pub rgba: Vec<u8>,
    /// Drawn, but not all as made (a title whose design failed shows its theme's words), and why.
    pub problem: Option<String>,
}

/// What [`DesignFrames::frame`] has for the moment.
#[derive(Debug, Clone)]
pub enum DesignLookup {
    /// The asked frame.
    Ready(Arc<DesignFrame>),
    /// Rendering; the nearest frame at or before it so far, if any, to show meanwhile.
    Pending(Option<Arc<DesignFrame>>),
    /// The design can't be drawn, and why.
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Slot {
    key: u64,
    index: u32,
    width: u32,
}

#[derive(Default)]
struct State {
    cache: HashMap<Slot, Arc<DesignFrame>>,
    order: VecDeque<Slot>,
    bytes: usize,
    queue: VecDeque<(Slot, Arc<dyn FrameJob>)>,
    busy: Vec<Slot>,
    failed: HashMap<u64, String>,
    stop: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        // A worker that panicked mid-render leaves plain data; carry on with it.
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The render service. Dropping it stops its threads.
pub struct DesignFrames {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for DesignFrames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.shared.lock();
        f.debug_struct("DesignFrames").field("workers", &self.workers.len()).field("cached", &s.cache.len()).finish()
    }
}

impl Default for DesignFrames {
    fn default() -> Self {
        Self::new()
    }
}

/// The frame number shown at `secs` (clamped to the design's last frame).
pub fn frame_index(design: &Design, secs: f64) -> u32 {
    let last = ((design.seconds() * FPS).ceil() as u32).saturating_sub(1);
    let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
    ((secs * FPS).floor() as u32).min(last)
}

impl DesignFrames {
    /// A service with up to four render threads (every core but the UI's).
    pub fn new() -> Self {
        let n = std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1)).clamp(1, 4);
        Self::with_workers(n)
    }

    /// A service with `n` render threads (at least one).
    pub fn with_workers(n: usize) -> Self {
        let shared = Arc::new(Shared { state: Mutex::new(State::default()), wake: Condvar::new() });
        let workers = (0..n.max(1))
            .filter_map(|k| {
                let for_worker = Arc::clone(&shared);
                std::thread::Builder::new().name(format!("design-frames-{k}")).spawn(move || work(&for_worker)).ok()
            })
            .collect();
        Self { shared, workers }
    }

    /// `job`'s frame at `secs`, `width` pixels wide; never waits. Asking queues it first and
    /// the next [`AHEAD`] frames after it.
    pub fn frame(&self, job: &Arc<DesignJob>, secs: f64, width: u32) -> DesignLookup {
        let index = frame_index(&job.design, secs);
        let job: Arc<dyn FrameJob> = Arc::clone(job) as Arc<dyn FrameJob>;
        self.frame_of(&job, index, width)
    }

    /// `job`'s frame `index` (at [`FPS`], or [`REST`]), `width` pixels wide; never waits.
    /// Asking queues it first and the next [`AHEAD`] frames after it.
    pub fn frame_of(&self, job: &Arc<dyn FrameJob>, index: u32, width: u32) -> DesignLookup {
        if self.workers.is_empty() {
            return DesignLookup::Failed("the design renderer could not start".into());
        }
        let width = width.clamp(16, 4096);
        let last = job.last_index();
        let index = if index == REST { REST } else { index.min(last) };
        let key = job.key();
        let mut s = self.shared.lock();
        if let Some(why) = s.failed.get(&key) {
            return DesignLookup::Failed(why.clone());
        }
        let want = Slot { key, index, width };
        if let Some(f) = s.cache.get(&want) {
            return DesignLookup::Ready(Arc::clone(f));
        }
        // The asked frame first, then the read-ahead; a slot already cached, queued or busy is
        // left alone.
        let mut fresh: Vec<Slot> = std::iter::once(want)
            .chain((index.saturating_add(1)..=index.saturating_add(AHEAD).min(last)).map(|i| Slot { index: i, ..want }))
            .filter(|slot| !s.cache.contains_key(slot) && !s.busy.contains(slot))
            .collect();
        s.queue.retain(|(slot, _)| !fresh.contains(slot));
        for slot in fresh.drain(..).rev() {
            s.queue.push_front((slot, Arc::clone(job)));
        }
        s.queue.truncate(MAX_QUEUE);
        let near = s
            .cache
            .iter()
            .filter(|(slot, _)| slot.key == key && slot.width == width && slot.index <= index)
            .max_by_key(|(slot, _)| slot.index)
            .map(|(_, f)| Arc::clone(f));
        drop(s);
        self.shared.wake.notify_all();
        DesignLookup::Pending(near)
    }

    /// Why the design under `key` couldn't render, if it failed.
    pub fn failure(&self, key: u64) -> Option<String> {
        self.shared.lock().failed.get(&key).cloned()
    }

    /// How many render threads are running.
    pub fn workers(&self) -> usize {
        self.workers.len()
    }
}

impl Drop for DesignFrames {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_all();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

fn work(shared: &Shared) {
    loop {
        let (slot, job) = {
            let mut s = shared.lock();
            loop {
                if s.stop {
                    return;
                }
                if let Some(next) = s.queue.pop_front() {
                    s.busy.push(next.0);
                    break next;
                }
                s = shared.wake.wait(s).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        // A job's own panic is contained: the slot is marked failed, the thread carries on.
        let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job.render(slot.index, slot.width)))
            .unwrap_or_else(|_| Err("the renderer stopped on this frame".into()));
        let mut s = shared.lock();
        s.busy.retain(|b| *b != slot);
        match rendered {
            Ok(f) => {
                let frame = Arc::new(f);
                s.bytes += frame.rgba.len();
                s.cache.insert(slot, frame);
                s.order.push_back(slot);
                while s.bytes > CACHE_BYTES {
                    let Some(old) = s.order.pop_front() else { break };
                    if let Some(f) = s.cache.remove(&old) {
                        s.bytes = s.bytes.saturating_sub(f.rgba.len());
                    }
                }
            }
            Err(e) => {
                s.failed.insert(slot.key, e);
                let key = slot.key;
                s.queue.retain(|(q, _)| q.key != key);
            }
        }
    }
}
