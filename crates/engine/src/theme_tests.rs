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
    // No files yet: the eight built-ins.
    let out = run(&mut s, "storyboard.themes", json!({}));
    let ids: Vec<&str> = out["themes"].as_array().unwrap().iter().filter_map(|t| t["id"].as_str()).collect();
    assert_eq!(ids, ["bold", "minimal", "cinematic", "paper", "noir-gold", "neon-night", "field-notes", "blueprint"]);
    assert!(out["themes"].as_array().unwrap().iter().all(|t| t["source"] == "builtin"));
    assert_eq!(out["folder"].as_str().map(PathBuf::from), Some(themes.clone()));
    // A good file, one with a warning, a broken one, one taking a built-in's id, a duplicate,
    // and a file that isn't a theme.
    let mut noir = assetcraft_storyboard::Theme::builtin(assetcraft_storyboard::TextTheme::Cinematic);
    noir.id = "gilded-age".into();
    noir.name = "Gilded Age".into();
    noir.type_.display.family = "Cinzel".into();
    std::fs::write(themes.join("gilded-age.theme.json"), noir.to_json()).unwrap();
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
    assert_eq!(ids, ["gilded-age", "neon"], "sorted by file name");
    let noir_row = user.iter().find(|t| t["id"] == "gilded-age").unwrap();
    assert!(noir_row["warnings"][0].as_str().unwrap().contains("Cinzel"), "{noir_row}");
    assert_eq!(noir_row["display"], "Cinzel");
    assert!(noir_row["path"].as_str().unwrap().ends_with("gilded-age.theme.json"));
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

/// A pack theme file: a copy of Bold with its own id and the given designs.
fn pack_theme(id: &str, designs: &[(&str, &str)]) -> String {
    let mut t = assetcraft_storyboard::Theme::builtin(assetcraft_storyboard::TextTheme::Bold);
    t.id = id.into();
    t.name = format!("Pack {id}");
    t.designs = designs.iter().map(|(r, p)| ((*r).to_string(), (*p).to_string())).collect();
    t.to_json()
}

#[test]
fn a_pack_folder_loads_with_its_designs_and_problems_are_warnings() {
    let (mut s, dir) = session("packs");
    let themes = dir.join("Themes");
    let banner = assetcraft_titles::builtin::builtin(assetcraft_titles::builtin::LASER_TRACE).unwrap().unwrap();
    // A good pack: a banner design, and a card named in theme.json that isn't there.
    let acme = themes.join("acme-brand");
    std::fs::create_dir_all(acme.join("designs")).unwrap();
    std::fs::create_dir_all(acme.join("images")).unwrap();
    std::fs::write(acme.join("designs").join("banner.ectemplate"), banner.to_template()).unwrap();
    std::fs::write(
        acme.join("theme.json"),
        pack_theme("acme-brand", &[("banner", "designs/banner.ectemplate"), ("card", "designs/card.ectemplate")]),
    )
    .unwrap();
    // A pack whose design isn't a template.
    let broken = themes.join("broken-pack");
    std::fs::create_dir_all(broken.join("designs")).unwrap();
    std::fs::write(broken.join("designs").join("banner.ectemplate"), b"PK not really").unwrap();
    std::fs::write(broken.join("theme.json"), pack_theme("broken-pack", &[("banner", "designs/banner.ectemplate")]))
        .unwrap();
    // A pack whose design path leaves the pack: the theme is refused.
    let escape = themes.join("escape");
    std::fs::create_dir_all(&escape).unwrap();
    std::fs::write(
        escape.join("theme.json"),
        pack_theme("escape", &[("banner", "../acme-brand/designs/banner.ectemplate")]),
    )
    .unwrap();
    // A folder that isn't a pack (no theme.json), and a loose theme naming designs.
    std::fs::create_dir_all(themes.join("just-a-folder")).unwrap();
    std::fs::write(themes.join("loose.theme.json"), pack_theme("loose", &[("banner", "designs/banner.ectemplate")]))
        .unwrap();

    let out = run(&mut s, "storyboard.themes", json!({}));
    let rows: Vec<&Value> = out["themes"].as_array().unwrap().iter().filter(|t| t["source"] != "builtin").collect();
    let ids: Vec<&str> = rows.iter().filter_map(|t| t["id"].as_str()).collect();
    assert_eq!(ids, ["acme-brand", "broken-pack", "loose"], "{rows:?}");
    let row = |id: &str| *rows.iter().find(|t| t["id"] == id).unwrap();

    let acme_row = row("acme-brand");
    assert_eq!(acme_row["source"], "pack");
    assert!(acme_row["pack"].as_str().unwrap().ends_with("acme-brand"));
    let designs = acme_row["designs"].as_array().unwrap();
    assert_eq!(designs.len(), 1);
    assert_eq!(designs[0]["role"], "banner");
    assert_eq!(designs[0]["name"], "Laser Trace");
    assert_eq!((designs[0]["width"].as_u64(), designs[0]["height"].as_u64()), (Some(1920), Some(1080)));
    let fills: Vec<&str> =
        designs[0]["controls"].as_array().unwrap().iter().filter_map(|c| c["fills"].as_str()).collect();
    assert_eq!(fills, ["Headline", "Caption", "Accent", "Surface", "Headline Font"]);
    let warns = acme_row["warnings"].to_string();
    assert!(warns.contains("design card") && warns.contains("isn't in the pack"), "{warns}");

    let broken_row = row("broken-pack");
    assert_eq!(broken_row["designs"].as_array().unwrap().len(), 0);
    assert!(broken_row["warnings"].to_string().contains("design banner"), "{broken_row}");

    let loose_row = row("loose");
    assert_eq!(loose_row["source"], "user");
    assert!(loose_row["warnings"].to_string().contains("only from a theme pack"), "{loose_row}");

    let problems = out["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0]["reason"].as_str().unwrap().contains("must stay inside the pack"), "{problems:?}");

    let builtins: Vec<&str> =
        out["builtinDesigns"].as_array().unwrap().iter().filter_map(|d| d["id"].as_str()).collect();
    assert_eq!(builtins, ["banner-laser-trace", "banner-stack"]);
    let _ = std::fs::remove_dir_all(&dir);
}
