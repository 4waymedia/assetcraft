//! A small CPU rasterizer: perspective orbit camera fitted to the bounding sphere, z-buffer
//! (interpolated 1/z), per-pixel shading (smooth from vertex normals when the mesh has them,
//! else flat) under a three-point light rig, bilinear base colour textures, both faces lit,
//! ordered-grid supersampling, a transparent or solid background, and the display modes a
//! viewer needs ([`DisplayMode`]: shaded, wireframe, normals, UV checker).
//!
//! Work is bounded: at most [`MAX_SAMPLES`] samples, and at most
//! [`RenderOptions::max_pixel_tests`] bounding-box pixel tests across all triangles (a file
//! made of millions of screen-filling triangles stops early with a partial image, instead of
//! spinning a worker for minutes). Textures are the caller's decoded pixels ([`Texture`]), one
//! per [`Mesh::images`] entry; a missing or `None` texture draws the material's colour alone.

use crate::{Mesh, MeshError, View};

/// Most samples (`width × height × supersample²`) one render may use (≈ 112 MiB of buffers).
pub const MAX_SAMPLES: usize = 16 * 1024 * 1024;

/// Largest image side [`render`] accepts.
const MAX_SIDE: u32 = 8192;

/// Vertical field of view at zoom 1 (radians).
const FOV: f32 = 0.52;

/// The light rig: weights of three directional lights fixed to the camera (key from the upper
/// right in front, fill from the left, rim from behind), an ambient term, and an exposure
/// multiplier on the result. Lambert only: no speculars, no shadows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lighting {
    /// Key light weight.
    pub key: f32,
    /// Fill light weight.
    pub fill: f32,
    /// Rim light weight.
    pub rim: f32,
    /// Light every surface gets regardless of orientation.
    pub ambient: f32,
    /// Multiplier on the lit colour before clamping.
    pub exposure: f32,
}

impl Lighting {
    /// The studio rig the viewer and thumbnails use (the look of `tools/glb_render.py`).
    pub const STUDIO: Lighting = Lighting { key: 1.0, fill: 0.55, rim: 0.4, ambient: 0.42, exposure: 1.0 };
}

impl Default for Lighting {
    fn default() -> Self {
        Self::STUDIO
    }
}

/// What the pixels show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    /// Lit material colour and texture.
    #[default]
    Shaded,
    /// Triangle edges over a dimmed shaded surface (hidden edges removed by the z-buffer).
    Wireframe,
    /// World-space normals as colours (`xyz` → `rgb`, −1..1 → 0..255), unlit.
    Normals,
    /// An 8 × 8 checker over the texture coordinates, tinted by `u` (red) and `v` (green);
    /// a mesh without texture coordinates shows magenta.
    UvChecker,
}

/// What is behind the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Background {
    /// Alpha 0 where nothing is drawn (thumbnails are cropped to the drawn pixels).
    #[default]
    Transparent,
    /// A solid sRGB colour, alpha 255 everywhere.
    Solid([u8; 3]),
}

