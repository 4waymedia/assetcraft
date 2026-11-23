//! SF1b-1a: `storyboard.renderTitles` — the plan (files, reuse, a stable hash) and the
//! completeness check, before anything renders.

use super::*;
use crate::launch::RecordingLauncher;
use assetcraft_fs::RealFs;
use assetcraft_storyboard::{AssetId, Clip, ClipId, DesignPick, SceneId, ShotType, ThemeId, Ticks};
use serde_json::json;
use std::path::PathBuf;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetcraft-sf1b1-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn library(rel: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library").join(rel).to_string_lossy().into_owned()
}

/// A saved board in `dir`: a title card, a shot, the shot again with an overlay.
fn saved(dir: &Path) -> Session {
    saved_with(dir, Prefs::default())
}

fn saved_with(dir: &Path, prefs: Prefs) -> Session {
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), prefs);
    run(&mut s, "storyboard.new", json!({ "name": "Hollow Saint" }));
    let board = s.boards.board.as_mut().unwrap();
    board.scenes.push(assetcraft_storyboard::Scene::new(SceneId(1), None));
    let mut card = Clip::new(ClipId(1), SceneId(1), ShotType::Ttl, Ticks::from_secs(3).unwrap());
    card.description = "Some saints were never saved".into();
    let mut shot = Clip::new(ClipId(2), SceneId(1), ShotType::Est, Ticks::from_secs(2).unwrap());
    shot.asset_ids.push(AssetId(library("environments/soul_light_forest.jpg")));
    let mut over = shot.clone();
    over.id = ClipId(3);
    over.overlay = Some("Over the shot".into());
    board.clips = vec![card, shot, over];
    let path = dir.join("Hollow Saint.storyboard.json");
    run(&mut s, "storyboard.saveAs", json!({ "path": path.to_string_lossy() }));
    s
}

fn plan(s: &mut Session) -> Value {
    run(s, "storyboard.renderTitles", json!({ "dryRun": true }))
}

#[test]
fn plans_every_title_into_the_media_folder() {
    let dir = temp("plan");
    let mut s = saved(&dir);
    let p = plan(&mut s);
    let folder = dir.join("Hollow Saint.media").join("titles");
    assert_eq!(p["folder"], json!(folder.to_string_lossy()));
    assert_eq!(p["dryRun"], json!(true));
    let titles = p["titles"].as_array().unwrap();
    assert_eq!(titles.len(), 2, "the plain shot has no title: {p}");
    assert_eq!((titles[0]["clip"].clone(), titles[0]["kind"].clone()), (json!(1), json!("card")));
    assert_eq!((titles[1]["clip"].clone(), titles[1]["kind"].clone()), (json!(3), json!("overlay")));
    assert_eq!(titles[1]["index"], json!(2));
    let fps = p["fps"].as_u64().unwrap();
    assert_eq!(titles[0]["frames"], json!(3 * fps));
    for t in titles {
        let path = PathBuf::from(t["path"].as_str().unwrap());
        assert_eq!(path.parent(), Some(folder.as_path()));
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name, format!("c{}-{}.mp4", t["clip"], t["hash"].as_str().unwrap()));
        assert_eq!(t["hash"].as_str().unwrap().len(), 16);
        assert_eq!(t["reuse"], json!(false));
    }
    assert_eq!(p["ready"], json!(true), "{p}");
    assert_eq!((p["render"].clone(), p["reuse"].clone()), (json!(2), json!(0)));
    assert!(!folder.exists(), "a dry run writes nothing");
}

#[test]
fn an_unchanged_title_reuses_its_file_and_a_changed_one_gets_a_new_name() {
    let dir = temp("reuse");
    let mut s = saved(&dir);
    let first = plan(&mut s);
    assert_eq!(plan(&mut s)["titles"], first["titles"], "the hash is stable");
    let card = PathBuf::from(first["titles"][0]["path"].as_str().unwrap());
    std::fs::create_dir_all(card.parent().unwrap()).unwrap();
    std::fs::write(&card, b"rendered").unwrap();
    let again = plan(&mut s);
    assert_eq!(again["titles"][0]["reuse"], json!(true));
    assert_eq!((again["render"].clone(), again["reuse"].clone()), (json!(1), json!(1)));

    // New words, a new length, a new area: each is a new file; the overlay is untouched.
    let board = s.boards.board.as_mut().unwrap();
    board.clips[0].description = "Some saints were saved".into();
    let changed = plan(&mut s);
    assert_ne!(changed["titles"][0]["hash"], first["titles"][0]["hash"]);
    assert_eq!(changed["titles"][0]["reuse"], json!(false));
    assert_eq!(changed["titles"][1]["hash"], first["titles"][1]["hash"]);
    s.boards.board.as_mut().unwrap().clips[0].duration = Ticks::from_secs(4).unwrap();
    assert_ne!(plan(&mut s)["titles"][0]["hash"], changed["titles"][0]["hash"]);
}

