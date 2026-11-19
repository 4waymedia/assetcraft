//! `storyboard.themes` (themes step 1a, `docs/design/themes.md`): the built-in themes and the
//! user's themes in `<working>/Themes/` (TH9), each checked by
//! `assetcraft_storyboard::Theme::parse` as hostile input. A file that fails is listed under
//! `problems` with its reason, never fatal; a file whose id repeats one before it is skipped.
//!
//! A user theme is a loose `<id>.theme.json`, or a **theme pack** (theme-packs.md TP1, TD1): a
//! folder with a `theme.json` and a `designs/` folder of EffectCraft templates named by the
//! theme's `designs` field. Each design is opened with `assetcraft_titles::Design::read` (size
//! caps, format checks); one that is missing, a link, or broken is a warning on its theme, and
//! the theme still loads. The built-in banner designs (`assetcraft_titles::builtin`) are listed
//! too.

use crate::{CmdResult, Session};
use assetcraft_fs::{EntryKind, ListOptions};
use assetcraft_storyboard::{DESIGNS_DIR, MAX_THEME_BYTES, PACK_THEME_FILE, THEME_SUFFIX, Theme, design_path};
use assetcraft_titles::{Design, MAX_FILE as MAX_DESIGN_BYTES};
use serde_json::{Value, json};
use std::path::PathBuf;

/// The Themes folder's name under the working folder.
pub const THEMES_DIR: &str = "Themes";
/// Most theme files read from it.
const MAX_USER_THEMES: usize = 200;

/// A theme file that loaded.
#[derive(Debug, Clone)]
pub struct UserTheme {
    /// Where it was read from.
    pub path: PathBuf,
    /// The theme.
    pub theme: Theme,
    /// Its warnings (a font not bundled, low contrast, a design that didn't open).
    pub warnings: Vec<String>,
    /// The pack folder, for a theme pack; `None` for a loose `.theme.json`.
    pub pack: Option<PathBuf>,
    /// The pack's designs that opened, by role.
    pub designs: Vec<PackDesign>,
}

/// A design in a theme pack that opened.
#[derive(Debug, Clone)]
pub struct PackDesign {
    /// Its role (`banner`, `card`, …).
    pub role: String,
    /// The file.
    pub path: PathBuf,
    /// The design.
    pub design: Design,
}

impl Session {
    /// `<working>/Themes`, where the user's theme files live.
    pub fn themes_folder(&self) -> Option<PathBuf> {
        self.working_folder().map(|w| w.join(THEMES_DIR))
    }

    /// The user's theme files: those that loaded, and `(path, reason)` for those that didn't.
    pub fn user_themes(&self) -> (Vec<UserTheme>, Vec<(PathBuf, String)>) {
        let (mut ok, mut bad) = (Vec::new(), Vec::new());
        let Some(dir) = self.themes_folder() else { return (ok, bad) };
        let opts = ListOptions { include_hidden: false, max_entries: MAX_USER_THEMES };
        let Ok(listing) = self.services.list_entries(&dir, &opts) else { return (ok, bad) };
        let mut files: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
        for e in listing.entries {
            if e.kind == EntryKind::File && e.name.to_ascii_lowercase().ends_with(THEME_SUFFIX) {
                files.push((e.path, None));
            } else if e.kind == EntryKind::Dir {
                // A pack: its theme.json (a pack without one isn't a theme, and is left alone).
                let file = e.path.join(PACK_THEME_FILE);
                if self.services.read_head(&file, 1).is_ok() {
                    files.push((file, Some(e.path)));
                }
            }
        }
        files.sort();
        let builtin: Vec<String> = Theme::builtins().into_iter().map(|t| t.id).collect();
        for (path, pack) in files {
            let bytes = match self.services.read_head(&path, MAX_THEME_BYTES + 1) {
                Ok(b) => b,
                Err(e) => {
                    bad.push((path, e.to_string()));
                    continue;
                }
            };
            match Theme::parse(&bytes, false) {
                Ok((theme, mut warnings)) => {
                    if builtin.contains(&theme.id) || ok.iter().any(|u: &UserTheme| u.theme.id == theme.id) {
                        bad.push((path, format!("id {:?} is already taken", theme.id)));
                        continue;
                    }
                    let designs = match &pack {
                        Some(dir) => self.pack_designs(dir, &theme, &mut warnings),
                        None => {
                            if !theme.designs.is_empty() {
                                warnings.push(
                                    "designs are read only from a theme pack (a folder with a theme.json)".into(),
                                );
                            }
                            Vec::new()
                        }
                    };
                    ok.push(UserTheme { path, theme, warnings, pack, designs });
                }
                Err(e) => bad.push((path, e.to_string())),
            }
        }
        (ok, bad)
    }

