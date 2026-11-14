//! Generate a storyboard from a track's cue map (docs/track-analysis.md §6, from scratch).
//!
//! Inputs: a [`CueMap`], a [`Pacing`] profile and an optional target length (which picks a Fit to
//! length from the cue map). Output: a [`Storyboard`] whose scenes follow the song's sections,
//! whose clips are cut on bars, beats and hits, and whose total is exactly the fitted track. Every
//! clip records why it starts where it does in its notes. Deterministic: same input, same board.
//!
//! The rules, in order:
//! 1. **Timeline:** the fitted track's output timeline ([`CueMap::timeline`]).
//! 2. **Scenes:** one per section; sections under 4 bars join their neighbour; sections over the
//!    profile's limit split at 16-bar phrases.
//! 3. **Title cards:** the opening (all profiles), every scene (Trailer), an end card (all but Calm).
//! 4. **Shot length:** from the profile's long/short range by the scene's energy; Trailer also
//!    shortens shots as the Cut goes on (escalation). Under one bar, shots snap to beats.
//! 5. **Hits:** a drop or strong hit inside a shot moves the cut onto the hit (profiles that cut
//!    on hits).
//! 6. **Shot types:** establishing or wide to open a scene, then by energy; short shots are inserts
//!    and close-ups.

use std::collections::HashSet;
use std::fmt;

use crate::cuemap::{CueMap, Fit, FitMode, Timeline};
use crate::model::{
    AssetId, Clip, ClipId, Scene, SceneId, ScenePart, ShotType, SongSection, Storyboard, Track, TrackEdit, Transition,
};
use crate::shape::{ResolvedShape, SceneTransition, Shape, Titles, pick_shot};
use crate::text::{TextAnimation, TextKind, TextPosition, TextStyle, TextTheme, set_text_kind};
use crate::time::{Bpm, Ticks};
use crate::validate::Limits;

/// A pacing profile (decision AD4: Calm, Cinematic, Energetic, Trailer; Custom comes later).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pacing {
    /// Longer shots, phrase-driven, few accents.
    Calm,
    /// Phrase and downbeat pacing, scene changes on sections.
    Cinematic,
    /// Short shots, cuts on beats and hits.
    Energetic,
    /// Staged escalation, title cards on section changes, impact on drops.
    Trailer,
    /// Even 1–2 bar pacing, a warm title on every scene, crossfades, weighted to close-ups.
    Family,
    /// Builds to a close-up at every drop, bold titles, hard cuts.
    Hero,
}

impl Pacing {
    /// Every profile, in menu order.
    pub const ALL: [Pacing; 6] =
        [Pacing::Trailer, Pacing::Energetic, Pacing::Cinematic, Pacing::Calm, Pacing::Family, Pacing::Hero];

