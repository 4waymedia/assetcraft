//! SF5-3: frames of stills, models, title cards, overlays and empty clips from `test_data`.

use super::*;
use assetcraft_fs::RealFs;
use assetcraft_storyboard::{AssetId, ClipId, MotionPreset, Scene, SceneId, TextAnimation};
use std::path::PathBuf;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
}

fn asset(rel: &str) -> AssetId {
    AssetId(library().join(rel).to_string_lossy().into_owned())
}

fn compositor(w: u32, h: u32) -> Compositor {
    Compositor::new(Arc::new(RealFs), w, h, RenderOptions::new(16, 9), DecodeOptions::default())
        .unwrap_or_else(|e| panic!("{e}"))
}

fn clip(id: u32, shot: ShotType, secs: u64) -> Clip {
    Clip::new(ClipId(id), SceneId(1), shot, Ticks::from_secs(secs).unwrap_or(Ticks::ZERO))
}

/// Pixels that aren't `rgb`, and the mean colour of those that aren't.
fn stats(img: &Rgba, rgb: [u8; 3]) -> (usize, [f32; 3]) {
    let mut n = 0usize;
    let mut sum = [0.0f32; 3];
    for p in img.data.chunks_exact(4) {
        if p[..3] != rgb {
            n += 1;
            for c in 0..3 {
                sum[c] += f32::from(p[c]);
            }
        }
    }
    let mean = if n > 0 { [sum[0] / n as f32, sum[1] / n as f32, sum[2] / n as f32] } else { [0.0; 3] };
    (n, mean)
}

#[test]
fn sizes_are_checked_and_a_missing_board_is_black() {
    assert!(matches!(
        Compositor::new(Arc::new(RealFs), 15, 10, RenderOptions::new(16, 9), DecodeOptions::default()),
        Err(AnimaticError::BadSize(15, 10))
    ));
    let mut c = compositor(64, 36);
    let board = Storyboard::new("empty");
    let f = c.frame(&board, Ticks::ZERO);
    assert_eq!((f.width, f.height), (64, 36));
    assert_eq!(stats(&f, [0, 0, 0]).0, 0, "no clips: black");
}

#[test]
fn a_still_is_letterboxed_and_its_pan_moves_the_picture() {
    let mut c = compositor(320, 180);
    // ruins_vertical.png is taller than wide: pillarboxed in a 16:9 frame.
    let mut still = clip(1, ShotType::Est, 4);
    still.asset_ids.push(asset("environments/ruins_vertical.png"));
    let rest = c.clip_frame(&still, f64::INFINITY);
    let (painted, _) = stats(&rest, [0, 0, 0]);
    assert!(painted > 0 && painted < 320 * 180, "pillarboxed: {painted} painted pixels");
    assert_eq!(rest.get(0, 90), [0, 0, 0, 255], "left pillar is black");
    assert_ne!(rest.get(160, 90)[..3], [0, 0, 0], "the middle shows the picture");

    // A pan: the frame at the start differs from the end, and both are full-height pictures.
    still.motion =
        Some(assetcraft_storyboard::Motion::preset(MotionPreset::PanRight, MotionKind::PanZoom, Camera::NONE));
    let start = c.clip_frame(&still, 0.0);
    let end = c.clip_frame(&still, 4.0);
    assert_ne!(start.data, end.data, "the window moved");
    assert!(stats(&start, [0, 0, 0]).0 > 0);
    // The decode is shared: one still, cached once.
    assert_eq!(c.stills.len(), 1);
    assert!(c.problems().is_empty(), "{:?}", c.problems());
}

#[test]
fn a_model_turns_through_its_camera_over_the_ground() {
    let mut c = compositor(256, 144);
    let mut model = clip(2, ShotType::Ms, 3);
    model.asset_ids.push(asset("props/models/soul_orb_icosphere.glb"));
    let at_rest = c.clip_frame(&model, f64::INFINITY);
    let (drawn, _) = stats(&at_rest, GROUND);
    assert!(drawn > 500, "the model covers pixels over the ground: {drawn}");
    assert!((drawn as f32) < 256.0 * 144.0 * 0.9);
    model.motion =
        Some(assetcraft_storyboard::Motion::preset(MotionPreset::Turntable, MotionKind::Orbit, Camera::NONE));
    let a = c.clip_frame(&model, 0.0);
    let b = c.clip_frame(&model, 0.75);
    assert_ne!(a.data, b.data, "a quarter turn later the picture differs");
    assert_eq!(c.models.len(), 1, "loaded once");
    // A textured model draws too.
    let mut crate_clip = clip(3, ShotType::Cu, 2);
    crate_clip.asset_ids.push(asset("props/models/rune_crate_textured.glb"));
    assert!(stats(&c.clip_frame(&crate_clip, 0.0), GROUND).0 > 500);
    assert!(c.problems().is_empty(), "{:?}", c.problems());
}

