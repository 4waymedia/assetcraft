use super::builtin::{self, BUILTINS, LASER_TRACE, STACK};
use super::*;

const SCALE: f64 = 0.5;

fn px(f: &Frame, x: f64, y: f64) -> [u8; 4] {
    let (x, y) = ((x * SCALE) as u32, (y * SCALE) as u32);
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
}

fn opaque(f: &Frame) -> usize {
    f.rgba.as_chunks::<4>().0.iter().filter(|p| p[3] > 16).count()
}

/// Pixels close to `c` inside the banner's box.
fn near(f: &Frame, c: [u8; 3], tol: i32) -> usize {
    let (x0, y0, x1, y1) = (
        (128.0 * SCALE) as u32,
        (716.0 * SCALE) as u32,
        ((128.0 + 1100.0) * SCALE) as u32,
        ((716.0 + 236.0) * SCALE) as u32,
    );
    let mut n = 0;
    for y in y0..y1.min(f.height) {
        for x in x0..x1.min(f.width) {
            let i = ((y * f.width + x) * 4) as usize;
            let d = (0..3).map(|k| (i32::from(f.rgba[i + k]) - i32::from(c[k])).abs()).max().unwrap_or(255);
            if d <= tol && f.rgba[i + 3] > 200 {
                n += 1;
            }
        }
    }
    n
}

fn laser() -> Design {
    builtin::builtin(LASER_TRACE).expect("listed").expect("builds")
}

/// Plate interior clear of the words (top right of the banner), the accent bar, a headline row.
const PLATE_AT: (f64, f64) = (128.0 + 1100.0 - 40.0, 716.0 + 20.0);
const BAR_AT: (f64, f64) = (128.0 + 5.0, 716.0 + 118.0);

#[test]
fn every_builtin_builds_with_a_packs_controls() {
    for id in BUILTINS {
        let d = builtin::builtin(id).expect("listed").expect("builds");
        assert_eq!(d.size(), (1920, 1080), "{id}");
        assert!((d.seconds() - 6.0).abs() < 1e-6, "{id}");
        let slots: Vec<Slot> = d.controls().iter().filter_map(|c| c.slot).collect();
        assert_eq!(slots, [Slot::Headline, Slot::Caption, Slot::Accent, Slot::Surface, Slot::HeadlineFont], "{id}");
    }
    assert!(builtin::builtin("nope").is_none());
}

#[test]
fn laser_trace_runs_in_hold_out() {
    let d = laser();
    let fill = Fill::default().text(Slot::Headline, "ISTANBUL").text(Slot::Caption, "TURKEY · 03:47 HRS");
    let at = |t: f64| d.render(&fill, t, SCALE).expect("renders");
    // Before the first beam: nothing (the banner sits over the shot).
    assert_eq!(opaque(&at(0.1)), 0);
    // Beams in flight, no fill yet.
    let flying = at(0.6);
    assert!(opaque(&flying) > 200, "beams: {}", opaque(&flying));
    assert_eq!(px(&flying, PLATE_AT.0, PLATE_AT.1)[3], 0, "no fill before the wipe");
    // Hold: dark plate, gold bar, white words, a gold border.
    let hold = at(3.0);
    let plate = px(&hold, PLATE_AT.0, PLATE_AT.1);
    assert!(plate[3] > 200 && plate[0] < 40 && plate[2] < 50, "plate: {plate:?}");
    let bar = px(&hold, BAR_AT.0, BAR_AT.1);
    assert!(bar[0] > 200 && bar[1] > 150 && bar[2] < 110, "gold bar: {bar:?}");
    assert!(near(&hold, [246, 248, 250], 12) > 400, "white headline letters: {}", near(&hold, [246, 248, 250], 12));
    // Out: gone by the end.
    assert_eq!(opaque(&at(5.9)), 0, "folded away");
    // The words decode: a frame mid-decode differs from the hold.
    assert_ne!(at(1.6), hold);
}