    /// Name used in commands.
    pub fn id(self) -> &'static str {
        match self {
            Pacing::Calm => "calm",
            Pacing::Cinematic => "cinematic",
            Pacing::Energetic => "energetic",
            Pacing::Trailer => "trailer",
            Pacing::Family => "family",
            Pacing::Hero => "hero",
        }
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            Pacing::Calm => "Calm",
            Pacing::Cinematic => "Cinematic",
            Pacing::Energetic => "Energetic",
            Pacing::Trailer => "Trailer",
            Pacing::Family => "Family",
            Pacing::Hero => "Hero",
        }
    }

    /// Parse a command id.
    pub fn from_id(id: &str) -> Option<Pacing> {
        Pacing::ALL.into_iter().find(|p| p.id() == id)
    }

    /// What the style does, in one line (Start screen).
    pub fn description(self) -> &'static str {
        match self {
            Pacing::Trailer => "A title per scene, cuts tightening toward each drop, end title card.",
            Pacing::Energetic => "Cuts on every beat or two, flash transitions, titles only at key moments.",
            Pacing::Cinematic => "Long holds weighted to wide shots, crossfades between scenes.",
            Pacing::Calm => "Gentle holds, an opening title only, dip to black between scenes.",
            Pacing::Family => "Even pacing, warm titles on every scene, weighted to close-ups.",
            Pacing::Hero => "Builds to a close-up at every drop, bold titles, hard cuts.",
        }
    }

    /// The style's own shape (Start screen v2 mock-up, 2026-10-16).
    pub fn shape(self) -> ResolvedShape {
        use SceneTransition::{Crossfade, Cut, Dip, Flash};
        let (titles, tighten, end_card, transition) = match self {
            Pacing::Trailer => (Titles::EveryScene, true, true, Cut),
            Pacing::Energetic => (Titles::KeyMoments, true, false, Flash),
            Pacing::Cinematic => (Titles::KeyMoments, false, true, Crossfade),
            Pacing::Calm => (Titles::OpeningOnly, false, true, Dip),
            Pacing::Family => (Titles::EveryScene, false, true, Crossfade),
            Pacing::Hero => (Titles::EveryScene, true, true, Cut),
        };
        ResolvedShape { titles, tighten, end_card, transition, overlays: false }
    }

    /// Tempos the profile suits, in BPM (Start screen v2: a ◆ when the track's tempo is inside).
    pub fn bpm_range(self) -> (f64, f64) {
        match self {
            Pacing::Calm => (60.0, 100.0),
            Pacing::Cinematic => (95.0, 120.0),
            Pacing::Energetic => (132.0, 180.0),
            Pacing::Trailer => (115.0, 135.0),
            Pacing::Family => (95.0, 125.0),
            Pacing::Hero => (118.0, 145.0),
        }
    }

    /// Whether the profile suits a track at `bpm`.
    pub fn suits(self, bpm: f64) -> bool {
        let (lo, hi) = self.bpm_range();
        bpm >= lo && bpm <= hi
    }

    /// The look of the profile's title cards: Calm and Cinematic fade in quietly, Energetic
    /// zooms, Trailer rises in bold.
    pub fn text_style(self) -> TextStyle {
        let (theme, animation) = match self {
            Pacing::Calm => (TextTheme::Minimal, TextAnimation::Fade),
            Pacing::Cinematic => (TextTheme::Cinematic, TextAnimation::Fade),
            Pacing::Energetic => (TextTheme::Bold, TextAnimation::Zoom),
            Pacing::Trailer => (TextTheme::Bold, TextAnimation::Rise),
            Pacing::Family => (TextTheme::Paper, TextAnimation::Fade),
            Pacing::Hero => (TextTheme::Bold, TextAnimation::Zoom),
        };
        // A preset picks its own in on purpose (Energetic zooms in on Bold); the rest of the
        // moves follow the theme.
        TextStyle { theme, animation: Some(animation), position: TextPosition::LowerThirdLeft, ..TextStyle::default() }
    }

    fn params(self) -> Params {
        match self {
            Pacing::Calm => Params {
                long_bars: 8.0,
                short_bars: 4.0,
                min_beats: 8,
                min_secs: 3.0,
                title_bars: 2,
                max_scene_bars: 32,
                cut_on_hits: false,
            },
            Pacing::Cinematic => Params {
                long_bars: 4.0,
                short_bars: 2.0,
                min_beats: 4,
                min_secs: 1.5,
                title_bars: 2,
                max_scene_bars: 32,
                cut_on_hits: true,
            },
            Pacing::Energetic => Params {
                long_bars: 2.0,
                short_bars: 0.5,
                min_beats: 2,
                min_secs: 0.6,
                title_bars: 1,
                max_scene_bars: 16,
                cut_on_hits: true,
            },
            Pacing::Trailer => Params {
                long_bars: 4.0,
                short_bars: 0.5,
                min_beats: 2,
                min_secs: 0.9,
                title_bars: 1,
                max_scene_bars: 16,
                cut_on_hits: true,
            },
            Pacing::Family => Params {
                long_bars: 2.0,
                short_bars: 1.0,
                min_beats: 4,
                min_secs: 1.5,
                title_bars: 2,
                max_scene_bars: 32,
                cut_on_hits: false,
            },
            Pacing::Hero => Params {
                long_bars: 2.0,
                short_bars: 0.5,
                min_beats: 2,
                min_secs: 0.9,
                title_bars: 1,
                max_scene_bars: 16,
                cut_on_hits: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Params {
    long_bars: f32,
    short_bars: f32,
    min_beats: u32,
    /// Shortest shot in seconds, whatever the tempo (fast songs would otherwise flicker).
    min_secs: f32,
    title_bars: u32,
    max_scene_bars: usize,
    cut_on_hits: bool,
}

/// What to generate.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateOptions {
    /// Storyboard name.
    pub name: String,
    /// Path of the audio file (the Track's asset id).
    pub track_path: String,
    /// Pacing profile.
    pub pacing: Pacing,
    /// Target length in seconds (picks a fit from the cue map); `None` = the whole track.
    pub seconds: Option<u32>,
    /// Force fade or edit; `None` = the cue map's choice.
    pub fit_mode: Option<FitMode>,
    /// Tone: how many shots, `0.5..=2.0` ([`DENSITY_RANGE`]); 1 is the profile as designed,
    /// 2 doubles the cuts (shots half as long), 0.5 halves them. Scales the shot lengths and the
    /// shortest shot together; title cards keep their length.
    pub density: f32,
    /// Overrides of the style's shape (titles, tighten, end card, transition, overlays).
    pub shape: Shape,
}

/// Allowed [`GenerateOptions::density`].
pub const DENSITY_RANGE: std::ops::RangeInclusive<f32> = 0.5..=2.0;

impl Params {
    /// The profile at `density` (clamped to [`DENSITY_RANGE`]; non-finite = 1).
    fn at_density(mut self, density: f32) -> Params {
        let d = if density.is_finite() { density.clamp(*DENSITY_RANGE.start(), *DENSITY_RANGE.end()) } else { 1.0 };
        self.long_bars /= d;
        self.short_bars /= d;
        self.min_beats = ((self.min_beats as f32 / d).round() as u32).max(1);
        self.min_secs = (self.min_secs / d).max(0.4);
        self
    }
}

/// What a generation did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GenerateReport {
    /// Clips made (title cards included).
    pub clips: usize,
    /// Scenes made.
    pub scenes: usize,
    /// Title cards (opening, scenes, end card).
    pub title_cards: usize,
    /// Cuts that land on a bar line.
    pub cuts_on_downbeats: usize,
    /// Cuts moved onto a hit or drop.
    pub cuts_on_hits: usize,
    /// Length of the Cut.
    pub length: Ticks,
    /// How the track was fitted (`Changes:` text), if it was.
    pub fit_changes: Option<String>,
}

/// Why nothing was generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateError {
    /// The cue map has no fit for this length (and the track is longer).
    NoFit {
        /// The length asked for.
        seconds: u32,
        /// The lengths the cue map has fits for.
        available: Vec<String>,
    },
    /// The result failed validation (a bug: reported, never panicked on).
    Invalid(String),
}

