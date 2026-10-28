//! The Folders panel's lists (docs/design/folders-panel.md, step F1; Paul, 2026-10-27).
//!
//! - **My Folders**: pins in the suite-wide `places.json` (`Session::with_pins_file`; the app
//!   passes `<OS config>/Crafting/places.json`). Each pin shows in **all** Crafting apps or **only**
//!   the listed ones. AssetCraft shows pins for `"all"` or `"assetcraft"`, and every write re-reads
//!   the file first and keeps the other apps' pins and fields, so one app never drops another's.
//!   Without a file (the CLI, tests) pins live in memory. The first time a file is set and none
//!   exists, AssetCraft's old Favorites (prefs) become its first pins, shared with all apps.
//! - **Recent**: the last [`MAX_RECENT`] folders opened (saved in prefs).
//! - **System favorites**: the OS sidebar (`Services::sidebar`), else the standard known folders.
//!
//! `places.json` is hostile input: at most [`MAX_PLACES_BYTES`] and [`MAX_PINS`] pins; a pin needs
//! an absolute UTF-8 `path`; anything else is skipped. A file that isn't a JSON object at all is
//! renamed `places.json.bad` before the next write, never silently overwritten.

use crate::{CmdError, Session, same_path};
use serde_json::{Map, Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

/// This app's id in `places.json`.
pub const APP_ID: &str = "assetcraft";
/// Most folders kept in Recent.
pub const MAX_RECENT: usize = 8;
/// Most pins read from or written to `places.json`.
pub const MAX_PINS: usize = 500;
/// Largest `places.json` read.
pub const MAX_PLACES_BYTES: u64 = 1024 * 1024;
/// `places.json` format version.
pub const PLACES_VERSION: u64 = 1;

/// Who sees a pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinScope {
    /// Every Crafting app.
    All,
    /// Only these apps (by id).
    Only(Vec<String>),
}

impl PinScope {
    fn parse(v: Option<&Value>) -> PinScope {
        match v {
            Some(Value::Array(ids)) => PinScope::Only(
                ids.iter().filter_map(Value::as_str).filter(|s| s.len() <= 64).map(str::to_owned).collect(),
            ),
            // "all", missing, or anything unexpected: shown everywhere (never lose a pin).
            _ => PinScope::All,
        }
    }

    fn to_json(&self) -> Value {
        match self {
            PinScope::All => json!("all"),
            PinScope::Only(ids) => json!(ids),
        }
    }

    fn shows_here(&self) -> bool {
        match self {
            PinScope::All => true,
            PinScope::Only(ids) => ids.iter().any(|i| i == APP_ID),
        }
    }
}

/// One pin, with every field another app may have written kept as is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pin {
    pub(crate) path: PathBuf,
    pub(crate) scope: PinScope,
    raw: Map<String, Value>,
}

impl Pin {
    fn new(path: PathBuf, scope: PinScope) -> Pin {
        Pin { path, scope, raw: Map::new() }
    }

    /// An old AssetCraft Favorite (prefs), shared with all apps once it moves into My Folders.
    pub(crate) fn legacy(path: PathBuf) -> Pin {
        Pin::new(path, PinScope::All)
    }

    fn to_json(&self) -> Value {
        let mut obj = self.raw.clone();
        obj.insert("path".into(), json!(self.path.to_string_lossy()));
        obj.insert("apps".into(), self.scope.to_json());
        Value::Object(obj)
    }
}

/// Parse `places.json`: `Some(pins)` for a JSON object (bad pins skipped), `None` for anything
/// else (not JSON, not an object).
pub(crate) fn parse_places(text: &str) -> Option<Vec<Pin>> {
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text) else { return None };
    let pins = root.get("pins").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out: Vec<Pin> = Vec::new();
    for v in pins.into_iter().take(MAX_PINS) {
        let Value::Object(mut raw) = v else { continue };
        let Some(path) = raw.get("path").and_then(Value::as_str).map(PathBuf::from) else { continue };
        if !path.has_root() || path.as_os_str().len() > 4096 || out.iter().any(|p| same_path(&p.path, &path)) {
            continue;
        }
        let scope = PinScope::parse(raw.get("apps"));
        // `raw` keeps only the fields other apps added; path and apps are written from the
        // cleaned values.
        raw.remove("path");
        raw.remove("apps");
        out.push(Pin { path, scope, raw });
    }
    Some(out)
}

