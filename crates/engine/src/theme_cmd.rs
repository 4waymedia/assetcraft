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

use crate::{CmdError, CmdResult, Session};
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

/// The user's themes as last loaded (at startup, on reload, after an import).
#[derive(Debug, Clone, Default)]
pub(crate) struct ThemeLibrary {
    themes: Vec<UserTheme>,
    problems: Vec<(PathBuf, String)>,
}

impl Session {
    /// Re-read `<working>/Themes` (startup, `storyboard.themes {reload}`, an import, a new
    /// working folder).
    pub(crate) fn reload_themes(&mut self) {
        let (themes, problems) = self.user_themes();
        self.themes = ThemeLibrary { themes, problems };
    }

    /// The user's themes and packs as loaded, in file-name order (built-ins not included).
    pub fn loaded_themes(&self) -> &[UserTheme] {
        &self.themes.themes
    }

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

/// `storyboard.themes {reload?: bool}`: the themes as loaded at startup; `reload` reads the
/// Themes folder again first.
pub(crate) fn sb_themes(s: &mut Session, p: &Value) -> CmdResult {
    let reload = match p.get("reload") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(_) => return Err(CmdError::BadParams("`reload` must be true or false".into())),
    };
    if reload {
        s.reload_themes();
    }
    let mut themes: Vec<Value> = Theme::builtins().iter().map(|t| theme_json(t, "builtin")).collect();
    let (ok, bad) = (&s.themes.themes, &s.themes.problems);
    for u in ok {
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
    let mut out = json!({
        "themes": themes,
        "problems": problems,
        "builtinDesigns": builtin_designs,
        "folder": s.themes_folder().map(|f| f.to_string_lossy().into_owned()),
    });
    if reload {
        let n = s.themes.themes.len();
        let bad = s.themes.problems.len();
        out["message"] = json!(match bad {
            0 => format!("Themes reloaded: {n} of yours"),
            _ => format!("Themes reloaded: {n} of yours, {bad} with problems (see storyboard.themes)"),
        });
    }
    Ok(out)
}

/// What an import copies: the theme file, and for a pack its listed designs that open.
struct ImportPlan {
    theme: Theme,
    pack: bool,
    dest: PathBuf,
    files: Vec<(PathBuf, PathBuf)>,
    warnings: Vec<String>,
}

/// `storyboard.importTheme {path, replace?, dryRun?}`: copy a `.theme.json` file or a theme pack
/// folder into `<working>/Themes`, then load it. The source is read as hostile input (the same
/// checks as loading); only the theme file and the designs it lists are copied, never anything
/// else in the folder. A theme already loaded under that id is replaced only with `replace`.
pub(crate) fn sb_import_theme(s: &mut Session, p: &Value) -> CmdResult {
    let src = p
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|x| !x.is_empty() && x.len() <= 4096)
        .map(PathBuf::from)
        .ok_or_else(|| CmdError::BadParams("`path` must be a .theme.json file or a theme pack folder".into()))?;
    let flag = |key: &str| match p.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(CmdError::BadParams(format!("`{key}` must be true or false"))),
    };
    let (replace, dry) = (flag("replace")?, flag("dryRun")?);
    let plan = s.import_plan(&src, replace)?;
    let kind = if plan.pack { "pack" } else { "file" };
    let designs = plan.files.len().saturating_sub(1);
    let files: Vec<Value> = plan.files.iter().map(|(_, to)| json!(to.to_string_lossy())).collect();
    let mut out = json!({
        "id": plan.theme.id, "name": plan.theme.name, "kind": kind, "to": plan.dest.to_string_lossy(),
        "files": files, "warnings": plan.warnings, "dryRun": dry,
    });
    if dry {
        out["message"] = json!(format!("Would import “{}”", plan.theme.name));
        return Ok(out);
    }
    for (from, to) in &plan.files {
        let bytes = s.services.read_head(from, MAX_DESIGN_BYTES.max(MAX_THEME_BYTES) + 1).map_err(CmdError::Fs)?;
        crate::document_cmd::write_atomic(to, &bytes)
            .map_err(|e| CmdError::Unavailable(format!("couldn't write {}: {e}", to.display())))?;
    }
    s.reload_themes();
    let what = match (plan.pack, designs) {
        (false, _) => String::new(),
        (true, 1) => " (a pack, 1 design)".into(),
        (true, n) => format!(" (a pack, {n} designs)"),
    };
    out["message"] = json!(format!("Imported theme “{}”{what}", plan.theme.name));
    Ok(out)
}

