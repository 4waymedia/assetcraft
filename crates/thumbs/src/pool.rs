//! Worker threads that render thumbnails off the UI thread.

use crate::queue::Queue;
use crate::{Thumb, ThumbCache, ThumbError, ThumbKey, render};
use assetcraft_fs::Services;
use std::collections::HashSet;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

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
}

impl Inner {
    /// Lock, recovering from a poisoned mutex (a worker can't panic inside the lock, but the
    /// app must never die on it if one ever did).
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A fixed set of worker threads plus the thumbnail cache.
///
/// The UI calls [`request`](Self::request) for visible cells each frame, [`retain`](Self::retain)
/// to drop work for cells that scrolled away, and [`poll`](Self::poll) to collect results, which
/// also fills the cache. Dropping the pool stops the workers.
pub struct ThumbPool {
    inner: Arc<Inner>,
    results: Receiver<ThumbResult>,
    cache: ThumbCache,
    failed: std::collections::HashMap<ThumbKey, ThumbError>,
    workers: Vec<JoinHandle<()>>,
}

impl ThumbPool {
    /// Start `workers` threads (at least one) reading through `services`.
    pub fn new(services: Arc<dyn Services>, workers: usize, cache: ThumbCache) -> Self {
        let inner = Arc::new(Inner { state: Mutex::new(Shared::default()), wake: Condvar::new() });
        let (tx, results) = channel();
        let workers = (0..workers.max(1))
            .filter_map(|n| {
                let inner = Arc::clone(&inner);
                let services = Arc::clone(&services);
                let tx: Sender<ThumbResult> = tx.clone();
                std::thread::Builder::new()
                    .name(format!("assetcraft-thumb-{n}"))
                    .spawn(move || worker(&inner, services.as_ref(), &tx))
                    .ok()
            })
            .collect();
        Self { inner, results, cache, failed: Default::default(), workers }
    }

    /// A pool sized for this machine: half the cores, between 1 and 8.
    pub fn with_default_workers(services: Arc<dyn Services>) -> Self {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        Self::new(services, (cores / 2).clamp(1, 8), ThumbCache::default())
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
    pub fn request(&mut self, key: ThumbKey, priority: u8) {
        if self.failed.contains_key(&key) || self.cache.get(&key).is_some() {
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

    /// Forget a cached thumbnail or failure, e.g. after the file changed on disk.
    pub fn invalidate(&mut self, key: &ThumbKey) {
        self.cache.remove(key);
        self.failed.remove(key);
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
                Err(e) => {
                    self.failed.insert(key.clone(), e.clone());
                }
            }
        }
    }
}

impl Drop for ThumbPool {
    fn drop(&mut self) {
        {
            let mut state = self.inner.lock();
            state.shutdown = true;
            state.queue.retain(|_| false);
        }
        self.inner.wake.notify_all();
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

fn worker(inner: &Inner, services: &dyn Services, tx: &Sender<ThumbResult>) {
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
        let result = render(services, &key).map(Arc::new);
        inner.lock().running.remove(&key);
        if tx.send((key, result)).is_err() {
            return; // pool dropped
        }
    }
}