/// A decoded texture: sRGB RGBA8, row-major. Texture coordinates outside 0..1 wrap.
#[derive(Clone, PartialEq, Eq)]
pub struct Texture {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` bytes.
    pub rgba: Vec<u8>,
}

impl Texture {
    /// A texture from its pixels; `None` when the sizes don't match or a side is 0.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        let expected = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
        (width > 0 && height > 0 && rgba.len() == expected).then_some(Self { width, height, rgba })
    }

    /// Bilinear sample at `(u, v)` (wrapping), as linear RGB.
    fn sample(&self, u: f32, v: f32) -> V3 {
        let (w, h) = (self.width as f32, self.height as f32);
        let x = (u - u.floor()) * w - 0.5;
        let y = (v - v.floor()) * h - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let wrap = |i: i64, n: u32| i.rem_euclid(i64::from(n)) as usize;
        let texel = |xi: i64, yi: i64| -> V3 {
            let at = (wrap(yi, self.height) * self.width as usize + wrap(xi, self.width)) * 4;
            match self.rgba.get(at..at + 3) {
                Some(p) => [to_linear(p[0]), to_linear(p[1]), to_linear(p[2])],
                None => [0.0; 3],
            }
        };
        let (a, b, c, d) = (texel(x0, y0), texel(x0 + 1, y0), texel(x0, y0 + 1), texel(x0 + 1, y0 + 1));
        let top = add(scale(a, 1.0 - fx), scale(b, fx));
        let bottom = add(scale(c, 1.0 - fx), scale(d, fx));
        add(scale(top, 1.0 - fy), scale(bottom, fy))
    }
}

impl std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Texture({}x{})", self.width, self.height)
    }
}

/// Output size, quality and look.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// The light rig.
    pub lighting: Lighting,
    /// What the pixels show.
    pub mode: DisplayMode,
    /// What is behind the model.
    pub background: Background,
    /// Shade from the mesh's vertex normals when it has them (`false` = flat faces).
    pub smooth: bool,
    /// Threads to rasterize on (row bands; 0 or 1 = this thread). The output is the same
    /// whatever the count. Ignored on `wasm32`, which has no threads.
    pub threads: usize,
}

impl RenderOptions {
    /// `width × height`, 2× supersampled, every triangle, 400 M pixel tests, studio lighting,
    /// shaded, smooth, on transparent, one thread.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            supersample: 2,
            stride: 1,
            max_pixel_tests: 400_000_000,
            lighting: Lighting::STUDIO,
            mode: DisplayMode::Shaded,
            background: Background::Transparent,
            smooth: true,
            threads: 1,
        }
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

/// An sRGB byte to linear 0..1 (table lookup).
fn to_linear(b: u8) -> f32 {
    LINEAR[b as usize]
}

/// sRGB byte → linear, computed once.
static LINEAR: [f32; 256] = {
    let mut t = [0.0f32; 256];
    let mut i = 0;
    while i < 256 {
        // `powf` isn't const: a 12-term series around the exact curve is well within 8 bits.
        t[i] = srgb_to_linear_const(i as f32 / 255.0);
        i += 1;
    }
    t
};

/// `((s + 0.055) / 1.055)^2.4` without `powf` (which is not `const`): `x^2.4 = x² · x^0.4`, and
/// `x^0.4 = exp(0.4 ln x)` from short series, accurate to better than 1/4000 over 0.04..1.
const fn srgb_to_linear_const(s: f32) -> f32 {
    if s <= 0.040_45 {
        return s / 12.92;
    }
    let x = (s + 0.055) / 1.055;
    // ln x for x in (0.04, 1]: ln x = 2·atanh((x−1)/(x+1)), 12 terms.
    let y = (x - 1.0) / (x + 1.0);
    let y2 = y * y;
    let mut ln = 0.0f32;
    let mut term = y;
    let mut k = 1;
    while k < 25 {
        ln += term / k as f32;
        term *= y2;
        k += 2;
    }
    ln *= 2.0;
    // exp(0.4 ln x), 14 terms.
    let a = 0.4 * ln;
    let mut e = 1.0f32;
    let mut t = 1.0f32;
    let mut n = 1;
    while n < 15 {
        t *= a / n as f32;
        e += t;
        n += 1;
    }
    x * x * e
}

/// Per-vertex data of a projected triangle corner.
#[derive(Clone, Copy)]
struct Corner {
    /// Screen x, y and 1/z.
    s: [f32; 3],
    /// World position.
    p: V3,
    /// Vertex normal, or `None` (flat).
    n: Option<V3>,
    /// Texture coordinates.
    uv: [f32; 2],
}

/// Draw `mesh` from `view` into a new image, untextured.
pub fn render(mesh: &Mesh, view: View, opts: &RenderOptions) -> Result<Image, MeshError> {
    render_with(mesh, &[], view, opts)
}

/// Draw `mesh` from `view` into a new image. `textures[i]` is the decoded `mesh.images[i]`
/// (or `None`); fewer entries than images, or `None`, draw those materials untextured.
pub fn render_with(
    mesh: &Mesh,
    textures: &[Option<Texture>],
    view: View,
    opts: &RenderOptions,
) -> Result<Image, MeshError> {
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
    let bg = match opts.background {
        Background::Transparent => [0u8; 3],
        Background::Solid(c) => c,
    };
    let mut depth = vec![0.0f32; sw * sh];
    let mut color = vec![bg; sw * sh];

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
        let rig = |r: f32, u: f32, e: f32| {
            normalize(add(add(scale(right, r), scale(up, u)), scale(towards_eye, e))).unwrap_or(up)
        };
        let lights = [
            (rig(0.5, 0.8, 0.6), opts.lighting.key),
            (rig(-0.7, 0.3, 0.4), opts.lighting.fill),
            (rig(0.2, -0.3, -0.9), opts.lighting.rim),
        ];
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

        let scene = Scene {
            mesh,
            textures,
            opts,
            projected: &projected,
            eye,
            lights,
            sw,
            sh,
            smooth: opts.smooth && mesh.normals.len() == mesh.positions.len(),
            has_uv: mesh.uvs.len() == mesh.positions.len(),
            edge_px: 0.5 * ss as f32,
        };

        // Row bands, one per thread; each band rasterizes every triangle clipped to its rows,
        // so no two threads touch the same sample and the result doesn't depend on the count.
        let threads = if cfg!(target_arch = "wasm32") { 1 } else { opts.threads.clamp(1, 64).min(sh.max(1)) };
        let rows = sh.div_ceil(threads).max(1);
        let budget = opts.max_pixel_tests / threads as u64;
        if threads == 1 {
            scene.draw_band(0, sh, &mut depth, &mut color, budget);
        } else {
            // Each band sits in a `Mutex<Option<_>>` so that, when a thread can't be spawned
            // (a resource limit), the same band is drawn on this thread instead.
            type Band<'b> = Option<(usize, usize, &'b mut [f32], &'b mut [[u8; 3]])>;
            let bands: Vec<std::sync::Mutex<Band<'_>>> = depth
                .chunks_mut(rows * sw)
                .zip(color.chunks_mut(rows * sw))
                .enumerate()
                .map(|(k, (depth, color))| {
                    std::sync::Mutex::new(Some((k * rows, (k * rows + rows).min(sh), depth, color)))
                })
                .collect();
            std::thread::scope(|scope| {
                for band in &bands {
                    let scene = &scene;
                    let draw = move || {
                        if let Some((y0, y1, depth, color)) = band.lock().ok().and_then(|mut b| b.take()) {
                            scene.draw_band(y0, y1, depth, color, budget);
                        }
                    };
                    if std::thread::Builder::new().spawn_scoped(scope, draw).is_err() {
                        draw();
                    }
                }
            });
        }
    }

    // Resolve the supersamples: coverage becomes alpha (transparent background) or every
    // sample counts (solid background); covered samples are averaged.
    let solid = matches!(opts.background, Background::Solid(_));
    let mut rgba = vec![0u8; w * h * 4];
    let full = (ss * ss) as u32;
    for (i, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (ox, oy) = (i % w * ss, i / w * ss);
        let (mut sum, mut covered) = ([0u32; 3], 0u32);
        for sy in 0..ss {
            for sx in 0..ss {
                let s = (oy + sy) * sw + ox + sx;
                if (solid || depth.get(s).is_some_and(|d| *d > 0.0))
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

/// Everything a band needs to rasterize, shared read-only between threads.
struct Scene<'a> {
    mesh: &'a Mesh,
    textures: &'a [Option<Texture>],
    opts: &'a RenderOptions,
    projected: &'a [Option<[f32; 3]>],
    eye: V3,
    lights: [(V3, f32); 3],
    sw: usize,
    sh: usize,
    smooth: bool,
    has_uv: bool,
    edge_px: f32,
}

impl Scene<'_> {
    /// Rasterize every triangle into sample rows `y_lo..y_hi`; `depth` and `color` hold exactly
    /// those rows. Stops when `budget` pixel tests are spent.
    fn draw_band(&self, y_lo: usize, y_hi: usize, depth: &mut [f32], color: &mut [[u8; 3]], budget: u64) {
        let Scene { mesh, textures, opts, projected, eye, lights, sw, sh, smooth, has_uv, edge_px } = *self;
        let (sw, sh, smooth, has_uv, edge_px, eye, lights) = (sw, sh, smooth, has_uv, edge_px, eye, lights);
        let mut budget = budget;
        for (t, tri) in mesh.triangles.iter().enumerate().step_by(opts.stride.max(1)) {
            let corner = |i: u32| -> Option<Corner> {
                let i = i as usize;
                let n = smooth.then(|| mesh.normals.get(i).copied()).flatten().filter(|n| *n != [0.0; 3]);
                Some(Corner {
                    s: (*projected.get(i)?)?,
                    p: *mesh.positions.get(i)?,
                    n,
                    uv: if has_uv { mesh.uvs.get(i).copied().unwrap_or([0.0; 2]) } else { [0.0; 2] },
                })
            };
            // The band's row test first, so a triangle outside it costs three lookups and
            // nothing else (every band walks every triangle).
            let (Some(sa), Some(sb), Some(sc)) = (
                projected.get(tri[0] as usize).copied().flatten(),
                projected.get(tri[1] as usize).copied().flatten(),
                projected.get(tri[2] as usize).copied().flatten(),
            ) else {
                continue;
            };
            let top = sa[1].min(sb[1]).min(sc[1]);
            let bottom = sa[1].max(sb[1]).max(sc[1]);
            // Rows floor(top)..=ceil(bottom) are what the box below rasterizes.
            if bottom.ceil() < y_lo as f32 || top.floor() >= y_hi as f32 || !(top.is_finite() && bottom.is_finite()) {
                continue;
            }
            let (Some(a), Some(b), Some(c)) = (corner(tri[0]), corner(tri[1]), corner(tri[2])) else { continue };
            let Some(face) = normalize(cross(sub(b.p, a.p), sub(c.p, a.p))) else { continue };
            // Both faces lit: a face normal pointing away from the eye is flipped, and so are
            // vertex normals on such a face (a back face of a single-sided model).
            let flip = dot(face, sub(eye, a.p)) < 0.0;
            let face = if flip { scale(face, -1.0) } else { face };
            let material = mesh.material_of(t);
            let base = mesh.triangle_color.get(t).and_then(|i| mesh.colors.get(*i as usize)).copied().unwrap_or([
                crate::NEUTRAL_GREY[0],
                crate::NEUTRAL_GREY[1],
                crate::NEUTRAL_GREY[2],
                1.0,
            ]);
            let texture = match opts.mode {
                DisplayMode::Shaded | DisplayMode::Wireframe => {
                    material.base_color_image.and_then(|i| textures.get(i as usize)).and_then(Option::as_ref)
                }
                _ => None,
            };
            let smooth_tri = a.n.is_some() && b.n.is_some() && c.n.is_some();

            let area = edge(&a.s, &b.s, c.s[0], c.s[1]);
            if area.is_nan() || area.abs() <= 1e-9 {
                continue;
            }
            let min_x = a.s[0].min(b.s[0]).min(c.s[0]).floor().max(0.0);
            let min_y = a.s[1].min(b.s[1]).min(c.s[1]).floor().max(y_lo as f32);
            let max_x = a.s[0].max(b.s[0]).max(c.s[0]).ceil().min(sw as f32 - 1.0);
            let max_y = a.s[1].max(b.s[1]).max(c.s[1]).ceil().min(y_hi as f32 - 1.0).min(sh as f32 - 1.0);
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
            // Screen-space edge lengths, for the wireframe's distance to the nearest edge.
            let len = |p: &[f32; 3], q: &[f32; 3]| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt().max(1e-6);
            let (lbc, lca, lab) = (len(&b.s, &c.s), len(&c.s, &a.s), len(&a.s, &b.s));
            for y in y0..=y1 {
                let py = y as f32 + 0.5;
                let row = (y - y_lo) * sw;
                for x in x0..=x1 {
                    let px = x as f32 + 0.5;
                    let w0 = edge(&b.s, &c.s, px, py) * inv;
                    let w1 = edge(&c.s, &a.s, px, py) * inv;
                    let w2 = edge(&a.s, &b.s, px, py) * inv;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = w0 * a.s[2] + w1 * b.s[2] + w2 * c.s[2];
                    let i = row + x;
                    let Some(d) = depth.get_mut(i) else { continue };
                    if z <= *d {
                        continue;
                    }
                    *d = z;
                    // Perspective-correct weights for attributes.
                    let (q0, q1, q2) = (w0 * a.s[2], w1 * b.s[2], w2 * c.s[2]);
                    let qs = (q0 + q1 + q2).max(1e-20);
                    let (q0, q1, q2) = (q0 / qs, q1 / qs, q2 / qs);
                    let normal = if smooth_tri {
                        let (na, nb, nc) = (a.n.unwrap_or(face), b.n.unwrap_or(face), c.n.unwrap_or(face));
                        let n = add(add(scale(na, q0), scale(nb, q1)), scale(nc, q2));
                        let n = normalize(n).unwrap_or(face);
                        if flip { scale(n, -1.0) } else { n }
                    } else {
                        face
                    };
                    let uv = [q0 * a.uv[0] + q1 * b.uv[0] + q2 * c.uv[0], q0 * a.uv[1] + q1 * b.uv[1] + q2 * c.uv[1]];
                    let rgb = match opts.mode {
                        DisplayMode::Normals => [
                            (normal[0] * 0.5 + 0.5) * 255.0,
                            (normal[1] * 0.5 + 0.5) * 255.0,
                            (normal[2] * 0.5 + 0.5) * 255.0,
                        ]
                        .map(|v| v.clamp(0.0, 255.0) as u8),
                        DisplayMode::Shaded | DisplayMode::Wireframe | DisplayMode::UvChecker => {
                            let albedo = match (opts.mode, texture) {
                                (DisplayMode::UvChecker, _) => {
                                    if has_uv {
                                        let cell = (uv[0] * 8.0).floor() as i64 + (uv[1] * 8.0).floor() as i64;
                                        let k = if cell.rem_euclid(2) == 0 { 0.75 } else { 0.28 };
                                        let (u, v) = (uv[0] - uv[0].floor(), uv[1] - uv[1].floor());
                                        [k * 0.8 + 0.2 * u, k * 0.8 + 0.2 * v, k]
                                    } else {
                                        [0.6, 0.12, 0.55]
                                    }
                                }
                                (_, Some(tex)) => {
                                    let s = tex.sample(uv[0], uv[1]);
                                    [s[0] * base[0], s[1] * base[1], s[2] * base[2]]
                                }
                                (_, None) => [base[0], base[1], base[2]],
                            };
                            let mut light = opts.lighting.ambient;
                            for (dir, weight) in lights {
                                light += weight * dot(normal, dir).max(0.0);
                            }
                            let light = (light * opts.lighting.exposure).clamp(0.0, 4.0);
                            let mut lit = scale(albedo, light);
                            if opts.mode == DisplayMode::Wireframe {
                                let dist =
                                    (w0 * area.abs() / lbc).min(w1 * area.abs() / lca).min(w2 * area.abs() / lab);
                                lit = if dist < edge_px { [0.85, 0.9, 1.0] } else { scale(lit, 0.2) };
                            }
                            [srgb(lit[0]), srgb(lit[1]), srgb(lit[2])]
                        }
                    };
                    if let Some(col) = color.get_mut(i) {
                        *col = rgb;
                    }
                }
            }
        }
    }
}

/// Twice the signed area of (a, b, p) in screen space.
fn edge(a: &[f32; 3], b: &[f32; 3], px: f32, py: f32) -> f32 {
    (px - a[0]) * (b[1] - a[1]) - (py - a[1]) * (b[0] - a[0])
}

#[cfg(test)]
mod lut_tests {
    #[test]
    fn the_const_srgb_table_matches_powf() {
        for b in 0..=255u8 {
            let s = f32::from(b) / 255.0;
            let exact = if s <= 0.040_45 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) };
            assert!((super::to_linear(b) - exact).abs() < 2e-4, "{b}: {} vs {exact}", super::to_linear(b));
        }
    }
}
