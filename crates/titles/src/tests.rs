use super::*;

use effectcraft_color::Label;
use effectcraft_keyframe::{Justify, Keyframe, TextDoc};
use effectcraft_project::build::{self, Ids};
use effectcraft_project::essential::{EgControl, EssentialGraphics};
use effectcraft_project::props::{Node, PropGroup};
use effectcraft_project::{Comp, ItemKind, Layer, LayerSource, Solid, Uid};
use effectcraft_time::FrameRate;

const W: u32 = 320;
const H: u32 = 180;
const ORANGE: [f64; 4] = [0.9, 0.35, 0.1, 1.0];

/// The uid of the first property with match id `id` inside `group`.
fn find(group: &PropGroup, id: &str) -> Option<Uid> {
    group.children.iter().find_map(|n| match n {
        Node::Prop(p) if p.match_id == id => Some(p.uid),
        Node::Prop(_) => None,
        Node::Group(g) => find(g, id),
    })
}

fn control(p: &mut Project, name: &str, layer: &Layer, prop: Uid, as_type: Option<ControlType>) -> EgControl {
    EgControl {
        id: p.alloc(),
        name: name.into(),
        kind: EgKind::Property { layer: layer.id, prop, links: vec![] },
        as_type,
    }
}

/// A card a designer might make: a dark solid, an orange plate exposed as `Accent`, white words
/// exposed as `Headline` (and as `Headline Font`), a blue bar exposed as `Brand Blue` (not a
/// slot: it keeps its colour), the plate's opacity exposed as `Text` (a slider: the wrong kind
/// for that slot) and a 1 s fade on the words.
pub(crate) fn sample() -> (Project, ItemId) {
    let mut p = Project::default();
    let comp = Comp::new(W, H, FrameRate::FPS_30, Tick::from_seconds_f64(2.0));
    let cid = p.add_item("Card", Label::Sandstone, None, ItemKind::Comp(comp.clone().into()));
    let sid = p.add_item(
        "Dark",
        Label::Sandstone,
        None,
        ItemKind::Solid(Solid { color: [0.05, 0.05, 0.08], width: W, height: H, pixel_aspect: 1.0 }),
    );
    let bg = build::layer(&mut p, &comp, "Background", LayerSource::Solid { item: sid }, (W, H), None);
    let mut plate = build::layer(&mut p, &comp, "Plate", LayerSource::Shape, (W, H), None);
    let mut bar = build::layer(&mut p, &comp, "Bar", LayerSource::Shape, (W, H), None);
    let mut next = p.next_id;
    let mut ids = Ids(&mut next);
    let plate_shape = {
        let rect = build::shape_rect(&mut ids, [240.0, 60.0], [0.0, 0.0], 8.0);
        let fill = build::shape_fill(&mut ids, ORANGE);
        build::shape_group(&mut ids, "Plate", vec![rect, fill])
    };
    let bar_shape = {
        let rect = build::shape_rect(&mut ids, [f64::from(W), 12.0], [0.0, f64::from(H) / 2.0 - 6.0], 0.0);
        let fill = build::shape_fill(&mut ids, [0.1, 0.3, 0.9, 1.0]);
        build::shape_group(&mut ids, "Bar", vec![rect, fill])
    };
    p.next_id = next;
    if let Some(c) = plate.props.sub_mut("contents") {
        c.children.push(plate_shape.into());
    }
    if let Some(c) = bar.props.sub_mut("contents") {
        c.children.push(bar_shape.into());
    }
    let mut words = build::layer(&mut p, &comp, "Words", LayerSource::Text, (W, H), None);
    let doc =
        TextDoc { text: "SAMPLE".into(), size: 28.0, fill: [1.0; 4], justify: Justify::Center, ..Default::default() };
    if let Some(prop) = words.props.prop_mut("text/sourceText") {
        prop.value = Value::Text(Box::new(doc));
    }
    if let Some(prop) = words.props.prop_mut("transform/position") {
        prop.value = Value::Vec3([f64::from(W) / 2.0, f64::from(H) / 2.0 + 10.0, 0.0]);
    }
    if let Some(prop) = words.props.prop_mut("transform/opacity") {
        prop.keys = vec![
            Keyframe::new(Tick::ZERO, Value::Scalar(0.0)),
            Keyframe::new(Tick::from_seconds_f64(1.0), Value::Scalar(100.0)),
        ];
    }
    let plate_color = find(&plate.props, "color").expect("plate fill colour");
    let bar_color = find(&bar.props, "color").expect("bar fill colour");
    let plate_opacity = plate.props.prop("transform/opacity").map(|p| p.uid).expect("plate opacity");
    let source = words.props.prop("text/sourceText").map(|p| p.uid).expect("source text");
    let controls = vec![
        control(&mut p, "Headline", &words, source, None),
        control(&mut p, "accent", &plate, plate_color, None),
        control(&mut p, "Brand Blue", &bar, bar_color, None),
        control(&mut p, "Text", &plate, plate_opacity, None),
        control(&mut p, "Headline Font", &words, source, Some(ControlType::Font)),
    ];
    if let Some(c) = p.comp_mut(cid) {
        // Layer 0 is the top layer.
        for l in [words, plate, bar, bg] {
            c.layers.push(l);
        }
        c.essential = Some(EssentialGraphics { name: "Sample Card".into(), controls });
    }
    (p, cid)
}

