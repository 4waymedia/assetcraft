//! Pixel work: an RGBA picture and the few operations a frame needs (fill, tint, cover,
//! contain through a window with bilinear sampling, alpha-blended rectangles, compositing).

/// An 8-bit RGBA picture, rows top to bottom.
#[derive(Clone, PartialEq, Eq)]
pub struct Rgba {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` bytes.
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Rgba {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rgba({}×{})", self.width, self.height)
    }
}

impl Rgba {
    /// A picture of one colour.
    pub fn filled(width: u32, height: u32, rgb: [u8; 3]) -> Rgba {
        let mut data = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width as usize * height as usize {
            data.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        Rgba { width, height, data }
    }

    /// The pixel at `(x, y)`, or black when out of range.
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 255];
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.data.get(i..i + 4).map_or([0, 0, 0, 255], |p| [p[0], p[1], p[2], p[3]])
    }

    fn blend(&mut self, x: u32, y: u32, rgb: [u8; 3], alpha: f32) {
        if x >= self.width || y >= self.height || alpha <= 0.0 {
            return;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let a = alpha.min(1.0);
        if let Some(p) = self.data.get_mut(i..i + 4) {
            for c in 0..3 {
                p[c] = (f32::from(p[c]) * (1.0 - a) + f32::from(rgb[c]) * a).round() as u8;
            }
            p[3] = 255;
        }
    }

    /// Bilinear sample at a fractional pixel position (clamped to the edges).
    fn sample(&self, fx: f32, fy: f32) -> [f32; 4] {
        let (w, h) = (self.width.max(1) as f32, self.height.max(1) as f32);
        let x = (fx - 0.5).clamp(0.0, w - 1.0);
        let y = (fy - 0.5).clamp(0.0, h - 1.0);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let p = |xx: u32, yy: u32| self.get(xx, yy).map(f32::from);
        let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
        let mut out = [0.0; 4];
        for i in 0..4 {
            let top = a[i] + (b[i] - a[i]) * tx;
            let bottom = c[i] + (d[i] - c[i]) * tx;
            out[i] = top + (bottom - top) * ty;
        }
        out
    }
}

/// Fill the whole picture.
pub fn fill(out: &mut Rgba, rgb: [u8; 3]) {
    for p in out.data.as_chunks_mut::<4>().0 {
        *p = [rgb[0], rgb[1], rgb[2], 255];
    }
}

/// Blend a colour over the whole picture at `alpha` (a scrim).
pub fn tint(out: &mut Rgba, rgb: [u8; 3], alpha: f32) {
    for y in 0..out.height {
        for x in 0..out.width {
            out.blend(x, y, rgb, alpha);
        }
    }
}

/// Blend a rectangle (fractional edges rounded) at `alpha`.
pub fn rect(out: &mut Rgba, x: f32, y: f32, w: f32, h: f32, rgb: [u8; 3], alpha: f32) {
    let x0 = x.round().max(0.0) as u32;
    let y0 = y.round().max(0.0) as u32;
    let x1 = ((x + w).round().max(0.0) as u32).min(out.width);
    let y1 = ((y + h).round().max(0.0) as u32).min(out.height);
    for yy in y0..y1 {
        for xx in x0..x1 {
            out.blend(xx, yy, rgb, alpha);
        }
    }
}

/// Composite `src` over `out` at `(x, y)` using its alpha.
pub fn over(out: &mut Rgba, src: &Rgba, x: i64, y: i64) {
    for sy in 0..src.height {
        let dy = y + i64::from(sy);
        if dy < 0 || dy >= i64::from(out.height) {
            continue;
        }
        for sx in 0..src.width {
            let dx = x + i64::from(sx);
            if dx < 0 || dx >= i64::from(out.width) {
                continue;
            }
            let p = src.get(sx, sy);
            out.blend(dx as u32, dy as u32, [p[0], p[1], p[2]], f32::from(p[3]) / 255.0);
        }
    }
}

/// The rectangle of `size`'s aspect that fits inside `(w, h)`, centred: `[x, y, w, h]`.
pub fn contain_rect(w: u32, h: u32, size: (u32, u32)) -> [f32; 4] {
    if size.0 == 0 || size.1 == 0 {
        return [0.0, 0.0, w as f32, h as f32];
    }
    let s = (w as f32 / size.0 as f32).min(h as f32 / size.1 as f32);
    let (tw, th) = (size.0 as f32 * s, size.1 as f32 * s);
    [(w as f32 - tw) / 2.0, (h as f32 - th) / 2.0, tw, th]
}

/// Draw the part of `image` inside `window` (`[u0, v0, u1, v1]`) into the letterboxed target
/// of the image's aspect (the Viewer's `contain`), bilinear.
pub fn contain_window(out: &mut Rgba, image: &Rgba, window: [f32; 4]) {
    let target = contain_rect(out.width, out.height, (image.width, image.height));
    draw_window(out, image, window, target);
}

/// Draw `image` through `window` so that it covers the whole picture (the Viewer's `cover`).
pub fn cover(out: &mut Rgba, image: &Rgba) {
    if image.width == 0 || image.height == 0 {
        return;
    }
    let (w, h) = (out.width as f32, out.height as f32);
    let (iw, ih) = (image.width as f32, image.height as f32);
    let s = (w / iw).max(h / ih);
    let (cw, ch) = (iw * s, ih * s);
    // The visible window of the image, centred.
    let u = (w / cw).min(1.0);
    let v = (h / ch).min(1.0);
    draw_window(out, image, [0.5 - u / 2.0, 0.5 - v / 2.0, 0.5 + u / 2.0, 0.5 + v / 2.0], [0.0, 0.0, w, h]);
}

fn draw_window(out: &mut Rgba, image: &Rgba, window: [f32; 4], target: [f32; 4]) {
    let [tx, ty, tw, th] = target;
    if tw <= 0.0 || th <= 0.0 || image.width == 0 || image.height == 0 {
        return;
    }
    let (u0, v0, u1, v1) = (window[0], window[1], window[2], window[3]);
    let x0 = tx.round().max(0.0) as u32;
    let y0 = ty.round().max(0.0) as u32;
    let x1 = ((tx + tw).round().max(0.0) as u32).min(out.width);
    let y1 = ((ty + th).round().max(0.0) as u32).min(out.height);
    let (iw, ih) = (image.width as f32, image.height as f32);
    for y in y0..y1 {
        let v = v0 + (v1 - v0) * ((y as f32 + 0.5 - ty) / th);
        for x in x0..x1 {
            let u = u0 + (u1 - u0) * ((x as f32 + 0.5 - tx) / tw);
            let p = image.sample(u * iw, v * ih);
            out.blend(x, y, [p[0].round() as u8, p[1].round() as u8, p[2].round() as u8], p[3] / 255.0);
        }
    }
}