#[test]
fn the_overlay_follows_its_shot() {
    let dir = temp("shot");
    let mut s = saved(&dir);
    let first = plan(&mut s)["titles"][1]["hash"].clone();
    let picture = dir.join("shot.jpg");
    std::fs::copy(library("environments/soul_light_forest.jpg"), &picture).unwrap();
    s.boards.board.as_mut().unwrap().clips[2].asset_ids = vec![AssetId(picture.to_string_lossy().into_owned())];
    let moved = plan(&mut s)["titles"][1]["hash"].clone();
    assert_ne!(moved, first, "another file under it");
    s.boards.board.as_mut().unwrap().clips[2].source_in = Ticks::from_secs(1).unwrap();
    assert_ne!(plan(&mut s)["titles"][1]["hash"], moved, "another in-point");
}

#[test]
fn whatever_would_draw_short_stops_it_with_a_list() {
    let dir = temp("check");
    let mut s = saved(&dir);
    let board = s.boards.board.as_mut().unwrap();
    board.clips[2].asset_ids = vec![AssetId(dir.join("gone.jpg").to_string_lossy().into_owned())];
    board.clips[0].text_style.theme_id = ThemeId::new("not-loaded");
    let p = plan(&mut s);
    assert_eq!(p["ready"], json!(false), "{p}");
    let problems = p["problems"].as_array().unwrap();
    let what: Vec<(Value, Value)> = problems.iter().map(|x| (x["clip"].clone(), x["what"].clone())).collect();
    assert!(what.contains(&(json!(1), json!("theme"))), "{p}");
    assert!(what.contains(&(json!(3), json!("media"))), "{p}");
    assert!(p["message"].as_str().unwrap().contains("stop the render"), "{p}");
    assert!(problems.iter().all(|x| !x["why"].as_str().unwrap().is_empty()));
}

#[test]
fn a_built_in_pick_that_loaded_is_not_a_problem() {
    let dir = temp("built");
    let mut s = saved(&dir);
    s.boards.board.as_mut().unwrap().clips[2].text_style.design = Some(DesignPick::Stack);
    let p = plan(&mut s);
    assert_eq!(p["ready"], json!(true), "{p}");
    assert_eq!(p["titles"][1]["renderer"], json!("effectCraft"));
    assert_eq!(p["titles"][0]["renderer"], json!("inApp"));
}

#[test]
fn needs_a_saved_board() {
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
    assert!(s.enabled("storyboard.renderTitles").is_err(), "no board");
    run(&mut s, "storyboard.new", json!({ "name": "Unsaved" }));
    assert!(s.enabled("storyboard.renderTitles").is_err(), "never saved");
    assert!(matches!(s.execute("storyboard.renderTitles", &json!({ "dryRun": true })), Err(CmdError::Unavailable(_))));
    let dir = temp("gate");
    let s = saved(&dir);
    assert!(s.enabled("storyboard.renderTitles").is_ok());
}

/// The board shrunk to 160 × 96 at 24 fps, so rendering is quick.
fn small(s: &mut Session) {
    let board = s.boards.board.as_mut().unwrap();
    board.frame = Some(assetcraft_storyboard::FrameSpec {
        width: 160,
        height: 96,
        fps: 24,
        ..assetcraft_storyboard::FrameSpec::default()
    });
}

fn leftovers(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|r| {
            r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".tmp")).collect()
        })
        .unwrap_or_default()
}