#[test]
fn laser_trace_takes_the_themes_accent_and_words() {
    let d = laser();
    let magenta = [1.0, 0.247, 0.682, 1.0];
    let neon = Fill::default().text(Slot::Headline, "NEON").color(Slot::Accent, magenta);
    let hold = d.render(&neon, 3.0, SCALE).expect("renders");
    let bar = px(&hold, BAR_AT.0, BAR_AT.1);
    assert!(bar[0] > 200 && bar[1] < 120 && bar[2] > 130, "magenta bar: {bar:?}");
    assert!(
        near(&hold, [255, 63, 174], 40) > 200,
        "magenta border and detail line: {}",
        near(&hold, [255, 63, 174], 40)
    );
    // Fewer letters ink less.
    let long = d.render(&Fill::default().text(Slot::Headline, "ISTANBUL"), 3.0, SCALE).expect("renders");
    assert!(near(&hold, [246, 248, 250], 12) < near(&long, [246, 248, 250], 12));
}

#[test]
fn builtins_save_as_templates_that_read_back_the_same() {
    let fill = Fill::default().text(Slot::Headline, "ISTANBUL");
    for id in BUILTINS {
        let d = builtin::builtin(id).expect("listed").expect("builds");
        let back = Design::read(&d.to_template()).expect("reads back");
        assert_eq!(back.name(), d.name(), "{id}");
        assert_eq!(back.controls(), d.controls(), "{id}");
        assert_eq!(
            back.render(&fill, 3.0, 0.25).expect("renders"),
            d.render(&fill, 3.0, 0.25).expect("renders"),
            "{id}"
        );
    }
}

fn stack() -> Design {
    builtin::builtin(STACK).expect("listed").expect("builds")
}

#[test]
fn stack_drops_in_holds_and_clears() {
    let d = stack();
    let fill = Fill::default().text(Slot::Headline, "ISTANBUL").text(Slot::Caption, "TURKEY · 03:47 HRS");
    let at = |t: f64| d.render(&fill, t, SCALE).expect("renders");
    assert_eq!(opaque(&at(0.1)), 0, "nothing before the first piece");
    // Mid-drop: some of the banner is in, not all.
    let mid = at(0.9);
    let full = at(3.0);
    assert!(opaque(&mid) > 100 && opaque(&mid) < opaque(&full) * 9 / 10, "{} of {}", opaque(&mid), opaque(&full));
    // Hold: the whole banner, seams closed, as Laser Trace's hold draws it.
    let plate = px(&full, PLATE_AT.0, PLATE_AT.1);
    assert!(plate[3] > 200 && plate[0] < 40 && plate[2] < 50, "plate: {plate:?}");
    let bar = px(&full, BAR_AT.0, BAR_AT.1);
    assert!(bar[0] > 200 && bar[1] > 150 && bar[2] < 110, "gold bar: {bar:?}");
    assert!(near(&full, [246, 248, 250], 12) > 400, "white headline letters");
    // Line clear: the bottom row goes first.
    let clearing = at(4.75);
    assert!(opaque(&clearing) < opaque(&full), "rows going");
    assert_eq!(opaque(&at(5.9)), 0, "cleared");
}

#[test]
fn stack_takes_the_accent_on_every_piece() {
    let d = stack();
    let neon = Fill::default().text(Slot::Headline, "NEON").color(Slot::Accent, [1.0, 0.247, 0.682, 1.0]);
    let hold = d.render(&neon, 3.0, SCALE).expect("renders");
    let bar = px(&hold, BAR_AT.0, BAR_AT.1);
    assert!(bar[0] > 200 && bar[1] < 120 && bar[2] > 130, "magenta bar: {bar:?}");
    // Every copy of the words takes the headline: no "LOCATION" letters left in any piece.
    let default = d.render(&Fill::default(), 3.0, SCALE).expect("renders");
    assert!(near(&hold, [246, 248, 250], 12) < near(&default, [246, 248, 250], 12), "NEON inks less than LOCATION");
}