    /// Open a pack's designs; each one that can't be used adds a warning instead.
    fn pack_designs(&self, pack: &std::path::Path, theme: &Theme, warnings: &mut Vec<String>) -> Vec<PackDesign> {
        let mut out = Vec::new();
        if theme.designs.is_empty() {
            return out;
        }
        // Only regular files in designs/ count: a link (symlink, junction) isn't followed.
        let dir = pack.join(DESIGNS_DIR);
        let opts = ListOptions { include_hidden: false, max_entries: MAX_USER_THEMES };
        let files: Vec<String> = match self.services.list_entries(&dir, &opts) {
            Ok(l) => l.entries.into_iter().filter(|e| e.kind == EntryKind::File).map(|e| e.name).collect(),
            Err(_) => Vec::new(),
        };
        for (role, rel) in &theme.designs {
            let Ok(parts) = design_path(rel) else { continue };
            let name = parts[1];
            if !files.iter().any(|f| f == name) {
                warnings.push(format!("design {role}: {rel} isn't in the pack (or is a link)"));
                continue;
            }
            let path = dir.join(name);
            let bytes = match self.services.read_head(&path, MAX_DESIGN_BYTES + 1) {
                Ok(b) => b,
                Err(e) => {
                    warnings.push(format!("design {role}: {e}"));
                    continue;
                }
            };
            match Design::read(&bytes) {
                Ok(design) => out.push(PackDesign { role: role.clone(), path, design }),
                Err(e) => warnings.push(format!("design {role}: {e}")),
            }
        }
        out
    }
}

/// A design as the listing shows it.
fn design_json(role: &str, d: &Design) -> Value {
    let (width, height) = d.size();
    let controls: Vec<Value> =
        d.controls().iter().map(|c| json!({ "name": c.name, "fills": c.slot.map(|s| s.name()) })).collect();
    json!({ "role": role, "name": d.name(), "width": width, "height": height, "seconds": d.seconds(), "controls": controls })
}

/// A theme as the listing shows it.
fn theme_json(t: &Theme, source: &str) -> Value {
    json!({
        "id": t.id, "name": t.name, "tags": t.tags, "source": source,
        "colors": { "background": t.color.background, "text": t.color.text, "accent": t.color.accent },
        "display": t.type_.display.family, "body": t.type_.body.family,
        "titleIn": t.motion.title_in,
        "titleEffect": t.motion.title_effect,
        "titleOut": t.motion.title_out,
        "titleSeconds": t.motion.title_seconds,
    })
}

/// `storyboard.themes {}`.
pub(crate) fn sb_themes(s: &mut Session, _p: &Value) -> CmdResult {
    let mut themes: Vec<Value> = Theme::builtins().iter().map(|t| theme_json(t, "builtin")).collect();
    let (ok, bad) = s.user_themes();
    for u in &ok {
        let mut v = theme_json(&u.theme, if u.pack.is_some() { "pack" } else { "user" });
        v["path"] = json!(u.path.to_string_lossy());
        v["warnings"] = json!(u.warnings);
        if let Some(pack) = &u.pack {
            v["pack"] = json!(pack.to_string_lossy());
            let designs: Vec<Value> = u
                .designs
                .iter()
                .map(|d| {
                    let mut j = design_json(&d.role, &d.design);
                    j["path"] = json!(d.path.to_string_lossy());
                    j
                })
                .collect();
            v["designs"] = json!(designs);
        }
        themes.push(v);
    }
    // The built-in banner designs (M1c), for every theme without its own banner.
    let builtin_designs: Vec<Value> = assetcraft_titles::builtin::BUILTINS
        .iter()
        .filter_map(|id| match assetcraft_titles::builtin::builtin(id) {
            Some(Ok(d)) => {
                let mut j = design_json("banner", &d);
                j["id"] = json!(id);
                Some(j)
            }
            _ => None,
        })
        .collect();
    let problems: Vec<Value> =
        bad.iter().map(|(p, why)| json!({ "path": p.to_string_lossy(), "reason": why })).collect();
    Ok(json!({
        "themes": themes,
        "problems": problems,
        "builtinDesigns": builtin_designs,
        "folder": s.themes_folder().map(|f| f.to_string_lossy().into_owned()),
    }))
}
