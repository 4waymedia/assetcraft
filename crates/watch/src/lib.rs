//! Folder watching: live updates for the open folder, behind a [`Watcher`] trait.
//!
//! Layer 1; depends on no workspace crate. [`NotifyWatcher`] wraps the `notify` crate's
//! recommended OS watcher (ReadDirectoryChangesW on Windows, inotify on Linux, FSEvents on
//! macOS), watching one folder non-recursively. Raw events go through a bounded channel into a
//! [`Debouncer`] that coalesces them: a batch is ready once the folder has been quiet for
//! [`QUIET`], or [`MAX_WAIT`] after its first event during a long storm, and a batch of more
//! than [`MAX_PATHS`] paths becomes a single [`Change::Rescan`]. Copying 1,000 files in is one
//! rescan, not 1,000 updates.
//!
//! Failure is a normal state, not an error to report repeatedly: a folder that can't be watched
//! (network shares, some removable media, OS watch limits) just has live updates off, and F5
//! still works. [`FakeWatcher`] stands in for tests; it shares its state between clones so a
//! test can keep a handle after giving the watcher away.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// A batch is ready once no event has arrived for this long.
pub const QUIET: Duration = Duration::from_millis(250);

/// …or this long after its first event, so a steady trickle can't postpone updates forever.
pub const MAX_WAIT: Duration = Duration::from_millis(1000);

/// Most distinct paths a batch lists; beyond this it becomes [`Change::Rescan`].
pub const MAX_PATHS: usize = 256;

/// Raw events buffered between the OS watcher thread and [`Watcher::poll`]. When the buffer is
/// full the next batch becomes a rescan, so memory stays bounded during any storm.
pub const CHANNEL_CAPACITY: usize = 4096;

/// Asynchronous watcher errors tolerated before the watch is dropped and reported as failed.
pub const MAX_ASYNC_ERRORS: usize = 3;

/// What changed in the watched folder since the last poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// These entries were created, modified, renamed or removed (sorted, deduplicated).
    Paths(Vec<PathBuf>),
    /// Too much changed to list (or the OS dropped events): re-list the whole folder.
    Rescan,
}

/// Watches one folder at a time.
pub trait Watcher: Send {
    /// Watch `dir` (non-recursively), replacing any previous watch. `Err` means live updates
    /// are off for this folder; the caller shows that quietly and relies on manual refresh.
    fn watch(&mut self, dir: &Path) -> Result<(), String>;
    /// Stop watching (e.g. when the view shows the list of drives).
    fn unwatch(&mut self);
    /// The next coalesced batch, if one is ready at `now`. Never blocks.
    fn poll(&mut self, now: Instant) -> Option<Change>;
    /// When a pending batch will be ready (for scheduling a repaint); `None` when idle.
    fn due(&self) -> Option<Instant>;
    /// Why watching stopped after it had started (repeated OS errors), if it did.
    fn failure(&self) -> Option<String>;
}

// ---- debouncer --------------------------------------------------------------------------

/// Coalesces raw events into batches. Pure (time is passed in), so it is tested directly.
#[derive(Debug, Clone)]
pub struct Debouncer {
    quiet: Duration,
    max_wait: Duration,
    max_paths: usize,
    paths: BTreeSet<PathBuf>,
    rescan: bool,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Default for Debouncer {
    fn default() -> Self {
        Self::new(QUIET, MAX_WAIT, MAX_PATHS)
    }
}

impl Debouncer {
    /// A debouncer with the given timings and path cap.
    pub fn new(quiet: Duration, max_wait: Duration, max_paths: usize) -> Self {
        Self { quiet, max_wait, max_paths, paths: BTreeSet::new(), rescan: false, first: None, last: None }
    }

