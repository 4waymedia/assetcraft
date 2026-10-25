//! Track analysis in the app (phase A2c-2; Paul, 2026-10-25: "there is no way to analyze them...
//! allow the user to click an 'analyze' button for each audio track").
//!
//! - `storyboard.analyse {paths?: [audio]}` queues tracks (default: every track the Start screen
//!   lists that isn't analysed yet). One background thread works through the queue: decode,
//!   curves (A2a), tempo (A2b), cue map (`assetcraft_storyboard::cue_map`, A2c-1).
//! - Results are **AssetCraft's own state**, never written beside the user's music: each track gets
//!   `<analysis dir>/<32 hex>.cuemap.json` plus `<32 hex>.source` (the audio's full path, so the
//!   cue map can find its song). The key is a SipHash-128 of the audio path (the suite cache's
//!   hasher). The app sets the folder (`Session::with_analysis_dir`: next to `prefs.json`); without
//!   one, `storyboard.analyse` is unavailable and nothing is written.
//! - `storyboard.tracks` lists stored analyses as analysed, and marks the rest `queued`,
//!   `analysing` or `failed` (with the reason); its `analysis` field counts the queue so the UI can
//!   refresh and say when a batch is done.
//!
//! Writes are atomic (temp file, then rename) and only ever inside the analysis folder.

use crate::generate_cmd::CUEMAP_SUFFIX;
use crate::{CmdError, CmdResult, Session};
use assetcraft_fs::Services;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

/// Suffix of the file that names a stored cue map's audio.
const SOURCE_SUFFIX: &str = ".source";
/// Most tracks one `storyboard.analyse` call queues.
pub(crate) const MAX_QUEUE: usize = 500;
/// Longest `.source` file read (a path).
const MAX_SOURCE_BYTES: usize = 8 * 1024;

/// Where a track is in the analyser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyseState {
    /// Waiting its turn.
    Queued,
    /// Being analysed now.
    Analysing,
    /// The last try failed, with why.
    Failed(String),
}

/// The analyser's queue, as the UI sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnalyseProgress {
    /// Tracks waiting, including the one being analysed.
    pub pending: usize,
    /// Analysed in the current (or last) batch.
    pub done: usize,
    /// Failed in the current (or last) batch.
    pub failed: usize,
    /// Bumped each time a batch finishes (the queue runs dry), so the UI can notify once.
    pub batches: u64,
}

#[derive(Debug, Default)]
struct Jobs {
    queue: VecDeque<PathBuf>,
    current: Option<PathBuf>,
    failed: BTreeMap<PathBuf, String>,
    running: bool,
    progress: AnalyseProgress,
}

/// The background analyser (one worker thread, started on demand, gone when the queue is empty).
#[derive(Debug, Default)]
pub(crate) struct Analyser {
    jobs: Arc<Mutex<Jobs>>,
}

fn lock(jobs: &Mutex<Jobs>) -> std::sync::MutexGuard<'_, Jobs> {
    jobs.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The store key for an audio file: 32 hex digits of its path's SipHash-128.
fn key(audio: &Path) -> String {
    let mut h = assetcraft_cache::hash::Hasher128::new();
    h.bytes(audio.to_string_lossy().as_bytes());
    h.finish().to_string()
}

/// Where `audio`'s stored cue map lives in `dir`.
pub(crate) fn stored_cue_path(dir: &Path, audio: &Path) -> PathBuf {
    dir.join(format!("{}{CUEMAP_SUFFIX}", key(audio)))
}

/// Write `bytes` to `path` atomically (a temp file in the same folder, flushed, then renamed).
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Analyse one track and store its cue map. Returns why not, on failure.
fn analyse_one(services: &dyn Services, dir: &Path, audio: &Path) -> Result<(), String> {
    let file = audio.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("no file name")?;
    let decoded = assetcraft_audio::load(services, audio).map_err(|e| e.to_string())?;
    let facts = assetcraft_storyboard::TrackFacts {
        file: &file,
        sample_rate: decoded.rate(),
        duration_frames: decoded.frames() as u64,
    };
    let curves = assetcraft_analysis::curves(&decoded.analysis_signal());
    drop(decoded);
    let tempo = assetcraft_analysis::tempo(&curves);
    let cue = assetcraft_storyboard::cue_map(&facts, &curves, tempo.as_ref()).map_err(|e| e.to_string())?;
    let mut bytes = serde_json::to_vec_pretty(&cue).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    std::fs::create_dir_all(dir).map_err(|e| format!("can't make {}: {e}", dir.display()))?;
    let cue_path = stored_cue_path(dir, audio);
    let source = cue_path.with_file_name(format!("{}{SOURCE_SUFFIX}", key(audio)));
    // The source first: a cue map on disk always has its song's path beside it.
    write_atomic(&source, audio.to_string_lossy().as_bytes()).map_err(|e| format!("can't save: {e}"))?;
    write_atomic(&cue_path, &bytes).map_err(|e| format!("can't save: {e}"))?;
    Ok(())
}