impl fmt::Display for GenerateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenerateError::NoFit { seconds, available } if available.is_empty() => {
                write!(f, "this track has no {seconds} s fit, and no fits at all")
            }
            GenerateError::NoFit { seconds, available } => {
                write!(f, "this track has no {seconds} s fit (available: {} s)", available.join(", "))
            }
            GenerateError::Invalid(e) => write!(f, "generated storyboard failed its checks: {e}"),
        }
    }
}

impl std::error::Error for GenerateError {}

/// One scene before clips: output-frame range, label, energy.
#[derive(Debug, Clone)]
struct SceneSpan {
    start: u64,
    end: u64,
    label: String,
    energy: f32,
}

/// One clip before ids: output-frame range and why.
#[derive(Debug, Clone)]
struct Span {
    start: u64,
    end: u64,
    shot: ShotType,
    description: String,
    reasons: Vec<String>,
    on_hit: Option<String>,
}

/// The cue map's fit for a `seconds` length (`None` = the full track, or a length within 2 s of
/// it). The one place that picks it: [`generate`] and the Start screen's Cut preview both use it.
pub fn fit_for(cue: &CueMap, seconds: Option<u32>, mode: Option<FitMode>) -> Result<Option<&Fit>, GenerateError> {
    let sr = u64::from(cue.sample_rate.max(1));
    let full_secs = cue.duration_frames / sr;
    match seconds {
        Some(s) if u64::from(s) + 2 < full_secs => {
            let available: Vec<String> = cue.fits.keys().cloned().collect();
            let choice = cue
                .fits
                .get(&s.to_string())
                .ok_or(GenerateError::NoFit { seconds: s, available: available.clone() })?;
            Ok(Some(choice.get(mode).ok_or(GenerateError::NoFit { seconds: s, available })?))
        }
        _ => Ok(None),
    }
}

/// The audio edit that plays `fit` of the track (`None` = the whole track, unedited).
pub fn track_edit(cue: &CueMap, fit: Option<&Fit>) -> Option<TrackEdit> {
    edit_of(cue, fit, &cue.timeline(fit))
}

/// The track a cue map makes at `seconds` (or whole), the way [`generate`] attaches it: the
/// song's tempo, length, beat grid, sections as song sections, and the Fit to length edit. For
/// a board that already has its clips — a template (`Storyboard::from_template`) — which
/// `recut` then fits to this track.
pub fn track_from_cue(
    cue: &CueMap,
    track_path: &str,
    seconds: Option<u32>,
    mode: Option<FitMode>,
) -> Result<Track, GenerateError> {
    let fit = fit_for(cue, seconds, mode)?;
    let tl = cue.timeline(fit);
    Ok(Track {
        id: AssetId(track_path.to_owned()),
        name: cue.track.title.clone(),
        bpm: Bpm::new(cue.bpm).unwrap_or(Bpm::DEFAULT),
        duration: cue.ticks(tl.length),
        beat_grid: tl.beats.iter().map(|f| cue.ticks(*f)).collect(),
        song_sections: tl
            .sections
            .iter()
            .map(|s| SongSection { name: s.label.clone(), start: cue.ticks(s.start), end: cue.ticks(s.end) })
            .collect(),
        edit: edit_of(cue, fit, &tl),
        level_db: 0,
        credit: Some(cue.track.credit.trim()).filter(|c| !c.is_empty()).map(str::to_owned),
    })
}

/// How a template's music runs under its sections (SR3; Paul, 2026-11-06: "the selected song
/// needs to use the 'chorus' section as a repeating background loop… the chorus music between
/// each title"; 2026-11-07: "use the intro for the first title, then rotate the chorus per
/// section, and for the final section use the outro… the section title should be the start of
/// the chorus audio").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MusicMode {
    /// The song from the top; it ends where it ends.
    Whole,
    /// Every chorus span of the cue map in order, joined with a one-beat crossfade, looped
    /// to the Cut's length; no chorus → the highest-energy section.
    ChorusLoop,
    /// The first chorus only, looped.
    FirstChorus,
    /// By the piece's parts: the song's intro under the opening scenes, a chorus starting on
    /// every section's title card (rotating through the choruses), the outro under the close.
    #[default]
    Structured,
}

impl MusicMode {
    /// The command parameter spelling.
    pub fn id(self) -> &'static str {
        match self {
            MusicMode::Whole => "whole",
            MusicMode::ChorusLoop => "chorus",
            MusicMode::FirstChorus => "firstChorus",
            MusicMode::Structured => "structured",
        }
    }

    /// From the command parameter spelling.
    pub fn parse(id: &str) -> Option<MusicMode> {
        match id {
            "whole" => Some(MusicMode::Whole),
            "chorus" => Some(MusicMode::ChorusLoop),
            "firstChorus" => Some(MusicMode::FirstChorus),
            "structured" => Some(MusicMode::Structured),
            _ => None,
        }
    }
}

