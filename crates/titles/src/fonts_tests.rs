use super::builtin::{self, LASER_TRACE};
use super::fonts::{FontFace, has_family, register_font};
use super::*;

const MONO: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");

/// `MONO` with its family renamed to `to` (the same length, so every offset holds) in both the
/// Mac (single-byte) and Windows (UTF-16 big-endian) name records: a font no one has bundled.
pub(crate) fn renamed_mono(to: &str) -> Vec<u8> {
    let from = "JetBrains Mono";
    assert_eq!(from.len(), to.len());
    let swap = |bytes: &mut Vec<u8>, a: &[u8], b: &[u8]| {
        let mut i = 0;
        while i + a.len() <= bytes.len() {
            if &bytes[i..i + a.len()] == a {
                bytes[i..i + a.len()].copy_from_slice(b);
                i += a.len();
            } else {
                i += 1;
            }
        }
    };
    let wide = |s: &str| s.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>();
    let mut out = MONO.to_vec();
    swap(&mut out, from.as_bytes(), to.as_bytes());
    swap(&mut out, &wide(from), &wide(to));
    out
}

#[test]
fn a_registered_font_is_the_family_a_design_draws_with() {
    assert!(!has_family("Acme Sans Mono"), "not before it's registered");
    let faces = register_font(renamed_mono("Acme Sans Mono")).expect("a font");
    assert_eq!(faces, [FontFace { family: "Acme Sans Mono".into(), style: "Regular".into() }]);
    assert!(has_family("acme sans mono") && has_family("Inter"));
    // The same bytes again: the same faces, and the database doesn't grow.
    // Counted by this family only (the database is app-wide; parallel tests may add others).
    let ours = || effectcraft_text::fonts::all_faces().iter().filter(|f| f.info.family == "Acme Sans Mono").count();
    let before = ours();
    assert_eq!(before, 1);
    assert_eq!(register_font(renamed_mono("Acme Sans Mono")).expect("again"), faces);
    assert_eq!(ours(), before);
    // A design naming it draws its glyphs (JetBrains Mono's), not Inter's.
    let d = builtin::builtin(LASER_TRACE).expect("listed").expect("builds");
    let words = |family: &str| {
        let fill = Fill::default().text(Slot::Headline, "ISTANBUL").font(Slot::HeadlineFont, family, "Regular");
        d.render(&fill, 3.0, 0.25).expect("renders")
    };
    assert_eq!(words("Acme Sans Mono"), words("JetBrains Mono"));
    assert_ne!(words("Acme Sans Mono"), words("Inter"));
}

#[test]
fn a_file_that_isnt_a_font_is_refused() {
    assert!(register_font(b"not a font at all".to_vec()).is_err());
    assert!(register_font(Vec::new()).is_err());
    let mut cut = MONO.to_vec();
    cut.truncate(40);
    assert!(register_font(cut).is_err(), "a truncated font");
    assert!(register_font(vec![0; fonts::MAX_FONT_BYTES + 1]).is_err(), "too big");
}

/// TP4c: an edit copy (the template's project, as EffectCraft saves it) reads as the same
/// design; one whose composition id moved falls back to its composition with controls; junk
/// and a project with no controls are refused.
#[test]
fn an_edit_copy_reads_like_its_template() {
    let d = builtin::builtin(builtin::STACK).expect("listed").expect("builds");
    let template = d.to_template();
    let (text, comp, name) = super::template_project(&template).expect("a template");
    assert_eq!(name.as_deref(), Some(d.name()));
    let fill = Fill::default().text(Slot::Headline, "EDIT");
    let copy = Design::read_project(&text, comp).expect("reads");
    assert_eq!(copy.render(&fill, 1.0, 0.25).expect("renders"), d.render(&fill, 1.0, 0.25).expect("renders"));
    let moved = Design::read_project(&text, comp + 9999).expect("falls back to the comp with controls");
    assert_eq!(moved.controls(), d.controls());
    assert!(Design::read_project("{ not a project", comp).is_err());
    assert!(Design::read_project(&effectcraft_project::Project::default().to_json(), comp).is_err(), "no controls");
    assert!(super::template_project(b"not a zip").is_err());
}

