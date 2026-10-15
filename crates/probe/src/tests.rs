use super::*;
use assetcraft_fs::{FakeFs, RealFs};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
}

fn d(width: u32, height: u32) -> Option<Dimensions> {
    Some(Dimensions { width, height })
}

/// (relative path, format, width × height).
type Expected = (&'static str, Format, Option<(u32, u32)>);

/// Expected values computed independently with Pillow from `test_data/library`.
const LIBRARY: &[Expected] = &[
    ("characters/reaper/reaper_concept_final.png", Format::Png, Some((1024, 1024))),
    ("characters/reaper/reaper_concept_v01.png", Format::Png, Some((1024, 1024))),
    ("characters/reaper/reaper_concept_v02.png", Format::Png, Some((1024, 1024))),
    ("characters/reaper/reaper_concept_v03.png", Format::Png, Some((1024, 1024))),
    ("characters/reaper/turnaround/reaper_back.jpg", Format::Jpeg, Some((800, 1200))),
    ("characters/reaper/turnaround/reaper_front.jpg", Format::Jpeg, Some((800, 1200))),
    ("characters/reaper/turnaround/reaper_side.jpg", Format::Jpeg, Some((800, 1200))),
    ("edge-cases/Réaper ünïcode name.png", Format::Png, Some((400, 400))),
    ("edge-cases/TINY_ICON.JPG", Format::Jpeg, Some((64, 64))),
    ("edge-cases/broken_not_an_image.jpg", Format::Unknown, None),
    ("edge-cases/panorama_20to1.jpg", Format::Jpeg, Some((4000, 200))),
    ("environments/dupes/snow_clearing_plate_bright.jpg", Format::Jpeg, Some((1920, 1080))),
    ("environments/dupes/snow_clearing_plate_copy.jpg", Format::Jpeg, Some((1920, 1080))),
    ("environments/dupes/snow_clearing_plate_small.jpg", Format::Jpeg, Some((960, 540))),
    ("environments/frozen_lake_wide.webp", Format::WebP, Some((2560, 1080))),
    ("environments/ruins_vertical.png", Format::Png, Some((1080, 1920))),
    ("environments/snow_clearing_plate.jpg", Format::Jpeg, Some((1920, 1080))),
    ("environments/soul_light_forest.jpg", Format::Jpeg, Some((1920, 1080))),
    ("environments/clearing_raw_orient6.dng", Format::CameraRaw, None),
    ("environments/lantern_dusk.heic", Format::Heif, None),
    ("palettes/reapers_palette.bmp", Format::Bmp, Some((600, 120))),
    ("palettes/reapers_palette_board.psd", Format::Psd, Some((320, 200))),
    ("props/scythe_transparent.png", Format::Png, Some((512, 512))),
    ("props/soul_orb_small.gif", Format::Gif, Some((256, 256))),
    ("storyboard/shot_010_intro.tiff", Format::Tiff, Some((1200, 800))),
    ("storyboard/shot_020_sketch.png", Format::Png, Some((1280, 720))),
    ("storyboard/shot_030_sketch.png", Format::Png, Some((1280, 720))),
    ("environments/location_breakdown.pdf", Format::Pdf, None),
    ("environments/snow_clearing_flythrough.mp4", Format::Mp4, None),
    ("environments/snow_clearing_flythrough.mov", Format::QuickTime, None),
    ("environments/soul_light_loop.webm", Format::Matroska, None),
    ("environments/frozen_lake_pan.avi", Format::Avi, None),
];

#[test]
fn every_library_file_probes_as_expected() {
    for (rel, format, expected) in LIBRARY {
        let p = probe(&RealFs, &library().join(rel)).unwrap();
        assert_eq!(p.format, *format, "{rel}");
        assert_eq!(p.dimensions, expected.map(|(w, h)| Dimensions { width: w, height: h }), "{rel}");
        assert!(p.size > 0, "{rel}");
        assert!(p.modified.is_some(), "{rel}");
    }
}

#[test]
fn broken_jpeg_is_flagged_unreadable() {
    let p = probe(&RealFs, &library().join("edge-cases/broken_not_an_image.jpg")).unwrap();
    assert_eq!(p.status, Status::Unrecognized);
    assert_eq!(p.claimed, Some(Format::Jpeg));
    assert!(p.is_unreadable());
    assert_eq!(p.category(), Category::Image);
    assert!(!p.extension_mismatch());
}

#[test]
fn uppercase_extension_is_understood() {
    let p = probe(&RealFs, &library().join("edge-cases/TINY_ICON.JPG")).unwrap();
    assert_eq!((p.format, p.claimed, p.status), (Format::Jpeg, Some(Format::Jpeg), Status::Ok));
}

#[test]
fn contents_beat_the_extension() {
    let png = std::fs::read(library().join("props/scythe_transparent.png")).unwrap();
    let fs = FakeFs::new().file_with("/x/really_a_png.jpg", png);
    let p = probe(&fs, Path::new("/x/really_a_png.jpg")).unwrap();
    assert_eq!((p.format, p.claimed), (Format::Png, Some(Format::Jpeg)));
    assert!(p.extension_mismatch());
    assert!(!p.is_unreadable());
}

#[test]
fn folders_are_not_probed() {
    assert!(probe(&RealFs, &library()).is_err());
}

#[test]
fn never_panics_on_any_prefix_of_any_library_file() {
    for (rel, _, _) in LIBRARY {
        let bytes = std::fs::read(library().join(rel)).unwrap();
        for len in 0..bytes.len().min(600) {
            let _ = probe_bytes(&bytes[..len], Some("jpg"));
        }
    }
}

#[test]
fn truncated_headers_are_damaged_not_ok() {
    let png = std::fs::read(library().join("props/scythe_transparent.png")).unwrap();
    assert_eq!(probe_bytes(&png[..20], None), (Format::Png, None, Status::Damaged));
    let jpg = std::fs::read(library().join("edge-cases/TINY_ICON.JPG")).unwrap();
    assert_eq!(probe_bytes(&jpg[..40], None).2, Status::Damaged);
}

// ---- hand-built headers ------------------------------------------------------------------

fn gif(w: u16, h: u16) -> Vec<u8> {
    let mut b = b"GIF89a".to_vec();
    b.extend(w.to_le_bytes());
    b.extend(h.to_le_bytes());
    b
}

#[test]
fn gif_header() {
    assert_eq!(probe_bytes(&gif(320, 200), None), (Format::Gif, d(320, 200), Status::Ok));
}

#[test]
fn bmp_top_down_and_core_headers() {
    let mut b = b"BM".to_vec();
    b.resize(14, 0);
    b.extend(40u32.to_le_bytes());
    b.extend(640i32.to_le_bytes());
    b.extend((-480i32).to_le_bytes());
    assert_eq!(probe_bytes(&b, None).1, d(640, 480));
    let mut core = b"BM".to_vec();
    core.resize(14, 0);
    core.extend(12u32.to_le_bytes());
    core.extend(100u16.to_le_bytes());
    core.extend(50u16.to_le_bytes());
    core.resize(26, 0);
    assert_eq!(probe_bytes(&core, None).1, d(100, 50));
}

fn riff_webp(chunk: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut b = b"RIFF\0\0\0\0WEBP".to_vec();
    b.extend(chunk);
    b.extend((payload.len() as u32).to_le_bytes());
    b.extend(payload);
    b
}

#[test]
fn webp_lossy_lossless_and_extended() {
    let mut vp8 = vec![0u8; 3];
    vp8.extend([0x9d, 0x01, 0x2a]);
    vp8.extend(1920u16.to_le_bytes());
    vp8.extend(1080u16.to_le_bytes());
    assert_eq!(probe_bytes(&riff_webp(b"VP8 ", &vp8), None).1, d(1920, 1080));

    let bits: u32 = (100 - 1) | ((50 - 1) << 14);
    let mut vp8l = vec![0x2f];
    vp8l.extend(bits.to_le_bytes());
    assert_eq!(probe_bytes(&riff_webp(b"VP8L", &vp8l), None).1, d(100, 50));

    let mut vp8x = vec![0u8; 4];
    vp8x.extend(&(4000u32 - 1).to_le_bytes()[..3]);
    vp8x.extend(&(3000u32 - 1).to_le_bytes()[..3]);
    assert_eq!(probe_bytes(&riff_webp(b"VP8X", &vp8x), None).1, d(4000, 3000));
}

fn tiff_be(width: u32, height: u16) -> Vec<u8> {
    let mut b = b"MM\0*".to_vec();
    b.extend(8u32.to_be_bytes());
    b.extend(2u16.to_be_bytes());
    // ImageWidth LONG
    b.extend(256u16.to_be_bytes());
    b.extend(4u16.to_be_bytes());
    b.extend(1u32.to_be_bytes());
    b.extend(width.to_be_bytes());
    // ImageLength SHORT
    b.extend(257u16.to_be_bytes());
    b.extend(3u16.to_be_bytes());
    b.extend(1u32.to_be_bytes());
    b.extend(height.to_be_bytes());
    b.extend([0, 0]);
    b
}

#[test]
fn tiff_big_endian_and_camera_raw() {
    assert_eq!(probe_bytes(&tiff_be(6000, 4000), None), (Format::Tiff, d(6000, 4000), Status::Ok));
    assert_eq!(probe_bytes(&tiff_be(6000, 4000), Some("nef")), (Format::CameraRaw, None, Status::Ok));
}

#[test]
fn tiff_ifd_outside_the_head_is_damaged() {
    let mut b = b"II*\0".to_vec();
    b.extend(u32::MAX.to_le_bytes());
    assert_eq!(probe_bytes(&b, None).2, Status::Damaged);
}

#[test]
fn psd_header() {
    let mut b = b"8BPS".to_vec();
    b.extend(1u16.to_be_bytes());
    b.extend([0u8; 6]);
    b.extend(3u16.to_be_bytes()); // channels
    b.extend(768u32.to_be_bytes()); // height
    b.extend(1024u32.to_be_bytes()); // width
    assert_eq!(probe_bytes(&b, None), (Format::Psd, d(1024, 768), Status::Ok));
}

#[test]
fn containers_and_media_are_sniffed() {
    let ftyp = |brand: &[u8; 4]| {
        let mut b = vec![0, 0, 0, 24];
        b.extend(b"ftyp");
        b.extend(brand);
        b
    };
    assert_eq!(probe_bytes(&ftyp(b"isom"), None).0, Format::Mp4);
    assert_eq!(probe_bytes(&ftyp(b"qt  "), None).0, Format::QuickTime);
    assert_eq!(probe_bytes(&ftyp(b"heic"), None).0, Format::Heif);
    assert_eq!(probe_bytes(&ftyp(b"avif"), None).0, Format::Avif);
    assert_eq!(probe_bytes(b"%PDF-1.7\n", None), (Format::Pdf, None, Status::Ok));
    assert_eq!(probe_bytes(&[0x1A, 0x45, 0xDF, 0xA3, 1], None).0, Format::Matroska);
    assert_eq!(probe_bytes(b"glTF\x02\0\0\0", None).0, Format::Glb);
    assert_eq!(probe_bytes(b"PK\x03\x04rest", Some("docx")).0, Format::Zip);
    assert_eq!(probe_bytes(b"RIFF\0\0\0\0AVI LIST", None).0, Format::Avi);
    assert_eq!(probe_bytes(b"RIFF\0\0\0\0WAVEfmt ", None).0, Format::Wav);
    assert_eq!(probe_bytes(b"ID3\x04\0", None).0, Format::Mp3);
}

#[test]
fn text_formats_need_text_and_the_right_extension() {
    assert_eq!(probe_bytes(b"  {\"asset\":{\"version\":\"2.0\"}}", Some("gltf")).0, Format::Gltf);
    assert_eq!(probe_bytes(b"v 0 0 0\nv 1 0 0\nf 1 2 3\n", Some("obj")).0, Format::Obj);
    assert_eq!(probe_bytes(b"v 0 0 0\n", Some("txt")).0, Format::Unknown);
    assert_eq!(probe_bytes(&[0u8, 159, 146, 150], Some("obj")).0, Format::Unknown);
}

#[test]
fn categories_and_aspect() {
    assert_eq!(Format::Psd.category(), Category::Image);
    assert_eq!(Format::Mp4.category(), Category::Video);
    assert_eq!(Format::Glb.category(), Category::Model3d);
    assert_eq!(Dimensions { width: 1920, height: 1080 }.aspect().map(|a| (a * 100.0).round()), Some(178.0));
    assert_eq!(Dimensions { width: 1, height: 0 }.aspect(), None);
}

/// A real JPEG with `segments` maximum-size APP2 segments (65,535 bytes each, like a big ICC
/// profile or XMP packet) inserted right after SOI, pushing the SOF marker far into the file.
fn jpeg_with_big_app_segments(segments: usize) -> Vec<u8> {
    let jpg = std::fs::read(library().join("edge-cases/TINY_ICON.JPG")).unwrap();
    let mut out = jpg[..2].to_vec();
    for _ in 0..segments {
        out.extend([0xFF, 0xE2, 0xFF, 0xFF]);
        out.extend(std::iter::repeat_n(0x42, 65_533));
    }
    out.extend(&jpg[2..]);
    out
}

#[test]
fn jpeg_sof_behind_more_than_head_bytes_of_metadata_is_found() {
    let big = jpeg_with_big_app_segments(5); // ~320 KiB of APP2 > HEAD_BYTES
    assert!(big.len() > HEAD_BYTES + 60_000);
    let fs = FakeFs::new().file_with("/icc.jpg", big.clone());
    let p = probe(&fs, Path::new("/icc.jpg")).unwrap();
    assert_eq!((p.format, p.dimensions, p.status), (Format::Jpeg, d(64, 64), Status::Ok));
    assert!(!p.is_unreadable());
    // Truncated after the metadata (no SOF anywhere): damaged, not a panic or a hang.
    let fs = FakeFs::new().file_with("/cut.jpg", big[..HEAD_BYTES + 70_000].to_vec());
    assert_eq!(probe(&fs, Path::new("/cut.jpg")).unwrap().status, Status::Damaged);
}

#[test]
fn jpeg_segment_hopping_is_bounded() {
    // 300 segments (~19 MiB) is past both the hop and the offset cap: give up as damaged.
    let fs = FakeFs::new().file_with("/huge.jpg", jpeg_with_big_app_segments(300));
    let p = probe(&fs, Path::new("/huge.jpg")).unwrap();
    assert_eq!((p.dimensions, p.status), (None, Status::Damaged));
}

#[test]
fn camera_raw_magic_is_sniffed_without_the_extension() {
    let mut cr3 = vec![0, 0, 0, 24];
    cr3.extend(b"ftypcrx ");
    assert_eq!(probe_bytes(&cr3, None), (Format::CameraRaw, None, Status::Ok));
    for magic in
        [&b"FUJIFILMCCD-RAW 0201"[..], b"IIRO\x08\0\0\0", b"IIRS\x08\0\0\0", b"MMOR\0\0\0\x08", b"IIU\0\x08\0\0\0"]
    {
        assert_eq!(probe_bytes(magic, None).0, Format::CameraRaw, "{magic:?}");
    }
    assert_eq!(probe_bytes(b"II*\0\x10\0\0\0CR\x02\0", None).0, Format::CameraRaw, "CR2");
    for ext in ["cr3", "raf", "rw2", "dng", "orf"] {
        assert_eq!(Format::from_extension(ext), Some(Format::CameraRaw), "{ext}");
    }
}

#[test]
fn library_models_probe_as_3d() {
    for (rel, format) in [
        ("props/models/soul_orb_icosphere.glb", Format::Glb),
        ("props/models/scythe_ring.gltf", Format::Gltf),
        ("props/models/tombstone.obj", Format::Obj),
    ] {
        let p = probe(&RealFs, &library().join(rel)).unwrap();
        assert_eq!((p.format, p.status, p.category()), (format, Status::Ok, Category::Model3d), "{rel}");
        assert_eq!(p.dimensions, None, "{rel}");
    }
}
