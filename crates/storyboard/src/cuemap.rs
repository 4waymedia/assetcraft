//! Cue map: a track's musical timing as data (docs/track-analysis.md §5).
//!
//! Version 1 is written by `tools/cuemap.py` (the bridge until the Rust analyser, phases A1–A4,
//! writes the same format). Times are **source sample frames** at `sampleRate`, so every value
//! converts to [`Ticks`] exactly. A cue map read from disk is hostile input: [`CueMap::from_json`]
//! caps the size, then checks every count, order and range before anything indexes it.
//!
//! [`CueMap::timeline`] applies a Fit to length (§6.5) and returns the **output timeline**: the
//! beats, bars, sections and hits as they fall in the fitted track, with removed stretches gone and
//! the song's ending shifted to follow the splice.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::time::{TICKS_PER_SECOND, Ticks};

/// Current cue map format version.
pub const CUEMAP_VERSION: u32 = 1;

/// Largest cue map file, in bytes.
pub const MAX_CUEMAP_BYTES: usize = 8 << 20;
/// Most beats (20 minutes at 400 BPM).
pub const MAX_BEATS: usize = 8_000;
/// Most sections, hits, or fit segments.
pub const MAX_ITEMS: usize = 2_000;
/// Longest track: 20 minutes (SoundCraft's Beat Detective limit).
pub const MAX_SECONDS: u64 = 20 * 60;

/// The track a cue map describes, with its licence and credit (credits policy C1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CueTrack {
    /// File name of the audio (no folders).
    pub file: String,
    /// Title.
    pub title: String,
    /// Artist.
    #[serde(default)]
    pub artist: String,
    /// Licence id (`CC-BY-4.0`, `CC-BY-3.0`, `FreeToUse`, `own`, …).
    #[serde(default)]
    pub license: String,
    /// Verbatim credit text to paste into a video description.
    #[serde(default)]
    pub credit: String,
}

/// A song section: a stretch with a label and an energy (0 quietest … 1 loudest in the track).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CueSection {
    /// First frame.
    pub start: u64,
    /// End frame (exclusive).
    pub end: u64,
    /// Label (`Intro`, `Verse`, `Build`, `Chorus`, `Outro`, …).
    pub label: String,
    /// Relative energy, 0..=1.
    pub energy: f32,
}

/// A hit: a drop, an accent, a strong downbeat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CueHit {
    /// Where.
    pub frame: u64,
    /// Kind (`drop`, `hit`, `accent`, `bassHit`, `swellPeak`, …).
    pub kind: String,
    /// Strength, 0..=1.
    pub strength: f32,
}

/// How a fitted track ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitMode {
    /// Fade out before the end.
    Fade,
    /// Cut to the song's own ending.
    Edit,
    /// Stop on a hit.
    Button,
}

/// A fade at the end of the fitted track, in output frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FadeOut {
    /// Where the fade starts.
    pub start: u64,
    /// How long it lasts.
    pub frames: u64,
}

/// One way to fit the track to a length (spec §6.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fit {
    /// How it ends.
    pub mode: FitMode,
    /// Length of the fitted track.
    pub length_frames: u64,
    /// Source ranges played in order, `[start, end)`.
    pub segments: Vec<[u64; 2]>,
    /// Crossfade at each splice.
    #[serde(default)]
    pub crossfade_frames: u64,
    /// Fade at the end, if any.
    #[serde(default)]
    pub fade_out: Option<FadeOut>,
    /// How alike the two sides of the splice sound (edits only).
    #[serde(default)]
    pub similarity: Option<f32>,
    /// Why this fit was chosen.
    #[serde(default)]
    pub reasons: Vec<String>,
    /// The CC BY `Changes:` line.
    #[serde(default)]
    pub changes: String,
}

/// The candidate fits for one target length; `chosen` names the default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FitChoice {
    /// The mode picked by default (`None` when no fit was found).
    pub chosen: Option<FitMode>,
    /// The fade candidate.
    #[serde(default)]
    pub fade: Option<Fit>,
    /// The edit-to-the-ending candidate.
    #[serde(default)]
    pub edit: Option<Fit>,
}

impl FitChoice {
    /// The chosen fit, or the one asked for.
    pub fn get(&self, mode: Option<FitMode>) -> Option<&Fit> {
        match mode.or(self.chosen)? {
            FitMode::Edit => self.edit.as_ref().or(self.fade.as_ref()),
            FitMode::Fade | FitMode::Button => self.fade.as_ref().or(self.edit.as_ref()),
        }
    }
}

