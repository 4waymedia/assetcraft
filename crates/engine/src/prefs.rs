//! Settings that survive a restart. The app decides where the JSON lives; the engine only
//! turns state into [`Prefs`] and back, and tolerates missing or unknown fields.

use crate::sort::SortKey;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Grid of thumbnails or a details list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    /// Thumbnail grid.
    #[default]
    Grid,
    /// Details list.
    List,
}

/// Content-panel view settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewPrefs {
    /// Grid or list.
    pub mode: ViewMode,
    /// Thumbnail side in pixels.
    pub thumb_size: u32,
    /// Sort key.
    pub sort: SortKey,
    /// Sort direction.
    pub ascending: bool,
    /// Show hidden files.
    pub show_hidden: bool,
}

/// Thumbnail sizes the slider allows.
pub const THUMB_SIZE_RANGE: (u32, u32) = (48, 512);

impl Default for ViewPrefs {
    fn default() -> Self {
        Self { mode: ViewMode::Grid, thumb_size: 160, sort: SortKey::Name, ascending: true, show_hidden: false }
    }
}

/// Everything persisted between sessions.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// File format version, for future migrations.
    pub version: u32,
    /// Favorite folders, in the user's order.
    pub favorites: Vec<PathBuf>,
    /// The folder open when the app last closed.
    pub last_folder: Option<PathBuf>,
    /// View settings.
    pub view: ViewPrefs,
}

/// Current [`Prefs::version`].
pub const PREFS_VERSION: u32 = 1;

impl Prefs {
    /// Parse saved prefs. Anything unreadable falls back to defaults rather than failing startup.
    pub fn from_json(text: &str) -> Self {
        let mut prefs: Prefs = serde_json::from_str(text).unwrap_or_default();
        prefs.view.thumb_size = prefs.view.thumb_size.clamp(THUMB_SIZE_RANGE.0, THUMB_SIZE_RANGE.1);
        prefs.version = PREFS_VERSION;
        prefs
    }

    /// Serialize for saving.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_junk() {
        let mut p = Prefs::default();
        p.favorites.push("/shots".into());
        p.view.mode = ViewMode::List;
        let back = Prefs::from_json(&p.to_json());
        assert_eq!(back.favorites, p.favorites);
        assert_eq!(back.view.mode, ViewMode::List);
        assert_eq!(Prefs::from_json("not json").view, ViewPrefs::default());
        assert_eq!(Prefs::from_json(r#"{"view":{"thumb_size":9999},"extra":1}"#).view.thumb_size, THUMB_SIZE_RANGE.1);
    }
}