    fn touch(&mut self, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// Record a change to `path`.
    pub fn push(&mut self, path: PathBuf, now: Instant) {
        self.touch(now);
        if self.rescan {
            return;
        }
        self.paths.insert(path);
        if self.paths.len() > self.max_paths {
            self.paths.clear();
            self.rescan = true;
        }
    }

    /// Record that the whole folder needs re-listing.
    pub fn push_rescan(&mut self, now: Instant) {
        self.touch(now);
        self.paths.clear();
        self.rescan = true;
    }

    /// `true` while a batch is pending.
    pub fn is_pending(&self) -> bool {
        self.first.is_some()
    }

    /// When the pending batch will be ready.
    pub fn due(&self) -> Option<Instant> {
        let (first, last) = (self.first?, self.last?);
        Some((last + self.quiet).min(first + self.max_wait))
    }

    /// The batch, if it is ready at `now`; clears it.
    pub fn take(&mut self, now: Instant) -> Option<Change> {
        if now < self.due()? {
            return None;
        }
        self.first = None;
        self.last = None;
        if std::mem::take(&mut self.rescan) {
            self.paths.clear();
            return Some(Change::Rescan);
        }
        Some(Change::Paths(std::mem::take(&mut self.paths).into_iter().collect()))
    }

    /// Drop anything pending (after switching folders).
    pub fn reset(&mut self) {
        *self = Self::new(self.quiet, self.max_wait, self.max_paths);
    }
}

// ---- notify ------------------------------------------------------------------------------

/// Called from the OS watcher thread when something arrives (e.g. to request a repaint).
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// What the OS watcher thread sends to [`NotifyWatcher::poll`].
enum Raw {
    Paths(Vec<PathBuf>),
    Rescan,
    Error(String),
}

/// Shared between the watcher thread and the poller.
#[derive(Default)]
struct Flags {
    overflow: AtomicBool,
    woken: AtomicBool,
}

/// The real watcher, using `notify`'s recommended backend for this OS.
pub struct NotifyWatcher {
    inner: Option<notify::RecommendedWatcher>,
    rx: Option<Receiver<Raw>>,
    flags: Arc<Flags>,
    waker: Option<Waker>,
    debouncer: Debouncer,
    errors: usize,
    failure: Option<String>,
}

impl NotifyWatcher {
    /// A watcher that calls `waker` (at most once per poll) when events arrive.
    pub fn new(waker: Option<Waker>) -> Self {
        Self {
            inner: None,
            rx: None,
            flags: Arc::new(Flags::default()),
            waker,
            debouncer: Debouncer::default(),
            errors: 0,
            failure: None,
        }
    }

    /// A watcher with custom debounce timings (tests).
    pub fn with_debouncer(waker: Option<Waker>, debouncer: Debouncer) -> Self {
        Self { debouncer, ..Self::new(waker) }
    }

    /// Move whatever the watcher thread sent into the debouncer.
    fn drain(&mut self, now: Instant) {
        let Some(rx) = &self.rx else { return };
        // Clear the flag *before* emptying the channel: an event sent after the last
        // `try_recv` then sets it again and wakes the UI, instead of being stranded until the
        // next unrelated repaint (lost wakeup).
        self.flags.woken.swap(false, Ordering::AcqRel);
        let mut error = None;
        while let Ok(raw) = rx.try_recv() {
            match raw {
                Raw::Paths(paths) => paths.into_iter().for_each(|p| self.debouncer.push(p, now)),
                Raw::Rescan => self.debouncer.push_rescan(now),
                Raw::Error(e) => {
                    self.debouncer.push_rescan(now);
                    error = Some(e);
                }
            }
        }
        if self.flags.overflow.swap(false, Ordering::AcqRel) {
            self.debouncer.push_rescan(now);
        }
        if let Some(e) = error {
            self.errors += 1;
            if self.errors >= MAX_ASYNC_ERRORS {
                self.failure = Some(e);
                self.inner = None;
                self.rx = None;
            }
        }
    }
}

/// Turn one notify event into a message, or nothing for events that don't change a listing.
fn classify(event: notify::Result<notify::Event>) -> Option<Raw> {
    use notify::EventKind;
    use notify::event::{AccessKind, AccessMode};
    match event {
        Err(e) => Some(Raw::Error(e.to_string())),
        Ok(event) if event.need_rescan() => Some(Raw::Rescan),
        // Reading a file (our own thumbnail decoder does) is not a change. Closing a file that
        // was open for writing is: the copy or save has finished.
        Ok(event) => match event.kind {
            EventKind::Access(AccessKind::Close(AccessMode::Write)) => Some(Raw::Paths(event.paths)),
            EventKind::Access(_) => None,
            _ if event.paths.is_empty() => Some(Raw::Rescan),
            _ => Some(Raw::Paths(event.paths)),
        },
    }
}

fn send(tx: &SyncSender<Raw>, flags: &Flags, waker: Option<&Waker>, raw: Raw) {
    if let Err(TrySendError::Full(_)) = tx.try_send(raw) {
        flags.overflow.store(true, Ordering::Release);
    }
    if !flags.woken.swap(true, Ordering::AcqRel)
        && let Some(wake) = waker
    {
        wake();
    }
}

impl Watcher for NotifyWatcher {
    fn watch(&mut self, dir: &Path) -> Result<(), String> {
        use notify::Watcher as _;
        self.unwatch();
        let (tx, rx) = std::sync::mpsc::sync_channel(CHANNEL_CAPACITY);
        let flags = Arc::new(Flags::default());
        let (thread_flags, waker) = (Arc::clone(&flags), self.waker.clone());
        let handler = move |event: notify::Result<notify::Event>| {
            if let Some(raw) = classify(event) {
                send(&tx, &thread_flags, waker.as_ref(), raw);
            }
        };
        let mut inner = notify::recommended_watcher(handler).map_err(|e| format!("live updates off: {e}"))?;
        inner
            .watch(dir, notify::RecursiveMode::NonRecursive)
            .map_err(|e| format!("live updates off for {}: {e}", dir.display()))?;
        self.inner = Some(inner);
        self.rx = Some(rx);
        self.flags = flags;
        Ok(())
    }

