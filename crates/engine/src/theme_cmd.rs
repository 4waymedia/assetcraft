//! `storyboard.themes` (themes step 1a, `docs/design/themes.md`): the built-in themes and the
//! user's `.theme.json` files in `<working>/Themes/` (TH9), each checked by
//! `assetcraft_storyboard::Theme::parse` as hostile input. A file that fails is listed under
//! `problems` with its reason, never fatal; a file whose id repeats one before it is skipped.

use crate::{CmdResult, Session};
use assetcraft_fs::{EntryKind, ListOptions};
use assetcraft_storyboard::{MAX_THEME_BYTES, THEME_SUFFIX, Theme};
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
    /// Its warnings (a font not bundled, low contrast).
    pub warnings: Vec<String>,
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
        let mut files: Vec<PathBuf> = listing
            .entries
            .into_iter()
            .filter(|e| e.kind == EntryKind::File && e.name.to_ascii_lowercase().ends_with(THEME_SUFFIX))
            .map(|e| e.path)
            .collect();
        files.sort();
        let builtin: Vec<String> = Theme::builtins().into_iter().map(|t| t.id).collect();
        for path in files {
            let bytes = match self.services.read_head(&path, MAX_THEME_BYTES + 1) {
                Ok(b) => b,
                Err(e) => {
                    bad.push((path, e.to_string()));
                    continue;
                }
            };
            match Theme::parse(&bytes, false) {
                Ok((theme, warnings)) => {
                    if builtin.contains(&theme.id) || ok.iter().any(|u: &UserTheme| u.theme.id == theme.id) {
                        bad.push((path, format!("id {:?} is already taken", theme.id)));
                    } else {
                        ok.push(UserTheme { path, theme, warnings });
                    }
                }
                Err(e) => bad.push((path, e.to_string())),
            }
        }
        (ok, bad)
    }
}

/// A theme as the listing shows it.
fn theme_json(t: &Theme, source: &str) -> Value {
    json!({
        "id": t.id, "name": t.name, "tags": t.tags, "source": source,
        "colors": { "background": t.color.background, "text": t.color.text, "accent": t.color.accent },
        "display": t.type_.display.family, "body": t.type_.body.family,
        "titleIn": t.motion.title_in,
    })
}

/// `storyboard.themes {}`.
pub(crate) fn sb_themes(s: &mut Session, _p: &Value) -> CmdResult {
    let mut themes: Vec<Value> = Theme::builtins().iter().map(|t| theme_json(t, "builtin")).collect();
    let (ok, bad) = s.user_themes();
    for u in &ok {
        let mut v = theme_json(&u.theme, "user");
        v["path"] = json!(u.path.to_string_lossy());
        v["warnings"] = json!(u.warnings);
        themes.push(v);
    }
    let problems: Vec<Value> =
        bad.iter().map(|(p, why)| json!({ "path": p.to_string_lossy(), "reason": why })).collect();
    Ok(json!({
        "themes": themes,
        "problems": problems,
        "folder": s.themes_folder().map(|f| f.to_string_lossy().into_owned()),
    }))
}