impl Analyser {
    /// Queue `paths` (those already queued or running are skipped) and start the worker if idle.
    /// Returns how many were added.
    fn enqueue(&self, services: Arc<dyn Services>, dir: PathBuf, paths: Vec<PathBuf>) -> usize {
        let mut jobs = lock(&self.jobs);
        if !jobs.running {
            // A new batch.
            jobs.progress.done = 0;
            jobs.progress.failed = 0;
        }
        let mut added = 0;
        for p in paths {
            if jobs.current.as_ref() == Some(&p) || jobs.queue.contains(&p) {
                continue;
            }
            jobs.failed.remove(&p);
            jobs.queue.push_back(p);
            added += 1;
        }
        jobs.progress.pending = jobs.queue.len() + usize::from(jobs.current.is_some());
        if added == 0 || jobs.running {
            return added;
        }
        jobs.running = true;
        drop(jobs);
        let shared = Arc::clone(&self.jobs);
        let spawned = std::thread::Builder::new().name("assetcraft-analyse".into()).spawn(move || {
            loop {
                let next = {
                    let mut jobs = lock(&shared);
                    let next = jobs.queue.pop_front();
                    jobs.current = next.clone();
                    if next.is_none() {
                        jobs.running = false;
                        jobs.progress.pending = 0;
                        jobs.progress.batches += 1;
                    }
                    next
                };
                let Some(audio) = next else { break };
                let result = analyse_one(services.as_ref(), &dir, &audio);
                let mut jobs = lock(&shared);
                jobs.current = None;
                match result {
                    Ok(()) => jobs.progress.done += 1,
                    Err(e) => {
                        jobs.progress.failed += 1;
                        jobs.failed.insert(audio, e);
                    }
                }
                jobs.progress.pending = jobs.queue.len();
            }
        });
        if let Err(e) = spawned {
            let mut jobs = lock(&self.jobs);
            let reason = format!("can't start the analyser: {e}");
            let queued: Vec<PathBuf> = jobs.queue.drain(..).collect();
            for p in queued {
                jobs.failed.insert(p, reason.clone());
            }
            jobs.running = false;
            jobs.progress.pending = 0;
            return 0;
        }
        added
    }

    fn state(&self, audio: &Path) -> Option<AnalyseState> {
        let jobs = lock(&self.jobs);
        if jobs.current.as_deref() == Some(audio) {
            Some(AnalyseState::Analysing)
        } else if jobs.queue.iter().any(|p| p == audio) {
            Some(AnalyseState::Queued)
        } else {
            jobs.failed.get(audio).map(|e| AnalyseState::Failed(e.clone()))
        }
    }

    fn progress(&self) -> AnalyseProgress {
        lock(&self.jobs).progress
    }
}

impl Session {
    /// Keep in-app track analyses in `dir` (the app passes a folder next to `prefs.json`).
    pub fn with_analysis_dir(mut self, dir: PathBuf) -> Self {
        self.analysis_dir = Some(dir);
        self
    }

    /// Keep in-app track analyses in `dir` (as [`Session::with_analysis_dir`], on a live session).
    pub fn set_analysis_dir(&mut self, dir: PathBuf) {
        self.analysis_dir = Some(dir);
    }

    /// Where in-app analyses are kept, if anywhere.
    pub fn analysis_dir(&self) -> Option<&Path> {
        self.analysis_dir.as_deref()
    }

    /// The analyser's queue (the UI polls this to refresh the track list and to notify).
    pub fn analyse_progress(&self) -> AnalyseProgress {
        self.analyser.progress()
    }

    /// Where `audio` is in the analyser, if it's queued, running or failed.
    pub fn analyse_state(&self, audio: &Path) -> Option<AnalyseState> {
        self.analyser.state(audio)
    }

