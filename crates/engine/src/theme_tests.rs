//! `storyboard.themes` (themes step 1a): the built-ins, the user's files from
//! `<working>/Themes/`, problems listed and never fatal.

use super::*;
use crate::launch::RecordingLauncher;
use assetcraft_fs::RealFs;
use serde_json::json;
use std::path::PathBuf;

/// A real session whose working folder is a temp dir, so Themes/ is real too.
fn session(tag: &str) -> (Session, PathBuf) {
    let dir = std::env::temp_dir().join(format!("assetcraft-themes-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Themes")).unwrap();
    // Canonical, so Windows' short temp names match the listing's (as `template_tests`).
    let dir = dir.canonicalize().unwrap();
    let prefs = Prefs { working_folder: Some(dir.clone()), ..Prefs::default() };
    (Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), prefs), dir)
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn themes_lists_the_built_ins_then_the_users_files_with_their_problems() {
    let (mut s, dir) = session("list");
    let themes = dir.join("Themes");
    // No files yet: the four built-ins.
    let out = run(&mut s, "storyboard.themes", json!({}));
    let ids: Vec<&str> = out["themes"].as_array().unwrap().iter().filter_map(|t| t["id"].as_str()).collect();
    assert_eq!(ids, ["bold", "minimal", "cinematic", "paper"]);
    assert!(out["themes"].as_array().unwrap().iter().all(|t| t["source"] == "builtin"));
    assert_eq!(out["folder"].as_str().map(PathBuf::from), Some(themes.clone()));
    // A good file, one with a warning, a broken one, one taking a built-in's id, a duplicate,
    // and a file that isn't a theme.
    let mut noir = assetcraft_storyboard::Theme::builtin(assetcraft_storyboard::TextTheme::Cinematic);
    noir.id = "noir-gold".into();
    noir.name = "Noir Gold".into();
    noir.type_.display.family = "Cinzel".into();
    std::fs::write(themes.join("noir-gold.theme.json"), noir.to_json()).unwrap();
    let mut neon = noir.clone();
    neon.id = "neon".into();
    neon.name = "Neon".into();
    neon.type_.display.family = "Inter".into();
    std::fs::write(themes.join("neon.theme.json"), neon.to_json()).unwrap();
    std::fs::write(themes.join("broken.theme.json"), b"{ not json").unwrap();
    let mut bold = neon.clone();
    bold.id = "bold".into();
    std::fs::write(themes.join("my-bold.theme.json"), bold.to_json()).unwrap();
    std::fs::write(themes.join("z-neon-again.theme.json"), neon.to_json()).unwrap();
    std::fs::write(themes.join("notes.txt"), b"not a theme").unwrap();
    let out = run(&mut s, "storyboard.themes", json!({}));
    let user: Vec<&Value> = out["themes"].as_array().unwrap().iter().filter(|t| t["source"] == "user").collect();
    let ids: Vec<&str> = user.iter().filter_map(|t| t["id"].as_str()).collect();
    assert_eq!(ids, ["neon", "noir-gold"], "sorted by file name");
    let noir_row = user.iter().find(|t| t["id"] == "noir-gold").unwrap();
    assert!(noir_row["warnings"][0].as_str().unwrap().contains("Cinzel"), "{noir_row}");
    assert_eq!(noir_row["display"], "Cinzel");
    assert!(noir_row["path"].as_str().unwrap().ends_with("noir-gold.theme.json"));
    let problems = out["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 3, "{problems:?}");
    let reason = |file: &str| {
        problems.iter().find(|p| p["path"].as_str().unwrap().ends_with(file)).unwrap()["reason"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert!(!reason("broken.theme.json").is_empty());
    assert!(reason("my-bold.theme.json").contains("already taken"));
    assert!(reason("z-neon-again.theme.json").contains("already taken"));
    let _ = std::fs::remove_dir_all(&dir);
}
