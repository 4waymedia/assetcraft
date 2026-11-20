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
    let before = effectcraft_text::fonts::all_faces().len();
    assert_eq!(register_font(renamed_mono("Acme Sans Mono")).expect("again"), faces);
    assert_eq!(effectcraft_text::fonts::all_faces().len(), before);
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