fn template(project: &Project, _comp: ItemId, manifest: serde_json::Value) -> Vec<u8> {
    effectcraft_lottie::zip::store(&[
        ("manifest.json".into(), manifest.to_string().into_bytes()),
        ("project.ecproj".into(), project.to_json().into_bytes()),
    ])
}

fn good_manifest(comp: ItemId) -> serde_json::Value {
    serde_json::json!({"format": TEMPLATE_FORMAT, "version": 1, "kind": "comp", "comp": comp.0, "name": "Acme Card"})
}

fn px(f: &Frame, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
}

/// A point on the plate clear of the words.
const PLATE: (u32, u32) = (W / 2 - 110, H / 2);
/// A point on the bar.
const BAR: (u32, u32) = (W / 2, H - 4);

fn white_in_middle(f: &Frame) -> usize {
    (H / 2 - 12..H / 2 + 14)
        .flat_map(|y| (W / 2 - 100..W / 2 + 100).map(move |x| (x, y)))
        .filter(|&(x, y)| px(f, x, y)[1] > 220)
        .count()
}

#[test]
fn a_saved_design_reads_back_with_its_controls() {
    let (p, comp) = sample();
    let d = Design::read(&template(&p, comp, good_manifest(comp))).unwrap();
    assert_eq!(d.name(), "Acme Card", "the manifest's name");
    assert_eq!(d.size(), (W, H));
    assert!((d.seconds() - 2.0).abs() < 1e-6);
    let c = d.controls();
    let got: Vec<(&str, ControlKind, Option<Slot>)> = c.iter().map(|c| (c.name.as_str(), c.kind, c.slot)).collect();
    assert_eq!(
        got,
        [
            ("Headline", ControlKind::Text, Some(Slot::Headline)),
            ("accent", ControlKind::Color, Some(Slot::Accent)),
            ("Brand Blue", ControlKind::Color, None),
            ("Text", ControlKind::Other, None),
            ("Headline Font", ControlKind::Font, Some(Slot::HeadlineFont)),
        ]
    );
}

#[test]
fn unfilled_the_design_draws_as_the_designer_made_it() {
    let (p, comp) = sample();
    let d = Design::from_project(p, comp).unwrap();
    assert_eq!(d.name(), "Sample Card", "the Essential Graphics name");
    let f = d.render(&Fill::default(), 1.5, 1.0).unwrap();
    assert_eq!((f.width, f.height), (W, H));
    let plate = px(&f, PLATE.0, PLATE.1);
    assert!(plate[0] > 200 && plate[2] < 60, "orange plate: {plate:?}");
    let bar = px(&f, BAR.0, BAR.1);
    assert!(bar[2] > 200 && bar[0] < 60, "blue bar: {bar:?}");
    assert!(white_in_middle(&f) > 100, "the words");
}

#[test]
fn a_fill_sets_the_words_and_the_accent_and_leaves_the_rest() {
    let (p, comp) = sample();
    let d = Design::from_project(p, comp).unwrap();
    let plain = d.render(&Fill::default(), 1.5, 1.0).unwrap();
    let fill = Fill::default()
        .text(Slot::Headline, "ACME WORKS")
        .color(Slot::Accent, [0.1, 0.8, 0.2, 1.0])
        // Not slots in this design (or the wrong kind): no effect.
        .color(Slot::Text, [1.0, 0.0, 0.0, 1.0])
        .color(Slot::Background, [1.0, 1.0, 0.0, 1.0]);
    assert_eq!(d.overrides(&fill).len(), 2, "Headline and Accent");
    let f = d.render(&fill, 1.5, 1.0).unwrap();
    let plate = px(&f, PLATE.0, PLATE.1);
    assert!(plate[1] > 180 && plate[0] < 60, "the accent is green now: {plate:?}");
    let bar = px(&f, BAR.0, BAR.1);
    assert!(bar[2] > 200 && bar[0] < 60, "the bar keeps its own blue: {bar:?}");
    assert_eq!(px(&f, 2, 2), px(&plain, 2, 2), "the background isn't a control");
    let (a, b) = (white_in_middle(&plain), white_in_middle(&f));
    assert!(b > a, "ten letters ink more than six: {a} → {b}");
    // The words keep the designer's size, colour and fade: half way through the fade at 0.5 s.
    let mid = white_in_middle(&d.render(&fill, 0.5, 1.0).unwrap());
    assert!(mid < b / 2 + b / 4, "fading in: {mid} of {b}");
}

