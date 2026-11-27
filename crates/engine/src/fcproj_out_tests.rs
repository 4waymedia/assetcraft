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
    let mut s = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default());
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
