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
    // Loaded at startup: files added since show after a reload.
    let before = run(&mut s, "storyboard.themes", json!({}));
    assert!(before["themes"].as_array().unwrap().iter().all(|t| t["source"] == "builtin"), "not re-read on every call");
    let out = run(&mut s, "storyboard.themes", json!({ "reload": true }));
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

    let out = run(&mut s, "storyboard.themes", json!({ "reload": true }));
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

#[test]
fn themes_in_the_folder_are_loaded_at_startup() {
    let (_, dir) = session("startup");
    let themes = dir.join("Themes");
    std::fs::write(themes.join("early.theme.json"), pack_theme("early", &[])).unwrap();
    // A new session (the app starting) finds it without a reload.
    let prefs = Prefs { working_folder: Some(dir.clone()), ..Prefs::default() };
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), prefs);
    assert_eq!(s.loaded_themes().len(), 1);
    let out = run(&mut s, "storyboard.themes", json!({}));
    assert!(out["themes"].as_array().unwrap().iter().any(|t| t["id"] == "early"));
    assert!(s.execute("storyboard.themes", &json!({ "reload": 1 })).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A folder outside the working folder, as the user's Downloads might be.
fn outside(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetcraft-theme-src-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn importing_a_theme_file_copies_it_in_and_loads_it() {
    let (mut s, dir) = session("import-file");
    let src = outside("file");
    let file = src.join("sunset.theme.json");
    std::fs::write(&file, pack_theme("sunset", &[])).unwrap();
    let path = file.to_string_lossy().to_string();
    // A dry run says what it would do and writes nothing.
    let dry = run(&mut s, "storyboard.importTheme", json!({ "path": path, "dryRun": true }));
    assert_eq!(dry["kind"], "file");
    assert!(dry["to"].as_str().unwrap().ends_with("sunset.theme.json"));
    assert!(!dir.join("Themes").join("sunset.theme.json").exists());
    let out = run(&mut s, "storyboard.importTheme", json!({ "path": path }));
    assert_eq!(out["message"], "Imported theme “Pack sunset”");
    assert!(dir.join("Themes").join("sunset.theme.json").exists());
    assert!(s.loaded_themes().iter().any(|u| u.theme.id == "sunset"), "loaded without a reload");
    // Again: refused unless replacing.
    let again = s.execute("storyboard.importTheme", &json!({ "path": path })).unwrap_err().to_string();
    assert!(again.contains("already loaded") && again.contains("replace"), "{again}");
    run(&mut s, "storyboard.importTheme", json!({ "path": path, "replace": true }));
    // Refused: a built-in's id, not a theme file, a theme already in Themes, bad params.
    let bold = src.join("bold.theme.json");
    let mut t = assetcraft_storyboard::Theme::builtin(assetcraft_storyboard::TextTheme::Bold);
    t.name = "Bolder".into();
    std::fs::write(&bold, t.to_json()).unwrap();
    let e = s.execute("storyboard.importTheme", &json!({ "path": bold.to_string_lossy() })).unwrap_err().to_string();
    assert!(e.contains("built-in"), "{e}");
    std::fs::write(src.join("notes.txt"), b"hi").unwrap();
    let e = s
        .execute("storyboard.importTheme", &json!({ "path": src.join("notes.txt").to_string_lossy() }))
        .unwrap_err()
        .to_string();
    assert!(e.contains(".theme.json"), "{e}");
    std::fs::write(src.join("broken.theme.json"), b"{ nope").unwrap();
    assert!(
        s.execute("storyboard.importTheme", &json!({ "path": src.join("broken.theme.json").to_string_lossy() }))
            .is_err()
    );
    let inside = dir.join("Themes").join("sunset.theme.json");
    let e = s
        .execute("storyboard.importTheme", &json!({ "path": inside.to_string_lossy(), "replace": true }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("already in the Themes folder"), "{e}");
    assert!(s.execute("storyboard.importTheme", &json!({})).is_err());
    assert!(s.execute("storyboard.importTheme", &json!({ "path": path, "replace": "yes" })).is_err());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&src);
}

#[test]
fn importing_a_pack_copies_its_theme_and_listed_designs_only() {
    let (mut s, dir) = session("import-pack");
    let src = outside("pack").join("Acme Brand");
    std::fs::create_dir_all(src.join("designs")).unwrap();
    std::fs::create_dir_all(src.join("images")).unwrap();
    let banner = assetcraft_titles::builtin::builtin(assetcraft_titles::builtin::STACK).unwrap().unwrap();
    std::fs::write(src.join("designs").join("banner.ectemplate"), banner.to_template()).unwrap();
    std::fs::write(src.join("designs").join("unlisted.ectemplate"), banner.to_template()).unwrap();
    std::fs::write(src.join("images").join("logo.png"), b"not listed in assets yet").unwrap();
    std::fs::write(src.join("notes.txt"), b"junk").unwrap();
    std::fs::write(
        src.join("theme.json"),
        pack_theme("acme-brand", &[("banner", "designs/banner.ectemplate"), ("card", "designs/card.ectemplate")]),
    )
    .unwrap();
    let out = run(&mut s, "storyboard.importTheme", json!({ "path": src.to_string_lossy() }));
    assert_eq!(out["kind"], "pack");
    assert_eq!(out["message"], "Imported theme “Pack acme-brand” (a pack, 1 design)");
    assert!(out["warnings"].to_string().contains("design card"), "{out}");
    let dest = dir.join("Themes").join("acme-brand");
    let mut copied: Vec<String> =
        walk(&dest).iter().map(|p| p.strip_prefix(&dest).unwrap().to_string_lossy().replace('\\', "/")).collect();
    copied.sort();
    assert_eq!(copied, ["designs/banner.ectemplate", "theme.json"], "only the theme and its listed designs");
    let loaded = s.loaded_themes().iter().find(|u| u.theme.id == "acme-brand").unwrap();
    assert_eq!(loaded.designs.len(), 1);
    assert_eq!(loaded.designs[0].design.name(), "Stack");
    // A folder without a theme.json isn't a pack.
    let empty = outside("empty");
    let e = s.execute("storyboard.importTheme", &json!({ "path": empty.to_string_lossy() })).unwrap_err().to_string();
    assert!(e.contains("theme.json"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(src.parent().unwrap());
    let _ = std::fs::remove_dir_all(&empty);
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn a_packs_designs_are_ready_to_draw_with_its_theme() {
    let (_, dir) = session("designs-fill");
    let pack = dir.join("Themes").join("acme-brand");
    std::fs::create_dir_all(pack.join("designs")).unwrap();
    let banner = assetcraft_titles::builtin::builtin(assetcraft_titles::builtin::STACK).unwrap().unwrap();
    std::fs::write(pack.join("designs").join("banner.ectemplate"), banner.to_template()).unwrap();
    std::fs::write(pack.join("designs").join("end.ectemplate"), banner.to_template()).unwrap();
    std::fs::write(
        pack.join("theme.json"),
        pack_theme("acme-brand", &[("banner", "designs/banner.ectemplate"), ("endCard", "designs/end.ectemplate")]),
    )
    .unwrap();
    let prefs = Prefs { working_folder: Some(dir.clone()), ..Prefs::default() };
    let s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), prefs);
    let designs = s.theme_designs();
    assert_eq!(designs.len(), 1, "card and banner only for now (endCard waits for its step)");
    assert_eq!((designs[0].theme.as_str(), designs[0].role.as_str()), ("acme-brand", "banner"));
    // The fill is the theme's: Bold's colours and its display face.
    let t = assetcraft_storyboard::Theme::builtin(assetcraft_storyboard::TextTheme::Bold);
    let fill = crate::theme_cmd::theme_fill(&t);
    use assetcraft_titles::{FillValue, Slot};
    let hex = |slot| match fill.get(slot) {
        Some(FillValue::Color(c)) => format!(
            "#{:02X}{:02X}{:02X}",
            (c[0] * 255.0).round() as u8,
            (c[1] * 255.0).round() as u8,
            (c[2] * 255.0).round() as u8
        ),
        v => format!("{v:?}"),
    };
    assert_eq!(hex(Slot::Background), t.color.background.to_uppercase());
    assert_eq!(hex(Slot::Accent), t.color.accent.to_uppercase());
    match fill.get(Slot::HeadlineFont) {
        Some(FillValue::Font { family, style }) => {
            assert_eq!(family, &t.type_.display.family);
            assert!(!style.is_empty());
        }
        v => panic!("{v:?}"),
    }
    assert_eq!(designs[0].fill, crate::theme_cmd::theme_fill(&s.loaded_themes()[0].theme));
    let _ = std::fs::remove_dir_all(&dir);
}

/// TP2b: the Viewer's lookup for a clip drawn through a pack design renders in the background
/// and matches a direct render; a clip without one gives `None`.
#[test]
fn the_viewer_gets_design_frames_for_clips_drawn_through_a_pack() {
    let (_, dir) = session("design-frames");
    let pack = dir.join("Themes").join("acme-brand");
    std::fs::create_dir_all(pack.join("designs")).unwrap();
    let banner = assetcraft_titles::builtin::builtin(assetcraft_titles::builtin::LASER_TRACE).unwrap().unwrap();
    std::fs::write(pack.join("designs").join("banner.ectemplate"), banner.to_template()).unwrap();
    std::fs::write(pack.join("theme.json"), pack_theme("acme-brand", &[("banner", "designs/banner.ectemplate")]))
        .unwrap();
    let prefs = Prefs { working_folder: Some(dir.clone()), ..Prefs::default() };
    let s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), prefs);
    let mut shot = assetcraft_storyboard::Clip::new(
        assetcraft_storyboard::ClipId(1),
        assetcraft_storyboard::SceneId(1),
        assetcraft_storyboard::ShotType::Ws,
        assetcraft_storyboard::Ticks::from_secs(6).unwrap(),
    );
    shot.overlay = Some("ISTANBUL".into());
    assert!(s.design_frame(&shot, 3.0, 320).is_none(), "a built-in theme: no design");
    shot.text_style.theme_id = assetcraft_storyboard::ThemeId::new("acme-brand");
    assert_eq!(s.clip_design(&shot), Some(("Laser Trace".to_string(), "banner".to_string())));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let f = loop {
        match s.design_frame(&shot, 3.0, 320).unwrap() {
            crate::DesignLookup::Ready(f) => break f,
            crate::DesignLookup::Failed(e) => panic!("{e}"),
            crate::DesignLookup::Pending(_) => {
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    };
    let designs = s.theme_designs();
    let d = &designs[0];
    let fill = assetcraft_animatic::design_fill(d, "ISTANBUL");
    let direct = d.design.render(&fill, 3.0, 320.0 / 1920.0).unwrap();
    assert_eq!((f.width, f.height, &f.rgba), (direct.width, direct.height, &direct.rgba), "as the export draws it");
    let _ = std::fs::remove_dir_all(&dir);
}
