//! Bounded 3D mesh parsing (glTF 2.0 JSON, GLB, Wavefront OBJ) and a CPU rasterizer.
//!
//! Layer 1. Pure functions over bytes: no file system, no GPU, no threads, so it runs the same
//! in tests, worker threads and headless CI. `assetcraft-thumbs` reads the files (and a glTF's
//! external `.bin` buffers) through `Services` and hands the bytes in.
//!
//! - [`parse_glb`], [`parse_gltf`] and [`parse_obj`] turn a file into a [`Mesh`]: world-space
//!   positions (node transforms applied), triangles, one material slot per triangle (a flat
//!   colour plus, for glTF, the material's textures), and per-vertex normals and texture
//!   coordinates when the file has them. Images stay as the encoded bytes found in the file
//!   ([`ImageData`]); decoding them is the caller's job (`assetcraft-thumbs`, which has the image
//!   decoders and their allocation limits). Every count and size read from the file is checked
//!   against [`Limits`] before anything is allocated; bad data is a [`MeshError`], never a panic.
//! - [`render`] and [`render_with`] draw a [`Mesh`] (the latter with decoded [`Texture`]s)
//!   with a z-buffered software rasterizer from an orbit [`View`] fitted to the mesh's bounds:
//!   smooth or flat shading under a three-point [`Lighting`] rig, bilinear base colour
//!   textures, a [`DisplayMode`] (shaded, wireframe, normals, UV checker) and a
//!   [`Background`].
//!
//! Not drawn: the metallic-roughness map (V1, with an environment term), vertex colours,
//! alpha. Not read: sparse accessors, Draco/meshopt compression (reported as
//! [`MeshError::Unsupported`]), OBJ `vn` / `vt` / `.mtl`, glTF samplers (wrap is assumed) and
//! texture coordinate sets other than `TEXCOORD_0`.

mod gltf;
mod obj;
mod raster;

pub use gltf::{parse_glb, parse_gltf};
pub use obj::parse_obj;
pub use raster::{Background, DisplayMode, Image, Lighting, MAX_SAMPLES, RenderOptions, Texture, render, render_with};

use std::fmt;

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// Colour for surfaces without a material colour (linear RGB, a neutral mid grey).
pub const NEUTRAL_GREY: [f32; 3] = [0.55, 0.55, 0.57];

/// Caps on what a parsed file may make us allocate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Most vertices in the merged mesh (after instancing nodes).
    pub max_vertices: usize,
    /// Most triangles in the merged mesh.
    pub max_triangles: usize,
    /// Most bytes of buffer data (embedded, data-URI and external together).
    pub max_buffer_bytes: usize,
    /// Most bytes of glTF JSON.
    pub max_json_bytes: usize,
    /// Most nodes visited while walking the scene graph (cycles and huge DAGs stop here). Also
    /// caps mesh instances × primitives placed, empty ones included.
    pub max_node_visits: usize,
    /// Most bytes of encoded images kept (each one, and all of them together). Images over it
    /// are dropped from the mesh (the material keeps no texture), not an error: the geometry
    /// still draws.
    pub max_image_bytes: usize,
}

impl Default for Limits {
    /// 2 M vertices / 2 M triangles (≈ 48 MiB of mesh), 128 MiB of buffers, 16 MiB of JSON,
    /// 96 MiB of encoded images.
    fn default() -> Self {
        Self {
            max_vertices: 2_000_000,
            max_triangles: 2_000_000,
            max_buffer_bytes: 128 * 1024 * 1024,
            max_json_bytes: 16 * 1024 * 1024,
            max_node_visits: 100_000,
            max_image_bytes: 96 * 1024 * 1024,
        }
    }
}

/// Why a mesh could not be read or drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshError {
    /// The data is not valid for its format.
    Malformed(String),
    /// Over one of the [`Limits`].
    TooLarge,
    /// Valid, but uses a feature we can't read (e.g. Draco compression).
    Unsupported(String),
    /// An external buffer could not be read.
    External(String),
    /// Nothing to draw: no triangles.
    Empty,
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(m) => write!(f, "malformed model: {m}"),
            Self::TooLarge => write!(f, "model too large"),
            Self::Unsupported(m) => write!(f, "unsupported model feature: {m}"),
            Self::External(m) => write!(f, "external buffer: {m}"),
            Self::Empty => write!(f, "model has no triangles"),
        }
    }
}

impl std::error::Error for MeshError {}

/// Shorthand for a [`MeshError::Malformed`].
pub(crate) fn bad(message: impl Into<String>) -> MeshError {
    MeshError::Malformed(message.into())
}

/// A material slot's textures and flags. Its base colour is the same-index entry of
/// [`Mesh::colors`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Material {
    /// Index into [`Mesh::images`] of the base colour texture (`TEXCOORD_0`), multiplied by the
    /// colour.
    pub base_color_image: Option<u32>,
    /// Index into [`Mesh::images`] of the metallic-roughness texture (glTF packs roughness in
    /// G and metalness in B).
    pub metallic_roughness_image: Option<u32>,
    /// Draw both faces (glTF `doubleSided`).
    pub double_sided: bool,
}

/// An image as encoded in the file (PNG, JPEG, …). Nothing here decodes it: the bytes are
/// bounded by [`Limits::max_image_bytes`] and handed to the caller's image decoder.
#[derive(Clone, PartialEq, Eq)]
pub struct ImageData {
    /// The file's declared MIME type (`image/png`, `image/jpeg`), or empty when unknown.
    pub mime: String,
    /// The encoded bytes.
    pub bytes: Vec<u8>,
}