#[test]
fn a_title_card_fills_with_its_theme_and_sets_its_words() {
    let mut c = compositor(320, 180);
    let mut card = clip(4, ShotType::Ttl, 2);
    card.description = "REAPERS".into();
    let f = c.clip_frame(&card, f64::INFINITY);
    let look = card.text_style.theme.look();
    let bg = rgb(look.background);
    let (ink, mean) = stats(&f, bg);
    assert!(ink > 100 && ink < 320 * 180 / 4, "letters over the theme background: {ink} pixels");
    let text = rgb(look.text);
    // The inked pixels lean towards the text colour (anti-aliasing mixes in the background).
    let towards = |c: usize| (mean[c] - f32::from(bg[c])).abs() > 0.0 && ((mean[c] - f32::from(text[c])).abs() < 128.0);
    assert!(towards(0) || towards(1) || towards(2), "{mean:?}");
    // The words are centred: ink left and right of centre.
    let ink_in = |x0: u32, x1: u32| {
        (x0..x1).flat_map(|x| (0..180).map(move |y| (x, y))).filter(|&(x, y)| f.get(x, y)[..3] != bg).count()
    };
    assert!(ink_in(0, 160) > 20 && ink_in(160, 320) > 20);
    assert!(ink_in(0, 40) == 0 && ink_in(280, 320) == 0, "nothing at the far edges");
    // A typewriter intro shows fewer letters early on.
    card.text_style.animation = TextAnimation::Typewriter;
    let early = c.clip_frame(&card, 0.05);
    assert!(stats(&early, bg).0 < ink, "fewer letters at 0.05 s");
    // A fade starts almost invisible.
    card.text_style.animation = TextAnimation::Fade;
    let faint = c.clip_frame(&card, 0.0);
    assert_eq!(stats(&faint, bg).0, 0, "alpha 0 at the start of a fade");
    // A card over a background asset shows the picture through a scrim.
    card.asset_ids.push(asset("environments/soul_light_forest.jpg"));
    let over = c.clip_frame(&card, f64::INFINITY);
    assert!(stats(&over, bg).0 > 320 * 180 / 2, "the picture shows through");
}

#[test]
fn an_overlay_sits_in_its_corner_and_an_empty_clip_says_so() {
    let mut c = compositor(320, 180);
    let mut shot = clip(5, ShotType::Ws, 3);
    shot.asset_ids.push(asset("environments/snow_clearing_plate.jpg"));
    let plain = c.clip_frame(&shot, f64::INFINITY);
    shot.overlay = Some("Lower third".into());
    let with = c.clip_frame(&shot, f64::INFINITY);
    assert_ne!(plain.data, with.data);
    // Default position is a lower-third left: the change is in the lower-left quarter.
    let changed = |x0: u32, x1: u32, y0: u32, y1: u32| {
        (x0..x1).flat_map(|x| (y0..y1).map(move |y| (x, y))).filter(|&(x, y)| plain.get(x, y) != with.get(x, y)).count()
    };
    assert!(changed(0, 160, 120, 180) > 50, "lower-left changed");
    assert_eq!(changed(160, 320, 0, 90), 0, "upper-right untouched");

    let empty = clip(6, ShotType::Cu, 2);
    let mut empty = empty;
    empty.description = "Hero close-up".into();
    let f = c.clip_frame(&empty, 0.0);
    let (ink, mean) = stats(&f, [0, 0, 0]);
    assert!(ink > 50, "the brief is drawn");
    assert!(
        mean[0] > 20.0 && (mean[0] - mean[1]).abs() < 2.0 && (mean[1] - mean[2]).abs() < 2.0,
        "grey text: {mean:?}"
    );
}