    /// The stored cue map for `audio`, if the analyser has made one.
    pub(crate) fn stored_cue(&self, audio: &Path) -> Option<PathBuf> {
        let dir = self.analysis_dir.as_deref()?;
        let path = stored_cue_path(dir, audio);
        self.services.entry(&path).ok().map(|_| path)
    }

    /// Whether `cue_path` is one the in-app analyser stored.
    pub(crate) fn is_stored_cue(&self, cue_path: &Path) -> bool {
        self.analysis_dir.as_deref().is_some_and(|d| cue_path.parent() == Some(d))
    }

    /// The song a stored cue map (one in the analysis folder) describes, from its `.source`.
    pub(crate) fn stored_audio(&self, cue_path: &Path, file: &str) -> Option<PathBuf> {
        let dir = self.analysis_dir.as_deref()?;
        if cue_path.parent() != Some(dir) {
            return None;
        }
        let name = cue_path.file_name()?.to_str()?;
        let stem = name.strip_suffix(CUEMAP_SUFFIX)?;
        let bytes = self.services.read_head(&dir.join(format!("{stem}{SOURCE_SUFFIX}")), MAX_SOURCE_BYTES).ok()?;
        let audio = PathBuf::from(String::from_utf8(bytes).ok()?.trim());
        // Hostile-input checks: a full path, to the file the cue map names, under this key.
        (audio.has_root() && audio.file_name().and_then(|n| n.to_str()) == Some(file) && key(&audio) == stem)
            .then_some(audio)
    }
}

/// `storyboard.analyse {paths?: [audio]}` → `{queued, pending, message}`: analyse tracks in the
/// background (default: every listed track that isn't analysed and isn't already queued).
pub(crate) fn sb_analyse(s: &mut Session, p: &Value) -> CmdResult {
    let Some(dir) = s.analysis_dir.clone() else {
        return Err(CmdError::Unavailable("no folder to keep analyses in (the app sets one)".into()));
    };
    let paths: Vec<PathBuf> = match p.get("paths") {
        None | Some(Value::Null) => {
            let tracks = crate::library_cmd::sb_tracks(s, &json!({}))?;
            tracks["tracks"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|t| t["analysed"] != true)
                        .filter_map(|t| t["audio"].as_str().map(PathBuf::from))
                        .collect()
                })
                .unwrap_or_default()
        }
        Some(Value::Array(a)) => {
            let mut out = Vec::new();
            for v in a {
                let path = v
                    .as_str()
                    .map(str::trim)
                    .filter(|x| !x.is_empty() && x.len() <= 4096)
                    .map(PathBuf::from)
                    .ok_or_else(|| CmdError::BadParams("`paths` must be audio file paths".into()))?;
                if !path.has_root() {
                    return Err(CmdError::BadParams("each path must be a full path".into()));
                }
                out.push(path);
            }
            out
        }
        Some(_) => return Err(CmdError::BadParams("`paths` must be a list of audio file paths".into())),
    };
    if paths.len() > MAX_QUEUE {
        return Err(CmdError::BadParams(format!("at most {MAX_QUEUE} tracks at a time")));
    }
    let queued = s.analyser.enqueue(Arc::clone(&s.services), dir, paths);
    let pending = s.analyser.progress().pending;
    let message = match queued {
        0 if pending > 0 => "Already analysing".to_string(),
        0 => "Nothing to analyse".to_string(),
        1 => "Analysing 1 track…".to_string(),
        n => format!("Analysing {n} tracks…"),
    };
    Ok(json!({ "queued": queued, "pending": pending, "message": message }))
}

/// The `analysis` field of `storyboard.tracks`.
pub(crate) fn progress_json(s: &Session) -> Value {
    let p = s.analyser.progress();
    json!({ "pending": p.pending, "done": p.done, "failed": p.failed, "batches": p.batches, "available": s.analysis_dir.is_some() })
}

/// A not-analysed row's `status` and `error` from the analyser.
pub(crate) fn row_status(s: &Session, audio: &Path) -> (Value, Value) {
    match s.analyser.state(audio) {
        Some(AnalyseState::Queued) => (json!("queued"), Value::Null),
        Some(AnalyseState::Analysing) => (json!("analysing"), Value::Null),
        Some(AnalyseState::Failed(e)) => (json!("failed"), json!(e)),
        None => (json!("new"), Value::Null),
    }
}