impl fmt::Debug for ImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ImageData({:?}, {} bytes)", self.mime, self.bytes.len())
    }
}

/// A triangle soup ready to draw: world-space positions, triangles, one material slot per
/// triangle, and optional per-vertex normals and texture coordinates.
///
/// Invariants: `normals` and `uvs` are each empty or exactly `positions.len()` long;
/// `materials.len() == colors.len()`; every image index in `materials` is in range.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    /// Vertex positions (finite).
    pub positions: Vec<[f32; 3]>,
    /// Triangles as indices into `positions` (all in range).
    pub triangles: Vec<[u32; 3]>,
    /// Index into `colors` / `materials` for each triangle.
    pub triangle_color: Vec<u16>,
    /// Linear RGBA colours (at most `u16::MAX + 1`), one per material slot.
    pub colors: Vec<[f32; 4]>,
    /// World-space unit normals, one per vertex, when the file has them (empty otherwise).
    /// `[0.0; 3]` marks a vertex whose primitive had none; draw it with its face normal.
    pub normals: Vec<[f32; 3]>,
    /// `TEXCOORD_0`, one per vertex, when the file has them (empty otherwise). Values outside
    /// `0..=1` wrap.
    pub uvs: Vec<[f32; 2]>,
    /// Textures and flags, one per `colors` entry.
    pub materials: Vec<Material>,
    /// Encoded images referenced by `materials`.
    pub images: Vec<ImageData>,
}

impl Mesh {
    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// The material slot of triangle `t`, or the default (no textures) when out of range.
    pub fn material_of(&self, t: usize) -> Material {
        self.triangle_color.get(t).and_then(|i| self.materials.get(*i as usize)).copied().unwrap_or_default()
    }

    /// Bytes of encoded images held.
    pub fn image_bytes(&self) -> usize {
        self.images.iter().map(|i| i.bytes.len()).sum()
    }

    /// Axis-aligned bounds `(min, max)`, or `None` for an empty mesh.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = *self.positions.first()?;
        Some(self.positions.iter().fold((first, first), |(lo, hi), p| {
            ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
        }))
    }

    /// Bytes this mesh holds, for memory budgets.
    pub fn cost(&self) -> usize {
        self.positions.len() * 12
            + self.triangles.len() * 14
            + self.colors.len() * (16 + std::mem::size_of::<Material>())
            + self.normals.len() * 12
            + self.uvs.len() * 8
            + self.image_bytes()
    }

    /// Add a material slot (a colour plus textures and flags, deduplicated through `lookup`,
    /// keyed by the colour's bit patterns and the material) and return its index. A hash
    /// lookup, so many instances over up to 65,536 slots stay linear.
    pub(crate) fn slot_index(
        &mut self,
        lookup: &mut std::collections::HashMap<([u32; 4], Material), u16>,
        color: [f32; 4],
        material: Material,
    ) -> Result<u16, MeshError> {
        let key = (color.map(f32::to_bits), material);
        if let Some(i) = lookup.get(&key) {
            return Ok(*i);
        }
        let i = u16::try_from(self.colors.len()).map_err(|_| MeshError::TooLarge)?;
        self.colors.push(color);
        self.materials.push(material);
        lookup.insert(key, i);
        Ok(i)
    }

    /// Fail with [`MeshError::Empty`] when there is nothing to draw.
    pub(crate) fn non_empty(self) -> Result<Self, MeshError> {
        if self.triangles.is_empty() { Err(MeshError::Empty) } else { Ok(self) }
    }
}

/// An orbit camera around the mesh's bounding sphere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Rotation around the vertical (+Y) axis in radians; 0 looks at the model's front (+Z).
    pub yaw: f32,
    /// Elevation in radians; positive looks down from above. Clamped to ±[`View::MAX_PITCH`].
    pub pitch: f32,
    /// Magnification; 1 fits the whole bounding sphere.
    pub zoom: f32,
}

impl View {
    /// The 3/4 view thumbnails use: front-right, from a little above.
    pub const THREE_QUARTER: View = View { yaw: 0.6, pitch: 0.42, zoom: 1.0 };
    /// Largest |pitch| (just short of straight up/down, where "up" is undefined).
    pub const MAX_PITCH: f32 = 1.55;
    /// Zoom range.
    pub const ZOOM_RANGE: (f32, f32) = (0.25, 16.0);

    /// Turn by `dyaw`/`dpitch` radians, keeping pitch in range and yaw in `(-π, π]`.
    pub fn orbit(self, dyaw: f32, dpitch: f32) -> View {
        let mut yaw = self.yaw + dyaw;
        if yaw.is_finite() {
            yaw = (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        } else {
            yaw = Self::THREE_QUARTER.yaw;
        }
        let pitch = if (self.pitch + dpitch).is_finite() { self.pitch + dpitch } else { 0.0 };
        View { yaw, pitch: pitch.clamp(-Self::MAX_PITCH, Self::MAX_PITCH), zoom: self.zoom }
    }

    /// Multiply the zoom by `factor`, kept in [`View::ZOOM_RANGE`].
    pub fn zoomed(self, factor: f32) -> View {
        let zoom = if (self.zoom * factor).is_finite() { self.zoom * factor } else { 1.0 };
        View { zoom: zoom.clamp(Self::ZOOM_RANGE.0, Self::ZOOM_RANGE.1), ..self }
    }
}

impl Default for View {
    fn default() -> Self {
        Self::THREE_QUARTER
    }
}

#[cfg(test)]
mod tests;
