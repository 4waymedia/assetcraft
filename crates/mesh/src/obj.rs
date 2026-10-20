//! Wavefront OBJ: `v` positions and `f` polygons (fan-triangulated, 1-based or negative
//! relative indices, `v/vt/vn` forms). Everything else (normals, UVs, groups, materials,
//! curves) is skipped. Lines that don't parse are ignored, like most OBJ readers do.

use crate::{Limits, Mesh, MeshError, NEUTRAL_GREY};

/// Most corners one face may have (larger polygons are skipped).
const MAX_FACE_CORNERS: usize = 256;

/// Parse an OBJ file's bytes.
pub fn parse_obj(bytes: &[u8], limits: &Limits) -> Result<Mesh, MeshError> {
    let mut mesh = Mesh::default();
    let color = mesh.slot_index(
        &mut std::collections::HashMap::new(),
        [NEUTRAL_GREY[0], NEUTRAL_GREY[1], NEUTRAL_GREY[2], 1.0],
        crate::Material::default(),
    )?;
    let mut corners: Vec<u32> = Vec::new();
    for raw in bytes.split(|b| *b == b'\n') {
        let Ok(line) = std::str::from_utf8(raw) else { continue };
        let mut words = line.split_ascii_whitespace();
        match words.next() {
            Some("v") => {
                let mut p = [0.0f32; 3];
                let mut ok = true;
                for v in &mut p {
                    match words.next().and_then(|w| w.parse::<f32>().ok()).filter(|f| f.is_finite()) {
                        Some(f) => *v = f,
                        None => ok = false,
                    }
                }
                // Keep bad vertices as the origin so later face indices still line up.
                if !ok {
                    p = [0.0; 3];
                }
                if mesh.positions.len() >= limits.max_vertices {
                    return Err(MeshError::TooLarge);
                }
                mesh.positions.push(p);
            }
            Some("f") => {
                corners.clear();
                let count = mesh.positions.len();
                for word in words {
                    if corners.len() >= MAX_FACE_CORNERS {
                        corners.clear();
                        break;
                    }
                    let Some(i) = word.split('/').next().and_then(|v| v.parse::<i64>().ok()) else {
                        corners.clear();
                        break;
                    };
                    let resolved = if i > 0 { i - 1 } else { count as i64 + i };
                    match u32::try_from(resolved).ok().filter(|r| (*r as usize) < count) {
                        Some(r) => corners.push(r),
                        None => {
                            corners.clear();
                            break;
                        }
                    }
                }
                if let Some((&first, rest)) = corners.split_first() {
                    for w in rest.windows(2) {
                        if first != w[0] && w[0] != w[1] && first != w[1] {
                            if mesh.triangles.len() >= limits.max_triangles {
                                return Err(MeshError::TooLarge);
                            }
                            mesh.triangles.push([first, w[0], w[1]]);
                            mesh.triangle_color.push(color);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    mesh.non_empty()
}
