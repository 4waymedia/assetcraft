//! Worker threads that render thumbnails off the UI thread.

use crate::disk::{DiskCache, cache_key_and_head};
use crate::queue::Queue;
use crate::{DecodeOptions, MAX_DEFAULT_WORKERS, Thumb, ThumbCache, ThumbError, ThumbKey, render_with};
use assetcraft_fs::Services;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

/// Most remembered failures. Past this the memory is cleared (failed keys are simply retried
/// once more), so browsing millions of unreadable files can't grow it without bound.
pub const MAX_FAILED: usize = 10_000;

/// One finished job.
pub type ThumbResult = (ThumbKey, Result<Arc<Thumb>, ThumbError>);

#[derive(Default)]
struct Shared {
    queue: Queue,
    running: HashSet<ThumbKey>,
    shutdown: bool,
}

struct Inner {
    state: Mutex<Shared>,
    wake: Condvar,
    /// Set when the pool stops; long-running decoders (the `filmcraft-cli` poll loop) see it
    /// through [`crate::decoders::cancelled`] and give up, so dropping the pool never waits
    /// for an external tool's 10 s timeout.
    stop: Arc<AtomicBool>,
    /// [`DecodeOptions::heic`], read by every job.
    heic: AtomicBool,
}

impl Inner {
    /// Lock, recovering from a poisoned mutex (a worker can't panic inside the lock, but the
    /// app must never die on it if one ever did).
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A fixed set of worker threads plus the thumbnail caches.
///
/// Lookups go memory ([`ThumbCache`], on the calling thread), then disk (an optional
/// [`DiskCache`], on a worker), then render (on a worker, writing the result to disk).
///
/// The UI calls [`request`](Self::request) for visible cells each frame, [`retain`](Self::retain)
/// to drop work for cells that scrolled away, and [`poll`](Self::poll) to collect results, which
/// also fills the cache. Dropping the pool stops the workers (see the `Drop` impl).
pub struct ThumbPool {
    inner: Arc<Inner>,
    results: Receiver<ThumbResult>,
    cache: ThumbCache,
    failed: std::collections::HashMap<ThumbKey, ThumbError>,
    workers: Vec<JoinHandle<()>>,
    disk: Option<Arc<DiskCache>>,
}

impl ThumbPool {
    /// Start `workers` threads (at least one) reading through `services`, memory cache only.
    pub fn new(services: Arc<dyn Services>, workers: usize, cache: ThumbCache) -> Self {
        Self::with_disk(services, workers, cache, None)
    }

    /// Like [`ThumbPool::new`], with an optional disk cache shared by the workers.
    pub fn with_disk(
        services: Arc<dyn Services>,
        workers: usize,
        cache: ThumbCache,
        disk: Option<Arc<DiskCache>>,
    ) -> Self {
        let inner = Arc::new(Inner {
            state: Mutex::new(Shared::default()),
            wake: Condvar::new(),
            stop: Arc::new(AtomicBool::new(false)),
            heic: AtomicBool::new(false),
        });
        let (tx, results) = channel();
        let workers = (0..workers.max(1))
            .filter_map(|n| {
                let inner = Arc::clone(&inner);
                let services = Arc::clone(&services);
                let tx: Sender<ThumbResult> = tx.clone();
                let disk = disk.clone();
                std::thread::Builder::new()
                    .name(format!("assetcraft-thumb-{n}"))
                    .spawn(move || {
                        crate::decoders::set_thread_cancel(Some(Arc::clone(&inner.stop)));
                        worker(&inner, services.as_ref(), disk.as_ref(), &tx);
                    })
                    .ok()
            })
            .collect();
        Self { inner, results, cache, failed: Default::default(), workers, disk }
    }

    /// A pool sized for this machine: [`default_workers`].
    pub fn with_default_workers(services: Arc<dyn Services>) -> Self {
        Self::new(services, default_workers(), ThumbCache::default())
    }

