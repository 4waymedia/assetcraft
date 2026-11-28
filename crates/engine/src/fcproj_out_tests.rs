//! FC-2: `storyboard.writeFcproj` — the Cut as a FilmCraft project, in-app titles as graphic
//! clips built from the compositor's own layout, read back with FilmCraft's own format crate.

use super::*;
use crate::launch::RecordingLauncher;
use assetcraft_fs::RealFs;
use assetcraft_storyboard::{
    AssetId, BoxAlign, Clip, ClipId, DesignPick, SceneId, ShotType, TICKS_PER_SECOND, TextBox, TextTheme, Ticks,
};
use filmcraft_project::{ParamValue, TrackItem};
use serde_json::json;
use std::path::{Path, PathBuf};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetcraft-fcproj-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn library(rel: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library").join(rel).to_string_lossy().into_owned()
}

/// A saved vertical board (270 × 480, 24 fps): a Bold title card, a still, the still again with
/// a Bold overlay in an area, an empty clip.
fn saved(dir: &Path) -> Session {
    saved_with(dir, Arc::new(RecordingLauncher::default()))
}

fn saved_with(dir: &Path, launcher: Arc<RecordingLauncher>) -> Session {
    let mut s = Session::new(Arc::new(RealFs), launcher, Prefs::default());
    run(&mut s, "storyboard.new", json!({ "name": "blossom" }));
    let board = s.boards.board.as_mut().unwrap();
    board.frame = Some(assetcraft_storyboard::FrameSpec {
        width: 270,
        height: 480,
        fps: 24,
        ..assetcraft_storyboard::FrameSpec::default()
    });
    board.scenes = vec![assetcraft_storyboard::Scene::new(SceneId(1), None)];
    let mut card = Clip::new(ClipId(1), SceneId(1), ShotType::Ttl, Ticks::from_secs(2).unwrap());
    card.description = "Some saints were never saved".into();
    card.text_style.theme = TextTheme::Bold;
    let mut still = Clip::new(ClipId(2), SceneId(1), ShotType::Est, Ticks::from_secs(1).unwrap());
    still.asset_ids.push(AssetId(library("environments/soul_light_forest.jpg")));
    let mut over = still.clone();
    over.id = ClipId(3);
    over.overlay = Some("Chorus title".into());
    over.text_style.theme = TextTheme::Bold;
    over.text_style.area = Some(TextBox { align: BoxAlign::Center, width: 900, height: 140, y: 500, x: None });
    let empty = Clip::new(ClipId(4), SceneId(1), ShotType::Cu, Ticks::from_secs(1).unwrap());
    board.clips = vec![card, still, over, empty];
    let path = dir.join("blossom.storyboard.json");
    run(&mut s, "storyboard.saveAs", json!({ "path": path.to_string_lossy() }));
    s
}

fn text(item: &TrackItem) -> Option<&filmcraft_project::EffectInstance> {
    item.effects.iter().find(|e| e.effect == filmcraft_project::graphic::TEXT_LAYER)
}

