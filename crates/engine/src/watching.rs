//! Live folder updates. The session owns an optional [`Watcher`] and keeps it pointed at the
//! open folder. The app polls [`Session::pending_changes`] each frame and feeds what it gets to
//! the `nav.changed` command, which updates the listing in place: selection, focus and scroll
//! position survive, and the result names the files whose thumbnails are stale.

use crate::{CmdError, CmdResult, Location, MAX_ITEMS, Session, same_path};
use assetcraft_fs::{Entry, FsError};
use assetcraft_watch::{Change, Watcher};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

/// Most paths `nav.changed` applies one by one; a longer list re-lists the folder instead.
pub const MAX_INCREMENTAL: usize = 256;

/// Whether the open folder updates live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchStatus {
    /// No watcher, or nothing to watch (the Computer view).
    Off,
    /// The open folder is watched.
    Live,
    /// Watching this folder failed (network share, OS limit…): F5 still works, and F5 or
    /// navigating retries. The text says why, for a tooltip; it is not an error to show.
    Unavailable(String),
}

impl Session {
    /// Give the session a folder watcher. It starts watching the open folder immediately and
    /// follows every navigation from then on.
    pub fn set_watcher(&mut self, watcher: Box<dyn Watcher>) {
        self.watcher = Some(watcher);
        self.watched = None;
        let loc = self.location.clone();
        self.retarget_watch(&loc);
    }

    /// Whether live updates are on for the open folder.
    pub fn watch_status(&self) -> &WatchStatus {
        &self.watch_status
    }

    /// The next coalesced batch of changes, as params for `nav.changed`, if one is ready.
    /// Doesn't change the listing; run `nav.changed` with the result for that. Paths that
    /// aren't valid UTF-8 can't travel in JSON, so a batch with one becomes `{rescan: true}`.
    pub fn pending_changes(&mut self) -> Option<Value> {
        let watcher = self.watcher.as_mut()?;
        let change = watcher.poll(Instant::now());
        if self.watch_status == WatchStatus::Live
            && let Some(reason) = watcher.failure()
        {
            watcher.unwatch();
            self.watched = None;
            self.watch_status = WatchStatus::Unavailable(reason);
            // One last re-list catches whatever happened while the watch was breaking down.
            return Some(json!({ "rescan": true }));
        }
        Some(match change? {
            Change::Rescan => json!({ "rescan": true }),
            Change::Paths(paths) => match paths.iter().map(|p| p.to_str()).collect::<Option<Vec<_>>>() {
                Some(texts) => json!({ "paths": texts }),
                None => json!({ "rescan": true }),
            },
        })
    }

    /// How long until [`Session::pending_changes`] may have something (for a repaint timer).
    pub fn changes_due_in(&self) -> Option<std::time::Duration> {
        let due = self.watcher.as_ref()?.due()?;
        Some(due.saturating_duration_since(Instant::now()))
    }

    /// Point the watcher at `loc` (a no-op if it is already live there). Failure is recorded in
    /// [`Session::watch_status`], never returned: navigation must not fail because of it.
    pub(crate) fn retarget_watch(&mut self, loc: &Location) {
        let Some(watcher) = self.watcher.as_mut() else { return };
        let Some(dir) = loc.path() else {
            watcher.unwatch();
            self.watched = None;
            self.watch_status = WatchStatus::Off;
            return;
        };
        let live_here = self.watch_status == WatchStatus::Live && self.watched.as_deref().is_some_and(|w| w == dir);
        if live_here {
            return;
        }
        match watcher.watch(dir) {
            Ok(()) => {
                self.watched = Some(dir.to_path_buf());
                self.watch_status = WatchStatus::Live;
            }
            Err(reason) => {
                self.watched = None;
                self.watch_status = WatchStatus::Unavailable(reason);
            }
        }
    }

