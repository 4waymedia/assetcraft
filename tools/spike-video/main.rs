mod mux;
use rusty_h264_common::YuvFrame;
use rusty_h264_encoder::{Encoder, EncoderConfig, Preset};
use std::time::Instant;

const W: usize = 1920;
const H: usize = 1080;
const FPS: u32 = 24;
const SECS: usize = 10;

/// A procedural "still": soft gradient with fine texture, like a concept painting.
fn plate() -> Vec<u8> {
    let (pw, ph) = (W + 256, H + 256);
    let mut p = vec![0u8; pw * ph];
    let mut seed = 12345u32;
    for y in 0..ph {
        for x in 0..pw {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 24) & 0x1f) as f32;
            let g = 40.0 + 150.0 * (x as f32 / pw as f32) + 40.0 * ((y as f32 / 90.0).sin());
            let ring = (((x as f32 - 900.0).powi(2) + (y as f32 - 600.0).powi(2)).sqrt() / 60.0).sin() * 20.0;
            p[y * pw + x] = (g + ring + noise).clamp(0.0, 255.0) as u8;
        }
    }
    p
}

fn frame_at(plate: &[u8], t: f32) -> YuvFrame {
    // Slow pan (push in / pan right) across the plate, plus a moving box (a "model").
    let pw = W + 256;
    let ox = (t * 20.0) as usize;
    let oy = (t * 6.0) as usize;
    let mut y = vec![0u8; W * H];
    for r in 0..H {
        y[r * W..(r + 1) * W].copy_from_slice(&plate[(r + oy) * pw + ox..(r + oy) * pw + ox + W]);
    }
    let bx = (400.0 + t * 90.0) as usize;
    for r in 400..700 {
        for c in bx..bx + 300 {
            y[r * W + c] = (((r + c) as f32 * 0.3 + t * 40.0).sin() * 60.0 + 150.0) as u8;
        }
    }
    let mut u = vec![128u8; W * H / 4];
    let v = vec![120u8; W * H / 4];
    for i in 0..u.len() { u[i] = 110 + ((i % (W / 2)) * 40 / (W / 2)) as u8; }
    YuvFrame { width: W, height: H, y, u, v }
}

fn main() {
    let plate = plate();
    let n = FPS as usize * SECS;
    println!("rendering {n} frames…");
    let frames: Vec<YuvFrame> = (0..n).map(|i| frame_at(&plate, i as f32 / FPS as f32)).collect();
    if std::env::var("Y4M").is_ok() {
        let mut y4m = Vec::new();
        y4m.extend_from_slice(format!("YUV4MPEG2 W{W} H{H} F{FPS}:1 Ip A1:1 C420jpeg\n").as_bytes());
        for f in &frames { y4m.extend_from_slice(b"FRAME\n"); y4m.extend_from_slice(&f.y); y4m.extend_from_slice(&f.u); y4m.extend_from_slice(&f.v); }
        std::fs::write("source.y4m", y4m).unwrap();
        println!("wrote source.y4m");
        return;
    }

    let qp: u8 = std::env::var("QP").ok().and_then(|q| q.parse().ok()).unwrap_or(0);
    let runs: Vec<(String, Preset, u32)> = if qp > 0 { vec![(format!("fast-qp{qp}"), Preset::Fast, 0)] } else { vec![("balanced-4M".into(), Preset::Balanced, 4_000_000u32), ("fast-4M".into(), Preset::Fast, 4_000_000)] };
    for (label, preset, bitrate) in runs {
        let mut cfg = EncoderConfig::new(W, H);
        cfg.preset = preset;
        cfg.gop_size = FPS * 2;
        cfg.bitrate = bitrate;
        if qp > 0 { cfg.qp = qp; }
        cfg.framerate = FPS as f32;
        cfg.level_idc = 40;
        let mut enc = Encoder::new(cfg).expect("encoder");
        let t0 = Instant::now();
        let mut stream = Vec::new();
        for f in &frames { stream.extend_from_slice(&enc.encode(f)); }
        stream.extend_from_slice(&enc.flush());
        let dt = t0.elapsed().as_secs_f64();
        println!("{label}: {} bytes video, {:.1} s = {:.1} fps, {:.2} Mb/s", stream.len(), dt, n as f64 / dt, stream.len() as f64 * 8.0 / SECS as f64 / 1e6);

        // Annex-B → AVCC samples.
        let nals = mux::annexb_nals(&stream);
        let mut sps = Vec::new(); let mut pps = Vec::new();
        let mut samples: Vec<(Vec<u8>, bool)> = Vec::new();
        let mut cur: Vec<u8> = Vec::new(); let mut cur_sync = false; let mut have = false;
        for nal in nals {
            match nal[0] & 0x1f {
                7 => sps = nal.to_vec(),
                8 => pps = nal.to_vec(),
                9 | 6 => {}
                t => {
                    // A new picture starts at each VCL NAL with first_mb_in_slice == 0 (bit 7 of byte 1 set).
                    let first_mb_zero = nal.get(1).is_some_and(|b| b & 0x80 != 0);
                    if have && first_mb_zero { samples.push((std::mem::take(&mut cur), cur_sync)); cur_sync = false; }
                    have = true;
                    if t == 5 { cur_sync = true; }
                    cur.extend_from_slice(&(nal.len() as u32).to_be_bytes()); cur.extend_from_slice(nal);
                }
            }
        }
        if have { samples.push((cur, cur_sync)); }
        println!("  {} samples, {} sync, sps {} B, pps {} B", samples.len(), samples.iter().filter(|s| s.1).count(), sps.len(), pps.len());

        // Audio: 10 s stereo, two tones with a beat, 44.1 kHz → AAC-LC 128 kb/s.
        let sr = 44_100u32;
        let mut pcm = Vec::with_capacity(sr as usize * SECS * 2);
        for i in 0..sr as usize * SECS {
            let t = i as f32 / sr as f32;
            let beat = if (t * 2.0).fract() < 0.1 { 0.6 } else { 0.2 };
            let l = (t * 220.0 * std::f32::consts::TAU).sin() * beat;
            let r = (t * 329.6 * std::f32::consts::TAU).sin() * beat;
            pcm.push(l); pcm.push(r);
        }
        let t0 = Instant::now();
        let mut aac = rusty_aac::AacEncoder::new(rusty_aac::AacEncoderConfig { bitrate_bps: 128_000, ..Default::default() });
        aac.push_pcm(&pcm, 2, sr).expect("pcm");
        aac.finish();
        let mut packets = Vec::new();
        while let Ok(p) = aac.next_packet() { packets.push(p.data); }
        let abytes: usize = packets.iter().map(|p| p.len()).sum();
        println!("  aac: {} packets, {} bytes in {:.2} s", packets.len(), abytes, t0.elapsed().as_secs_f64());
        let audio = mux::AudioTrack { sample_rate: sr, channels: 2, asc: rusty_aac::audio_specific_config_bytes(sr, 2), samples: packets, samples_per_frame: 1024 };
        let video = mux::VideoTrack { width: W as u16, height: H as u16, timescale: FPS * 1000, frame_duration: 1000, sps, pps, samples };
        for (mov, ext) in [(false, "mp4"), (true, "mov")] {
            let bytes = mux::mux(&video, Some(&audio), mov);
            if std::env::var("QP").is_ok() && mov { continue; }
            let name = format!("spike-{label}.{ext}");
            std::fs::write(&name, &bytes).unwrap();
            println!("  wrote {name}: {} bytes", bytes.len());
        }
    }
}
