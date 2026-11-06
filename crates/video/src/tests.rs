//! SF5-2: the encoders and the muxer, checked by decoding the result back and walking the boxes.

use super::*;

/// A moving gradient with a sharp square, `w × h` RGBA at time `t` (seconds).
fn frame(w: u32, h: u32, t: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    let sx = (t * 40.0) as u32;
    for y in 0..h {
        for x in 0..w {
            let in_box = (sx..sx + w / 4).contains(&x) && (h / 3..h / 3 + h / 4).contains(&y);
            let r = if in_box { 230 } else { (x * 255 / w) as u8 };
            let g = if in_box { 40 } else { (y * 255 / h) as u8 };
            let b = if in_box { 60 } else { 120 };
            out.extend_from_slice(&[r, g, b, 255]);
        }
    }
    out
}

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let mse: f64 = a.iter().zip(b).map(|(x, y)| (f64::from(*x) - f64::from(*y)).powi(2)).sum::<f64>() / a.len() as f64;
    if mse <= 1e-9 { 99.0 } else { 10.0 * (255.0f64 * 255.0 / mse).log10() }
}

/// The samples back as Annex-B, parameter sets first, for the reference decoder.
fn annex_b(track: &VideoTrack) -> Vec<u8> {
    let (sps, pps) = track.parameter_sets();
    let mut out = Vec::new();
    for ps in [sps, pps] {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(ps);
    }
    for s in track.samples() {
        let mut i = 0;
        while i + 4 <= s.data.len() {
            let n = u32::from_be_bytes([s.data[i], s.data[i + 1], s.data[i + 2], s.data[i + 3]]) as usize;
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(&s.data[i + 4..i + 4 + n]);
            i += 4 + n;
        }
    }
    out
}

/// Top-level boxes: (type, start, size).
fn boxes(bytes: &[u8]) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 8 <= bytes.len() {
        let size = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        let kind = String::from_utf8_lossy(&bytes[i + 4..i + 8]).into_owned();
        out.push((kind, i, size));
        if size < 8 {
            break;
        }
        i += size;
    }
    out
}

fn find(bytes: &[u8], kind: &[u8; 4]) -> Option<usize> {
    bytes.windows(4).position(|w| w == kind)
}

#[test]
fn settings_are_checked_before_anything_is_allocated() {
    for (s, why) in [
        (Settings { width: 15, ..Settings::HD }, "too narrow"),
        (Settings { width: 1921, ..Settings::HD }, "odd"),
        (Settings { height: 9000, ..Settings::HD }, "too tall"),
        (Settings { fps: 0, ..Settings::HD }, "fps 0"),
        (Settings { fps: 121, ..Settings::HD }, "fps 121"),
        (Settings { qp: 52, ..Settings::HD }, "qp 52"),
    ] {
        assert!(matches!(s.check(), Err(VideoError::BadSettings(_))), "{why}");
        assert!(VideoEncoder::new(s).is_err(), "{why}");
    }
    assert!(Settings::HD.check().is_ok());
    assert_eq!(level_for(1920, 1080, 24), 40);
    assert_eq!(level_for(3840, 2160, 30), 51);
    assert_eq!(level_for(640, 360, 24), 31);
    let mut enc =
        VideoEncoder::new(Settings { width: 64, height: 32, fps: 24, qp: 24 }).unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(enc.push_rgba(&[0; 10]), Err(VideoError::FrameSize { expected: 8192, got: 10 })));
    assert_eq!(enc.frames(), 0);
}