/// A span of the Cut the music follows (SR3b): where it starts, how long it runs, what part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MusicSpan {
    /// Where in the Cut it starts.
    pub start: Ticks,
    /// How long it runs.
    pub length: Ticks,
    /// Opening, a section, or the close.
    pub part: ScenePart,
}

impl Storyboard {
    /// The music spans of this board: one per run of scenes with the same part, in Cut order
    /// — the opening scenes as one span, every section its own (a chorus starts on each), the
    /// closing scenes as one.
    pub fn music_spans(&self) -> Vec<MusicSpan> {
        let starts = self.starts();
        let mut out: Vec<MusicSpan> = Vec::new();
        for scene in &self.scenes {
            let clips: Vec<usize> =
                self.clips.iter().enumerate().filter(|(_, c)| c.scene_id == scene.id).map(|(i, _)| i).collect();
            let (Some(&first), Some(&last)) = (clips.first(), clips.last()) else { continue };
            let start = starts.get(first).copied().unwrap_or(Ticks::ZERO);
            let end = starts
                .get(last)
                .copied()
                .unwrap_or(Ticks::ZERO)
                .checked_add(self.clips[last].duration)
                .unwrap_or(Ticks::ZERO);
            let length = Ticks(end.0.saturating_sub(start.0));
            match out.last_mut() {
                Some(prev) if prev.part == scene.part && scene.part != ScenePart::Section => {
                    prev.length = Ticks(prev.length.0.saturating_add(length.0));
                }
                _ => out.push(MusicSpan { start, length, part: scene.part }),
            }
        }
        out
    }
}

/// The song's intro, choruses and outro as source spans snapped to bars (SR3b). Choruses as
/// [`loop_spans`]; the intro is the section labelled intro, else what comes before the first
/// chorus; the outro the section labelled outro, else what follows the last chorus. Each falls
/// back to the choruses when the song has nothing of its own.
struct SongParts {
    intro: Vec<[u64; 2]>,
    choruses: Vec<[u64; 2]>,
    outro: Vec<[u64; 2]>,
}

fn song_parts(cue: &CueMap) -> Option<SongParts> {
    let choruses = loop_spans(cue, false);
    if choruses.is_empty() {
        return None;
    }
    let sr = u64::from(cue.sample_rate.max(1));
    let beat = ((60.0 / cue.bpm.max(1.0)) * sr as f64).round().max(1.0) as u64;
    let bar = beat * u64::from(cue.beats_per_bar.max(1));
    let grid: &[u64] = if cue.downbeats.is_empty() { &cue.beats } else { &cue.downbeats };
    let snap = |[a, b]: [u64; 2]| -> Option<[u64; 2]> {
        let start = grid.iter().copied().find(|g| *g >= a).unwrap_or(a);
        let end = grid.iter().copied().rev().find(|g| *g <= b).unwrap_or(b);
        (end > start && end - start >= bar.min(sr)).then_some([start, end])
    };
    let labelled = |word: &str| -> Vec<[u64; 2]> {
        cue.sections
            .iter()
            .filter(|s| s.label.to_ascii_lowercase().contains(word))
            .filter_map(|s| snap([s.start, s.end]))
            .collect()
    };
    let first_chorus = choruses.first().map_or(0, |c| c[0]);
    let last_chorus = choruses.last().map_or(0, |c| c[1]);
    let mut intro = labelled("intro");
    if intro.is_empty() {
        intro = snap([0, first_chorus]).into_iter().collect();
    }
    let mut outro = labelled("outro");
    if outro.is_empty() {
        outro = snap([last_chorus, cue.duration_frames]).into_iter().collect();
    }
    Some(SongParts { intro, choruses, outro })
}

/// Fill `want` frames from `source` spans played in order and repeated, pushing pieces onto
/// `segments`; a tail shorter than a beat goes to the previous piece.
fn fill_from(segments: &mut Vec<[u64; 2]>, source: &[[u64; 2]], want: u64, beat: u64, song_len: u64) -> u64 {
    let mut have = 0u64;
    if source.is_empty() || want == 0 {
        return 0;
    }
    'outer: while have < want {
        for &[a, b] in source {
            if have >= want || segments.len() >= LOOP_MAX_SEGMENTS {
                break 'outer;
            }
            let left = want - have;
            let len = (b - a).min(left);
            if len < beat
                && let Some(last) = segments.last_mut()
                && last[1] + len <= song_len
            {
                last[1] += len;
                have += len;
                break 'outer;
            }
            segments.push([a, a + len]);
            have += len;
        }
    }
    have
}

