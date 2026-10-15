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
//! - [`fill`]: Quick fill, placing assets into empty clips in order.
//! - [`sample`]: the handoff's 24-clip sample board (placeholder content).
//! - [`validate`]: loading a storyboard document from bytes as hostile input, with [`Limits`].
//!
//! Commands that change a storyboard live in the engine (L5); this crate only answers "what
//! would the result be", so every rule is testable without a session.

pub mod fill;
pub mod model;
pub mod recut;
pub mod sample;
pub mod time;
pub mod validate;

pub use fill::{FillError, auto_fill, clear, next_empty, place};
pub use model::{
    AssetId, Clip, ClipId, Scene, SceneId, ShotType, SongSection, Story, Storyboard, TextClip, Track, Transition,
};
pub use recut::{RecutOptions, RecutReport, recut};
pub use sample::sample;
pub use time::{Bpm, TICKS_PER_SECOND, Ticks};
pub use validate::{BoardError, FORMAT_VERSION, Limits};

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests;