#[test]
fn frames_round_trip_through_the_reference_decoder_with_one_idr_per_second() {
    let settings = Settings { width: 320, height: 176, fps: 12, qp: 22 };
    let n = 30; // 2.5 s: two whole GOPs and a partial one, all in one batch (48 frames)
    let sources: Vec<Vec<u8>> =
        (0..n).map(|i| frame(settings.width, settings.height, i as f32 / settings.fps as f32)).collect();
    let mut enc = VideoEncoder::new(settings).unwrap_or_else(|e| panic!("{e}"));
    for f in &sources {
        enc.push_rgba(f).unwrap_or_else(|e| panic!("{e}"));
    }
    let track = enc.finish().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(track.samples().len(), n);
    assert_eq!(track.samples().iter().filter(|s| s.sync).count(), 3, "an IDR every second (12 frames): 0, 12, 24");
    assert!(track.samples()[0].sync && track.samples()[12].sync && track.samples()[24].sync);
    let (sps, pps) = track.parameter_sets();
    assert!(!sps.is_empty() && !pps.is_empty());
    assert!((track.seconds() - 2.5).abs() < 1e-9);

    let decoded =
        rusty_h264_decoder::Decoder::new().decode_stream(&annex_b(&track)).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(decoded.len(), n);
    for (i, (src, dec)) in sources.iter().zip(&decoded).enumerate() {
        let want = rgba_to_i420(src, settings.width, settings.height);
        assert_eq!((dec.width, dec.height), (320, 176));
        let p = psnr(&want.y, &dec.y);
        assert!(p > 36.0, "frame {i}: luma PSNR {p:.1} dB");
    }
}

#[test]
fn batches_of_gops_are_byte_identical_to_one_batch() {
    // 2 fps keeps it quick: BATCH_GOPS × 2 = 8 frames per batch; 20 frames = 2 batches + 4.
    let settings = Settings { width: 64, height: 48, fps: 2, qp: 30 };
    let sources: Vec<Vec<u8>> = (0..20).map(|i| frame(64, 48, i as f32 * 0.3)).collect();
    let mut enc = VideoEncoder::new(settings).unwrap_or_else(|e| panic!("{e}"));
    for f in &sources {
        enc.push_rgba(f).unwrap_or_else(|e| panic!("{e}"));
    }
    let batched = enc.finish().unwrap_or_else(|e| panic!("{e}"));
    // The same frames through the encoder's own one-shot path.
    let cfg = settings.config();
    let yuv: Vec<_> = sources.iter().map(|f| rgba_to_i420(f, 64, 48)).collect();
    let units =
        Encoder::new(cfg).unwrap_or_else(|e| panic!("{e:?}")).encode_all(&yuv).unwrap_or_else(|e| panic!("{e:?}"));
    let mut whole = VideoTrack::new(64, 48, 2);
    for u in &units {
        whole.push_annex_b(u);
    }
    assert_eq!(batched.samples().len(), 20);
    assert_eq!(batched.samples(), whole.samples(), "GOPs are independent at constant QP");
    assert_eq!(batched.samples().iter().filter(|s| s.sync).count(), 10);
}

#[test]
fn audio_encodes_to_aac_lc_with_the_right_config() {
    let sr = 44_100;
    let pcm: Vec<f32> = (0..sr as usize)
        .flat_map(|i| {
            let t = i as f32 / sr as f32;
            [(t * 440.0 * std::f32::consts::TAU).sin() * 0.5, (t * 660.0 * std::f32::consts::TAU).sin() * 0.5]
        })
        .collect();
    let track = encode_audio(&pcm, 2, sr, DEFAULT_AUDIO_BITRATE).unwrap_or_else(|e| panic!("{e}"));
    // 44 100 / 1024 = 43.07 frames, plus the encoder's priming and flush.
    assert!((43..=46).contains(&track.len()), "{} access units", track.len());
    assert!((track.seconds() - 1.0).abs() < 0.1);
    assert!(track.bytes() > 10_000 && track.bytes() < 30_000, "{} bytes for 1 s at 160 kb/s", track.bytes());
    assert_eq!(rusty_aac::audio_specific_config_bytes(sr, 2), vec![0x12, 0x10], "AAC-LC, 44.1 kHz, stereo");
    assert!(matches!(encode_audio(&pcm, 3, sr, 128_000), Err(VideoError::Audio(_))));
    assert!(matches!(encode_audio(&pcm, 2, 12_345, 128_000), Err(VideoError::Audio(_))));
    assert!(matches!(encode_audio(&pcm[..5], 2, sr, 128_000), Err(VideoError::Audio(_))));
}