/// The music by the piece's parts (SR3b): the intro under the opening span, a chorus starting
/// at every section span (rotating through the song's choruses), the outro under the closing
/// span — each cut or looped to its span's length, every splice a one-beat crossfade, a
/// two-second fade at the end. `None` when the song gives nothing to work with.
pub fn structured_fit(cue: &CueMap, spans: &[MusicSpan]) -> Option<Fit> {
    let parts = song_parts(cue)?;
    if spans.is_empty() {
        return None;
    }
    let sr = u64::from(cue.sample_rate.max(1));
    let beat = ((60.0 / cue.bpm.max(1.0)) * sr as f64).round().max(1.0) as u64;
    let mut segments: Vec<[u64; 2]> = Vec::new();
    let mut have = 0u64;
    let mut chorus_i = 0usize;
    for span in spans {
        let want = cue.frames(span.length);
        // Every span starts where the Cut says: the music so far is padded to it (silence is
        // never inserted: the previous piece is extended into the song, or cut).
        let at = cue.frames(span.start);
        if at > have
            && let Some(last) = segments.last_mut()
        {
            let extra = (at - have).min(cue.duration_frames.saturating_sub(last[1]));
            last[1] += extra;
            have += extra;
        }
        let source: Vec<[u64; 2]> = match span.part {
            ScenePart::Opening => parts.intro.clone(),
            ScenePart::Close => parts.outro.clone(),
            ScenePart::Section => {
                let c = parts.choruses[chorus_i % parts.choruses.len()];
                chorus_i += 1;
                vec![c]
            }
        };
        let source = if source.is_empty() { parts.choruses.clone() } else { source };
        have += fill_from(&mut segments, &source, want, beat, cue.duration_frames);
    }
    if segments.is_empty() {
        return None;
    }
    let length = have;
    let fade = ((LOOP_FADE_SECS * sr as f64) as u64).min(length / 2).max(1);
    let sections = spans.iter().filter(|s| s.part == ScenePart::Section).count();
    Some(Fit {
        mode: FitMode::Fade,
        length_frames: length,
        segments,
        crossfade_frames: beat,
        fade_out: Some(crate::cuemap::FadeOut { start: length - fade, frames: fade }),
        similarity: None,
        reasons: vec![format!(
            "Intro under the opening, {} chorus{} over {sections} section{}, the outro under the close",
            parts.choruses.len(),
            if parts.choruses.len() == 1 { "" } else { "es" },
            if sections == 1 { "" } else { "s" }
        )],
        changes: "Intro · a chorus per section · outro".into(),
    })
}

/// Most pieces a loop is built from (a one-bar chorus under a six-hour Cut stops here; the
/// music ends and the rest is silent).
pub const LOOP_MAX_SEGMENTS: usize = 2048;
/// The fade at the end of a loop, in seconds.
pub const LOOP_FADE_SECS: f64 = 2.0;

/// The loop source: every section labelled chorus (or the first, with `first`), snapped to
/// whole bars on the downbeat grid; no chorus → the highest-energy section. Empty when the
/// cue map has no sections at all.
fn loop_spans(cue: &CueMap, first: bool) -> Vec<[u64; 2]> {
    let sr = u64::from(cue.sample_rate.max(1));
    let beat = ((60.0 / cue.bpm.max(1.0)) * sr as f64).round().max(1.0) as u64;
    let bar = beat * u64::from(cue.beats_per_bar.max(1));
    let grid: &[u64] = if cue.downbeats.is_empty() { &cue.beats } else { &cue.downbeats };
    let snap = |[a, b]: [u64; 2]| -> Option<[u64; 2]> {
        let start = grid.iter().copied().find(|g| *g >= a).unwrap_or(a);
        let end = grid.iter().copied().rev().find(|g| *g <= b).unwrap_or(b);
        (end > start && end - start >= bar.min(sr)).then_some([start, end])
    };
    let mut chorus: Vec<[u64; 2]> = cue
        .sections
        .iter()
        .filter(|s| s.label.to_ascii_lowercase().contains("chorus"))
        .filter_map(|s| snap([s.start, s.end]))
        .collect();
    if chorus.is_empty() {
        chorus = cue
            .sections
            .iter()
            .max_by(|a, b| a.energy.total_cmp(&b.energy))
            .and_then(|s| snap([s.start, s.end]))
            .into_iter()
            .collect();
    }
    if first {
        chorus.truncate(1);
    }
    chorus
}

/// A loop of the song's chorus (SR3) long enough for `total`: the spans from [`loop_spans`]
/// played in order and repeated, each splice crossfaded over a beat, the last piece cut to
/// length, a two-second fade at the end. `None` when the cue map gives nothing to loop.
pub fn chorus_loop(cue: &CueMap, total: Ticks, first: bool) -> Option<Fit> {
    let spans = loop_spans(cue, first);
    if spans.is_empty() {
        return None;
    }
    let sr = u64::from(cue.sample_rate.max(1));
    let beat = ((60.0 / cue.bpm.max(1.0)) * sr as f64).round().max(1.0) as u64;
    let want = cue.frames(total).max(1);
    let mut segments: Vec<[u64; 2]> = Vec::new();
    let mut have = 0u64;
    'outer: while have < want {
        for &[a, b] in &spans {
            if have >= want || segments.len() >= LOOP_MAX_SEGMENTS {
                break 'outer;
            }
            let left = want - have;
            let len = (b - a).min(left);
            // A tail shorter than a beat would starve the crossfade: give it to the previous
            // piece instead (the song continues past the chorus for a moment).
            if len < beat
                && let Some(last) = segments.last_mut()
                && last[1] + len <= cue.duration_frames
            {
                last[1] += len;
                have += len;
                break 'outer;
            }
            segments.push([a, a + len]);
            have += len;
        }
    }
    let length = have.min(want);
    let fade = ((LOOP_FADE_SECS * sr as f64) as u64).min(length / 2).max(1);
    let n = spans.len();
    Some(Fit {
        mode: FitMode::Fade,
        length_frames: length,
        segments,
        crossfade_frames: beat,
        fade_out: Some(crate::cuemap::FadeOut { start: length - fade, frames: fade }),
        similarity: None,
        reasons: vec![format!("Chorus loop: {n} span{} repeated", if n == 1 { "" } else { "s" })],
        changes: "Chorus looped under the Cut".into(),
    })
}

