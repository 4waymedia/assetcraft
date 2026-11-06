//! RGBA → I420 (4:2:0 planar, BT.601 limited range), the form the encoder takes.

use rusty_h264_common::YuvFrame;

/// Convert one RGBA frame. `rgba` must hold `width × height × 4` bytes (checked by the caller);
/// a short buffer is treated as black beyond its end rather than read past. Chroma is the mean
/// of each 2 × 2 block; width and height are even (the caller's `Settings::check`).
pub fn rgba_to_i420(rgba: &[u8], width: u32, height: u32) -> YuvFrame {
    let (w, h) = (width as usize, height as usize);
    let mut y = vec![16u8; w * h];
    let (cw, ch) = (w / 2, h / 2);
    let mut u = vec![128u8; cw * ch];
    let mut v = vec![128u8; cw * ch];
    let px = |x: usize, yy: usize| -> (f32, f32, f32) {
        let i = (yy * w + x) * 4;
        match rgba.get(i..i + 3) {
            Some(p) => (f32::from(p[0]), f32::from(p[1]), f32::from(p[2])),
            None => (0.0, 0.0, 0.0),
        }
    };
    for yy in 0..h {
        for x in 0..w {
            let (r, g, b) = px(x, yy);
            let luma = 16.0 + 0.257 * r + 0.504 * g + 0.098 * b;
            y[yy * w + x] = luma.round().clamp(16.0, 235.0) as u8;
        }
    }
    for cy in 0..ch {
        for cx in 0..cw {
            let mut cb = 0.0f32;
            let mut cr = 0.0f32;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (r, g, b) = px(cx * 2 + dx, cy * 2 + dy);
                cb += 128.0 - 0.148 * r - 0.291 * g + 0.439 * b;
                cr += 128.0 + 0.439 * r - 0.368 * g - 0.071 * b;
            }
            u[cy * cw + cx] = (cb / 4.0).round().clamp(16.0, 240.0) as u8;
            v[cy * cw + cx] = (cr / 4.0).round().clamp(16.0, 240.0) as u8;
        }
    }
    YuvFrame { width: w, height: h, y, u, v }
}