#[test]
fn a_cut_is_drawn_by_time_and_bad_assets_are_reported_not_fatal() {
    let mut c = compositor(160, 90);
    let mut board = Storyboard::new("cut");
    board.scenes.push(Scene { id: SceneId(1), song_section: None });
    let mut a = clip(1, ShotType::Ttl, 1);
    a.description = "A".into();
    let mut b = clip(2, ShotType::Est, 2);
    b.asset_ids.push(asset("edge-cases/broken_not_an_image.jpg"));
    let mut d = clip(3, ShotType::Ms, 1);
    d.asset_ids.push(asset("palettes/reapers_palette.bmp"));
    board.clips = vec![a, b, d];
    let bg = rgb(board.clips[0].text_style.theme.look().background);
    let f0 = c.frame(&board, Ticks::ZERO);
    assert_eq!(f0.get(2, 2)[..3], bg, "the card at 0 s");
    let f1 = c.frame(&board, Ticks::from_secs_f64(1.5).unwrap_or(Ticks::ZERO));
    assert_eq!(stats(&f1, [0, 0, 0]).0, 0, "the broken still draws nothing, no panic");
    let f3 = c.frame(&board, Ticks::from_secs_f64(3.5).unwrap_or(Ticks::ZERO));
    assert!(stats(&f3, [0, 0, 0]).0 > 0, "the palette at 3.5 s");
    let after = c.frame(&board, Ticks::from_secs(10).unwrap_or(Ticks::ZERO));
    assert_eq!(stats(&after, [0, 0, 0]).0, 0, "past the end: black");
    let problems = c.problems();
    assert_eq!(problems.len(), 1);
    assert!(problems[0].path.ends_with("broken_not_an_image.jpg"));
    assert!(!problems[0].reason.is_empty());
}

#[test]
fn window_and_orbit_view_match_the_viewers() {
    assert_eq!(window(Camera::NONE), [0.0, 0.0, 1.0, 1.0]);
    let zoomed = window(Camera { zoom: 2.0, pan: [0.5, 0.0], ..Camera::NONE });
    assert_eq!(zoomed, [0.5, 0.25, 1.0, 0.75], "kept inside the picture");
    let v = orbit_view(Camera { yaw: 0.5, ..Camera::NONE });
    assert!((v.yaw - (View::default().yaw + 0.5)).abs() < 1e-6);
    assert_eq!(v.zoom, View::default().zoom);
}

/// `ANIMATIC_DUMP=<dir> cargo test -p assetcraft-animatic dump -- --ignored` writes PPM frames to look at.
#[test]
#[ignore]
fn dump() {
    let Ok(dir) = std::env::var("ANIMATIC_DUMP") else { return };
    let mut c = compositor(960, 540);
    let mut still = clip(1, ShotType::Est, 4);
    still.asset_ids.push(asset("environments/soul_light_forest.jpg"));
    still.motion = Some(assetcraft_storyboard::Motion::preset(MotionPreset::PushIn, MotionKind::PanZoom, Camera::NONE));
    still.overlay = Some("Soul light forest".into());
    let mut model = clip(2, ShotType::Ms, 3);
    model.asset_ids.push(asset("props/models/rune_crate_textured.glb"));
    model.motion =
        Some(assetcraft_storyboard::Motion::preset(MotionPreset::Turntable, MotionKind::Orbit, Camera::NONE));
    let mut card = clip(3, ShotType::Ttl, 2);
    card.description = "The Reapers of the Clearing".into();
    let mut empty = clip(4, ShotType::Cu, 2);
    empty.description = "Hero close-up, dusk".into();
    for (name, clip, t) in [
        ("still-0", &still, 0.0),
        ("still-4", &still, 4.0),
        ("model", &model, 1.0),
        ("card", &card, f64::INFINITY),
        ("empty", &empty, 0.0),
    ] {
        let f = c.clip_frame(clip, t);
        let mut ppm = format!("P6 {} {} 255\n", f.width, f.height).into_bytes();
        for p in f.data.chunks_exact(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(format!("{dir}/{name}.ppm"), ppm).unwrap_or_else(|e| panic!("{e}"));
    }
}