/// `AC_TIMING=1` prints frame times (run with `--release`).
#[test]
fn timing_when_asked() {
    if std::env::var_os("AC_TIMING").is_none() {
        return;
    }
    for id in BUILTINS {
        let d = builtin::builtin(id).expect("listed").expect("builds");
        let fill = Fill::default().text(Slot::Headline, "ISTANBUL");
        for scale in [1.0 / 3.0, 1.0] {
            let mut ms: Vec<f64> = [0.9, 1.5, 3.0, 4.7]
                .iter()
                .map(|&t| {
                    let s = std::time::Instant::now();
                    let _ = d.render(&fill, t, scale);
                    s.elapsed().as_secs_f64() * 1000.0
                })
                .collect();
            ms.sort_by(f64::total_cmp);
            eprintln!("TIMING {id} scale {scale:.2}: {ms:.0?} ms");
        }
    }
}

/// `AC_FILMSTRIP=<folder>` writes each built-in's filmstrip there (for proofs; never in CI).
#[test]
fn filmstrip_when_asked() {
    let Some(dir) = std::env::var_os("AC_FILMSTRIP") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("folder");
    let times = |id: &str| {
        if id == STACK {
            [0.3, 0.6, 0.9, 1.2, 1.5, 3.0, 4.5, 4.7, 4.9]
        } else {
            [0.4, 0.7, 1.1, 1.5, 1.9, 3.0, 4.6, 5.0, 5.2]
        }
    };
    for id in BUILTINS {
        let d = builtin::builtin(id).expect("listed").expect("builds");
        for (name, fill) in [
            ("gold", Fill::default().text(Slot::Headline, "ISTANBUL").text(Slot::Caption, "TURKEY · 03:47 HRS")),
            (
                "neon",
                Fill::default()
                    .text(Slot::Headline, "ISTANBUL")
                    .text(Slot::Caption, "TURKEY · 03:47 HRS")
                    .color(Slot::Accent, [1.0, 0.247, 0.682, 1.0]),
            ),
        ] {
            let frames: Vec<Frame> = times(id).iter().map(|&t| d.render(&fill, t, 0.25).expect("renders")).collect();
            let (w, h) = (frames[0].width, frames[0].height);
            let cols = 3;
            let rows = frames.len().div_ceil(cols) as u32;
            let mut sheet = image::RgbaImage::new(w * cols as u32, h * rows);
            for (i, f) in frames.iter().enumerate() {
                let (ox, oy) = ((i % cols) as u32 * w, (i / cols) as u32 * h);
                for y in 0..h {
                    for x in 0..w {
                        let k = ((y * w + x) * 4) as usize;
                        let a = f32::from(f.rgba[k + 3]) / 255.0;
                        // Over a dusk-blue backdrop, like the mock's shot.
                        let bg = [40.0, 45.0, 75.0];
                        let c = |j: usize| (f32::from(f.rgba[k + j]) * a + bg[j] * (1.0 - a)) as u8;
                        sheet.put_pixel(ox + x, oy + y, image::Rgba([c(0), c(1), c(2), 255]));
                    }
                }
            }
            sheet.save(dir.join(format!("{id}-{name}.png"))).expect("saved");
        }
    }
}

/// L4: a built-in banner covers its banner box at rest (1100 × 236 at (128, 716) of 1920 ×
/// 1080), give or take its glow; worked out once.
#[test]
fn a_built_in_banner_covers_its_box_at_rest() {
    for d in [laser(), stack()] {
        let [x, y, w, h] = d.visible_bounds().expect("it draws at rest");
        let near = |a: f32, b: f32, tol: f32| (a - b).abs() <= tol;
        assert!(near(x, 128.0 / 1920.0, 0.03) && near(w, 1100.0 / 1920.0, 0.06), "{:?}: x {x} w {w}", d.name());
        assert!(near(y, 716.0 / 1080.0, 0.04) && near(h, 236.0 / 1080.0, 0.08), "{:?}: y {y} h {h}", d.name());
        assert_eq!(d.visible_bounds(), Some([x, y, w, h]), "kept");
    }
}