#[test]
fn time_holds_at_the_end_and_scale_sets_the_size() {
    let (p, comp) = sample();
    let d = Design::from_project(p, comp).unwrap();
    let fill = Fill::default();
    assert_eq!(
        d.render(&fill, 60.0, 1.0).unwrap(),
        d.render(&fill, 1.99, 1.0).unwrap(),
        "past the end holds the last frame"
    );
    assert_eq!(d.render(&fill, f64::NAN, 1.0).unwrap(), d.render(&fill, 0.0, 1.0).unwrap());
    let half = d.render(&fill, 1.0, 0.5).unwrap();
    assert_eq!((half.width, half.height), (W / 2, H / 2));
    assert!((d.fit_scale(1920, 1080) - 6.0f64.min(4.0)).abs() < 1e-9, "capped at 4×");
    assert!((d.fit_scale(160, 1000) - 0.5).abs() < 1e-9);
    let fallback = d.render(&fill, 1.0, f64::INFINITY).unwrap();
    assert_eq!((fallback.width, fallback.height), (W, H));
}

#[test]
fn a_font_fill_changes_the_face_and_keeps_the_size() {
    let (p, comp) = sample();
    let d = Design::from_project(p, comp).unwrap();
    let fill = Fill::default().font(Slot::HeadlineFont, "JetBrains Mono", "Regular");
    let o = d.overrides(&fill);
    assert_eq!(o.len(), 1);
    match &o[0].value {
        Value::Text(doc) => {
            assert_eq!((doc.font.as_str(), doc.style.as_str()), ("JetBrains Mono", "Regular"));
            assert!((doc.size - 28.0).abs() < 1e-9, "the design's size");
            assert_eq!(doc.text, "SAMPLE", "the words stay");
        }
        v => panic!("{v:?}"),
    }
    let plain = d.render(&Fill::default(), 1.5, 1.0).unwrap();
    assert_ne!(d.render(&fill, 1.5, 1.0).unwrap(), plain, "another face draws other letters");
}

#[test]
fn slot_names_match_any_case() {
    for s in Slot::ALL {
        assert_eq!(Slot::from_name(s.name()), Some(s));
        assert_eq!(Slot::from_name(&format!("  {}  ", s.name().to_uppercase())), Some(s));
    }
    assert_eq!(Slot::from_name("Accent2"), None, "the space is part of the name");
    assert_eq!(Slot::from_name(""), None);
}

#[test]
fn bad_files_are_refused_with_a_reason() {
    let (p, comp) = sample();
    let err = |bytes: &[u8]| Design::read(bytes).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err(b"").contains("not an EffectCraft template"));
    assert!(err(b"PK\x03\x04 garbage").contains("not an EffectCraft template"));
    assert!(err(&vec![0u8; MAX_FILE + 1]).contains("too large"));
    let only = |name: &str, body: &[u8]| effectcraft_lottie::zip::store(&[(name.to_string(), body.to_vec())]);
    assert!(err(&only("readme.txt", b"hi")).contains("no manifest.json"));
    assert!(err(&only("manifest.json", b"{")).contains("manifest.json"));
    let with = |m: serde_json::Value| template(&p, comp, m);
    assert!(
        err(&with(serde_json::json!({"format": "lottie", "comp": comp.0}))).contains("isn't an EffectCraft template")
    );
    assert!(
        err(&with(serde_json::json!({"format": TEMPLATE_FORMAT, "version": 9, "comp": comp.0})))
            .contains("newer EffectCraft")
    );
    assert!(err(&with(serde_json::json!({"format": TEMPLATE_FORMAT, "version": 1}))).contains("names no composition"));
    assert!(
        err(&with(serde_json::json!({"format": TEMPLATE_FORMAT, "version": 1, "comp": 999_999})))
            .contains("names no composition")
    );
    let broken = effectcraft_lottie::zip::store(&[
        ("manifest.json".into(), good_manifest(comp).to_string().into_bytes()),
        ("project.ecproj".into(), b"{ not json".to_vec()),
    ]);
    assert!(err(&broken).contains("project can't be read"));
    let big = effectcraft_lottie::zip::store(&[("manifest.json".into(), vec![b' '; MAX_MANIFEST + 1])]);
    assert!(err(&big).contains("manifest is too large"));
    // A comp too small to export.
    let mut tiny = p.clone();
    if let Some(c) = tiny.comp_mut(comp) {
        c.width = 8;
    }
    assert!(Design::from_project(tiny, comp).err().map(|e| e.to_string()).unwrap_or_default().contains("8 × 180"));
}

#[test]
fn every_library_file_is_refused_or_read_without_a_crash() {
    // Hostile input: whatever is in the test library, read as a design.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library");
    let mut seen = 0;
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path)
                && bytes.len() <= MAX_FILE
            {
                let _ = Design::read(&bytes);
                seen += 1;
            }
        }
    }
    assert!(seen > 20, "{seen} files");
}