/// The track a template plays under a board ([`track_from_cue`] with the music mode): the
/// whole song, the chorus loop cut to the Cut's length, or the structured music by the
/// board's parts (the whole song when the cue map gives nothing to work with). The second
/// value says what was done, for the message.
pub fn track_under(cue: &CueMap, track_path: &str, board: &Storyboard, music: MusicMode) -> (Track, &'static str) {
    let total = board.total();
    let first = music == MusicMode::FirstChorus;
    let fit = match music {
        MusicMode::Whole => None,
        MusicMode::ChorusLoop | MusicMode::FirstChorus => chorus_loop(cue, total, first),
        MusicMode::Structured => structured_fit(cue, &board.music_spans()),
    };
    let what = match (&fit, music) {
        (Some(_), MusicMode::FirstChorus) => "the first chorus looped",
        (Some(_), MusicMode::Structured) => "intro · a chorus per section · outro",
        (Some(_), _) => "the chorus looped",
        (None, MusicMode::Whole) => "the whole song",
        (None, _) => "the whole song (nothing to loop)",
    };
    let tl = cue.timeline(fit.as_ref());
    let track = Track {
        id: AssetId(track_path.to_owned()),
        name: cue.track.title.clone(),
        bpm: Bpm::new(cue.bpm).unwrap_or(Bpm::DEFAULT),
        duration: cue.ticks(tl.length),
        beat_grid: tl.beats.iter().map(|f| cue.ticks(*f)).collect(),
        song_sections: tl
            .sections
            .iter()
            .map(|s| SongSection { name: s.label.clone(), start: cue.ticks(s.start), end: cue.ticks(s.end) })
            .collect(),
        edit: edit_of(cue, fit.as_ref(), &tl),
        level_db: 0,
        credit: Some(cue.track.credit.trim()).filter(|c| !c.is_empty()).map(str::to_owned),
    };
    (track, what)
}

fn edit_of(cue: &CueMap, fit: Option<&Fit>, tl: &Timeline) -> Option<TrackEdit> {
    fit.map(|f| TrackEdit {
        sample_rate: cue.sample_rate,
        segments: f.segments.clone(),
        crossfade_frames: f.crossfade_frames,
        fade_out: tl.fade_out.as_ref().map(|o| [o.start, o.frames]),
        length_frames: tl.length,
    })
}

