use super::*;

use std::sync::Arc;

use effectcraft_project::build;
use effectcraft_project::essential::{EgControl, EssentialGraphics};
use effectcraft_project::{Comp, Footage, LayerSource, Solid};
use effectcraft_time::FrameRate;

const W: u32 = 320;
const H: u32 = 180;

fn px(f: &Frame, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
}

fn footage(p: &mut Project, name: &str, w: u32, h: u32) -> ItemId {
    let f = Footage {
        path: name.into(),
        kind: FootageKind::Still,
        width: w,
        height: h,
        has_video: true,
        ..Default::default()
    };
    p.add_item(name, Label::None, None, ItemKind::Footage(f))
}

/// A card with a dark backdrop, a full-frame `Background Image` placeholder and a 100 × 100
/// `Logo` placeholder in the middle, each exposed as a Media control.
fn card() -> Design {
    card_with("Background Image")
}

/// [`card`] with its full-frame control named `background`.
fn card_with(background: &str) -> Design {
    let mut p = Project::default();
    let comp = Comp::new(W, H, FrameRate::FPS_30, Tick::from_seconds_f64(2.0));
    let cid = p.add_item("Card", Label::None, None, ItemKind::Comp(comp.clone().into()));
    let sid = p.add_item(
        "Dark",
        Label::None,
        None,
        ItemKind::Solid(Solid { color: [0.05, 0.05, 0.08], width: W, height: H, pixel_aspect: 1.0 }),
    );
    let bg_item = footage(&mut p, "placeholder-bg.png", W, H);
    let logo_item = footage(&mut p, "placeholder-logo.png", 100, 100);
    let dark = build::layer(&mut p, &comp, "Dark", LayerSource::Solid { item: sid }, (W, H), None);
    let bg = build::layer(&mut p, &comp, "Background", LayerSource::Footage { item: bg_item }, (W, H), None);
    let logo = build::layer(&mut p, &comp, "Logo", LayerSource::Footage { item: logo_item }, (100, 100), None);
    let controls = vec![
        EgControl { id: p.alloc(), name: "Logo".into(), kind: EgKind::Media { layer: logo.id }, as_type: None },
        EgControl { id: p.alloc(), name: background.into(), kind: EgKind::Media { layer: bg.id }, as_type: None },
    ];
    if let Some(c) = p.comp_mut(cid) {
        for l in [logo, bg, dark] {
            c.layers.push(l);
        }
        c.essential = Some(EssentialGraphics { name: "Media Card".into(), controls });
    }
    Design::from_project(p, cid).expect("a design")
}

