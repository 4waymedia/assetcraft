//! Settings that survive a restart. The app decides where the JSON lives; the engine only
//! turns state into [`Prefs`] and back, and tolerates missing or unknown fields.

use crate::sort::SortKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

/// Colour theme. Dark by default, like Bridge and the sibling Crafting Apps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Follow the operating system's light / dark setting.
    System,
    /// Dark.
    #[default]
    Dark,
    /// Light.
    Light,
}

impl ThemeMode {
    /// Parse `system` / `dark` / `light`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "system" => Some(Self::System),
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }

    /// The command-parameter name.
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
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
    /// Colour theme.
    pub theme: ThemeMode,
}

/// Which optional formats are decoded (saved as `formats` in prefs). Everything is off by
/// default; a missing or junk value reads as off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatPrefs {
    /// HEIF/HEIC thumbnails (HEVC; patent-encumbered, so opt-in). Has an effect only in a build
    /// with the `heic` feature.
    pub heic: bool,
}

/// Thumbnail sizes the slider allows.
pub const THUMB_SIZE_RANGE: (u32, u32) = (48, 512);

impl Default for ViewPrefs {
    fn default() -> Self {
        Self {
            mode: ViewMode::Grid,
            thumb_size: 160,
            sort: SortKey::Name,
            ascending: true,
            show_hidden: false,
            theme: ThemeMode::Dark,
        }
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
    /// The window's panel arrangement, owned by the UI layer and stored here as opaque JSON
    /// (see the `view.panels` command). `None` means the UI's default layout.
    pub panels: Option<Value>,
    /// Optional formats (`formats.heic`).
    pub formats: FormatPrefs,
}

/// Largest panel-layout JSON accepted by `view.panels` and restored from disk.
pub const MAX_PANELS_JSON: usize = 64 * 1024;

/// Current [`Prefs::version`].
pub const PREFS_VERSION: u32 = 1;

impl Prefs {
    /// Parse saved prefs. Anything unreadable falls back to defaults rather than failing startup,
    /// field by field: one bad value (say a mistyped `thumb_size`) keeps every other setting.
    pub fn from_json(text: &str) -> Self {
        Self::parse(text).0
    }

    /// Like [`Prefs::from_json`], also saying whether `text` was a JSON object at all (`false`:
    /// the whole file was unreadable and only defaults came back, so the caller may want to
    /// keep a copy before overwriting it).
    pub fn parse(text: &str) -> (Self, bool) {
        let mut prefs = Prefs::default();
        let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text) else {
            prefs.version = PREFS_VERSION;
            return (prefs, false);
        };
        if let Some(Value::Array(items)) = root.get("favorites") {
            prefs.favorites = items.iter().filter_map(Value::as_str).map(PathBuf::from).collect();
        }
        prefs.last_folder = root.get("last_folder").and_then(Value::as_str).map(PathBuf::from);
        if let Some(Value::Object(view)) = root.get("view") {
            let v = &mut prefs.view;
            set_field(&mut v.mode, view.get("mode"));
            set_field(&mut v.thumb_size, view.get("thumb_size"));
            set_field(&mut v.sort, view.get("sort"));
            set_field(&mut v.ascending, view.get("ascending"));
            set_field(&mut v.show_hidden, view.get("show_hidden"));
            set_field(&mut v.theme, view.get("theme"));
        }
        prefs.panels = root.get("panels").filter(|v| panels_json_ok(v)).cloned();
        if let Some(Value::Object(formats)) = root.get("formats") {
            set_field(&mut prefs.formats.heic, formats.get("heic"));
        }
        prefs.view.thumb_size = prefs.view.thumb_size.clamp(THUMB_SIZE_RANGE.0, THUMB_SIZE_RANGE.1);
        prefs.version = PREFS_VERSION;
        (prefs, true)
    }

    /// Serialize for saving. Paths that aren't valid UTF-8 can't be written as JSON strings, so
    /// those favorites (and such a last folder) are left out instead of voiding the whole file.
    pub fn to_json(&self) -> String {
        let mut clean = self.clone();
        clean.favorites.retain(|p| p.to_str().is_some());
        clean.last_folder = clean.last_folder.filter(|p| p.to_str().is_some());
        // Nothing left can fail to serialize; the fallback only guards future fields.
        serde_json::to_string_pretty(&clean).unwrap_or_else(|_| "{}".into())
    }
}