/// A track's timing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CueMap {
    /// Format version ([`CUEMAP_VERSION`]).
    pub version: u32,
    /// What wrote it.
    #[serde(default)]
    pub analyzer: String,
    /// The audio file and its credit.
    pub track: CueTrack,
    /// Source sample rate.
    pub sample_rate: u32,
    /// Track length.
    pub duration_frames: u64,
    /// Tempo.
    pub bpm: f64,
    /// Beats per bar.
    pub beats_per_bar: u8,
    /// How clear the pulse is, 0..=1 (low = free time).
    #[serde(default)]
    pub pulse_clarity: f32,
    /// Beats, ascending.
    pub beats: Vec<u64>,
    /// Bar starts, ascending.
    pub downbeats: Vec<u64>,
    /// Sections, in order, covering the track.
    pub sections: Vec<CueSection>,
    /// Hits, ascending.
    #[serde(default)]
    pub hits: Vec<CueHit>,
    /// Fit to length per target in seconds (`"30"`, `"60"`, …).
    #[serde(default)]
    pub fits: BTreeMap<String, FitChoice>,
}

/// Why a cue map was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CueMapError {
    /// Larger than [`MAX_CUEMAP_BYTES`].
    TooLarge(usize),
    /// Not valid cue map JSON.
    Parse(String),
    /// Written by a newer AssetCraft.
    NewerVersion(u32),
    /// A value is out of range or out of order.
    Invalid(String),
}

impl fmt::Display for CueMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CueMapError::TooLarge(n) => write!(f, "cue map is too large ({n} bytes)"),
            CueMapError::Parse(e) => write!(f, "not a cue map: {e}"),
            CueMapError::NewerVersion(v) => {
                write!(f, "cue map format {v} is newer than this AssetCraft ({CUEMAP_VERSION})")
            }
            CueMapError::Invalid(e) => write!(f, "cue map is damaged: {e}"),
        }
    }
}

impl std::error::Error for CueMapError {}

fn bad(msg: impl Into<String>) -> CueMapError {
    CueMapError::Invalid(msg.into())
}

fn ascending(name: &str, v: &[u64], end: u64) -> Result<(), CueMapError> {
    if v.windows(2).any(|w| w[0] >= w[1]) {
        return Err(bad(format!("{name} are not in order")));
    }
    if v.last().is_some_and(|x| *x > end) {
        return Err(bad(format!("{name} run past the end of the track")));
    }
    Ok(())
}

impl CueMap {
    /// Parse and validate.
    pub fn from_json(bytes: &[u8]) -> Result<CueMap, CueMapError> {
        if bytes.len() > MAX_CUEMAP_BYTES {
            return Err(CueMapError::TooLarge(bytes.len()));
        }
        let map: CueMap = serde_json::from_slice(bytes).map_err(|e| CueMapError::Parse(e.to_string()))?;
        map.validate()?;
        Ok(map)
    }

    /// Check counts, ranges and order.
    pub fn validate(&self) -> Result<(), CueMapError> {
        if self.version > CUEMAP_VERSION {
            return Err(CueMapError::NewerVersion(self.version));
        }
        if self.version == 0 {
            return Err(bad("version 0"));
        }
        if !(8_000..=384_000).contains(&self.sample_rate) {
            return Err(bad(format!("sample rate {} Hz", self.sample_rate)));
        }
        let max = u64::from(self.sample_rate) * MAX_SECONDS;
        let end = self.duration_frames;
        if end == 0 || end > max {
            return Err(bad("track length is zero or over 20 minutes"));
        }
        if !(self.bpm.is_finite() && (20.0..=400.0).contains(&self.bpm)) {
            return Err(bad(format!("tempo {} BPM", self.bpm)));
        }
        if !(2..=12).contains(&self.beats_per_bar) {
            return Err(bad(format!("{} beats per bar", self.beats_per_bar)));
        }
        if self.beats.len() > MAX_BEATS || self.downbeats.len() > MAX_BEATS {
            return Err(bad("too many beats"));
        }
        if self.sections.len() > MAX_ITEMS || self.hits.len() > MAX_ITEMS || self.fits.len() > 16 {
            return Err(bad("too many sections, hits or fits"));
        }
        for s in [&self.track.file, &self.track.title, &self.track.artist, &self.track.license, &self.analyzer] {
            if s.len() > 1024 {
                return Err(bad("a text field is over 1 KiB"));
            }
        }
        if self.track.credit.len() > 8192 {
            return Err(bad("credit text is over 8 KiB"));
        }
        if self.track.file.contains(['/', '\\']) {
            return Err(bad("track file must be a bare file name"));
        }
        ascending("beats", &self.beats, end)?;
        ascending("downbeats", &self.downbeats, end)?;
        let hits: Vec<u64> = self.hits.iter().map(|h| h.frame).collect();
        if hits.windows(2).any(|w| w[0] > w[1]) || hits.last().is_some_and(|x| *x > end) {
            return Err(bad("hits are not in order"));
        }
        let mut prev = 0;
        for s in &self.sections {
            if s.start != prev || s.end <= s.start || s.end > end {
                return Err(bad(format!("section {:?} doesn't follow on", s.label)));
            }
            if !(s.energy.is_finite() && (0.0..=1.0).contains(&s.energy)) || s.label.len() > 64 {
                return Err(bad(format!("section {:?} has a bad energy or label", s.label)));
            }
            prev = s.end;
        }
        if !self.sections.is_empty() && prev != end {
            return Err(bad("sections don't reach the end of the track"));
        }
        for h in &self.hits {
            if !(h.strength.is_finite() && (0.0..=1.0).contains(&h.strength)) || h.kind.len() > 32 {
                return Err(bad("a hit has a bad strength or kind"));
            }
        }
        for (k, c) in &self.fits {
            if k.parse::<u32>().is_err() {
                return Err(bad(format!("fit key {k:?} is not seconds")));
            }
            for fit in [&c.fade, &c.edit].into_iter().flatten() {
                fit.validate(end)?;
            }
        }
        Ok(())
    }