fn places_json(pins: &[Pin]) -> String {
    let doc =
        json!({ "version": PLACES_VERSION, "pins": pins.iter().take(MAX_PINS).map(Pin::to_json).collect::<Vec<_>>() });
    serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".into())
}

/// Read `path`: `Ok(Some(pins))`, `Ok(None)` when there is no file, `Err` when it exists but
/// can't be used (too large, not UTF-8, not a JSON object).
fn read_places(path: &Path) -> Result<Option<Vec<Pin>>, String> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if !meta.is_file() || meta.len() > MAX_PLACES_BYTES {
        return Err("not a file, or too large".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_places(&text).map(Some).ok_or_else(|| "not a places file".into())
}

fn write_places(path: &Path, pins: &[Pin]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(places_json(pins).as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

impl Session {
    /// Keep My Folders in the suite-wide `places.json` at `path` (the app passes
    /// `<OS config>/Crafting/places.json`). Reads it now; when there is none yet, AssetCraft's
    /// old Favorites become its first pins and the file is written.
    pub fn with_pins_file(mut self, path: PathBuf) -> Self {
        match read_places(&path) {
            Ok(Some(pins)) => self.pins = pins,
            Ok(None) => {
                if !self.pins.is_empty() {
                    let _ = write_places(&path, &self.pins);
                }
            }
            Err(_) => {
                // Unusable: keep it aside rather than overwrite what another app (or a newer
                // version) wrote; the next change writes a fresh file.
                let _ = std::fs::rename(&path, path.with_extension("json.bad"));
            }
        }
        self.pins_file = Some(path);
        self.refresh_favorites();
        self
    }

    /// Where My Folders is kept, if in a file.
    pub fn pins_file(&self) -> Option<&Path> {
        self.pins_file.as_deref()
    }

    /// `Some(true)` for a pin shared with all Crafting apps, `Some(false)` for AssetCraft only,
    /// `None` when `path` isn't pinned here.
    pub fn pin_shared(&self, path: &Path) -> Option<bool> {
        self.pins.iter().find(|p| same_path(&p.path, path) && p.scope.shows_here()).map(|p| p.scope == PinScope::All)
    }

    /// Recent folders, newest first.
    pub fn recent(&self) -> &[PathBuf] {
        &self.recent
    }

    /// System → Favorites: the OS sidebar when it has one, else the standard known folders.
    /// `(label, path)`, in the OS's order.
    pub fn system_places(&self) -> Vec<(String, PathBuf)> {
        if !self.sidebar.is_empty() {
            return self
                .sidebar
                .iter()
                .map(|i| (i.name.clone().unwrap_or_else(|| assetcraft_fs::display_name(&i.path)), i.path.clone()))
                .collect();
        }
        self.known.iter().map(|(k, p)| (k.label().to_string(), p.clone())).collect()
    }

    /// `true` when System → Favorites mirrors the OS sidebar (not the standard list).
    pub fn system_places_from_os(&self) -> bool {
        !self.sidebar.is_empty()
    }

    /// Remember `path` in Recent (newest first).
    pub(crate) fn note_recent(&mut self, path: &Path) {
        self.recent.retain(|p| !same_path(p, path));
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }

    /// The visible pins' paths (what `favorites()` returns), from `pins`.
    pub(crate) fn refresh_favorites(&mut self) {
        self.favorites = self.pins.iter().filter(|p| p.scope.shows_here()).map(|p| p.path.clone()).collect();
    }

    /// Change My Folders: re-read the file (another app may have changed it), apply `change`,
    /// write it back. In memory when there is no file.
    pub(crate) fn edit_pins(&mut self, change: impl FnOnce(&mut Vec<Pin>)) -> Result<(), CmdError> {
        if let Some(file) = self.pins_file.clone() {
            if let Ok(Some(fresh)) = read_places(&file) {
                self.pins = fresh;
            }
            change(&mut self.pins);
            write_places(&file, &self.pins)
                .map_err(|e| CmdError::Unavailable(format!("can't save My Folders ({}): {e}", file.display())))?;
        } else {
            change(&mut self.pins);
        }
        self.refresh_favorites();
        Ok(())
    }
}

/// `scope` param: `all` (every Crafting app, the default) or `app` (AssetCraft only).
pub(crate) fn scope_param(p: &Value, default: PinScope) -> Result<PinScope, CmdError> {
    match p.get("scope").and_then(Value::as_str) {
        None if p.get("scope").is_none_or(Value::is_null) => Ok(default),
        Some("all") => Ok(PinScope::All),
        Some("app") => Ok(PinScope::Only(vec![APP_ID.into()])),
        _ => Err(CmdError::BadParams("`scope`: all (every Crafting app) or app (AssetCraft only)".into())),
    }
}

/// Pin `path` (or widen an existing pin to this app).
pub(crate) fn pin(pins: &mut Vec<Pin>, path: PathBuf, scope: PinScope) {
    let full = pins.len() >= MAX_PINS;
    match pins.iter_mut().find(|p| same_path(&p.path, &path)) {
        Some(p) => match (&mut p.scope, scope) {
            (_, PinScope::All) => p.scope = PinScope::All,
            (PinScope::Only(ids), PinScope::Only(_)) if !ids.iter().any(|i| i == APP_ID) => ids.push(APP_ID.into()),
            _ => {}
        },
        None if !full => pins.push(Pin::new(path, scope)),
        None => {}
    }
}

/// Unpin `path` here: a shared pin goes for every app (it's one list); an app-only pin loses this
/// app, and goes when no app is left.
pub(crate) fn unpin(pins: &mut Vec<Pin>, path: &Path) {
    pins.retain_mut(|p| {
        if !same_path(&p.path, path) {
            return true;
        }
        match &mut p.scope {
            PinScope::All => false,
            PinScope::Only(ids) => {
                ids.retain(|i| i != APP_ID);
                !ids.is_empty()
            }
        }
    });
}

/// Set who sees the pin at `path`.
pub(crate) fn set_scope(pins: &mut [Pin], path: &Path, scope: PinScope) -> bool {
    match pins.iter_mut().find(|p| same_path(&p.path, path)) {
        Some(p) => {
            p.scope = scope;
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_json_is_hostile_input_and_keeps_unknown_fields() {
        let text = r#"{"version":1,"pins":[
            {"path":"/art","apps":"all","name":"Art","added":"2026-10-27"},
            {"path":"/plates","apps":["designcraft"]},
            {"path":"relative"},{"apps":"all"},7,
            {"path":"/art","apps":["assetcraft"]},
            {"path":"/odd","apps":42}
        ]}"#;
        let pins = parse_places(text).unwrap();
        assert_eq!(
            pins.iter().map(|p| p.path.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            ["/art", "/plates", "/odd"]
        );
        assert_eq!(pins[1].scope, PinScope::Only(vec!["designcraft".into()]));
        assert_eq!(pins[2].scope, PinScope::All, "an unreadable scope never hides a pin");
        let out = places_json(&pins);
        assert!(out.contains("\"name\": \"Art\"") && out.contains("\"added\": \"2026-10-27\""), "fields kept: {out}");
        assert_eq!(parse_places(&out).unwrap(), pins, "round trip");
        assert!(parse_places("[1,2]").is_none() && parse_places("not json").is_none());
        assert_eq!(parse_places("{}").unwrap(), []);
    }

    #[test]
    fn pin_unpin_and_scope_keep_other_apps_pins() {
        let mut pins = vec![Pin::new("/plates".into(), PinScope::Only(vec!["designcraft".into()]))];
        pin(&mut pins, "/art".into(), PinScope::All);
        pin(&mut pins, "/plates".into(), PinScope::Only(vec![APP_ID.into()]));
        assert_eq!(pins[0].scope, PinScope::Only(vec!["designcraft".into(), APP_ID.into()]));
        unpin(&mut pins, Path::new("/plates"));
        assert_eq!(pins[0].scope, PinScope::Only(vec!["designcraft".into()]), "DesignCraft keeps it");
        assert!(set_scope(&mut pins, Path::new("/art"), PinScope::Only(vec![APP_ID.into()])));
        unpin(&mut pins, Path::new("/art"));
        assert_eq!(pins.len(), 1);
        pin(&mut pins, "/shared".into(), PinScope::All);
        unpin(&mut pins, Path::new("/shared"));
        assert_eq!(pins.len(), 1, "a shared pin goes for everyone");
    }
}
