//! One background decode per track, shared by everything that needs a track's audio: the Start
//! screen's preview (`track.preview`), its waveform (`track.peaks`) and Watch your Cut.
//!
//! Before this, the waveform and playback each decoded the same file on their own thread, so
//! picking a song decoded it once for the waveform and clicking one of its sections decoded it
//! again before any sound came; and playback had a single slot, so a preview evicted the Cut's
//! track (Paul, 2026-10-17: "glitchy when the user switches songs"). Now the first request starts
//! the decode, every later one reuses it, and the last [`KEEP`] tracks stay decoded.

use assetcraft_audio::Audio;
use assetcraft_fs::Services;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

/// Decoded tracks kept (a 3-minute stereo track is about 60 MB as f32).
pub(crate) const KEEP: usize = 3;
/// Waveform buckets computed with each decode; `track.peaks` merges down from these.
pub(crate) const OVERVIEW_BUCKETS: usize = 4096;

/// A decoded track and its waveform overview (computed on the decode thread).
#[derive(Debug)]
pub(crate) struct Decoded {
    pub(crate) audio: Arc<Audio>,
    pub(crate) overview: Arc<Vec<(f32, f32)>>,
}

/// Where a decode is.
#[derive(Debug, Clone)]
pub(crate) enum Load {
    Loading,
    Ready(Arc<Decoded>),
    Failed(String),
}

pub(crate) type Slot = Arc<Mutex<Load>>;

/// Decodes by track, the most recently asked last.
#[derive(Debug, Default)]
pub(crate) struct Decodes {
    slots: Vec<(PathBuf, Slot)>,
}

impl Decodes {
    /// The slot for `path`, starting its decode on a background thread if it isn't kept.
    pub(crate) fn slot(&mut self, services: &Arc<dyn Services>, path: &Path) -> Slot {
        if let Some(i) = self.slots.iter().position(|(p, _)| p == path) {
            let kept = self.slots.remove(i);
            let slot = Arc::clone(&kept.1);
            self.slots.push(kept);
            return slot;
        }
        let slot: Slot = Arc::new(Mutex::new(Load::Loading));
        let (services, file, out) = (Arc::clone(services), path.to_path_buf(), Arc::clone(&slot));
        let spawned = std::thread::Builder::new().name("assetcraft-track-decode".into()).spawn(move || {
            let result = match assetcraft_audio::load(services.as_ref(), &file) {
                Ok(a) => {
                    let overview = Arc::new(a.overview(OVERVIEW_BUCKETS));
                    Load::Ready(Arc::new(Decoded { audio: Arc::new(a), overview }))
                }
                Err(e) => Load::Failed(e.to_string()),
            };
            *out.lock().unwrap_or_else(PoisonError::into_inner) = result;
        });
        if let Err(e) = spawned {
            *slot.lock().unwrap_or_else(PoisonError::into_inner) = Load::Failed(format!("can't start decoding: {e}"));
        }
        if self.slots.len() >= KEEP {
            self.slots.remove(0);
        }
        self.slots.push((path.to_path_buf(), Arc::clone(&slot)));
        slot
    }

    /// Where `path`'s decode is, without starting one (`None` = not asked for, or evicted).
    pub(crate) fn state(&self, path: &Path) -> Option<Load> {
        let (_, slot) = self.slots.iter().find(|(p, _)| p == path)?;
        Some(slot.lock().unwrap_or_else(PoisonError::into_inner).clone())
    }

    /// Tracks kept now (for tests).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }
}