/// Overwrite `slot` with `value` when it deserializes as the slot's type; keep it otherwise.
fn set_field<T: serde::de::DeserializeOwned>(slot: &mut T, value: Option<&Value>) {
    if let Some(parsed) = value.and_then(|v| serde_json::from_value(v.clone()).ok()) {
        *slot = parsed;
    }
}

/// A panel layout must be a JSON object no larger than [`MAX_PANELS_JSON`].
pub fn panels_json_ok(value: &Value) -> bool {
    value.is_object() && serde_json::to_string(value).is_ok_and(|s| s.len() <= MAX_PANELS_JSON)
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
        assert_eq!(Prefs::from_json(r#"{"panels":{"root":null}}"#).panels, Some(serde_json::json!({"root": null})));
        assert_eq!(Prefs::from_json(r#"{"panels":[1,2]}"#).panels, None, "a non-object layout is dropped");
    }

    #[test]
    fn heic_defaults_to_off_round_trips_and_junk_is_off() {
        assert!(!Prefs::default().formats.heic);
        assert!(!Prefs::from_json("{}").formats.heic);
        for junk in [r#"{"formats":{"heic":"yes"}}"#, r#"{"formats":{"heic":1}}"#, r#"{"formats":true}"#] {
            assert!(!Prefs::from_json(junk).formats.heic, "{junk}");
        }
        let mut p = Prefs::default();
        p.formats.heic = true;
        assert!(Prefs::from_json(&p.to_json()).formats.heic);
        assert!(p.to_json().contains("\"heic\": true"));
    }

    #[test]
    fn theme_defaults_to_dark_round_trips_and_tolerates_junk() {
        assert_eq!(Prefs::default().view.theme, ThemeMode::Dark);
        assert_eq!(Prefs::from_json("{}").view.theme, ThemeMode::Dark);
        assert_eq!(Prefs::from_json(r#"{"view":{"theme":"purple","mode":"list"}}"#).view.theme, ThemeMode::Dark);
        assert_eq!(Prefs::from_json(r#"{"view":{"theme":"purple","mode":"list"}}"#).view.mode, ViewMode::List);
        for mode in [ThemeMode::System, ThemeMode::Dark, ThemeMode::Light] {
            let mut p = Prefs::default();
            p.view.theme = mode;
            assert_eq!(Prefs::from_json(&p.to_json()).view.theme, mode);
            assert_eq!(ThemeMode::parse(mode.name()), Some(mode));
        }
    }

    #[test]
    fn one_bad_field_keeps_the_others() {
        let text = r#"{"favorites":["/a",5,"/b"],"last_folder":7,
            "view":{"mode":"list","thumb_size":"huge","sort":"colour","ascending":false},
            "panels":"nope"}"#;
        let (p, whole) = Prefs::parse(text);
        assert!(whole);
        assert_eq!(p.favorites, [PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(p.last_folder, None);
        assert_eq!(p.view.mode, ViewMode::List, "a bad thumb_size must not reset the mode");
        assert_eq!(p.view.thumb_size, ViewPrefs::default().thumb_size);
        assert_eq!(p.view.sort, SortKey::Name);
        assert!(!p.view.ascending);
        assert_eq!(p.panels, None);
        assert!(!Prefs::parse("{ not json").1);
        assert!(!Prefs::parse("[1]").1);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_are_dropped_not_the_whole_file() {
        use std::os::unix::ffi::OsStrExt;
        let odd = PathBuf::from(std::ffi::OsStr::from_bytes(b"/bad\xff"));
        let p = Prefs {
            favorites: vec!["/good".into(), odd.clone()],
            last_folder: Some(odd),
            view: ViewPrefs { mode: ViewMode::List, ..ViewPrefs::default() },
            ..Prefs::default()
        };
        let back = Prefs::from_json(&p.to_json());
        assert_eq!(back.favorites, [PathBuf::from("/good")]);
        assert_eq!(back.last_folder, None);
        assert_eq!(back.view.mode, ViewMode::List);
    }
}