    /// Frames → ticks, exactly (rounded to the nearest tick).
    pub fn ticks(&self, frames: u64) -> Ticks {
        let sr = u128::from(self.sample_rate.max(1));
        let t = (u128::from(frames) * u128::from(TICKS_PER_SECOND) + sr / 2) / sr;
        Ticks(u64::try_from(t).unwrap_or(u64::MAX))
    }

    /// The output timeline for a fit (or the whole track when `fit` is `None`).
    pub fn timeline(&self, fit: Option<&Fit>) -> Timeline {
        let whole = [[0, self.duration_frames]];
        let segments: &[[u64; 2]] = match fit {
            Some(f) if !f.segments.is_empty() => &f.segments,
            _ => &whole,
        };
        // Output offset of each segment.
        let mut offsets = Vec::with_capacity(segments.len());
        let mut at = 0u64;
        for s in segments {
            offsets.push(at);
            at = at.saturating_add(s[1].saturating_sub(s[0]));
        }
        let length = fit.map_or(at, |f| f.length_frames.min(at).max(1));
        let map = |src: u64| -> Option<u64> {
            segments.iter().zip(&offsets).find(|(s, _)| src >= s[0] && src < s[1]).map(|(s, o)| o + (src - s[0]))
        };
        let pts = |v: &[u64]| -> Vec<u64> {
            let mut out: Vec<u64> = v.iter().filter_map(|f| map(*f)).filter(|f| *f < length).collect();
            out.sort_unstable();
            out.dedup();
            out
        };
        let mut sections = Vec::new();
        for (s, o) in segments.iter().zip(&offsets) {
            for c in &self.sections {
                let a = c.start.max(s[0]);
                let b = c.end.min(s[1]);
                if a < b {
                    let (oa, ob) = (o + (a - s[0]), (o + (b - s[0])).min(length));
                    if oa < ob {
                        sections.push(CueSection { start: oa, end: ob, label: c.label.clone(), energy: c.energy });
                    }
                }
            }
        }
        // Merge neighbours with the same label that a splice cut apart.
        let mut merged: Vec<CueSection> = Vec::new();
        for s in sections {
            match merged.last_mut() {
                Some(m) if m.label == s.label && m.end == s.start => m.end = s.end,
                _ => merged.push(s),
            }
        }
        if merged.is_empty() {
            merged.push(CueSection { start: 0, end: length, label: "Track".into(), energy: 0.5 });
        }
        if let Some(last) = merged.last_mut() {
            last.end = length;
        }
        let hits = self
            .hits
            .iter()
            .filter_map(|h| map(h.frame).filter(|f| *f < length).map(|f| CueHit { frame: f, ..h.clone() }))
            .collect();
        Timeline {
            length,
            beats: pts(&self.beats),
            downbeats: pts(&self.downbeats),
            sections: merged,
            hits,
            fade_out: fit.and_then(|f| f.fade_out),
        }
    }
}

impl Fit {
    fn validate(&self, end: u64) -> Result<(), CueMapError> {
        if self.segments.is_empty() || self.segments.len() > 16 || self.reasons.len() > 16 {
            return Err(bad("a fit has no segments or too many"));
        }
        let mut prev = 0u64;
        let mut total = 0u64;
        for (i, s) in self.segments.iter().enumerate() {
            if s[0] >= s[1] || s[1] > end || (i > 0 && s[0] < prev) {
                return Err(bad("a fit's segments are out of order"));
            }
            prev = s[1];
            total += s[1] - s[0];
        }
        if self.length_frames == 0 || self.length_frames > total {
            return Err(bad("a fit's length doesn't match its segments"));
        }
        if let Some(f) = self.fade_out
            && (f.frames == 0 || f.start.saturating_add(f.frames) > self.length_frames.saturating_add(1))
        {
            return Err(bad("a fade runs past the end"));
        }
        if self.similarity.is_some_and(|s| !s.is_finite()) || self.changes.len() > 1024 {
            return Err(bad("a fit has a bad similarity or changes text"));
        }
        Ok(())
    }
}

/// A track's timing on the output timeline (after Fit to length), in output frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Timeline {
    /// Length.
    pub length: u64,
    /// Beats.
    pub beats: Vec<u64>,
    /// Bar starts.
    pub downbeats: Vec<u64>,
    /// Sections, covering `0..length`.
    pub sections: Vec<CueSection>,
    /// Hits.
    pub hits: Vec<CueHit>,
    /// Fade at the end, if any.
    pub fade_out: Option<FadeOut>,
}
