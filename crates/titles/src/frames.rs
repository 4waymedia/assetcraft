//! Design frames for the Viewer (theme-packs.md TP2b): rendered on background threads, kept in
//! a size-capped cache, read ahead while playing, so a design (Stack's drop costs 40–75 ms a
//! frame at 640 × 360, R-T5) never stalls the UI. Mirrors `assetcraft_media::VideoFrames`: a
//! lookup never waits; while a frame renders it hands back the nearest one at or before it.

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
    queue: VecDeque<(Slot, Arc<DesignJob>)>,
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
        if self.workers.is_empty() {
            return DesignLookup::Failed("the design renderer could not start".into());
        }
        let width = width.clamp(16, 4096);
        let index = frame_index(&job.design, secs);
        let last = frame_index(&job.design, f64::MAX);
        let mut s = self.shared.lock();
        if let Some(why) = s.failed.get(&job.key) {
            return DesignLookup::Failed(why.clone());
        }
        let want = Slot { key: job.key, index, width };
        if let Some(f) = s.cache.get(&want) {
            return DesignLookup::Ready(Arc::clone(f));
        }
        // The asked frame first, then the read-ahead; a slot already cached, queued or busy is
        // left alone.
        let mut fresh: Vec<Slot> = std::iter::once(want)
            .chain((index + 1..=index.saturating_add(AHEAD).min(last)).map(|i| Slot { index: i, ..want }))
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
            .filter(|(slot, _)| slot.key == job.key && slot.width == width && slot.index <= index)
            .max_by_key(|(slot, _)| slot.index)
            .map(|(_, f)| Arc::clone(f));
        drop(s);
        self.shared.wake.notify_all();
        DesignLookup::Pending(near)
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
        let (dw, _) = job.design.size();
        let scale = f64::from(slot.width) / f64::from(dw.max(1));
        let rendered = job.design.render(&job.fill, f64::from(slot.index) / FPS, scale);
        let mut s = shared.lock();
        s.busy.retain(|b| *b != slot);
        match rendered {
            Ok(f) => {
                let frame = Arc::new(DesignFrame { index: slot.index, width: f.width, height: f.height, rgba: f.rgba });
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
                s.failed.insert(slot.key, e.to_string());
                let key = slot.key;
                s.queue.retain(|(q, _)| q.key != key);
            }
        }
    }
}