impl Session {
    fn import_plan(&self, src: &std::path::Path, replace: bool) -> Result<ImportPlan, CmdError> {
        let themes = self.themes_folder().ok_or_else(|| CmdError::Unavailable("there's no working folder".into()))?;
        let is_dir = self.services.entry(src).map_err(CmdError::Fs)?.kind == EntryKind::Dir;
        let file = if is_dir { src.join(PACK_THEME_FILE) } else { src.to_path_buf() };
        if !is_dir && !file.to_string_lossy().to_ascii_lowercase().ends_with(THEME_SUFFIX) {
            return Err(CmdError::BadParams(format!("pick a {THEME_SUFFIX} file or a theme pack folder")));
        }
        let bytes = self.services.read_head(&file, MAX_THEME_BYTES + 1).map_err(|e| {
            CmdError::BadParams(if is_dir {
                format!("that folder has no {PACK_THEME_FILE}: {e}")
            } else {
                e.to_string()
            })
        })?;
        let (theme, mut warnings) = Theme::parse(&bytes, false).map_err(|e| CmdError::BadParams(e.to_string()))?;
        if Theme::builtins().iter().any(|t| t.id == theme.id) {
            return Err(CmdError::BadParams(format!(
                "{:?} is a built-in theme's id; give the theme its own id",
                theme.id
            )));
        }
        let dest = if is_dir { themes.join(&theme.id) } else { themes.join(format!("{}{THEME_SUFFIX}", theme.id)) };
        if let Some(have) = self.themes.themes.iter().find(|u| u.theme.id == theme.id) {
            if have.pack.is_some() != is_dir {
                return Err(CmdError::BadParams(format!(
                    "“{}” is already loaded as a {}; remove it from Themes first",
                    have.theme.name,
                    if have.pack.is_some() { "pack" } else { "theme file" }
                )));
            }
            if !replace {
                return Err(CmdError::BadParams(format!(
                    "“{}” is already loaded; pass replace: true to replace it",
                    have.theme.name
                )));
            }
        } else if self.services.entry(&dest).is_ok() && !replace {
            return Err(CmdError::BadParams(format!(
                "{} already exists; pass replace: true to replace it",
                dest.display()
            )));
        }
        let theme_dest = if is_dir { dest.join(PACK_THEME_FILE) } else { dest.clone() };
        let real = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        if real(src).starts_with(real(&themes)) {
            return Err(CmdError::BadParams("that theme is already in the Themes folder; use Reload Themes".into()));
        }
        let mut files = vec![(file, theme_dest)];
        if is_dir {
            files.extend(self.import_designs(src, &dest, &theme, &mut warnings));
        } else if !theme.designs.is_empty() {
            warnings.push("designs are read only from a theme pack (a folder with a theme.json)".into());
        }
        Ok(ImportPlan { theme, pack: is_dir, dest, files, warnings })
    }

    /// The pack's designs that are regular files and open, as `(from, to)`.
    fn import_designs(
        &self,
        src: &std::path::Path,
        dest: &std::path::Path,
        theme: &Theme,
        warnings: &mut Vec<String>,
    ) -> Vec<(PathBuf, PathBuf)> {
        let dir = src.join(DESIGNS_DIR);
        let opts = ListOptions { include_hidden: false, max_entries: MAX_USER_THEMES };
        let present: Vec<String> = match self.services.list_entries(&dir, &opts) {
            Ok(l) => l.entries.into_iter().filter(|e| e.kind == EntryKind::File).map(|e| e.name).collect(),
            Err(_) => Vec::new(),
        };
        let mut out = Vec::new();
        for (role, rel) in &theme.designs {
            let Ok(parts) = design_path(rel) else { continue };
            let name = parts[1];
            if !present.iter().any(|f| f == name) {
                warnings.push(format!("design {role}: {rel} isn't in the pack (or is a link); not copied"));
                continue;
            }
            let from = dir.join(name);
            match self
                .services
                .read_head(&from, MAX_DESIGN_BYTES + 1)
                .map_err(|e| e.to_string())
                .and_then(|b| Design::read(&b).map_err(|e| e.to_string()))
            {
                Ok(_) => out.push((from, dest.join(DESIGNS_DIR).join(name))),
                Err(e) => warnings.push(format!("design {role}: {e}; not copied")),
            }
        }
        out
    }
}