    /// `nav.changed`: apply changes to the open folder in place.
    pub(crate) fn apply_changes(&mut self, params: &Value) -> CmdResult {
        let Location::Folder(dir) = self.location.clone() else {
            return Ok(
                json!({ "added": [], "removed": [], "modified": [], "rescan": false, "items": self.items.len() }),
            );
        };
        let paths: Option<Vec<PathBuf>> = params
            .get("paths")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).map(PathBuf::from).collect());
        let asked_rescan = params.get("rescan").and_then(Value::as_bool).unwrap_or(false);
        if paths.is_none() && !asked_rescan {
            return Err(CmdError::BadParams("need `paths` (list) or `rescan: true`".into()));
        }
        let paths = paths.unwrap_or_default();
        let rescan =
            asked_rescan || self.truncated || paths.len() > MAX_INCREMENTAL || paths.iter().any(|p| same_path(p, &dir));

        let before: HashMap<PathBuf, (u64, Option<SystemTime>)> =
            self.items.iter().map(|e| (e.path.clone(), (e.size, e.modified))).collect();
        if rescan {
            let (items, truncated, skipped) = match self.list(&self.location) {
                Ok(listing) => listing,
                Err(e) => {
                    // The open folder is gone or unreadable (deleted, drive removed): its watch
                    // is dead too. Say so; F5 retries once the folder is back.
                    if let Some(watcher) = self.watcher.as_mut() {
                        watcher.unwatch();
                    }
                    self.watched = None;
                    if self.watcher.is_some() {
                        self.watch_status = WatchStatus::Unavailable(format!("the folder can't be read: {e}"));
                    }
                    return Err(e);
                }
            };
            self.items = items;
            self.truncated = truncated;
            self.skipped = skipped;
        } else {
            for path in paths.iter().filter(|p| p.parent().is_some_and(|parent| same_path(parent, &dir))) {
                self.apply_one(path);
            }
            crate::sort::sort_entries(&mut self.items, self.view.sort, self.view.ascending);
        }
        self.item_paths = self.items.iter().map(|e| e.path.clone()).collect();

        let mut added = Vec::new();
        let mut modified = Vec::new();
        for entry in &self.items {
            match before.get(&entry.path) {
                None => added.push(entry.path.clone()),
                Some(old) if *old != (entry.size, entry.modified) => modified.push(entry.path.clone()),
                Some(_) => {}
            }
        }
        let now: std::collections::HashSet<&Path> = self.item_paths.iter().map(PathBuf::as_path).collect();
        let mut removed: Vec<PathBuf> = before.keys().filter(|p| !now.contains(p.as_path())).cloned().collect();
        removed.sort();
        for path in modified.iter().chain(&removed) {
            self.probes.remove(path);
        }
        self.selection.retain_existing(&self.item_paths);
        self.preview_open &= self.selection.focus().is_some();
        Ok(json!({
            "added": lossy_all(&added),
            "removed": lossy_all(&removed),
            "modified": lossy_all(&modified),
            "rescan": rescan,
            "items": self.items.len(),
        }))
    }

    /// Re-read one entry: insert, replace or remove it.
    fn apply_one(&mut self, path: &Path) {
        let fresh: Option<Entry> = match self.services.entry(path) {
            Ok(entry) if entry.hidden && !self.view.show_hidden => None,
            Ok(entry) => Some(entry),
            Err(FsError::NotFound(_)) => None,
            // Briefly locked (a copy in progress) or unreadable: keep what we have; the
            // finished write sends another event.
            Err(_) => return,
        };
        let index = self.items.iter().position(|e| e.path == path);
        match (index, fresh) {
            (Some(i), Some(entry)) => self.items[i] = entry,
            (Some(i), None) => {
                self.items.remove(i);
            }
            (None, Some(entry)) if self.items.len() < MAX_ITEMS => self.items.push(entry),
            (None, Some(_)) => self.truncated = true,
            (None, None) => {}
        }
    }
}

fn lossy_all(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.to_string_lossy().into_owned()).collect()
}