/// Generate a storyboard from a cue map.
pub fn generate(cue: &CueMap, opts: &GenerateOptions) -> Result<(Storyboard, GenerateReport), GenerateError> {
    let sr = u64::from(cue.sample_rate.max(1));
    let fit = fit_for(cue, opts.seconds, opts.fit_mode)?;
    let tl = cue.timeline(fit);
    let p = opts.pacing.params().at_density(opts.density);
    let shape = opts.shape.resolve(opts.pacing);
    let bpb = u64::from(cue.beats_per_bar.max(1));
    let beat_frames = ((60.0 / cue.bpm) * sr as f64).round().max(1.0) as u64;
    let bar_frames = beat_frames * bpb;

    let bars = grid(&tl.downbeats, tl.length, bar_frames);
    let beats = grid(&tl.beats, tl.length, beat_frames);
    let bar_set: HashSet<u64> = tl.downbeats.iter().copied().collect();

    // 2. Scenes.
    let scenes = scene_spans(&tl, &bars, bar_frames, p.max_scene_bars);

    // 3–6. Clips.
    let end_card_len = if shape.end_card && tl.length > 20 * sr { 2 * bar_frames } else { 0 };
    let end_card_start = snap_down(&bars, tl.length.saturating_sub(end_card_len));
    let min_len = (u64::from(p.min_beats) * beat_frames).max((f64::from(p.min_secs) * sr as f64) as u64);
    let mut spans: Vec<(usize, Span)> = Vec::new();
    for (si, sc) in scenes.iter().enumerate() {
        let scene_end = if end_card_len > 0 && sc.end > end_card_start { end_card_start.max(sc.start) } else { sc.end };
        let mut cursor = sc.start;
        let has_drop = tl.hits.iter().any(|h| h.kind == "drop" && h.frame >= sc.start && h.frame < sc.end);
        // A drop on the scene's first bar: the scene's first shot (after any title) lands on it.
        let opens_on_drop =
            tl.hits.iter().any(|h| h.kind == "drop" && h.frame >= sc.start && h.frame < sc.start + bar_frames);
        let title = match shape.titles {
            Titles::EveryScene => true,
            Titles::KeyMoments => si == 0 || has_drop || sc.label == "Chorus",
            Titles::OpeningOnly => si == 0,
            Titles::None => false,
        };
        if title && scene_end > cursor {
            let want = cursor + u64::from(p.title_bars) * bar_frames;
            let mut t_end = snap_near(&bars, want, cursor + min_len.min(bar_frames), scene_end);
            if scene_end.saturating_sub(t_end) < min_len {
                t_end = scene_end;
            }
            let description = if si == 0 { "Opening title".to_string() } else { format!("{} title", sc.label) };
            spans.push((
                si,
                Span {
                    start: cursor,
                    end: t_end,
                    shot: ShotType::Ttl,
                    description,
                    reasons: vec![format!("{} starts", sc.label)],
                    on_hit: None,
                },
            ));
            cursor = t_end;
        }
        let position = sc.start as f32 / tl.length.max(1) as f32;
        let energy = if shape.tighten { sc.energy.max(position * 0.9) } else { sc.energy };
        let shot_bars = p.long_bars - (p.long_bars - p.short_bars) * energy.clamp(0.0, 1.0);
        // Shots under one bar step on beats, but never below the profile's shortest shot.
        let (step, points) = if shot_bars >= 1.0 {
            ((shot_bars.round().max(1.0) as u64) * bar_frames, &bars)
        } else {
            ((shot_bars * bpb as f32).round().max(1.0) as u64 * beat_frames, &beats)
        };
        let step = step.max(min_len);
        let mut k = 0usize;
        while cursor < scene_end {
            let target = cursor + step;
            let mut end = snap_near(points, target, cursor + min_len, scene_end);
            let mut on_hit = None;
            if p.cut_on_hits
                && let Some(h) = tl
                    .hits
                    .iter()
                    .find(|h| h.strength >= 0.5 && h.frame > cursor + min_len && h.frame + min_len / 2 < end)
            {
                end = h.frame;
                on_hit = Some(h.kind.clone());
            }
            if scene_end.saturating_sub(end) < min_len {
                end = scene_end;
            }
            let after_drop =
                spans.last().and_then(|(_, s)| s.on_hit.as_deref()) == Some("drop") || (k == 0 && opens_on_drop);
            let shot = pick_shot(opts.pacing, si, k, sc.energy, after_drop);
            let first_bar = bar_index(&bars, cursor) + 1;
            let last_bar = bar_index(&bars, end.saturating_sub(1)) + 1;
            let mut reasons = Vec::new();
            if bar_set.contains(&cursor) {
                reasons.push(format!("starts on bar {first_bar}"));
            }
            if let Some(kind) = spans.last().and_then(|(_, s)| s.on_hit.clone()) {
                reasons.push(format!("cut on the {kind}"));
            }
            let description = if first_bar == last_bar {
                format!("{} · bar {first_bar}", sc.label)
            } else {
                format!("{} · bars {first_bar}–{last_bar}", sc.label)
            };
            spans.push((si, Span { start: cursor, end, shot, description, reasons, on_hit }));
            cursor = end;
            k += 1;
        }
    }
    if end_card_len > 0 && end_card_start < tl.length {
        let si = scenes.len().saturating_sub(1);
        spans.push((
            si,
            Span {
                start: end_card_start,
                end: tl.length,
                shot: ShotType::Ttl,
                description: "End card: title and date".into(),
                reasons: vec!["final bars".into()],
                on_hit: None,
            },
        ));
    }

    // Build the storyboard.
    let mut board = Storyboard::new(opts.name.clone());
    board.format = Some(opts.pacing.name().to_owned());
    let length_label = match opts.seconds.filter(|_| fit.is_some()) {
        Some(s) => format!("{}:{:02}", s / 60, s % 60),
        None => "full length".into(),
    };
    board.preset = Some(format!("{} · {length_label}", cue.track.title));
    board.scenes = scenes
        .iter()
        .enumerate()
        .map(|(i, s)| Scene {
            id: SceneId(i as u32 + 1),
            song_section: Some(s.label.clone()),
            inset: None,
            part: ScenePart::Section,
        })
        .collect();
    let mut report = GenerateReport {
        scenes: scenes.len(),
        fit_changes: fit.map(|f| f.changes.clone()),
        ..GenerateReport::default()
    };
    let n = spans.len();
    let (scene_out, scene_in) = shape.transition.transitions();
    let mut prev_scene = None;
    for (i, (si, s)) in spans.into_iter().enumerate() {
        let start_t = cue.ticks(s.start);
        let end_t = cue.ticks(s.end);
        let mut clip =
            Clip::new(ClipId(i as u32 + 1), SceneId(si as u32 + 1), s.shot, Ticks(end_t.0.saturating_sub(start_t.0)));
        clip.description = s.description;
        clip.notes = s.reasons.join("; ");
        clip.sync_to_downbeat = bar_set.contains(&s.start);
        if i > 0 && clip.notes.contains("cut on the drop") {
            clip.transition_in = Transition::SmashCut;
        }
        if s.shot == ShotType::Ttl {
            clip.text_style = opts.pacing.text_style();
            report.title_cards += 1;
        }
        if i > 0 && bar_set.contains(&s.start) {
            report.cuts_on_downbeats += 1;
        }
        if s.on_hit.is_some() {
            report.cuts_on_hits += 1;
        }
        if i == 0 {
            clip.transition_in = Transition::FadeIn;
        }
        if i + 1 == n && tl.fade_out.is_some() {
            clip.transition_out = Transition::FadeOut;
        }
        // The style's transition where one scene hands over to the next (a smash cut on a drop stays).
        if i > 0 && prev_scene != Some(si) {
            if clip.transition_in == Transition::Cut {
                clip.transition_in = scene_in;
            }
            if let Some(last) = board.clips.last_mut()
                && last.transition_out == Transition::Cut
            {
                last.transition_out = scene_out;
            }
        }
        prev_scene = Some(si);
        board.clips.push(clip);
    }
    // Text overlays: scene titles after the opening become text over a shot (the end card stays a card).
    if shape.overlays {
        let scene_titles: Vec<ClipId> = board
            .clips
            .iter()
            .enumerate()
            .filter(|(i, c)| *i > 0 && c.shot_type == ShotType::Ttl && !c.description.starts_with("End card"))
            .map(|(_, c)| c.id)
            .collect();
        for id in scene_titles {
            let _ = set_text_kind(&mut board, id, TextKind::Overlay);
        }
    }
    report.clips = board.clips.len();
    report.length = cue.ticks(tl.length);
    board.track = Some(Track {
        id: AssetId(opts.track_path.clone()),
        name: cue.track.title.clone(),
        bpm: Bpm::new(cue.bpm).unwrap_or(Bpm::DEFAULT),
        duration: cue.ticks(tl.length),
        beat_grid: tl.beats.iter().map(|f| cue.ticks(*f)).collect(),
        song_sections: scenes
            .iter()
            .map(|s| SongSection { name: s.label.clone(), start: cue.ticks(s.start), end: cue.ticks(s.end) })
            .collect(),
        edit: edit_of(cue, fit, &tl),
        level_db: 0,
        credit: Some(cue.track.credit.trim()).filter(|c| !c.is_empty()).map(str::to_owned),
    });
    board.validate(&Limits::default()).map_err(|e| GenerateError::Invalid(e.to_string()))?;
    if board.total() != report.length {
        return Err(GenerateError::Invalid(format!(
            "clips add up to {:?}, the track is {:?}",
            board.total(),
            report.length
        )));
    }
    Ok((board, report))
}

