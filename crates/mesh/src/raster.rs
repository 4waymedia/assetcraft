//! A small CPU rasterizer: perspective orbit camera fitted to the bounding sphere, z-buffer
//! (interpolated 1/z), flat Lambert shading from a key light and a camera headlight, both
//! faces lit, ordered-grid supersampling, transparent background.
//!
//! Work is bounded: at most [`MAX_SAMPLES`] samples, and at most
//! [`RenderOptions::max_pixel_tests`] bounding-box pixel tests across all triangles (a file
//! made of millions of screen-filling triangles stops early with a partial image, instead of
//! spinning a worker for minutes).

use crate::{Mesh, MeshError, View};

/// Most samples (`width × height × supersample²`) one render may use (≈ 112 MiB of buffers).
pub const MAX_SAMPLES: usize = 16 * 1024 * 1024;

/// Largest image side [`render`] accepts.
const MAX_SIDE: u32 = 8192;

/// Vertical field of view at zoom 1 (radians).
const FOV: f32 = 0.52;

/// Output size and quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderOptions {
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Samples per pixel along each axis (1 = no anti-aliasing, 2 = 4 samples), 1..=4.
    /// Lowered automatically to stay within [`MAX_SAMPLES`].
    pub supersample: u32,
    /// Draw every `stride`-th triangle (1 = all). A cheap decimation for interactive previews
    /// of very large meshes.
    pub stride: usize,
    /// Stop after this many bounding-box pixel tests (summed over triangles).
    pub max_pixel_tests: u64,
}

impl RenderOptions {
    /// `width × height`, 2× supersampled, every triangle, 400 M pixel tests.
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, supersample: 2, stride: 1, max_pixel_tests: 400_000_000 }
    }
}

/// An unpremultiplied RGBA8 (sRGB) image, row-major.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` bytes.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