    /// The disk cache, if this pool has one.
    pub fn disk(&self) -> Option<&Arc<DiskCache>> {
        self.disk.as_ref()
    }

    /// A pool whose worker threads all failed to start (tests the degraded path).
    #[cfg(test)]
    pub(crate) fn without_workers(services: Arc<dyn Services>) -> Self {
        let mut pool = Self::new(services, 1, ThumbCache::default());
        pool.stop_workers();
        pool
    }

    /// Worker threads actually running (spawning can fail on a starved machine).
    pub fn workers(&self) -> usize {
        self.workers.len()
    }

    /// Failures remembered (bounded by [`MAX_FAILED`]).
    pub fn failed_len(&self) -> usize {
        self.failed.len()
    }

    /// The cached thumbnail, if any. Marks it recently used.
    pub fn get(&mut self, key: &ThumbKey) -> Option<Arc<Thumb>> {
        self.cache.get(key)
    }

    /// The error from the last attempt, if rendering this key failed.
    pub fn error(&self, key: &ThumbKey) -> Option<&ThumbError> {
        self.failed.get(key)
    }

    /// Ask for a thumbnail. No-op if it is cached, already failed, or already running;
    /// otherwise queues it (or bumps it to the front of its `priority`).
    ///
    /// If no worker thread could be started, the request fails at once (so the UI shows a badge
    /// instead of a placeholder that repaints forever).
    pub fn request(&mut self, key: ThumbKey, priority: u8) {
        if self.failed.contains_key(&key) || self.cache.get(&key).is_some() {
            return;
        }
        if self.workers.is_empty() {
            self.remember_failure(key, ThumbError::Decode("no thumbnail worker could be started".into()));
            return;
        }
        let mut state = self.inner.lock();
        if state.running.contains(&key) {
            return;
        }
        state.queue.push(key, priority);
        drop(state);
        self.inner.wake.notify_one();
    }

    /// Drop queued (not yet running) requests for which `keep` returns false.
    pub fn retain(&self, keep: impl FnMut(&ThumbKey) -> bool) {
        self.inner.lock().queue.retain(keep);
    }

    /// Change which formats are decoded. When anything changes, every cached thumbnail and
    /// failure is dropped (as on F5) so affected files re-render under the new options.
    pub fn set_options(&mut self, options: DecodeOptions) {
        let was = self.options();
        self.inner.heic.store(options.heic, Ordering::Release);
        if was != options {
            self.clear();
        }
    }

    /// The decode options jobs run with.
    pub fn options(&self) -> DecodeOptions {
        DecodeOptions { heic: self.inner.heic.load(Ordering::Acquire) }
    }

    /// Forget a cached thumbnail or failure, e.g. after the file changed on disk.
    pub fn invalidate(&mut self, key: &ThumbKey) {
        self.cache.remove(key);
        self.failed.remove(key);
    }

    /// Forget everything: cached thumbnails, failures and queued work (F5 / refresh), so failed
    /// files are retried and changed files re-render. Jobs already running still deliver.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.failed.clear();
        self.inner.lock().queue.retain(|_| false);
    }

    /// Collect finished jobs (non-blocking), caching successes and remembering failures.
    pub fn poll(&mut self) -> Vec<ThumbResult> {
        let finished: Vec<ThumbResult> = self.results.try_iter().collect();
        self.record(&finished);
        finished
    }

    /// Like [`poll`](Self::poll), but waits up to `timeout` for the first result.
    pub fn poll_timeout(&mut self, timeout: Duration) -> Vec<ThumbResult> {
        let mut finished: Vec<ThumbResult> = self.results.recv_timeout(timeout).into_iter().collect();
        finished.extend(self.results.try_iter());
        self.record(&finished);
        finished
    }

    /// Jobs queued but not started.
    pub fn pending(&self) -> usize {
        self.inner.lock().queue.len()
    }