/// L4c: a built-in banner built in a box of its own: its words at the size its height gives,
/// smaller when they don't fit, and what it draws at rest is that box.
#[test]
fn a_built_in_banner_is_built_in_its_box_with_its_words_fitted() {
    use builtin::Banner;
    let d = Banner::DEFAULT;
    let own = [d.x, d.y, d.w, d.h];
    assert_eq!(Banner::fit(own, "LOCATION", "DETAIL · 00:00 HRS"), d, "the mock's words fit the mock's box");
    // A long headline: smaller, and it fits between the bar and the right edge.
    let long = Banner::fit(own, "How to <do the thing> and a lot more besides", "");
    assert!(long.title_size < d.title_size * 0.8, "{}", long.title_size);
    assert_eq!(long.detail_size, d.detail_size);
    // A taller box: bigger words; a box past the comp's edge is kept inside it.
    let tall = Banner::fit([100.0, 300.0, 1700.0, 472.0], "LOCATION", "");
    assert!((tall.title_size - 184.0).abs() < 0.5, "{}", tall.title_size);
    let off = Banner::fit([1800.0, 1000.0, 800.0, 300.0], "A", "");
    assert!(off.x + off.w <= Banner::COMP.0 && off.y + off.h <= Banner::COMP.1, "{off:?}");
    // Built in a wide, short box at the top: at rest it covers that box (its glow aside).
    let b = Banner::fit([300.0, 120.0, 1320.0, 180.0], "How to <do the thing> and a lot more besides", "Step one");
    for id in BUILTINS {
        let design = builtin::builtin_in(id, b).expect("listed").expect("builds");
        let [x, y, w, h] = design.visible_bounds().expect("draws at rest");
        let (cw, ch) = Banner::COMP;
        let near = |a: f64, want: f64, tol: f64| (a - want).abs() <= tol;
        assert!(near(f64::from(x) * cw, 300.0, 24.0) && near(f64::from(w) * cw, 1320.0, 48.0), "{id}: x {x} w {w}");
        assert!(near(f64::from(y) * ch, 120.0, 24.0) && near(f64::from(h) * ch, 180.0, 48.0), "{id}: y {y} h {h}");
        // Kept: the same box is built once.
        let a = builtin::builtin_kept(id, b).expect("listed").expect("builds");
        let again = builtin::builtin_kept(id, b).expect("listed").expect("builds");
        assert!(std::sync::Arc::ptr_eq(&a, &again));
    }
}

/// R-T18: a built-in's own text fields (its Caption, not the Headline) and a fill by name.
#[test]
fn a_designs_own_fields_fill_by_name() {
    for d in [laser(), stack()] {
        assert_eq!(d.text_fields(), [("Caption".to_owned(), "DETAIL · 00:00 HRS".to_owned())]);
        let base = crate::Fill::default().text(crate::Slot::Headline, "ISTANBUL");
        let mock = d.render(&base, 3.0, 0.25).expect("renders");
        let named = d.render(&base.clone().named("caption", ""), 3.0, 0.25).expect("renders");
        let slot = d.render(&base.clone().text(crate::Slot::Caption, ""), 3.0, 0.25).expect("renders");
        assert_ne!(mock.rgba, named.rgba, "{}: the field by name, any case", d.name());
        assert_eq!(named.rgba, slot.rgba, "{}: the same as its slot", d.name());
        // By name over the slot.
        let both = base.text(crate::Slot::Caption, "FROM THE SLOT").named("Caption", "");
        assert_eq!(d.render(&both, 3.0, 0.25).expect("renders").rgba, slot.rgba);
    }
}
