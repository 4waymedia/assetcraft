//! Worker threads that probe files (format, dimensions, times) off the UI thread.
//!
//! A probe reads up to 256 KiB per file (`assetcraft_probe::HEAD_BYTES`), so a list view of 40
//! rows on a network share could stall a frame for seconds. The UI asks with
//! [`ProbePool::request`], shows "…" meanwhile, and collects results with [`ProbePool::poll`],
//! just like thumbnails. [`ProbePool::clear`] starts a new epoch: queued work is dropped and
//! results of jobs that were already running are discarded when they arrive.

use crate::queue::Queue;
use assetcraft_fs::{FsError, Services};
use assetcraft_probe::Probe;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

/// Default probe workers: probing is small reads, mostly waiting on the disk.
pub const DEFAULT_PROBE_WORKERS: usize = 2;

/// One finished probe.
pub type ProbeResult = (PathBuf, Result<Probe, FsError>);

#[derive(Default)]
struct Shared {
    queue: Queue<PathBuf>,
    running: HashSet<PathBuf>,
    epoch: u64,
    shutdown: bool,
}

struct Inner {
    state: Mutex<Shared>,
    wake: Condvar,
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A few threads running `assetcraft_probe::probe`.
pub struct ProbePool {
    inner: Arc<Inner>,
    results: Receiver<(u64, ProbeResult)>,
    workers: Vec<JoinHandle<()>>,
}

impl ProbePool {
    /// Start `workers` threads (at least one) reading through `services`.
    pub fn new(services: Arc<dyn Services>, workers: usize) -> Self {
        let inner = Arc::new(Inner { state: Mutex::new(Shared::default()), wake: Condvar::new() });
        let (tx, results) = channel();
        let workers = (0..workers.max(1))
            .filter_map(|n| {
                let inner = Arc::clone(&inner);
                let services = Arc::clone(&services);
                let tx = tx.clone();
                std::thread::Builder::new()
                    .name(format!("assetcraft-probe-{n}"))
                    .spawn(move || worker(&inner, services.as_ref(), &tx))
                    .ok()
            })
            .collect();
        Self { inner, results, workers }
    }

    /// Worker threads actually running. With none, [`ProbePool::request`] does nothing and the
    /// caller should fall back to probing synchronously.
    pub fn workers(&self) -> usize {
        self.workers.len()
    }

    /// Queue a probe (newest requests first). No-op if it is queued or running already.
    pub fn request(&self, path: PathBuf) {
        let mut state = self.inner.lock();
        if state.running.contains(&path) {
            return;
        }
        state.queue.push(path, 0);
        drop(state);
        self.inner.wake.notify_one();
    }

    /// Drop queued (not running) requests for which `keep` returns false.
    pub fn retain(&self, keep: impl FnMut(&PathBuf) -> bool) {
        self.inner.lock().queue.retain(keep);
    }

    /// Drop all queued work and ignore results of jobs already running (a new folder, F5).
    pub fn clear(&self) {
        let mut state = self.inner.lock();
        state.epoch += 1;
        state.queue.retain(|_| false);
    }

    /// Queued plus running probes.
    pub fn busy(&self) -> usize {
        let state = self.inner.lock();
        state.queue.len() + state.running.len()
    }

    /// Finished probes of the current epoch (non-blocking).
    pub fn poll(&self) -> Vec<ProbeResult> {
        let epoch = self.inner.lock().epoch;
        self.results.try_iter().filter(|(e, _)| *e == epoch).map(|(_, r)| r).collect()
    }

    /// Like [`ProbePool::poll`], waiting up to `timeout` for the first result.
    pub fn poll_timeout(&self, timeout: Duration) -> Vec<ProbeResult> {
        let first = self.results.recv_timeout(timeout).ok();
        let epoch = self.inner.lock().epoch;
        first.into_iter().chain(self.results.try_iter()).filter(|(e, _)| *e == epoch).map(|(_, r)| r).collect()
    }
}

/// Dropping the pool stops the workers (a probe in flight finishes first; probes are bounded
/// reads).
impl Drop for ProbePool {
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

fn worker(inner: &Inner, services: &dyn Services, tx: &Sender<(u64, ProbeResult)>) {
    loop {
        let (path, epoch) = {
            let mut state = inner.lock();
            loop {
                if state.shutdown {
                    return;
                }
                if let Some(path) = state.queue.pop() {
                    state.running.insert(path.clone());
                    break (path, state.epoch);
                }
                state = inner.wake.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };
        let result = assetcraft_probe::probe(services, &path);
        inner.lock().running.remove(&path);
        if tx.send((epoch, (path, result))).is_err() {
            return;
        }
    }
}