    /// Jobs currently being rendered.
    pub fn running(&self) -> usize {
        self.inner.lock().running.len()
    }

    /// The cache, for statistics.
    pub fn cache(&self) -> &ThumbCache {
        &self.cache
    }

    fn record(&mut self, finished: &[ThumbResult]) {
        for (key, result) in finished {
            match result {
                Ok(thumb) => self.cache.insert(key.clone(), Arc::clone(thumb)),
                Err(e) => self.remember_failure(key.clone(), e.clone()),
            }
        }
    }

    fn remember_failure(&mut self, key: ThumbKey, error: ThumbError) {
        if self.failed.len() >= MAX_FAILED {
            self.failed.clear();
        }
        self.failed.insert(key, error);
    }

    /// Signal shutdown, drop queued work and join the workers.
    fn stop_workers(&mut self) {
        {
            let mut state = self.inner.lock();
            state.shutdown = true;
            state.queue.retain(|_| false);
        }
        self.inner.stop.store(true, Ordering::Release);
        self.inner.wake.notify_all();
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

/// Default worker count: half the cores, between 1 and [`MAX_DEFAULT_WORKERS`].
pub fn default_workers() -> usize {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    (cores / 2).clamp(1, MAX_DEFAULT_WORKERS)
}

/// Dropping the pool sets the shutdown flag, clears the queue and joins the workers. Idle
/// workers exit at once; a worker in the middle of a decode finishes that one file first
/// (decodes are bounded by the limits in `lib.rs`, typically milliseconds, worst case a large
/// file's full decode) and then exits without picking up more work. A running `filmcraft-cli`
/// is killed within one poll (≈ 20 ms) instead of being waited on. That bounded wait is
/// accepted so no thread outlives the pool holding file handles.
impl Drop for ThumbPool {
    fn drop(&mut self) {
        self.stop_workers();
    }
}

fn worker(inner: &Inner, services: &dyn Services, disk: Option<&Arc<DiskCache>>, tx: &Sender<ThumbResult>) {
    loop {
        let key = {
            let mut state = inner.lock();
            loop {
                if state.shutdown {
                    return;
                }
                if let Some(key) = state.queue.pop() {
                    state.running.insert(key.clone());
                    break key;
                }
                state = inner.wake.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };
        let options = DecodeOptions { heic: inner.heic.load(Ordering::Acquire) };
        let result = job(services, disk, &key, &options).map(Arc::new);
        inner.lock().running.remove(&key);
        if tx.send((key, result)).is_err() {
            return; // pool dropped
        }
    }
}

/// One job: disk hit, else render and store. The disk is skipped (render only) when the key
/// can't be computed, e.g. the file vanished; `render` then reports the real error. A format
/// the options don't allow (HEIC with the switch off) never reaches the disk cache, so a
/// thumbnail made while it was on can't come back while it is off.
fn job(
    services: &dyn Services,
    disk: Option<&Arc<DiskCache>>,
    key: &ThumbKey,
    options: &DecodeOptions,
) -> Result<Thumb, ThumbError> {
    let render = |services: &dyn Services, key: &ThumbKey| render_with(services, key, options);
    let Some(disk) = disk else { return render(services, key) };
    let Ok((hash, source, head)) = cache_key_and_head(services, &key.path, key.max_side) else {
        return render(services, key);
    };
    let ext = key.path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    let (sniffed, _, _) = assetcraft_probe::probe_bytes(&head, ext.as_deref());
    if !options.allows(sniffed) {
        return render(services, key);
    }
    // Cache files are our own, but a damaged one is still untrusted input to the PNG decoder.
    let max_side = key.max_side;
    let hit =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| disk.get(hash, &source, max_side))).ok().flatten();
    if let Some(thumb) = hit {
        return Ok(thumb);
    }
    let thumb = render(services, key)?;
    if disk.put(hash, &source, &thumb) {
        disk.prune_in_background();
    }
    Ok(thumb)
}
