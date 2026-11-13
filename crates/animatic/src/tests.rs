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
/// Pixels of `img` that differ from `blank` (the same frame without its words) and their mean.
fn ink_over(img: &Rgba, blank: &Rgba) -> (usize, [f32; 3]) {
    let mut n = 0usize;
    let mut sum = [0.0f32; 3];
    for (p, q) in img.data.as_chunks::<4>().0.iter().zip(blank.data.as_chunks::<4>().0) {
        if p[..3] != q[..3] {
            n += 1;
            for c in 0..3 {
                sum[c] += f32::from(p[c]);
            }
        }
    }
    let mean = if n > 0 { [sum[0] / n as f32, sum[1] / n as f32, sum[2] / n as f32] } else { [0.0; 3] };
    (n, mean)
}

fn stats(img: &Rgba, rgb: [u8; 3]) -> (usize, [f32; 3]) {
    let mut n = 0usize;
    let mut sum = [0.0f32; 3];
    for p in img.data.as_chunks::<4>().0 {
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
    // The theme's backdrop and bar without the words: what the letters are drawn over.
    let mut blank_card = card.clone();
    blank_card.description = " ".into();
    let blank = c.clip_frame(&blank_card, f64::INFINITY);
    assert!(stats(&blank, [0, 0, 0]).0 > 320 * 170, "the backdrop fills the card");
    let (ink, mean) = ink_over(&f, &blank);
    assert!(ink > 100 && ink < 320 * 180 / 4, "letters over the theme background: {ink} pixels");
    let text = rgb(look.text);
    // The inked pixels lean towards the text colour (anti-aliasing mixes in the background).
    let towards = |c: usize| (mean[c] - f32::from(bg[c])).abs() > 0.0 && ((mean[c] - f32::from(text[c])).abs() < 128.0);
    assert!(towards(0) || towards(1) || towards(2), "{mean:?}");
    // The words are centred: ink left and right of centre.
    let ink_in = |x0: u32, x1: u32| {
        (x0..x1)
            .flat_map(|x| (0..180).map(move |y| (x, y)))
            .filter(|&(x, y)| f.get(x, y)[..3] != blank.get(x, y)[..3])
            .count()
    };
    assert!(ink_in(0, 160) > 20 && ink_in(160, 320) > 20);
    assert!(ink_in(0, 40) == 0 && ink_in(280, 320) == 0, "nothing at the far edges");
    // A typewriter intro shows fewer letters early on.
    card.text_style.animation = TextAnimation::Typewriter;
    let early = c.clip_frame(&card, 0.05);
    assert!(ink_over(&early, &blank).0 < ink, "fewer letters at 0.05 s");
    // A fade starts almost invisible.
    card.text_style.animation = TextAnimation::Fade;
    let faint = c.clip_frame(&card, 0.0);
    assert_eq!(ink_over(&faint, &blank).0, 0, "alpha 0 at the start of a fade");
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
    board.scenes.push(Scene::new(SceneId(1), None));
    let mut a = clip(1, ShotType::Ttl, 1);
    a.description = "A".into();
    let mut b = clip(2, ShotType::Est, 2);
    b.asset_ids.push(asset("edge-cases/broken_not_an_image.jpg"));
    let mut d = clip(3, ShotType::Ms, 1);
    d.asset_ids.push(asset("palettes/reapers_palette.bmp"));
    board.clips = vec![a, b, d];
    let backdrop = board.clips[0].text_style.theme.look().backdrop;
    let want = backdrop.color_at(2.5 / 160.0, 2.5 / 90.0).map(|v| v.round() as u8);
    let f0 = c.frame(&board, Ticks::ZERO);
    assert_eq!(f0.get(2, 2)[..3], want, "the card at 0 s");
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
fn a_model_fills_a_frame_wider_than_the_renderer_cap() {
    // 4K: the renderer caps at 2048 a side; the picture must still be centred and full-width.
    assert_eq!(render_size(3840, 2160), (2048, 1152));
    assert_eq!(render_size(1920, 1080), (1920, 1080));
    assert_eq!(render_size(2160, 3840), (1152, 2048));
    let mut c = compositor(3840, 2160);
    let mut model = clip(9, ShotType::Ms, 2);
    model.asset_ids.push(asset("props/models/soul_orb_icosphere.glb"));
    let f = c.clip_frame(&model, f64::INFINITY);
    // Pixels that aren't the ground, by column: the right half must have some, and the
    // picture's horizontal centre of mass sits near the middle.
    let mut sum_x = 0.0f64;
    let mut n = 0.0f64;
    let mut right = 0usize;
    for y in (0..2160).step_by(8) {
        for x in (0..3840).step_by(8) {
            if f.get(x, y)[..3] != GROUND {
                sum_x += f64::from(x);
                n += 1.0;
                if x >= 1920 {
                    right += 1;
                }
            }
        }
    }
    assert!(n > 100.0, "the model is drawn");
    assert!(right > 0, "something on the right half");
    let centre = sum_x / n;
    assert!((centre - 1920.0).abs() < 300.0, "centre of mass at x = {centre:.0}");
    assert!(c.problems().is_empty(), "{:?}", c.problems());
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
        for p in f.data.as_chunks::<4>().0 {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(format!("{dir}/{name}.ppm"), ppm).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn a_sections_inset_is_drawn_in_its_corner_a_video_as_a_poster_box_and_swap_flips_them() {
    use assetcraft_storyboard::{Corner, Inset, InsetKind, InsetSize};
    let mut comp = compositor(320, 180);
    let mut board = Storyboard::new("Inset");
    board.scenes.push(Scene::new(SceneId(1), None));
    let mut card = clip(1, ShotType::Ttl, 2);
    card.description = "Step 1".into();
    let mut shot = clip(2, ShotType::Ms, 4);
    shot.asset_ids.push(asset("environments/soul_light_forest.jpg"));
    board.clips = vec![card, shot];
    let at_shot = Ticks::from_secs(3).unwrap();
    let plain = comp.frame(&board, at_shot);
    // A graphic in the bottom-right corner, medium: 30 % of the width, 16:9, inset 2.5 %.
    board.scenes[0].inset =
        Some(Inset { asset: Some(asset("environments/snow_clearing_plate.jpg")), ..Inset::slot(InsetKind::Graphic) });
    let with = comp.frame(&board, at_shot);
    let corner = |img: &Rgba| -> Vec<[u8; 4]> {
        // The middle of the bottom-right inset box: x in [224+8, 312-8], y in [172-54+8 .. 172-8].
        (120..160).flat_map(|y| (240..300).map(move |x| (x, y))).map(|(x, y)| img.get(x, y)).collect()
    };
    assert_ne!(corner(&plain), corner(&with), "the corner changed");
    let top_left = |img: &Rgba| -> Vec<[u8; 4]> {
        (10..60).flat_map(|y| (10..100).map(move |x| (x, y))).map(|(x, y)| img.get(x, y)).collect()
    };
    assert_eq!(top_left(&plain), top_left(&with), "the shot is untouched elsewhere");
    // A title card never carries the inset.
    assert_eq!(comp.frame(&board, Ticks::from_secs(1).unwrap()), {
        let mut no = board.clone();
        no.scenes[0].inset = None;
        comp.frame(&no, Ticks::from_secs(1).unwrap())
    });
    // Top-left, small: the top-left changes, the bottom-right is the plain shot again.
    board.scenes[0].inset.as_mut().unwrap().corner = Corner::TopLeft;
    board.scenes[0].inset.as_mut().unwrap().size = InsetSize::Small;
    let tl = comp.frame(&board, at_shot);
    assert_ne!(top_left(&plain), top_left(&tl));
    assert_eq!(corner(&plain), corner(&tl));
    // A video: a dark poster box with its name (no decoder here).
    board.scenes[0].inset = Some(Inset {
        asset: Some(asset("environments/snow_clearing_flythrough.mov")),
        ..Inset::slot(InsetKind::Video)
    });
    let video = comp.frame(&board, at_shot);
    let (_, mean) = stats(&Rgba { width: 60, height: 40, data: corner(&video).concat() }, [0, 0, 0]);
    assert!(mean[0] < 0x60 as f32 && mean[2] < 0x70 as f32, "a dark box: {mean:?}");
    assert!(comp.problems().is_empty(), "{:?}", comp.problems());
    // Swap: the inset fills the frame and the shot sits in the corner.
    board.scenes[0].inset.as_mut().unwrap().swap = true;
    board.scenes[0].inset.as_mut().unwrap().asset = Some(asset("environments/snow_clearing_plate.jpg"));
    board.scenes[0].inset.as_mut().unwrap().kind = InsetKind::Graphic;
    let swapped = comp.frame(&board, at_shot);
    assert_ne!(top_left(&swapped), top_left(&plain), "the graphic is the picture now");
    assert_ne!(corner(&swapped), corner(&with), "the shot is in the corner");
    // An empty slot draws nothing.
    board.scenes[0].inset = Some(Inset::slot(InsetKind::Video));
    assert_eq!(comp.frame(&board, at_shot), plain);
}

/// Themes 1d: a card's backdrop is a gradient with a vignette, and the accent bar is drawn.
#[test]
fn a_title_card_draws_its_gradient_and_accent_bar() {
    let mut c = compositor(320, 180);
    let mut card = Clip::new(ClipId(1), SceneId(1), ShotType::Ttl, Ticks::from_secs(4).unwrap());
    card.description = "Step 3".into();
    card.text_style.theme = assetcraft_storyboard::TextTheme::Blueprint;
    let f = c.clip_frame(&card, f64::INFINITY);
    let (top, bottom, corner) = (f.get(160, 4), f.get(160, 175), f.get(0, 0));
    assert!(top[2] > bottom[2] + 20, "lighter blue at the top: {top:?} vs {bottom:?}");
    assert!(corner[2] < f.get(160, 30)[2], "vignette darkens the corner");
    let yellow = f.data.as_chunks::<4>().0.iter().filter(|p| p[0] > 230 && p[1] > 190 && p[2] < 110).count();
    assert!(yellow > 20, "the yellow accent bar is drawn ({yellow} px)");
    // Cinematic has no bar; its black carries grain, the same every time.
    card.text_style.theme = assetcraft_storyboard::TextTheme::Cinematic;
    let a = c.clip_frame(&card, f64::INFINITY);
    assert!(a.data.as_chunks::<4>().0.iter().take(320).any(|p| p[0] > 0), "grain on black");
    assert_eq!(a, c.clip_frame(&card, f64::INFINITY), "grain is steady between frames");
}

/// Themes 1e: Blueprint's corner marks, frame and grid are in the exported card.
#[test]
fn a_title_card_draws_its_decor() {
    let mut c = compositor(640, 360);
    let mut card = Clip::new(ClipId(1), SceneId(1), ShotType::Ttl, Ticks::from_secs(4).unwrap());
    card.description = "Step 3".into();
    card.text_style.theme = assetcraft_storyboard::TextTheme::Blueprint;
    let f = c.clip_frame(&card, f64::INFINITY);
    let yellow = |p: &[u8]| p[0] > 200 && p[1] > 170 && p[2] < 120;
    // The top-left corner mark sits inside the frame line, 6.5 % in.
    let corner = (20..40).flat_map(|y| (20..40).map(move |x| (x, y))).filter(|&(x, y)| yellow(&f.get(x, y))).count();
    assert!(corner > 10, "corner mark: {corner} px");
    // The frame runs along the top, 4 % in.
    assert!(yellow(&f.get(320, 14)) || yellow(&f.get(320, 15)), "frame line");
    // Grid lines are faint white over the blue: a column at the grid line is lighter than beside it.
    let grid_x = (320.0f32 % 36.0) as u32 + 36;
    assert!(f.get(grid_x, 100)[0] > f.get(grid_x + 9, 100)[0], "grid line lighter than the paper");
    // Bold draws no card decor: only the backdrop, the words and its bar.
    card.text_style.theme = assetcraft_storyboard::TextTheme::Bold;
    let b = c.clip_frame(&card, f64::INFINITY);
    assert!(!yellow(&b.get(30, 30)));
}

/// Themes 1f: Neon Night's glow and Bold's shadow are in the exported card.
#[test]
fn a_title_card_draws_its_text_finish() {
    let mut c = compositor(640, 360);
    let mut card = Clip::new(ClipId(1), SceneId(1), ShotType::Ttl, Ticks::from_secs(4).unwrap());
    card.description = "Neon".into();
    card.text_style.theme = assetcraft_storyboard::TextTheme::NeonNight;
    let glow = c.clip_frame(&card, f64::INFINITY);
    let pinkish = |p: &[u8; 4]| {
        let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
        r > g + 40 && b > g + 40 && r > 120
    };
    let halo = glow.data.as_chunks::<4>().0.iter().filter(|p| pinkish(p)).count();
    assert!(halo > 400, "a magenta halo round the letters: {halo} px");
    // Bold: a dark pixel with a white letter just up and to the left of it is the shadow
    // (the black accent bar has no letter above-left of it).
    card.text_style.theme = assetcraft_storyboard::TextTheme::Bold;
    let bold = c.clip_frame(&card, f64::INFINITY);
    let shadowed = (3..360)
        .flat_map(|y| (3..640).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let (p, up_left) = (bold.get(x, y), bold.get(x - 3, y - 3));
            p[0] < 120 && up_left[0] > 230 && up_left[1] > 230
        })
        .count();
    assert!(shadowed > 30, "a shadow down-right of the letters: {shadowed} px");
}