fn layer_names(item: &TrackItem) -> Vec<String> {
    item.effects
        .iter()
        .filter_map(|e| match e.param("name").map(|p| &p.value) {
            Some(ParamValue::Text(t)) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn in_app_titles_become_graphic_clips_filmcraft_reads() {
    let dir = temp("graphics");
    let mut s = saved(&dir);
    let dry = run(&mut s, "storyboard.writeFcproj", json!({ "dryRun": true }));
    assert_eq!((dry["graphics"].clone(), dry["rendered"].clone()), (json!([1, 3]), json!([])), "{dry}");
    assert!(!dir.join("blossom.fcproj").exists(), "a dry run writes nothing");
    let out = run(&mut s, "storyboard.writeFcproj", json!({}));
    assert_eq!(out["written"], json!(true), "{out}");
    assert!(!dir.join("blossom.media").exists(), "no title was rendered: nothing on disk but the project");
    let bytes = std::fs::read(dir.join("blossom.fcproj")).unwrap();
    let loaded = filmcraft_format::decode(&bytes).expect("FilmCraft's format reads it");
    let project = loaded.project;
    let seq = project
        .items
        .values()
        .find_map(|i| match &i.kind {
            filmcraft_project::ItemKind::Sequence(q) => Some(q.clone()),
            _ => None,
        })
        .expect("a sequence");
    assert_eq!((seq.settings.width, seq.settings.height), (270, 480));
    let tick = TICKS_PER_SECOND as i64 / 24;
    // V1: the card (a graphic clip where it sits), the still, the overlay's shot. V2: the overlay.
    let v1 = &seq.video_tracks[0].items;
    assert_eq!(v1.len(), 3, "{:?}", v1.iter().map(|i| &i.name).collect::<Vec<_>>());
    let card = &v1[0];
    assert_eq!((card.start.0, card.duration.0), (0, 48 * tick));
    assert_eq!(layer_names(card)[0], "Background", "a card's backdrop first: {:?}", layer_names(card));
    let over = &seq.video_tracks[1].items[0];
    assert_eq!((over.start.0, over.duration.0), (72 * tick, 24 * tick), "over its shot on V2");
    assert_eq!(layer_names(over), ["Plate", "Accent", "Words"]);
    // The words as the compositor sets them: as typed, FilmCraft's capitals on, Inter Regular at
    // its em size, one line, the first baseline at the layout's.
    let compositor_layout = {
        let board = s.boards.board.as_ref().unwrap();
        let mut c = assetcraft_animatic::Compositor::new(
            Arc::clone(&s.services),
            270,
            480,
            s.model.render_look(),
            s.decode_options(),
        )
        .unwrap()
        .with_crop(board.frame.as_ref());
        c.title_layout(&board.clips[2]).unwrap()
    };
    let words = text(over).unwrap();
    let get = |id: &str| words.param(id).map(|p| p.value.clone());
    assert_eq!(get("text"), Some(ParamValue::Text("Chorus title".into())));
    assert_eq!(get("caps"), Some(ParamValue::Choice(1)));
    assert_eq!(get("font"), Some(ParamValue::Text("Inter".into())));
    assert_eq!(get("font_style"), Some(ParamValue::Text("Regular".into())));
    let Some(ParamValue::Float(size)) = get("size") else { panic!("a size") };
    assert!((size - f64::from(compositor_layout.words.metrics.em)).abs() < 1e-3, "{size}");
    // Placed on the block's middle and anchored there, so the first baseline lands on the
    // layout's.
    let base = words.param("position").unwrap().vec2_at(filmcraft_time::Tick(48 * tick));
    let Some(ParamValue::Vec2(anchor)) = get("anchor") else { panic!("an anchor") };
    let first = &compositor_layout.words.lines[0];
    assert!((base.y - anchor.y - f64::from(first.baseline)).abs() < 0.01, "{base:?} {anchor:?} vs {first:?}");
    // The in: opacity keyed from 0 up.
    let opacity = words.param("opacity").unwrap();
    assert!(opacity.is_animated(), "the theme's in is keyed");
    let keys: Vec<(i64, String)> =
        opacity.keyframes.iter().map(|k| (k.time.0 / tick, format!("{:?}", k.value))).collect();
    assert!(
        opacity.f64_at(filmcraft_time::Tick(0)) < 1.0 && opacity.f64_at(filmcraft_time::Tick(12 * tick)) > 99.0,
        "{keys:?}"
    );
    // GC-12: the still is framed as the board frames it — FilmCraft fits it, the Motion scales it
    // on to the board's fill and moves it to its snap.
    let (pw, ph) = {
        let p = assetcraft_probe::probe(s.services.as_ref(), Path::new(&library("environments/soul_light_forest.jpg")))
            .unwrap();
        let d = p.dimensions.unwrap();
        (f64::from(d.width), f64::from(d.height))
    };
    let spec = s.boards.board.as_ref().unwrap().frame_spec();
    let [x, y, fw, fh] = spec.place(pw as f32, ph as f32).map(f64::from);
    let still = &v1[1];
    assert!(still.scale_to_frame);
    let motion = still.effect("motion").expect("a Motion");
    let fit = (270.0 / pw).min(480.0 / ph);
    let scale = motion.f64_at("scale", filmcraft_time::Tick(0));
    assert!((scale / 100.0 * fit * pw - fw * 270.0).abs() < 0.01, "{scale}");
    let at = motion.vec2_at("position", filmcraft_time::Tick(0));
    assert!((at.x - (x + fw / 2.0) * 270.0).abs() < 0.01 && (at.y - (y + fh / 2.0) * 480.0).abs() < 0.01, "{at:?}");
    assert!(scale > 100.0, "a landscape picture fills a tall frame: bigger than fitted");
    // A card's background doesn't fade with its words.
    let background = &card
        .effects
        .iter()
        .find(|e| matches!(e.param("name").map(|p| &p.value), Some(ParamValue::Text(t)) if t == "Background"))
        .unwrap();
    assert!(!background.param("opacity").is_some_and(|p| p.is_animated()));
    assert!(text(card).unwrap().param("opacity").unwrap().is_animated());
    // Its key, on the graphic clip.
    assert!(over.markers[0].comment.starts_with("assetcraft clip 3 · bold"), "{:?}", over.markers);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_design_drawn_title_goes_as_its_render() {
    let dir = temp("render");
    let mut s = saved(&dir);
    s.boards.board.as_mut().unwrap().clips[2].text_style.design = Some(DesignPick::Stack);
    let dry = run(&mut s, "storyboard.writeFcproj", json!({ "dryRun": true }));
    assert_eq!((dry["graphics"].clone(), dry["unrendered"].clone()), (json!([1]), json!([3])), "{dry}");
    let err = s.execute("storyboard.writeFcproj", &json!({})).unwrap_err().to_string();
    assert!(err.contains("renderTitles"), "{err}");
    run(&mut s, "storyboard.renderTitles", json!({ "wait": true }));
    let out = run(&mut s, "storyboard.writeFcproj", json!({}));
    assert_eq!(out["rendered"], json!([3]), "{out}");
    let loaded = filmcraft_format::decode(&std::fs::read(dir.join("blossom.fcproj")).unwrap()).unwrap();
    let media: Vec<String> = loaded
        .project
        .items
        .values()
        .filter_map(|i| match &i.kind {
            filmcraft_project::ItemKind::Media(m) => Some(format!("{:?}", m.media)),
            _ => None,
        })
        .collect();
    assert!(media.iter().any(|m| m.contains("blossom.media") && m.contains("c3-")), "{media:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// SF1b-3: Send to FilmCraft renders a design-drawn title first (and only that one: the in-app
/// card goes as a graphic clip), then writes the project and opens it in FilmCraft.
#[test]
fn send_to_filmcraft_renders_then_writes_and_opens() {
    let dir = temp("send");
    let launcher = Arc::new(RecordingLauncher::default());
    let mut s = saved_with(&dir, launcher.clone()).with_app_locator(Arc::new(crate::open_in::FakeLocator {
        installed: vec![("filmcraft", PathBuf::from("/opt/filmcraft"))],
    }));
    s.boards.board.as_mut().unwrap().clips[2].text_style.design = Some(DesignPick::Stack);
    let first = run(&mut s, "storyboard.sendToFilmCraft", json!({}));
    assert_eq!((first["stage"].clone(), first["render"].clone()), (json!("rendering"), json!(1)), "{first}");
    if let Some(h) = s.exporter.thread.take() {
        h.join().unwrap();
    }
    let titles = dir.join("blossom.media").join("titles");
    let names: Vec<String> =
        std::fs::read_dir(&titles).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(names.len() == 1 && names[0].starts_with("c3-"), "only the design-drawn title renders: {names:?}");
    let sent = run(&mut s, "storyboard.sendToFilmCraft", json!({}));
    assert_eq!(sent["stage"], json!("opened"), "{sent}");
    let path = dir.join("blossom.fcproj");
    assert!(path.is_file());
    let spawned = launcher.spawned.lock().unwrap().clone();
    assert_eq!(spawned.len(), 1, "{spawned:?}");
    assert_eq!(spawned[0].0, PathBuf::from("/opt/filmcraft"));
    assert!(spawned[0].1.iter().any(|a| Path::new(a) == path), "{spawned:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without FilmCraft installed Send is disabled with the info text (D-SF9), and the project can
/// still be written on its own; a title problem stops a send before anything is written.
#[test]
fn send_needs_filmcraft_and_title_problems_stop_it() {
    let dir = temp("send-none");
    let mut s = saved(&dir).with_app_locator(Arc::new(crate::open_in::FakeLocator { installed: vec![] }));
    assert_eq!(s.enabled("storyboard.sendToFilmCraft"), Err(crate::open_in::FILMCRAFT_MISSING.to_owned()));
    let err = s.execute("storyboard.sendToFilmCraft", &json!({})).unwrap_err().to_string();
    assert!(err.contains("installing it"), "{err}");
    assert!(!dir.join("blossom.fcproj").exists());
    run(&mut s, "storyboard.writeFcproj", json!({}));
    assert!(dir.join("blossom.fcproj").is_file());

    let mut s = s.with_app_locator(Arc::new(crate::open_in::FakeLocator {
        installed: vec![("filmcraft", PathBuf::from("/opt/filmcraft"))],
    }));
    let gone = AssetId(dir.join("gone.jpg").to_string_lossy().into_owned());
    s.boards.board.as_mut().unwrap().clips[1].asset_ids = vec![gone.clone()];
    s.boards.board.as_mut().unwrap().clips[2].asset_ids = vec![gone];
    let err = s.execute("storyboard.sendToFilmCraft", &json!({})).unwrap_err().to_string();
    assert!(err.contains("problem"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