impl Image {
    /// Crop away transparent margins, keeping `pad` (a fraction of the content's longer side,
    /// at least 1 px) around the drawn pixels. An empty image is returned unchanged.
    pub fn crop_to_content(self, pad: f32) -> Image {
        let (w, h) = (self.width as usize, self.height as usize);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        for y in 0..h {
            for x in 0..w {
                if self.rgba.get((y * w + x) * 4 + 3).is_some_and(|a| *a > 0) {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x0 >= x1 || y0 >= y1 {
            return self;
        }
        let margin = ((x1 - x0).max(y1 - y0) as f32 * pad.clamp(0.0, 1.0)).ceil().max(1.0) as usize;
        let (x0, y0) = (x0.saturating_sub(margin), y0.saturating_sub(margin));
        let (x1, y1) = ((x1 + margin).min(w), (y1 + margin).min(h));
        let mut rgba = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
        for y in y0..y1 {
            if let Some(row) = self.rgba.get((y * w + x0) * 4..(y * w + x1) * 4) {
                rgba.extend_from_slice(row);
            }
        }
        Image { width: (x1 - x0) as u32, height: (y1 - y0) as u32, rgba }
    }
}

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn normalize(a: V3) -> Option<V3> {
    let len = dot(a, a).sqrt();
    (len > 1e-20 && len.is_finite()).then(|| scale(a, 1.0 / len))
}

/// Linear 0..1 to an sRGB byte.
fn srgb(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

/// Draw `mesh` from `view` into a new transparent image.
pub fn render(mesh: &Mesh, view: View, opts: &RenderOptions) -> Result<Image, MeshError> {
    if opts.width == 0 || opts.height == 0 || opts.width > MAX_SIDE || opts.height > MAX_SIDE {
        return Err(MeshError::TooLarge);
    }
    let (w, h) = (opts.width as usize, opts.height as usize);
    let mut ss = opts.supersample.clamp(1, 4) as usize;
    while ss > 1 && w * h * ss * ss > MAX_SAMPLES {
        ss -= 1;
    }
    if w * h * ss * ss > MAX_SAMPLES {
        return Err(MeshError::TooLarge);
    }
    let (sw, sh) = (w * ss, h * ss);
    let mut depth = vec![0.0f32; sw * sh];
    let mut color = vec![[0u8; 3]; sw * sh];

    if let Some((lo, hi)) = mesh.bounds() {
        let center = scale(add(lo, hi), 0.5);
        let radius = (dot(sub(hi, lo), sub(hi, lo)).sqrt() * 0.5).max(1e-6);
        let half = FOV * 0.5;
        let distance = radius / half.sin() * 1.04;
        let zoom = view.zoom.clamp(View::ZOOM_RANGE.0, View::ZOOM_RANGE.1);
        let focal = (sw.min(sh) as f32 * 0.5) / half.tan() * zoom;
        let pitch = view.pitch.clamp(-View::MAX_PITCH, View::MAX_PITCH);
        let towards_eye = [view.yaw.sin() * pitch.cos(), pitch.sin(), view.yaw.cos() * pitch.cos()];
        let eye = add(center, scale(towards_eye, distance));
        let forward = scale(towards_eye, -1.0);
        let right = normalize(cross(forward, [0.0, 1.0, 0.0])).unwrap_or([1.0, 0.0, 0.0]);
        let up = cross(right, forward);
        let key = normalize(add(add(scale(right, -0.45), scale(up, 0.7)), scale(towards_eye, 0.55))).unwrap_or(up);
        let near = distance * 1e-3;

        // Every vertex to screen space once: (x, y, 1/z), or None behind the near plane.
        let projected: Vec<Option<[f32; 3]>> = mesh
            .positions
            .iter()
            .map(|&p| {
                let rel = sub(p, eye);
                let z = dot(rel, forward);
                if z.is_nan() || z <= near || !z.is_finite() {
                    return None;
                }
                let sx = sw as f32 * 0.5 + dot(rel, right) / z * focal;
                let sy = sh as f32 * 0.5 - dot(rel, up) / z * focal;
                (sx.is_finite() && sy.is_finite()).then_some([sx, sy, 1.0 / z])
            })
            .collect();

        let shades = &mesh.colors;
        let mut budget = opts.max_pixel_tests;
        for (t, tri) in mesh.triangles.iter().enumerate().step_by(opts.stride.max(1)) {
            let (Some(Some(a)), Some(Some(b)), Some(Some(c))) =
                (projected.get(tri[0] as usize), projected.get(tri[1] as usize), projected.get(tri[2] as usize))
            else {
                continue;
            };
            let (Some(pa), Some(pb), Some(pc)) = (
                mesh.positions.get(tri[0] as usize),
                mesh.positions.get(tri[1] as usize),
                mesh.positions.get(tri[2] as usize),
            ) else {
                continue;
            };
            let Some(mut n) = normalize(cross(sub(*pb, *pa), sub(*pc, *pa))) else { continue };
            if dot(n, sub(eye, *pa)) < 0.0 {
                n = scale(n, -1.0);
            }
            let light = 0.16 + 0.62 * dot(n, key).max(0.0) + 0.27 * dot(n, towards_eye).max(0.0);
            let base = mesh.triangle_color.get(t).and_then(|i| shades.get(*i as usize)).copied().unwrap_or([
                crate::NEUTRAL_GREY[0],
                crate::NEUTRAL_GREY[1],
                crate::NEUTRAL_GREY[2],
                1.0,
            ]);
            let rgb = [srgb(base[0] * light), srgb(base[1] * light), srgb(base[2] * light)];

            let area = edge(a, b, c[0], c[1]);
            if area.is_nan() || area.abs() <= 1e-9 {
                continue;
            }
            let min_x = a[0].min(b[0]).min(c[0]).floor().max(0.0);
            let min_y = a[1].min(b[1]).min(c[1]).floor().max(0.0);
            let max_x = a[0].max(b[0]).max(c[0]).ceil().min(sw as f32 - 1.0);
            let max_y = a[1].max(b[1]).max(c[1]).ceil().min(sh as f32 - 1.0);
            if min_x > max_x || min_y > max_y {
                continue;
            }
            let (x0, x1, y0, y1) = (min_x as usize, max_x as usize, min_y as usize, max_y as usize);
            let tests = ((x1 - x0 + 1) * (y1 - y0 + 1)) as u64;
            if tests > budget {
                break;
            }
            budget -= tests;
            let inv = 1.0 / area;
            for y in y0..=y1 {
                let py = y as f32 + 0.5;
                let row = y * sw;
                for x in x0..=x1 {
                    let px = x as f32 + 0.5;
                    let w0 = edge(b, c, px, py) * inv;
                    let w1 = edge(c, a, px, py) * inv;
                    let w2 = edge(a, b, px, py) * inv;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                    let i = row + x;
                    if let (Some(d), Some(col)) = (depth.get_mut(i), color.get_mut(i))
                        && z > *d
                    {
                        *d = z;
                        *col = rgb;
                    }
                }
            }
        }
    }

    // Resolve the supersamples: coverage becomes alpha, covered samples are averaged.
    let mut rgba = vec![0u8; w * h * 4];
    let full = (ss * ss) as u32;
    for (i, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (ox, oy) = (i % w * ss, i / w * ss);
        let (mut sum, mut covered) = ([0u32; 3], 0u32);
        for sy in 0..ss {
            for sx in 0..ss {
                let s = (oy + sy) * sw + ox + sx;
                if depth.get(s).is_some_and(|d| *d > 0.0)
                    && let Some(c) = color.get(s)
                {
                    covered += 1;
                    for k in 0..3 {
                        sum[k] += u32::from(c[k]);
                    }
                }
            }
        }
        if let (Some(r), Some(g), Some(b)) =
            (sum[0].checked_div(covered), sum[1].checked_div(covered), sum[2].checked_div(covered))
        {
            *px = [r as u8, g as u8, b as u8, ((covered * 255 + full / 2) / full) as u8];
        }
    }
    Ok(Image { width: opts.width, height: opts.height, rgba })
}

/// Twice the signed area of (a, b, p) in screen space.
fn edge(a: &[f32; 3], b: &[f32; 3], px: f32, py: f32) -> f32 {
    (px - a[0]) * (b[1] - a[1]) - (py - a[1]) * (b[0] - a[0])
}
