//! Mesh simplification by vertex clustering, for the light copy that interactive (drag and
//! budgeted) frames draw. Vertices are snapped to a cubic grid over the mesh's bounds and
//! merged per cell; triangles that collapse (two corners in one cell) or repeat are dropped.
//! Surfaces stay closed — blobbier edges, never the holes that skipping every n-th triangle
//! leaves — and a TRELLIS-sized export (500 k triangles) comes out at a few tens of thousands.
//!
//! Pure and deterministic: the same mesh and grid give the same result on every platform.
//! Attributes: a cell's position is the mean of its vertices; its normal the normalized mean;
//! its texture coordinate the first vertex's (a seam inside a cell shows as a small smear, fine
//! for a frame that is gone in a moment). Material slots are kept per triangle.

use std::collections::HashMap;

use crate::Mesh;

/// Grids tried, finest first, by [`simplify_to`].
const GRIDS: [u32; 5] = [128, 96, 64, 48, 32];
/// Smallest grid [`simplify`] accepts (coarser makes a blob).
pub const MIN_GRID: u32 = 8;
/// Largest grid (finer than this keeps nearly every vertex).
pub const MAX_GRID: u32 = 512;

/// Simplify `mesh` on a `grid`³ cell grid over its bounds. An empty mesh stays empty.
pub fn simplify(mesh: &Mesh, grid: u32) -> Mesh {
    let grid = grid.clamp(MIN_GRID, MAX_GRID);
    let Some((lo, hi)) = mesh.bounds() else { return Mesh::default() };
    let extent = (0..3).map(|a| hi[a] - lo[a]).fold(0.0f32, f32::max).max(f32::MIN_POSITIVE);
    let cell = extent / grid as f32;
    let has_normals = mesh.normals.len() == mesh.positions.len();
    let has_uvs = mesh.uvs.len() == mesh.positions.len();

    // Vertex → cluster, and per cluster the sums that make its attributes.
    let mut cluster_of: Vec<u32> = Vec::with_capacity(mesh.positions.len());
    let mut lookup: HashMap<[u32; 3], u32> = HashMap::new();
    let mut sum_pos: Vec<[f64; 3]> = Vec::new();
    let mut sum_nrm: Vec<[f64; 3]> = Vec::new();
    let mut first_uv: Vec<[f32; 2]> = Vec::new();
    let mut count: Vec<u32> = Vec::new();
    for (i, p) in mesh.positions.iter().enumerate() {
        let key = [0, 1, 2].map(|a| (((p[a] - lo[a]) / cell).floor().max(0.0) as u32).min(grid - 1));
        let id = *lookup.entry(key).or_insert_with(|| {
            sum_pos.push([0.0; 3]);
            sum_nrm.push([0.0; 3]);
            first_uv.push(if has_uvs { mesh.uvs[i] } else { [0.0; 2] });
            count.push(0);
            (sum_pos.len() - 1) as u32
        });
        cluster_of.push(id);
        let c = id as usize;
        for a in 0..3 {
            sum_pos[c][a] += f64::from(p[a]);
            if has_normals {
                sum_nrm[c][a] += f64::from(mesh.normals[i][a]);
            }
        }
        count[c] += 1;
    }

    // Triangles over clusters: drop collapsed and repeated ones, keep the first's winding.
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    let mut triangle_color: Vec<u16> = Vec::new();
    let mut seen: HashMap<[u32; 3], ()> = HashMap::new();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        let Some(c) = tri.iter().map(|&v| cluster_of.get(v as usize).copied()).collect::<Option<Vec<u32>>>() else {
            continue;
        };
        let (a, b, d) = (c[0], c[1], c[2]);
        if a == b || b == d || a == d {
            continue;
        }
        let mut key = [a, b, d];
        key.sort_unstable();
        if seen.insert(key, ()).is_some() {
            continue;
        }
        triangles.push([a, b, d]);
        triangle_color.push(mesh.triangle_color.get(t).copied().unwrap_or(0));
    }

    // Clusters that no triangle uses are dropped; the rest are renumbered in order.
    let mut used = vec![false; sum_pos.len()];
    for tri in &triangles {
        for &c in tri {
            used[c as usize] = true;
        }
    }
    let mut renumber = vec![u32::MAX; sum_pos.len()];
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for c in 0..sum_pos.len() {
        if !used[c] {
            continue;
        }
        renumber[c] = positions.len() as u32;
        let n = f64::from(count[c].max(1));
        positions.push([0, 1, 2].map(|a| (sum_pos[c][a] / n) as f32));
        if has_normals {
            let v = sum_nrm[c];
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            normals.push(if len > 1e-12 { [0, 1, 2].map(|a| (v[a] / len) as f32) } else { [0.0; 3] });
        }
        if has_uvs {
            uvs.push(first_uv[c]);
        }
    }
    for tri in &mut triangles {
        for c in tri.iter_mut() {
            *c = renumber[*c as usize];
        }
    }
    Mesh {
        positions,
        triangles,
        triangle_color,
        colors: mesh.colors.clone(),
        normals,
        uvs,
        materials: mesh.materials.clone(),
        images: Vec::new(),
    }
}

/// Simplify `mesh` to at most `target` triangles when a grid from 128³ down to 32³ gets there,
/// else the coarsest try. A mesh already within `target` comes back as is (a cheap clone).
pub fn simplify_to(mesh: &Mesh, target: usize) -> Mesh {
    if mesh.triangle_count() <= target {
        return Mesh {
            positions: mesh.positions.clone(),
            triangles: mesh.triangles.clone(),
            triangle_color: mesh.triangle_color.clone(),
            colors: mesh.colors.clone(),
            normals: mesh.normals.clone(),
            uvs: mesh.uvs.clone(),
            materials: mesh.materials.clone(),
            images: Vec::new(),
        };
    }
    let mut last = None;
    for grid in GRIDS {
        let light = simplify(mesh, grid);
        if light.triangle_count() <= target {
            return light;
        }
        last = Some(light);
    }
    last.unwrap_or_default()
}
