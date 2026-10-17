//! Storyboard model for AssetCraft's Storyboard mode (Plan, Stage 4).
//!
//! Layer 3; depends on no workspace crate. Pure data and rules, no I/O and no UI:
//! - [`model`]: [`Storyboard`], [`Clip`], [`Scene`], [`Track`], [`Story`], [`TextClip`], using the
//!   binding vocabulary of the Storyboard Mode design handoff (Organizer, Storyboard, Story,
//!   Asset, Clip, Scene, Title card, Format › Preset, Track, Beat grid, Shape to music, Re-cut,
//!   Quick fill, Shot map, Cut).
//! - [`time`]: exact time in [`Ticks`] (FilmCraft's 254,016,000,000 per second, so nothing drifts)
//!   and beat arithmetic.
//! - [`recut`]: the Re-cut rule that fits clip durations to a track's beat grid.
//! - [`cuemap`]: a track's musical timing (beats, bars, sections, hits, Fit to length) as data.
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

pub mod cuemap;
pub mod fill;
pub mod generate;
pub mod model;
pub mod recut;
pub mod sample;
pub mod shape;
pub mod text;
pub mod time;
pub mod validate;

pub use cuemap::{CUEMAP_VERSION, CueMap, CueMapError, Fit, FitMode, Timeline};
pub use fill::{Deleted, FillError, auto_fill, clear, delete, next_empty, place};
pub use generate::{DENSITY_RANGE, GenerateError, GenerateOptions, GenerateReport, Pacing, generate};
pub use model::{
    AssetId, Clip, ClipId, Scene, SceneId, ShotType, SongSection, Story, Storyboard, TextClip, Track, TrackEdit,
    Transition,
};
pub use recut::{RecutOptions, RecutReport, recut};
pub use sample::sample;
pub use shape::{ResolvedShape, SceneTransition, Shape, Titles};
pub use text::{
    AnimFrame, KindChange, Look, TextAnimation, TextError, TextKind, TextPosition, TextStyle, TextTheme, set_text_kind,
    set_text_style,
};
pub use time::{Bpm, TICKS_PER_SECOND, Ticks};
pub use validate::{BoardError, FORMAT_VERSION, Limits};

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests;
