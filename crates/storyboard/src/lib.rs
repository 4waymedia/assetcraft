//! Storyboard model for AssetCraft's Storyboard mode (Plan, Stage 4).
//!
//! Layer 3; depends only on `assetcraft-analysis` (L2). Pure data and rules, no I/O and no UI:
//! - [`model`]: [`Storyboard`], [`Clip`], [`Scene`], [`Track`], [`Story`], [`TextClip`], using the
//!   binding vocabulary of the Storyboard Mode design handoff (Organizer, Storyboard, Story,
//!   Asset, Clip, Scene, Title card, Format › Preset, Track, Beat grid, Shape to music, Re-cut,
//!   Quick fill, Shot map, Cut).
//! - [`time`]: exact time in [`Ticks`] (FilmCraft's 254,016,000,000 per second, so nothing drifts)
//!   and beat arithmetic.
//! - [`recut`]: the Re-cut rule that fits clip durations to a track's beat grid.
//! - [`cuemap`]: a track's musical timing (beats, bars, sections, hits, Fit to length) as data.
//! - [`analyse`]: a cue map from the analysis curves and tempo (the Rust `tools/cuemap.py`, phase A2c).
//! - [`generate`]: a storyboard from a cue map and a pacing profile.
//! - [`text`]: text clips, a Card or an Overlay, with theme, animation and position.
//! - [`shape`]: a preset's shape on top of a pacing profile (titles, tighten, end card, transition,
//!   overlays, shot mix).
//! - [`fill`]: Quick fill, placing assets into empty clips in order.
//! - [`sample`]: the handoff's 24-clip sample board (placeholder content).
//! - [`validate`]: loading a storyboard document from bytes as hostile input, with [`Limits`].
//!
//! Commands that change a storyboard live in the engine (L5); this crate only answers "what
//! would the result be", so every rule is testable without a session.

pub mod analyse;
pub mod cuemap;
pub mod decor;
pub mod fill;
pub mod generate;
pub mod inset;
pub mod model;
pub mod motion;
pub mod recut;
pub mod sample;
pub mod shape;
pub mod template;
pub mod text;
pub mod theme;
pub mod time;
pub mod validate;

pub use analyse::{ANALYZER, AnalyseError, FIT_TARGETS, TrackFacts, cue_map};
pub use cuemap::{CUEMAP_VERSION, CueMap, CueMapError, Fit, FitMode, Timeline};
pub use decor::{
    DecorRect, FinishPass, LookDecor, MAX_PATTERN_SHAPES, Pattern, Rules, TextFinish, card_decor, finish_passes,
    text_decor,
};
pub use fill::{Deleted, FillError, auto_fill, clear, delete, next_empty, place};
pub use generate::{
    DENSITY_RANGE, GenerateError, GenerateOptions, GenerateReport, LOOP_FADE_SECS, LOOP_MAX_SEGMENTS, MusicMode,
    MusicSpan, Pacing, chorus_loop, fit_for, generate, structured_fit, track_edit, track_from_cue, track_under,
};
pub use inset::{Corner, DEFAULT_MUSIC_DB, Inset, InsetKind, InsetSize, MUSIC_DB_RANGE, VOICE_DB_RANGE};
pub use model::{
    AssetId, Clip, ClipId, Scene, SceneId, ScenePart, ShotType, SongSection, Story, Storyboard, TextClip, Track,
    TrackEdit, Transition, Voiceover,
};
pub use motion::{
    Camera, Ease, MAX_PITCH, Motion, MotionEdit, MotionError, MotionKind, MotionPreset, STILL_PAN_TRAVEL,
    STILL_PAN_ZOOM, clear_motion, set_motion, swap_motion,
};
pub use recut::{RecutOptions, RecutReport, recut};
pub use sample::sample;
pub use shape::{ResolvedShape, SceneTransition, Shape, Titles};
pub use template::{
    BUNDLED, CREDITS_NAME, STEP_PATTERN, STEP_SECS, STEP_SECS_RANGE, STEPS_RANGE, TITLE_SECS, TemplateInfo,
    bundled as bundled_template, bundled_sized as bundled_template_sized, bundled_with as bundled_template_with,
    credits as credits_template,
};
pub use text::{
    AnimFrame, Backdrop, KindChange, Look, MAX_TEXT_MS, MIN_TEXT_MS, TextAnimation, TextEffect, TextError, TextExit,
    TextKind, TextPosition, TextStyle, TextTheme, accent_bar, set_text_kind, set_text_style, set_text_timing,
};
pub use theme::{
    BUNDLED_FONTS, Background, BackgroundKind, Decor, Face, MAX_THEME_BYTES, THEME_SUFFIX, Theme, ThemeError, contrast,
    luminance, parse_color,
};
pub use time::{Bpm, TICKS_PER_SECOND, Ticks};
pub use validate::{BoardError, FORMAT_VERSION, Limits};

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod analyse_tests;
#[cfg(test)]
mod tests;
