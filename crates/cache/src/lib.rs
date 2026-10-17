//! The suite cache (`docs/design/cache.md`, phase C1): disposable, regenerable data the app owns
//! (thumbnails, waveforms, track analysis), kept between runs the way Bridge keeps its cache.
//!
//! - [`root`]: where a cache lives on Windows, macOS and Linux (R1–R3): a setting, then
//!   `<APP>_CACHE_DIR` (empty = memory only), then portable mode, then the platform folder; the
//!   shared `Crafting` cache for results any Crafting app can reuse (R19–R22).
//! - [`Cache`]: one kind in one root, `<root>/v<N>/<kind>/<2 hex>/<32 hex>.crft` (R4–R6), keyed
//!   by [`Hash128`] (R7), each file an [`entry`] that names its source and carries a checksum
//!   (R8, R9), written to a temp file and renamed (R10–R14), within a budget (R15, R16).
//! - Memory only when no folder can be used: same API, nothing touches the disk.
//!
//! The cache is AssetCraft's own data, so writing it is allowed before Stage 2 (AGENTS.md rule 5
//! covers the user's files); it never writes anywhere else. Every file it reads is hostile input:
//! a bad one is a miss and is deleted, never a panic.

pub mod entry;
pub mod hash;
pub mod root;
pub mod source;
pub mod store;

pub use hash::{Hash128, Hasher128};
pub use root::{App, Os, Resolved, Rule};
pub use source::SourceId;
pub use store::{Cache, KindInfo};

#[cfg(test)]
mod tests;