    fn unwatch(&mut self) {
        // Dropping the notify watcher stops its thread and closes the channel.
        self.inner = None;
        self.rx = None;
        self.debouncer.reset();
        self.errors = 0;
        self.failure = None;
    }

    fn poll(&mut self, now: Instant) -> Option<Change> {
        self.drain(now);
        self.debouncer.take(now)
    }

    fn due(&self) -> Option<Instant> {
        // Events may be waiting in the channel; they become due after one quiet period.
        let waiting = self.flags.woken.load(Ordering::Acquire);
        match (self.debouncer.due(), waiting) {
            (Some(due), _) => Some(due),
            (None, true) => Some(Instant::now()),
            (None, false) => None,
        }
    }

    fn failure(&self) -> Option<String> {
        self.failure.clone()
    }
}

// ---- fake --------------------------------------------------------------------------------

#[derive(Debug, Default)]
struct FakeState {
    watching: Option<PathBuf>,
    fail_with: Option<String>,
    failure: Option<String>,
    debouncer: Option<Debouncer>,
    watch_calls: usize,
}

/// A watcher driven by the test: [`FakeWatcher::emit`] queues paths, [`FakeWatcher::fail_with`]
/// makes the next `watch` fail. Clones share state. Batches are ready immediately (zero quiet
/// time) but still coalesce everything emitted between two polls, with the real path cap.
#[derive(Debug, Clone, Default)]
pub struct FakeWatcher {
    state: Arc<Mutex<FakeState>>,
    polls_with_change: Arc<AtomicUsize>,
}

impl FakeWatcher {
    /// A fake that watches successfully.
    pub fn new() -> Self {
        Self::default()
    }

    fn with<R>(&self, f: impl FnOnce(&mut FakeState) -> R) -> Option<R> {
        self.state.lock().ok().map(|mut s| f(&mut s))
    }

    fn debouncer(state: &mut FakeState) -> &mut Debouncer {
        state.debouncer.get_or_insert_with(|| Debouncer::new(Duration::ZERO, Duration::ZERO, MAX_PATHS))
    }

    /// Make every later `watch` fail with `reason` (`None` to succeed again).
    pub fn fail_with(&self, reason: Option<&str>) {
        self.with(|s| s.fail_with = reason.map(str::to_owned));
    }

    /// Simulate watching breaking down after it started (see [`Watcher::failure`]).
    pub fn break_down(&self, reason: &str) {
        self.with(|s| {
            s.failure = Some(reason.to_owned());
            s.watching = None;
        });
    }

    /// Queue a change to `path` (ignored unless something is being watched, like the OS).
    pub fn emit(&self, path: impl Into<PathBuf>) {
        let path = path.into();
        self.with(|s| {
            if s.watching.is_some() {
                Self::debouncer(s).push(path, Instant::now());
            }
        });
    }

    /// Queue a rescan.
    pub fn emit_rescan(&self) {
        self.with(|s| {
            if s.watching.is_some() {
                Self::debouncer(s).push_rescan(Instant::now());
            }
        });
    }

    /// The folder being watched.
    pub fn watching(&self) -> Option<PathBuf> {
        self.with(|s| s.watching.clone()).flatten()
    }

    /// How many times `watch` was called.
    pub fn watch_calls(&self) -> usize {
        self.with(|s| s.watch_calls).unwrap_or(0)
    }

    /// How many polls returned a change.
    pub fn batches(&self) -> usize {
        self.polls_with_change.load(Ordering::Acquire)
    }
}

impl Watcher for FakeWatcher {
    fn watch(&mut self, dir: &Path) -> Result<(), String> {
        self.with(|s| {
            s.watch_calls += 1;
            s.failure = None;
            Self::debouncer(s).reset();
            match &s.fail_with {
                Some(reason) => {
                    s.watching = None;
                    Err(reason.clone())
                }
                None => {
                    s.watching = Some(dir.to_path_buf());
                    Ok(())
                }
            }
        })
        .unwrap_or_else(|| Err("watcher state poisoned".into()))
    }

    fn unwatch(&mut self) {
        self.with(|s| {
            s.watching = None;
            s.failure = None;
            Self::debouncer(s).reset();
        });
    }

    fn poll(&mut self, now: Instant) -> Option<Change> {
        let change = self.with(|s| Self::debouncer(s).take(now)).flatten();
        if change.is_some() {
            self.polls_with_change.fetch_add(1, Ordering::AcqRel);
        }
        change
    }

    fn due(&self) -> Option<Instant> {
        self.with(|s| s.debouncer.as_ref().and_then(Debouncer::due)).flatten()
    }

    fn failure(&self) -> Option<String> {
        self.with(|s| s.failure.clone()).flatten()
    }
}

#[cfg(test)]
mod tests;