/// Grid points: the detected ones, continued at `step` before the first and after the last (beat
/// trackers stop before a ring-out or start after a quiet intro), or an even grid when there are
/// too few; always includes `0` and `length`.
fn grid(points: &[u64], length: u64, step: u64) -> Vec<u64> {
    let step = step.max(1);
    let mut g: Vec<u64> = if points.len() >= 2 {
        let mut g = points.to_vec();
        if let Some(&first) = points.first() {
            let mut f = first;
            while f >= step {
                f -= step;
                g.push(f);
            }
        }
        if let Some(&last) = points.last() {
            let mut f = last.saturating_add(step);
            while f < length {
                g.push(f);
                f = f.saturating_add(step);
            }
        }
        g
    } else {
        (0..).map(|i| i * step).take_while(|f| *f < length).collect()
    };
    g.push(0);
    g.push(length);
    g.retain(|f| *f <= length);
    g.sort_unstable();
    g.dedup();
    g
}

/// The grid point nearest `target` within `[lo, hi]`, else `hi`.
fn snap_near(points: &[u64], target: u64, lo: u64, hi: u64) -> u64 {
    points.iter().copied().filter(|p| *p >= lo && *p <= hi).min_by_key(|p| p.abs_diff(target)).unwrap_or(hi)
}

/// The last grid point at or before `f`.
fn snap_down(points: &[u64], f: u64) -> u64 {
    points.iter().copied().filter(|p| *p <= f).max().unwrap_or(0)
}

/// Index of the bar containing `f` (0-based).
fn bar_index(bars: &[u64], f: u64) -> usize {
    bars.iter().filter(|b| **b <= f).count().saturating_sub(1)
}

/// Scenes from sections: short ones merged, long ones split at 16-bar phrases.
fn scene_spans(tl: &Timeline, bars: &[u64], bar_frames: u64, max_bars: usize) -> Vec<SceneSpan> {
    let mut merged: Vec<SceneSpan> = Vec::new();
    for s in &tl.sections {
        let span = SceneSpan { start: s.start, end: s.end, label: s.label.clone(), energy: s.energy };
        match merged.last_mut() {
            Some(m) if s.end - s.start < 4 * bar_frames => {
                m.end = s.end;
                m.energy = m.energy.max(s.energy);
            }
            _ => merged.push(span),
        }
    }
    if merged.len() > 1 && merged[0].end - merged[0].start < 4 * bar_frames {
        let first = merged.remove(0);
        merged[0].start = first.start;
    }
    let mut out = Vec::new();
    for s in merged {
        let inside: Vec<u64> = bars.iter().copied().filter(|b| *b > s.start && *b < s.end).collect();
        if inside.len() < max_bars {
            out.push(s);
            continue;
        }
        let mut start = s.start;
        for (i, b) in inside.iter().enumerate() {
            let n = i + 1;
            if n % 16 == 0 && s.end - *b >= 8 * bar_frames {
                out.push(SceneSpan { start, end: *b, label: s.label.clone(), energy: s.energy });
                start = *b;
            }
        }
        out.push(SceneSpan { start, end: s.end, label: s.label.clone(), energy: s.energy });
    }
    out
}