#[test]
fn renders_the_missing_titles_as_mp4s_then_reuses_them() {
    let dir = temp("render");
    let mut s = saved(&dir);
    small(&mut s);
    let out = run(&mut s, "storyboard.renderTitles", json!({ "wait": true }));
    assert_eq!(out["started"], json!(true));
    let status = &out["status"];
    assert_eq!(status["stage"], json!("done"), "{out}");
    assert_eq!((status["filesDone"].clone(), status["filesTotal"].clone()), (json!(2), json!(2)));
    assert_eq!(status["framesDone"], json!(3 * 24 + 2 * 24), "the card's 3 s and the overlay's 2 s");
    assert!(out["message"].as_str().unwrap().starts_with("Rendered 2 title(s)"), "{out}");
    let folder = dir.join("Hollow Saint.media").join("titles");
    assert!(leftovers(&folder).is_empty());
    for t in out["titles"].as_array().unwrap() {
        let path = PathBuf::from(t["path"].as_str().unwrap());
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[4..8], b"ftyp", "{}", path.display());
        assert!(bytes.windows(4).any(|w| w == b"avc1") && !bytes.windows(4).any(|w| w == b"mp4a"), "video only");
        let back = assetcraft_media::VideoReader::open(&path).unwrap();
        assert_eq!(back.size(), (160, 96));
    }
    // The overlay is merged with its shot: the picture is under the words.
    let overlay = PathBuf::from(out["titles"][1]["path"].as_str().unwrap());
    let mut back = assetcraft_media::VideoReader::open(&overlay).unwrap();
    let frame = back.frame(1.0, 160).unwrap();
    let lit = frame.rgba.as_chunks::<4>().0.iter().filter(|p| p[..3].iter().any(|c| *c > 16)).count();
    assert!(lit > 160 * 96 / 2, "the shot, not black: {lit}");

    // Everything is there now: the plan reuses both, and a render renders nothing.
    let plan = plan(&mut s);
    assert_eq!((plan["render"].clone(), plan["reuse"].clone()), (json!(0), json!(2)), "{plan}");
    let again = run(&mut s, "storyboard.renderTitles", json!({ "wait": true }));
    assert_eq!(again["status"]["filesTotal"], json!(0), "{again}");
    // `overwrite` renders them all again; a changed title renders only itself.
    let all = run(&mut s, "storyboard.renderTitles", json!({ "wait": true, "overwrite": true }));
    assert_eq!(all["status"]["filesDone"], json!(2), "{all}");
    s.boards.board.as_mut().unwrap().clips[0].description = "Some saints were saved".into();
    let one = run(&mut s, "storyboard.renderTitles", json!({ "wait": true }));
    assert_eq!(one["status"]["filesDone"], json!(1), "{one}");
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 3, "the old card's file stays (SF1b-3 tidies)");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_problem_stops_the_render_and_nothing_is_written() {
    let dir = temp("stop");
    let mut s = saved(&dir);
    small(&mut s);
    s.boards.board.as_mut().unwrap().clips[2].asset_ids =
        vec![AssetId(dir.join("gone.jpg").to_string_lossy().into_owned())];
    let err = s.execute("storyboard.renderTitles", &json!({ "wait": true })).unwrap_err().to_string();
    assert!(err.contains("stop the render") && err.contains("(media)") && err.contains("gone.jpg"), "{err}");
    assert!(!dir.join("Hollow Saint.media").exists(), "nothing written");
    assert_eq!(run(&mut s, "storyboard.exportStatus", json!({}))["stage"], json!("idle"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pack theme in `<dir>/Themes/<id>/` whose banner is Laser Trace (or `banner` bytes), its
/// display font `font`.
fn pack(dir: &Path, id: &str, font: &str, banner: Option<&[u8]>) {
    let folder = dir.join("Themes").join(id);
    std::fs::create_dir_all(folder.join("designs")).unwrap();
    let laser = assetcraft_titles::builtin::builtin(assetcraft_titles::builtin::LASER_TRACE).unwrap().unwrap();
    let bytes = banner.map_or_else(|| laser.to_template(), <[u8]>::to_vec);
    std::fs::write(folder.join("designs").join("banner.ectemplate"), bytes).unwrap();
    let mut t = assetcraft_storyboard::Theme::builtin(assetcraft_storyboard::TextTheme::Bold);
    t.id = id.into();
    t.name = format!("Pack {id}");
    t.type_.display.family = font.into();
    t.designs = [("banner".to_string(), "designs/banner.ectemplate".to_string())].into_iter().collect();
    std::fs::write(folder.join("theme.json"), t.to_json()).unwrap();
}

#[test]
fn inside_a_design_a_missing_font_stops_it_and_so_does_a_design_that_did_not_open() {
    let dir = temp("packs");
    pack(&dir, "acme", "No Such Family 9", None);
    pack(&dir, "fine", "Inter", None);
    pack(&dir, "broken", "Inter", Some(b"PK not really"));
    let prefs = Prefs { working_folder: Some(dir.clone()), ..Prefs::default() };
    let mut s = saved_with(&dir, prefs);
    run(&mut s, "storyboard.themes", json!({ "reload": true }));
    let theme = |id: &str| ThemeId::new(id);
    s.boards.board.as_mut().unwrap().clips[2].text_style.theme_id = theme("fine");
    let p = plan(&mut s);
    assert_eq!(p["ready"], json!(true), "a pack whose design draws as designed: {p}");
    assert_eq!(p["titles"][1]["renderer"], json!("effectCraft"));

    s.boards.board.as_mut().unwrap().clips[2].text_style.theme_id = theme("acme");
    let p = plan(&mut s);
    let problems = p["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{p}");
    assert_eq!((problems[0]["clip"].clone(), problems[0]["what"].clone()), (json!(3), json!("font")));
    assert!(problems[0]["why"].as_str().unwrap().contains("No Such Family 9"), "{p}");

    s.boards.board.as_mut().unwrap().clips[2].text_style.theme_id = theme("broken");
    let p = plan(&mut s);
    let what: Vec<Value> = p["problems"].as_array().unwrap().iter().map(|x| x["what"].clone()).collect();
    assert_eq!(what, [json!("design")], "the banner didn't open: {p}");
    let _ = std::fs::remove_dir_all(&dir);
}