#[test]
fn the_muxer_writes_a_progressive_file_whose_tables_point_at_the_samples() {
    let settings = Settings { width: 64, height: 48, fps: 4, qp: 28 };
    let mut enc = VideoEncoder::new(settings).unwrap_or_else(|e| panic!("{e}"));
    for i in 0..8 {
        enc.push_rgba(&frame(64, 48, i as f32 * 0.25)).unwrap_or_else(|e| panic!("{e}"));
    }
    let video = enc.finish().unwrap_or_else(|e| panic!("{e}"));
    let pcm: Vec<f32> = (0..2 * 44_100 * 2).map(|i| ((i / 2) as f32 * 0.05).sin() * 0.3).collect();
    let audio = encode_audio(&pcm, 2, 44_100, 96_000).unwrap_or_else(|e| panic!("{e}"));

    for container in [Container::Mp4, Container::Mov] {
        let mut bytes = Vec::new();
        let written = mux(&video, Some(&audio), container, &mut bytes).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(written, bytes.len() as u64);
        assert_eq!(mux_size(&video, Some(&audio), container).unwrap_or_else(|e| panic!("{e}")), written);
        let top: Vec<String> = boxes(&bytes).iter().map(|(k, _, _)| k.clone()).collect();
        assert_eq!(top, ["ftyp", "mdat", "moov"], "{container:?}");
        let brand = &bytes[8..12];
        assert_eq!(brand, if container == Container::Mov { b"qt  " } else { b"isom" });
        // mdat holds every sample, video first, and stco's first offset points at sample 0.
        let (_, mdat_at, mdat_size) = boxes(&bytes)[1].clone();
        assert_eq!(mdat_size - 8, video.bytes() + audio.bytes());
        let first = &video.samples()[0].data;
        assert_eq!(&bytes[mdat_at + 8..mdat_at + 8 + first.len()], &first[..]);
        let stco = find(&bytes, b"stco").unwrap_or_else(|| panic!("stco"));
        let first_offset =
            u32::from_be_bytes([bytes[stco + 12], bytes[stco + 13], bytes[stco + 14], bytes[stco + 15]]) as usize;
        assert_eq!(first_offset, mdat_at + 8);
        for kind in [b"avc1", b"avcC", b"mp4a", b"esds", b"stss", b"stsz", b"mvhd", b"vide", b"soun"] {
            assert!(find(&bytes, kind).is_some(), "{container:?} has {}", String::from_utf8_lossy(kind));
        }
        // stss lists the two IDRs (4 fps, 8 frames → frames 1 and 5).
        let stss = find(&bytes, b"stss").unwrap_or_else(|| panic!("stss"));
        assert_eq!(&bytes[stss + 8..stss + 20], &[0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 5]);
        // Deterministic.
        let mut again = Vec::new();
        mux(&video, Some(&audio), container, &mut again).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(bytes, again);
    }
    // Video only.
    let mut bytes = Vec::new();
    mux(&video, None, Container::Mp4, &mut bytes).unwrap_or_else(|e| panic!("{e}"));
    assert!(find(&bytes, b"mp4a").is_none() && find(&bytes, b"avc1").is_some());
}

#[test]
fn rgba_to_i420_is_bt601_limited_range() {
    let black = rgba_to_i420(&[0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255], 2, 2);
    assert_eq!((black.y[0], black.u[0], black.v[0]), (16, 128, 128));
    let white = rgba_to_i420(&[255; 16], 2, 2);
    assert_eq!((white.y[0], white.u[0], white.v[0]), (235, 128, 128));
    let red = rgba_to_i420(&[255, 0, 0, 255].repeat(4), 2, 2);
    assert_eq!((red.y[0], red.u[0], red.v[0]), (82, 90, 240));
    // A short buffer never reads past its end: the rest is black.
    let short = rgba_to_i420(&[255; 4], 2, 2);
    assert_eq!((short.y[0], short.y[3]), (235, 16));
}
