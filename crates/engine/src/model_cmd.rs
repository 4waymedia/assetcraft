//! `asset.meshInfo`: the facts of a 3D model file (3D viewer V0.4), read through
//! `assetcraft-thumbs`' loader under its limits. Nothing is decoded beyond the geometry: image
//! sizes come from the headers.

use crate::{CmdError, CmdResult, Session};
use assetcraft_thumbs::model::{Mesh, image_dimensions, load_path};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Above this many triangles the interactive preview decimates (`PREVIEW_MAX_TRIANGLES`).
const PREVIEW_MAX_TRIANGLES: usize = assetcraft_thumbs::model::PREVIEW_MAX_TRIANGLES;

/// The `path` param: a full path to a GLB, glTF or OBJ file.
fn model_path(p: &Value) -> Result<PathBuf, CmdError> {
    let path = p
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|x| !x.is_empty() && x.len() <= 4096)
        .map(PathBuf::from)
        .ok_or_else(|| CmdError::BadParams("need `path` (a 3D model file)".into()))?;
    if !path.has_root() {
        return Err(CmdError::BadParams("`path` must be a full path".into()));
    }
    Ok(path)
}

/// `asset.meshInfo {path}` → counts, bounds, materials, images and warnings.
pub(crate) fn mesh_info(s: &mut Session, p: &Value) -> CmdResult {
    let path = model_path(p)?;
    let (format, bytes) = {
        let probe = s.probe(&path)?;
        (probe.format.label(), probe.size)
    };
    let mesh = load_path(s.services.as_ref(), &path).map_err(|e| CmdError::Unavailable(e.to_string()))?;
    let mut info = describe(&mesh);
    if let Value::Object(map) = &mut info {
        map.insert("path".into(), json!(path.to_string_lossy()));
        map.insert("format".into(), json!(format));
        map.insert("bytes".into(), json!(bytes));
    }
    Ok(info)
}

/// The JSON facts of a parsed mesh (no path: the caller adds it).
pub(crate) fn describe(mesh: &Mesh) -> Value {
    let has_normals = !mesh.normals.is_empty();
    let has_uvs = !mesh.uvs.is_empty();
    let mut warnings = Vec::new();
    if !has_normals {
        warnings.push("no normals: faces are shaded flat".to_string());
    }
    if !has_uvs && !mesh.images.is_empty() {
        warnings.push("no texture coordinates: textures can't be drawn".to_string());
    }
    if mesh.triangle_count() > PREVIEW_MAX_TRIANGLES {
        warnings.push(format!("over {PREVIEW_MAX_TRIANGLES} triangles: the preview decimates while dragging"));
    }
    let images: Vec<Value> = mesh
        .images
        .iter()
        .enumerate()
        .map(|(i, image)| {
            let drawn = mesh.materials.iter().any(|m| m.base_color_image == Some(i as u32));
            let dims = image_dimensions(&image.bytes);
            if dims.is_none() {
                warnings.push(format!(
                    "image {i}: dimensions unreadable ({})",
                    if image.mime.is_empty() { "unknown type" } else { &image.mime }
                ));
            }
            json!({
                "index": i,
                "mime": image.mime,
                "bytes": image.bytes.len(),
                "width": dims.map(|d| d.0),
                "height": dims.map(|d| d.1),
                "drawn": drawn,
            })
        })
        .collect();
    let bounds = mesh
        .bounds()
        .map(|(lo, hi)| json!({ "min": lo, "max": hi, "size": [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]] }));
    json!({
        "vertices": mesh.positions.len(),
        "triangles": mesh.triangle_count(),
        "materials": mesh.materials.len(),
        "normals": has_normals,
        "uvs": has_uvs,
        "bounds": bounds,
        "images": images,
        "warnings": warnings,
    })
}
