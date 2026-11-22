//! SF1b-1b: `Design::gaps` — effects the renderer doesn't know, fonts that aren't there, footage
//! the renderer isn't given.

use super::*;

use effectcraft_project::props::PropGroup;
use effectcraft_project::{GroupKind, Layer};

/// The sample card with an effect `id` on its Plate layer (`enabled` or not).
fn with_effect(id: &str, enabled: bool) -> Design {
    let (mut p, comp) = crate::tests::sample();
    let uid = p.alloc();
    let mut group = PropGroup::new(uid, id, id);
    group.kind = GroupKind::Effect { effect: id.into() };
    group.enabled = enabled;
    let plate: &mut Layer =
        p.comp_mut(comp).and_then(|c| c.layers.iter_mut().find(|l| l.name == "Plate")).expect("the plate layer");
    match plate.props.sub_mut("effects") {
        Some(fx) => fx.children.push(group.into()),
        None => panic!("a layer has an effects group"),
    }
    Design::from_project(p, comp).expect("a design")
}

#[test]
fn the_sample_draws_as_designed() {
    let (p, comp) = crate::tests::sample();
    let d = Design::from_project(p, comp).unwrap();
    assert_eq!(d.gaps(&Fill::default()), []);
    assert_eq!(d.gaps(&Fill::default().text(Slot::Headline, "Hello").color(Slot::Accent, [1.0; 4])), []);
}

#[test]
fn an_effect_the_renderer_does_not_know_is_a_gap() {
    let known = effectcraft_effects::registry().first().expect("some effects").id;
    assert_eq!(with_effect(known, true).gaps(&Fill::default()), [], "{known} is known");
    let gaps = with_effect("ec.not.an.effect", true).gaps(&Fill::default());
    assert_eq!(
        gaps,
        [Gap { kind: GapKind::Effect, name: "ec.not.an.effect".into(), layer: "Plate".into(), field: None }]
    );
    assert!(gaps[0].to_string().contains("ec.not.an.effect"), "{}", gaps[0]);
    assert_eq!(with_effect("ec.not.an.effect", false).gaps(&Fill::default()), [], "switched off: not drawn");
}

#[test]
fn a_font_that_is_not_there_is_a_gap_and_the_fill_decides_which_font() {
    let (p, comp) = crate::tests::sample();
    let d = Design::from_project(p, comp).unwrap();
    let gaps = d.gaps(&Fill::default().font(Slot::HeadlineFont, "No Such Family 9", "Bold"));
    assert_eq!(gaps.len(), 1, "{gaps:?}");
    assert_eq!(
        (gaps[0].kind, gaps[0].name.as_str(), gaps[0].layer.as_str()),
        (GapKind::Font, "No Such Family 9 Bold", "Words")
    );
    // A bundled face resolves.
    assert_eq!(d.gaps(&Fill::default().font(Slot::HeadlineFont, "JetBrains Mono", "Regular")), []);
}

#[test]
fn media_the_renderer_is_not_given_is_a_gap_until_its_field_is_filled() {
    let d = crate::picture_tests::card();
    let fields = |fill: &Fill| -> Vec<Option<String>> { d.gaps(fill).into_iter().map(|g| g.field).collect() };
    assert_eq!(fields(&Fill::default()), [Some("Background Image".to_string()), Some("Logo".to_string())]);
    let logo = Fill::default().picture(Slot::Logo, crate::picture_tests::bands(1, 100, 100, &[[200, 20, 20]]));
    let gaps = d.gaps(&logo);
    assert_eq!(gaps.len(), 1);
    assert_eq!((gaps[0].kind, gaps[0].field.as_deref()), (GapKind::Media, Some("Background Image")));
    assert!(gaps[0].to_string().contains("“Background Image” field"), "{}", gaps[0]);
    let both = logo.picture(Slot::BackgroundImage, crate::picture_tests::bands(2, 320, 180, &[[20, 20, 200]]));
    assert_eq!(d.gaps(&both), []);
}

#[test]
fn the_built_in_banners_draw_as_designed() {
    for id in ["banner-laser-trace", "banner-stack"] {
        let d = crate::builtin::builtin(id).expect("a built-in").expect("it opens");
        let fill = Fill::default().text(Slot::Headline, "Hollow Saint").text(Slot::Caption, "Part one");
        assert_eq!(d.gaps(&fill), [], "{id}");
    }
}