/// A `w × h` picture whose columns run through `colors` in equal bands.
fn bands(id: u64, w: u32, h: u32, colors: &[[u8; 3]]) -> Picture {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..h {
        for x in 0..w {
            let c = colors[(x as usize * colors.len()) / w as usize];
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Picture::new(id, w, h, rgba).expect("a picture")
}

const RED: [u8; 3] = [230, 20, 20];
const BLUE: [u8; 3] = [20, 40, 230];
const GREEN: [u8; 3] = [20, 200, 40];
const YELLOW: [u8; 3] = [240, 220, 20];

fn close(p: [u8; 4], c: [u8; 3]) -> bool {
    (0..3).all(|k| (i32::from(p[k]) - i32::from(c[k])).abs() <= 12) && p[3] > 240
}

#[test]
fn media_controls_fill_their_slots() {
    let d = card();
    let slots: Vec<Option<Slot>> = d.controls().iter().map(|c| c.slot).collect();
    assert_eq!(slots, [Some(Slot::Logo), Some(Slot::BackgroundImage)]);
    assert_eq!(Slot::Logo.fit(), Fit::Contain);
    assert_eq!(Slot::BackgroundImage.fit(), Fit::Cover);
}

#[test]
fn without_pictures_the_placeholders_draw_nothing() {
    let f = card().render(&Fill::default(), 1.0, 1.0).expect("renders");
    let dark = px(&f, W / 2, H / 2);
    assert!(dark[0] < 30 && dark[2] < 40 && dark[3] > 240, "the backdrop: {dark:?}");
}

#[test]
fn a_logo_shows_whole_inside_its_placeholder() {
    // A 2:1 logo in the 100 × 100 placeholder: 100 × 50 across its middle, clear above and below.
    let fill = Fill::default().picture(Slot::Logo, bands(1, 200, 100, &[RED]));
    let f = card().render(&fill, 1.0, 1.0).expect("renders");
    let (cx, cy) = (W / 2, H / 2);
    assert!(close(px(&f, cx, cy), RED), "centre: {:?}", px(&f, cx, cy));
    assert!(close(px(&f, cx - 45, cy), RED), "left edge: {:?}", px(&f, cx - 45, cy));
    let above = px(&f, cx, cy - 40);
    assert!(above[0] < 30 && above[3] > 240, "above the logo, the backdrop: {above:?}");
    assert!(px(&f, cx - 60, cy)[0] < 30, "outside the placeholder");
}

#[test]
fn a_background_covers_its_placeholder_centre_cropped() {
    // 640 × 180 in four bands over a 320 × 180 placeholder: the middle two show.
    let fill = Fill::default().picture(Slot::BackgroundImage, bands(2, 640, 180, &[RED, BLUE, GREEN, YELLOW]));
    let f = card().render(&fill, 1.0, 1.0).expect("renders");
    assert!(close(px(&f, 10, 20), BLUE), "left: {:?}", px(&f, 10, 20));
    assert!(close(px(&f, W - 10, 20), GREEN), "right: {:?}", px(&f, W - 10, 20));
    // Both together, at half size; the logo draws over the background.
    let both = fill.picture(Slot::Logo, bands(1, 100, 100, &[YELLOW]));
    let f = card().render(&both, 1.0, 0.5).expect("renders");
    assert_eq!((f.width, f.height), (W / 2, H / 2));
    assert!(close(px(&f, W / 4, H / 4), YELLOW), "logo: {:?}", px(&f, W / 4, H / 4));
}

#[test]
fn pictures_survive_the_template_round_trip_and_bad_ones_are_refused() {
    let d = Design::read(&card().to_template()).expect("reads back");
    let fill = Fill::default().picture(Slot::Logo, bands(1, 100, 100, &[GREEN]));
    let f = d.render(&fill, 0.5, 1.0).expect("renders");
    assert!(close(px(&f, W / 2, H / 2), GREEN));
    assert!(Picture::new(1, 0, 10, vec![]).is_none());
    assert!(Picture::new(1, 2, 2, vec![0; 15]).is_none());
    assert!(Picture::new(1, picture::MAX_SIDE + 1, 1, vec![0; (picture::MAX_SIDE as usize + 1) * 4]).is_none());
    // A picture's debug form is short (it goes into the Viewer's cache key).
    assert_eq!(format!("{:?}", bands(0xab, 3, 2, &[RED])), "Picture(#00000000000000ab, 3x2)");
}

/// A 1.5 s movie: red for its first second, then blue; it notes the sides it was asked for.
struct TwoTone {
    sides: std::sync::Mutex<Vec<u32>>,
}

impl MovieFrames for TwoTone {
    fn frame(&self, secs: f64, side: u32) -> Option<Picture> {
        self.sides.lock().ok()?.push(side);
        assert!((0.0..1.5).contains(&secs), "asked inside the movie: {secs}");
        let c = if secs < 1.0 { RED } else { BLUE };
        Some(bands(if secs < 1.0 { 10 } else { 11 }, 64, 36, &[c]))
    }
}

#[test]
fn a_background_video_plays_and_loops_over_the_design() {
    let d = card_with("Background Video");
    assert_eq!(d.controls()[1].slot, Some(Slot::BackgroundVideo));
    let frames = Arc::new(TwoTone { sides: std::sync::Mutex::new(Vec::new()) });
    let movie = Movie::new(7, 1280, 720, 1.5, frames.clone()).expect("a movie");
    let fill = Fill::default().movie(Slot::BackgroundVideo, movie.clone());
    let at = |t: f64| px(&d.render(&fill, t, 1.0).expect("renders"), 10, 20);
    assert!(close(at(0.5), RED), "0.5 s: {:?}", at(0.5));
    assert!(close(at(1.2), BLUE), "1.2 s: {:?}", at(1.2));
    assert!(close(at(1.8), RED), "1.8 s loops to 0.3 s: {:?}", at(1.8));
    // Asked at the size it draws: the 320 × 180 placeholder covered by a 16:9 movie, at 1× and ½×.
    let _ = d.render(&fill, 0.5, 0.5).expect("renders");
    let sides = frames.sides.lock().expect("sides").clone();
    assert_eq!(sides.last(), Some(&160));
    assert!(sides.iter().all(|s| *s == 320 || *s == 160), "{sides:?}");
    assert!((movie.time_at(3.2) - 0.2).abs() < 1e-9 && movie.time_at(f64::NAN) == 0.0);
    assert!(Movie::new(1, 10, 10, 0.0, frames.clone()).is_none());
    assert!(Movie::new(1, 0, 10, 1.0, frames).is_none());
    assert_eq!(format!("{movie:?}"), "Movie(#0000000000000007, 1280x720, 1.500s)");
}

/// A movie that can't be read leaves its placeholder empty; the design still draws.
struct Unreadable;

impl MovieFrames for Unreadable {
    fn frame(&self, _secs: f64, _side: u32) -> Option<Picture> {
        None
    }
}

#[test]
fn an_unreadable_movie_leaves_its_placeholder_empty() {
    let d = card_with("Background Video");
    let movie = Movie::new(8, 640, 360, 2.0, Arc::new(Unreadable)).expect("a movie");
    let f = d.render(&Fill::default().movie(Slot::BackgroundVideo, movie), 0.5, 1.0).expect("renders");
    let dark = px(&f, 10, 20);
    assert!(dark[0] < 30 && dark[2] < 40, "the backdrop: {dark:?}");
}