/// TP4c-2: Save into Template keeps the template's other entries and its name, takes the
/// edit's project (and composition), and reads back as the edit.
#[test]
fn an_edit_saves_into_its_template() {
    let laser = builtin::builtin(builtin::LASER_TRACE).expect("listed").expect("builds");
    let stack = builtin::builtin(builtin::STACK).expect("listed").expect("builds");
    // A template with one more entry (a poster), which must survive.
    let mut entries = effectcraft_lottie::zip::read_stored(&laser.to_template());
    entries.push(("poster.png".into(), vec![1, 2, 3]));
    let template = effectcraft_lottie::zip::store(&entries);
    let (edit, _, _) = super::template_project(&stack.to_template()).expect("a template");
    let saved = super::template_with_project(&template, &edit).expect("saved");
    let back = Design::read(&saved).expect("reads back");
    assert_eq!(back.name(), laser.name(), "the template's name");
    let fill = Fill::default().text(Slot::Headline, "SAVED");
    assert_eq!(back.render(&fill, 0.9, 0.25).expect("renders"), stack.render(&fill, 0.9, 0.25).expect("renders"));
    let kept = effectcraft_lottie::zip::read_stored(&saved);
    assert!(kept.iter().any(|(n, b)| n == "poster.png" && b == &[1, 2, 3]));
    assert_eq!(kept.len(), entries.len());
    // A broken edit, or a template that isn't one, changes nothing.
    assert!(super::template_with_project(&template, "{ nope").is_err());
    assert!(super::template_with_project(b"not a zip", &edit).is_err());
}

/// R-T16: a design's comp markers say where its words rest (`hold`, its duration ending the
/// hold) and leave (`out`); a marker outside the design, or a hold after the out, is ignored.
#[test]
fn comp_markers_time_a_designs_in_and_out() {
    use effectcraft_project::Marker;
    let m = |t: f64, d: f64, c: &str| Marker {
        time: Tick::from_seconds_f64(t),
        duration: Tick::from_seconds_f64(d),
        comment: c.into(),
        ..Default::default()
    };
    let ph = |ms: Vec<Marker>| super::phases_of(&ms, 6.0);
    let close = |p: (Option<f64>, Option<f64>), a: Option<f64>, b: Option<f64>| {
        let near = |x: Option<f64>, y: Option<f64>| match (x, y) {
            (Some(x), Some(y)) => (x - y).abs() < 1e-6,
            (None, None) => true,
            _ => false,
        };
        near(p.0, a) && near(p.1, b)
    };
    assert!(close(ph(vec![]), None, None));
    assert!(close(ph(vec![m(1.5, 0.0, "hold"), m(4.0, 0.0, "out")]), Some(1.5), Some(4.0)));
    assert!(close(ph(vec![m(1.5, 2.0, " Hold ")]), Some(1.5), Some(3.5)), "a hold's duration ends it");
    assert!(close(ph(vec![m(1.5, 2.0, "hold"), m(4.5, 0.0, "OUT")]), Some(1.5), Some(4.5)), "an out marker wins");
    assert!(close(ph(vec![m(4.2, 0.0, "out")]), None, Some(4.2)), "one end alone");
    assert!(close(ph(vec![m(0.0, 0.0, "hold"), m(6.5, 0.0, "out")]), None, None), "outside the design");
    assert!(close(ph(vec![m(4.0, 0.0, "hold"), m(2.0, 0.0, "out")]), None, None), "a hold after the out");
    assert!(close(ph(vec![m(1.0, 0.0, "chapter 1")]), None, None), "other markers");
    // The built-ins are marked (and keep it through a template).
    for id in builtin::BUILTINS {
        let d = builtin::builtin(id).expect("listed").expect("builds");
        let (hold, out) = d.phases();
        assert!(
            hold.is_some_and(|h| h > 0.5 && h < 3.0) && out.is_some_and(|o| (o - 4.4).abs() < 1e-6),
            "{id}: {:?}",
            d.phases()
        );
        assert_eq!(Design::read(&d.to_template()).expect("reads back").phases(), d.phases(), "{id}");
    }
}
