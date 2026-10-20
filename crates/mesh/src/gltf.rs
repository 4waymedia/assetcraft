//! glTF 2.0 (JSON and binary GLB): a small reader for triangle geometry, normals, texture
//! coordinates, base colours and the material's images.
//!
//! Reads `scenes` → `nodes` (matrix or TRS, nested) → `meshes` → `primitives` with a `POSITION`
//! accessor (float, or integer per `KHR_mesh_quantization`), optional `NORMAL` and `TEXCOORD_0`
//! (float or normalized integers), optional `indices` (u8/u16/u32) and modes triangles / strip /
//! fan; `materials` → `textures` → `images` for the base colour and metallic-roughness maps,
//! whose encoded bytes come out as [`crate::ImageData`]. Buffers and images come from the GLB
//! `BIN` chunk, base64 `data:` URIs, or the caller's resolver for relative external files. Every
//! offset, stride and count is checked against the actual bytes and the [`Limits`] before use.

use crate::{ImageData, Limits, Material, Mesh, MeshError, NEUTRAL_GREY, bad};
use serde_json::Value;
use std::collections::HashMap;

/// Reads an external buffer: `(relative path with '/' separators, byte length) -> bytes`.
/// The path has been percent-decoded and checked: relative, no `..`, no scheme or drive.
pub type Resolver<'a> = dyn FnMut(&str, usize) -> Result<Vec<u8>, String> + 'a;

const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

/// Geometry compression we can't decode.
const COMPRESSION: &[&str] = &["KHR_draco_mesh_compression", "EXT_meshopt_compression", "KHR_meshopt_compression"];

/// Parse a binary glTF (`.glb`).
pub fn parse_glb(bytes: &[u8], limits: &Limits, resolve: &mut Resolver<'_>) -> Result<Mesh, MeshError> {
    if bytes.get(..4) != Some(b"glTF".as_slice()) {
        return Err(bad("not a GLB file"));
    }
    let version = u32_at(bytes, 4).ok_or_else(|| bad("GLB header truncated"))?;
    if version != 2 {
        return Err(MeshError::Unsupported(format!("glTF version {version}")));
    }
    let declared = u32_at(bytes, 8).ok_or_else(|| bad("GLB header truncated"))? as usize;
    let end = declared.min(bytes.len());
    let (mut json, mut bin) = (None, None);
    let mut offset = 12usize;
    while offset.saturating_add(8) <= end {
        let (Some(len), Some(kind)) = (u32_at(bytes, offset), u32_at(bytes, offset + 4)) else { break };
        let start = offset + 8;
        let stop =
            start.checked_add(len as usize).filter(|s| *s <= end).ok_or_else(|| bad("GLB chunk runs past the end"))?;
        let data = bytes.get(start..stop).ok_or_else(|| bad("GLB chunk runs past the end"))?;
        match kind {
            CHUNK_JSON if json.is_none() => json = Some(data),
            CHUNK_BIN if bin.is_none() => bin = Some(data),
            _ => {}
        }
        offset = stop.saturating_add(3) & !3;
    }
    let json = json.ok_or_else(|| bad("GLB has no JSON chunk"))?;
    parse_document(json, bin, limits, resolve)
}

/// Parse a glTF JSON document (`.gltf`). Buffers are base64 `data:` URIs or external files read
/// through `resolve`.
pub fn parse_gltf(json: &[u8], limits: &Limits, resolve: &mut Resolver<'_>) -> Result<Mesh, MeshError> {
    parse_document(json, None, limits, resolve)
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let b = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn parse_document(
    json: &[u8],
    bin: Option<&[u8]>,
    limits: &Limits,
    resolve: &mut Resolver<'_>,
) -> Result<Mesh, MeshError> {
    if json.len() > limits.max_json_bytes {
        return Err(MeshError::TooLarge);
    }
    let json = json.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(json);
    let root: Value = serde_json::from_slice(json).map_err(|e| bad(format!("glTF JSON: {e}")))?;
    if !root.is_object() {
        return Err(bad("glTF JSON is not an object"));
    }
    if let Some(version) = root.pointer("/asset/version").and_then(Value::as_str)
        && !version.starts_with('2')
    {
        return Err(MeshError::Unsupported(format!("glTF version {version}")));
    }
    if let Some(required) = root.get("extensionsRequired").and_then(Value::as_array)
        && let Some(ext) = required.iter().filter_map(Value::as_str).find(|e| COMPRESSION.contains(e))
    {
        return Err(MeshError::Unsupported(ext.to_string()));
    }
    let mut buffers = Buffers { root: &root, bin, loaded: HashMap::new(), total: 0, elements: 0, limits, resolve };
    let instances = instances(&root, limits)?;
    let mut mesh = Mesh::default();
    let mut colors = HashMap::new();
    let mut images = Images { loaded: HashMap::new(), total: 0 };
    let mut cache: HashMap<usize, Vec<Primitive>> = HashMap::new();
    // Budgets over everything cached (shared accessors are re-read per primitive) and over the
    // work of placing instances, so neither huge shared index buffers nor thousands of empty
    // primitives × instances can run away.
    let (mut cached_vertices, mut cached_triangles, mut work) = (0usize, 0usize, 0usize);
    let mut first_error = None;
    for (mesh_index, matrix) in instances {
        if let std::collections::hash_map::Entry::Vacant(slot) = cache.entry(mesh_index) {
            let mut prims = Vec::new();
            let list =
                array(&root, "meshes").get(mesh_index).and_then(|m| m.get("primitives")).and_then(Value::as_array);
            for prim in list.into_iter().flatten() {
                work = work.saturating_add(1);
                if work > limits.max_node_visits {
                    return Err(MeshError::TooLarge);
                }
                match read_primitive(&root, &mut buffers, &mut images, &mut mesh, prim, limits) {
                    Ok(Some(p)) if p.triangles.is_empty() => {} // nothing to draw: not cached
                    Ok(Some(p)) => {
                        cached_vertices = cached_vertices.saturating_add(p.positions.len());
                        cached_triangles = cached_triangles.saturating_add(p.triangles.len());
                        if cached_vertices > limits.max_vertices || cached_triangles > limits.max_triangles {
                            return Err(MeshError::TooLarge);
                        }
                        prims.push(p);
                    }
                    Ok(None) => {}
                    Err(e @ (MeshError::TooLarge | MeshError::External(_))) => return Err(e),
                    Err(e) => {
                        first_error.get_or_insert(e);
                    }
                }
            }
            slot.insert(prims);
        }
        work = work.saturating_add(1);
        for prim in cache.get(&mesh_index).into_iter().flatten() {
            work = work.saturating_add(1);
            if work > limits.max_node_visits {
                return Err(MeshError::TooLarge);
            }
            append(&mut mesh, &mut colors, prim, &matrix, limits)?;
        }
        if work > limits.max_node_visits {
            return Err(MeshError::TooLarge);
        }
    }
    match (mesh.triangles.is_empty(), first_error) {
        (true, Some(e)) => Err(e),
        _ => mesh.non_empty(),
    }
}

fn array<'v>(root: &'v Value, key: &str) -> &'v [Value] {
    root.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn index(v: &Value, key: &str) -> Option<usize> {
    v.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok())
}

// ---- scene graph ---------------------------------------------------------------------------

/// Column-major 4×4 matrix.
type Mat4 = [f32; 16];

const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum();
        }
    }
    out
}

fn numbers<const N: usize>(v: Option<&Value>) -> Option<[f32; N]> {
    let list = v?.as_array()?;
    if list.len() != N {
        return None;
    }
    let mut out = [0.0f32; N];
    for (o, x) in out.iter_mut().zip(list) {
        *o = x.as_f64()? as f32;
        if !o.is_finite() {
            return None;
        }
    }
    Some(out)
}

fn local_matrix(node: &Value) -> Mat4 {
    if let Some(m) = numbers::<16>(node.get("matrix")) {
        return m;
    }
    let [tx, ty, tz] = numbers::<3>(node.get("translation")).unwrap_or([0.0; 3]);
    let [x, y, z, w] = numbers::<4>(node.get("rotation")).unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let [sx, sy, sz] = numbers::<3>(node.get("scale")).unwrap_or([1.0; 3]);
    let n = (x * x + y * y + z * z + w * w).sqrt();
    let (x, y, z, w) = if n > 1e-12 { (x / n, y / n, z / n, w / n) } else { (0.0, 0.0, 0.0, 1.0) };
    let r = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
    ];
    [
        r[0] * sx,
        r[1] * sx,
        r[2] * sx,
        0.0,
        r[3] * sy,
        r[4] * sy,
        r[5] * sy,
        0.0,
        r[6] * sz,
        r[7] * sz,
        r[8] * sz,
        0.0,
        tx,
        ty,
        tz,
        1.0,
    ]
}

/// Every (mesh, world matrix) the default scene draws.
fn instances(root: &Value, limits: &Limits) -> Result<Vec<(usize, Mat4)>, MeshError> {
    let nodes = array(root, "nodes");
    let scenes = array(root, "scenes");
    let roots: Vec<usize> = if let Some(scene) = index(root, "scene").and_then(|i| scenes.get(i)).or(scenes.first()) {
        array(scene, "nodes").iter().filter_map(Value::as_u64).filter_map(|n| usize::try_from(n).ok()).collect()
    } else {
        // No scenes: every node that is nobody's child.
        let mut child = vec![false; nodes.len()];
        for node in nodes {
            for c in array(node, "children").iter().filter_map(Value::as_u64) {
                if let Some(flag) = usize::try_from(c).ok().and_then(|c| child.get_mut(c)) {
                    *flag = true;
                }
            }
        }
        (0..nodes.len()).filter(|i| !child[*i]).collect()
    };
    let mut out = Vec::new();
    if nodes.is_empty() {
        // Geometry without a scene graph: draw every mesh once.
        out.extend((0..array(root, "meshes").len()).map(|m| (m, IDENTITY)));
        return Ok(out);
    }
    let mut visits = 0usize;
    let mut stack: Vec<(usize, Mat4, usize)> = roots.into_iter().rev().map(|n| (n, IDENTITY, 0)).collect();
    while let Some((n, parent, depth)) = stack.pop() {
        visits += 1;
        if visits > limits.max_node_visits {
            return Err(MeshError::TooLarge);
        }
        let Some(node) = nodes.get(n) else { continue };
        if depth > 64 {
            continue;
        }
        let world = mul(&parent, &local_matrix(node));
        if let Some(m) = index(node, "mesh") {
            out.push((m, world));
        }
        for c in array(node, "children").iter().rev().filter_map(Value::as_u64) {
            if let Ok(c) = usize::try_from(c) {
                stack.push((c, world, depth + 1));
            }
        }
    }
    Ok(out)
}

// ---- images --------------------------------------------------------------------------------

/// Which glTF images have been copied into [`Mesh::images`], and how many bytes they hold.
struct Images {
    /// glTF image index → `Mesh::images` index, or `None` when it was refused (over the limit,
    /// unreadable); a refusal is remembered so it isn't retried per primitive.
    loaded: HashMap<usize, Option<u32>>,
    total: usize,
}

impl Images {
    /// Copy glTF image `i` into `mesh.images` once and return its index there. An image that
    /// is missing, malformed, over [`Limits::max_image_bytes`] or unreadable yields `None`: the
    /// geometry still draws, untextured.
    fn get(&mut self, root: &Value, buffers: &mut Buffers<'_, '_>, mesh: &mut Mesh, i: usize) -> Option<u32> {
        if let Some(found) = self.loaded.get(&i) {
            return *found;
        }
        let limit = buffers.limits.max_image_bytes;
        let loaded = array(root, "images").get(i).and_then(|img| {
            let mime = img.get("mimeType").and_then(Value::as_str).unwrap_or_default().to_string();
            let bytes: Vec<u8> = if let Some(view_index) = index(img, "bufferView") {
                let view = array(root, "bufferViews").get(view_index)?;
                let len = index(view, "byteLength")?;
                if len > limit || self.total.saturating_add(len) > limit {
                    return None;
                }
                let offset = index(view, "byteOffset").unwrap_or(0);
                let data = buffers.get(index(view, "buffer")?).ok()?;
                data.get(offset..offset.checked_add(len)?)?.to_vec()
            } else {
                let uri = img.get("uri").and_then(Value::as_str)?;
                if let Some(rest) = uri.strip_prefix("data:") {
                    let (head, payload) = rest.split_once(";base64,")?;
                    if payload.len() / 4 * 3 > limit || self.total.saturating_add(payload.len() / 4 * 3) > limit {
                        return None;
                    }
                    let decoded = base64(payload)?;
                    if decoded.len() > limit || self.total.saturating_add(decoded.len()) > limit {
                        return None;
                    }
                    let mime = if mime.is_empty() { head.to_string() } else { mime };
                    self.total += decoded.len();
                    let n = u32::try_from(mesh.images.len()).ok()?;
                    mesh.images.push(ImageData { mime, bytes: decoded });
                    return Some(n);
                }
                let path = relative_path(uri)?;
                let data = (buffers.resolve)(&path, limit).ok()?;
                if data.len() > limit || self.total.saturating_add(data.len()) > limit {
                    return None;
                }
                data
            };
            self.total += bytes.len();
            let n = u32::try_from(mesh.images.len()).ok()?;
            mesh.images.push(ImageData { mime, bytes });
            Some(n)
        });
        self.loaded.insert(i, loaded);
        loaded
    }
}

// ---- buffers -------------------------------------------------------------------------------

struct Buffers<'a, 'r> {
    root: &'a Value,
    bin: Option<&'a [u8]>,
    loaded: HashMap<usize, Vec<u8>>,
    total: usize,
    /// Accessor elements read so far (positions + indices), across all primitives.
    elements: usize,
    limits: &'a Limits,
    resolve: &'a mut Resolver<'r>,
}

impl Buffers<'_, '_> {
    /// Count `n` accessor elements against the whole document's budget (every vertex and index
    /// the limits allow, once) before they are read: K primitives sharing one huge accessor
    /// would otherwise re-read and re-allocate it K times.
    fn charge(&mut self, n: usize) -> Result<(), MeshError> {
        let budget = self.limits.max_vertices.saturating_add(self.limits.max_triangles.saturating_mul(3));
        self.elements = self.elements.saturating_add(n);
        if self.elements > budget { Err(MeshError::TooLarge) } else { Ok(()) }
    }

    fn get(&mut self, i: usize) -> Result<&[u8], MeshError> {
        let spec = array(self.root, "buffers").get(i).ok_or_else(|| bad(format!("no buffer {i}")))?;
        let len = index(spec, "byteLength").ok_or_else(|| bad("buffer without byteLength"))?;
        let uri = spec.get("uri").and_then(Value::as_str);
        if uri.is_none() {
            let bin = self.bin.filter(|_| i == 0).ok_or_else(|| bad("buffer without data"))?;
            return bin.get(..len).ok_or_else(|| bad("GLB BIN chunk shorter than its buffer"));
        }
        if !self.loaded.contains_key(&i) {
            if len > self.limits.max_buffer_bytes || self.total.saturating_add(len) > self.limits.max_buffer_bytes {
                return Err(MeshError::TooLarge);
            }
            let uri = uri.unwrap_or_default();
            let mut data = if let Some(rest) = uri.strip_prefix("data:") {
                let (_, payload) =
                    rest.split_once(";base64,").ok_or_else(|| MeshError::Unsupported("non-base64 data URI".into()))?;
                if payload.len() / 4 * 3 > self.limits.max_buffer_bytes {
                    return Err(MeshError::TooLarge);
                }
                base64(payload).ok_or_else(|| bad("bad base64 in data URI"))?
            } else {
                let path =
                    relative_path(uri).ok_or_else(|| MeshError::External(format!("refused buffer URI {uri:?}")))?;
                (self.resolve)(&path, len).map_err(MeshError::External)?
            };
            if data.len() < len {
                return Err(bad(format!("buffer {i} is shorter than its byteLength")));
            }
            data.truncate(len);
            self.total += len;
            self.loaded.insert(i, data);
        }
        self.loaded.get(&i).map(Vec::as_slice).ok_or_else(|| bad("buffer cache"))
    }
}

/// Decode standard base64 (padding optional, whitespace ignored).
fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            c if c.is_ascii_whitespace() => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Percent-decode a relative URI and refuse anything that could leave the model's folder.
pub(crate) fn relative_path(uri: &str) -> Option<String> {
    let bytes = uri.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    let text = String::from_utf8(decoded).ok()?;
    if text.is_empty() || text.contains(':') || text.contains('\0') || text.starts_with(['/', '\\']) {
        return None;
    }
    let parts: Vec<&str> = text.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".").collect();
    if parts.is_empty() || parts.contains(&"..") {
        return None;
    }
    Some(parts.join("/"))
}

// ---- accessors -----------------------------------------------------------------------------

struct Accessor {
    count: usize,
    component: u64,
    components: usize,
    normalized: bool,
    offset: usize,
    stride: usize,
    buffer: usize,
    view_offset: usize,
    view_len: usize,
}

fn component_size(component: u64) -> Option<usize> {
    match component {
        5120 | 5121 => Some(1),
        5122 | 5123 => Some(2),
        5125 | 5126 => Some(4),
        _ => None,
    }
}

fn accessor(root: &Value, i: usize, limit: usize) -> Result<Accessor, MeshError> {
    let acc = array(root, "accessors").get(i).ok_or_else(|| bad(format!("no accessor {i}")))?;
    if acc.get("sparse").is_some() {
        return Err(MeshError::Unsupported("sparse accessor".into()));
    }
    let view_index =
        index(acc, "bufferView").ok_or_else(|| MeshError::Unsupported("accessor without bufferView".into()))?;
    let view = array(root, "bufferViews").get(view_index).ok_or_else(|| bad("no such bufferView"))?;
    let count = index(acc, "count").ok_or_else(|| bad("accessor without count"))?;
    if count > limit {
        return Err(MeshError::TooLarge);
    }
    let component =
        acc.get("componentType").and_then(Value::as_u64).ok_or_else(|| bad("accessor without componentType"))?;
    let size = component_size(component).ok_or_else(|| bad(format!("componentType {component}")))?;
    let components = match acc.get("type").and_then(Value::as_str) {
        Some("SCALAR") => 1,
        Some("VEC2") => 2,
        Some("VEC3") => 3,
        Some("VEC4") => 4,
        other => return Err(bad(format!("accessor type {other:?}"))),
    };
    let element = size * components;
    let stride = match index(view, "byteStride") {
        Some(s) if s >= element && s <= 252 => s,
        Some(_) => return Err(bad("bad byteStride")),
        None => element,
    };
    let offset = index(acc, "byteOffset").unwrap_or(0);
    let view_offset = index(view, "byteOffset").unwrap_or(0);
    let view_len = index(view, "byteLength").ok_or_else(|| bad("bufferView without byteLength"))?;
    if count > 0 {
        let end =
            (count - 1).checked_mul(stride).and_then(|n| n.checked_add(offset)).and_then(|n| n.checked_add(element));
        if end.is_none_or(|end| end > view_len) {
            return Err(bad("accessor runs past its bufferView"));
        }
    }
    let buffer = index(view, "buffer").ok_or_else(|| bad("bufferView without buffer"))?;
    Ok(Accessor {
        count,
        component,
        components,
        normalized: acc.get("normalized").and_then(Value::as_bool).unwrap_or(false),
        offset,
        stride,
        buffer,
        view_offset,
        view_len,
    })
}

/// The bytes of an accessor's bufferView.
fn view_bytes<'b>(acc: &Accessor, buffers: &'b mut Buffers<'_, '_>) -> Result<&'b [u8], MeshError> {
    let data = buffers.get(acc.buffer)?;
    let end = acc.view_offset.checked_add(acc.view_len).ok_or_else(|| bad("bufferView overflow"))?;
    data.get(acc.view_offset..end).ok_or_else(|| bad("bufferView runs past its buffer"))
}

fn component(bytes: &[u8], at: usize, component: u64, normalized: bool) -> Option<f32> {
    let b = |n: usize| bytes.get(at..at + n);
    Some(match component {
        5126 => {
            let b = b(4)?;
            f32::from_le_bytes([b[0], b[1], b[2], b[3]])
        }
        5120 => {
            let v = f32::from(i8::from_le_bytes([*b(1)?.first()?]));
            if normalized { (v / 127.0).max(-1.0) } else { v }
        }
        5121 => {
            let v = f32::from(*b(1)?.first()?);
            if normalized { v / 255.0 } else { v }
        }
        5122 => {
            let b = b(2)?;
            let v = f32::from(i16::from_le_bytes([b[0], b[1]]));
            if normalized { (v / 32767.0).max(-1.0) } else { v }
        }
        5123 => {
            let b = b(2)?;
            let v = f32::from(u16::from_le_bytes([b[0], b[1]]));
            if normalized { v / 65535.0 } else { v }
        }
        5125 => {
            let b = b(4)?;
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32
        }
        _ => return None,
    })
}

/// Read a `VEC<N>` vertex attribute of floats or (normalized) small integers.
fn read_vec<const N: usize>(
    root: &Value,
    buffers: &mut Buffers<'_, '_>,
    i: usize,
    limits: &Limits,
    name: &str,
) -> Result<Vec<[f32; N]>, MeshError> {
    let acc = accessor(root, i, limits.max_vertices)?;
    buffers.charge(acc.count)?;
    if acc.components != N || acc.component == 5125 {
        return Err(bad(format!("{name} must be a VEC{N} of floats or small integers")));
    }
    let bytes = view_bytes(&acc, buffers)?;
    let size = component_size(acc.component).unwrap_or(4);
    let mut out = Vec::with_capacity(acc.count);
    for n in 0..acc.count {
        let base = acc.offset + n * acc.stride;
        let mut p = [0.0f32; N];
        for (j, v) in p.iter_mut().enumerate() {
            *v = component(bytes, base + j * size, acc.component, acc.normalized)
                .ok_or_else(|| bad(format!("{name} read")))?;
        }
        out.push(p);
    }
    Ok(out)
}

/// A per-vertex attribute that must match `POSITION`'s count; a mismatch or a read error
/// drops the attribute (geometry still draws) rather than failing the primitive.
fn read_attribute<const N: usize>(
    root: &Value,
    buffers: &mut Buffers<'_, '_>,
    prim: &Value,
    name: &str,
    count: usize,
    limits: &Limits,
) -> Result<Option<Vec<[f32; N]>>, MeshError> {
    let Some(i) =
        prim.pointer(&format!("/attributes/{name}")).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok())
    else {
        return Ok(None);
    };
    match read_vec::<N>(root, buffers, i, limits, name) {
        Ok(v) if v.len() == count => Ok(Some(v)),
        Ok(_) | Err(MeshError::Malformed(_) | MeshError::Unsupported(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

fn read_indices(root: &Value, buffers: &mut Buffers<'_, '_>, i: usize, limits: &Limits) -> Result<Vec<u32>, MeshError> {
    let acc = accessor(root, i, limits.max_triangles.saturating_mul(3))?;
    buffers.charge(acc.count)?;
    if acc.components != 1 || !matches!(acc.component, 5121 | 5123 | 5125) {
        return Err(bad("indices must be unsigned SCALAR"));
    }
    let bytes = view_bytes(&acc, buffers)?;
    let mut out = Vec::with_capacity(acc.count);
    for n in 0..acc.count {
        let at = acc.offset + n * acc.stride;
        let v = match acc.component {
            5121 => bytes.get(at).map(|b| u32::from(*b)),
            5123 => bytes.get(at..at + 2).map(|b| u32::from(u16::from_le_bytes([b[0], b[1]]))),
            _ => bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        };
        out.push(v.ok_or_else(|| bad("index read"))?);
    }
    Ok(out)
}

// ---- primitives ----------------------------------------------------------------------------

struct Primitive {
    positions: Vec<[f32; 3]>,
    /// Object-space unit normals, one per position, when the file has them.
    normals: Option<Vec<[f32; 3]>>,
    uvs: Option<Vec<[f32; 2]>>,
    triangles: Vec<[u32; 3]>,
    color: [f32; 4],
    material: Material,
}

fn read_primitive(
    root: &Value,
    buffers: &mut Buffers<'_, '_>,
    images: &mut Images,
    mesh: &mut Mesh,
    prim: &Value,
    limits: &Limits,
) -> Result<Option<Primitive>, MeshError> {
    if prim.get("extensions").and_then(Value::as_object).is_some_and(|e| COMPRESSION.iter().any(|c| e.contains_key(*c)))
    {
        return Err(MeshError::Unsupported("compressed geometry".into()));
    }
    let mode = prim.get("mode").and_then(Value::as_u64).unwrap_or(4);
    if !matches!(mode, 4..=6) {
        return Ok(None); // points and lines have no surface to shade
    }
    let pos = prim.pointer("/attributes/POSITION").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok());
    let Some(pos) = pos else { return Ok(None) };
    let positions = read_vec::<3>(root, buffers, pos, limits, "POSITION")?;
    let normals = read_attribute::<3>(root, buffers, prim, "NORMAL", positions.len(), limits)?
        .map(|v| v.into_iter().map(unit_or_zero).collect());
    let uvs = read_attribute::<2>(root, buffers, prim, "TEXCOORD_0", positions.len(), limits)?
        .map(|v| v.into_iter().map(|[u, w]| [finite_or_zero(u), finite_or_zero(w)]).collect());
    let indices = match index(prim, "indices") {
        Some(i) => read_indices(root, buffers, i, limits)?,
        None => (0..u32::try_from(positions.len()).map_err(|_| MeshError::TooLarge)?).collect(),
    };
    let n = positions.len();
    let mut triangles = Vec::new();
    let mut push = |a: u32, b: u32, c: u32| {
        if [a, b, c].iter().all(|i| (*i as usize) < n) && a != b && b != c && a != c {
            triangles.push([a, b, c]);
        }
    };
    match mode {
        4 => {
            let (tris, _) = indices.as_chunks::<3>();
            for t in tris {
                push(t[0], t[1], t[2]);
            }
        }
        5 => {
            for (k, w) in indices.windows(3).enumerate() {
                if k % 2 == 0 { push(w[0], w[1], w[2]) } else { push(w[1], w[0], w[2]) }
            }
        }
        _ => {
            if let Some((&first, rest)) = indices.split_first() {
                for w in rest.windows(2) {
                    push(first, w[0], w[1]);
                }
            }
        }
    }
    if triangles.len() > limits.max_triangles {
        return Err(MeshError::TooLarge);
    }
    let (color, material) = material_of(root, buffers, images, mesh, index(prim, "material"));
    Ok(Some(Primitive { positions, normals, uvs, triangles, color, material }))
}

fn finite_or_zero(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

/// Normalize, or `[0.0; 3]` ("no normal") for zero, huge or non-finite input.
fn unit_or_zero(n: [f32; 3]) -> [f32; 3] {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 1e-20 && len.is_finite() { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0; 3] }
}

/// The glTF image behind `textures[texture.index]`, when the texture uses `TEXCOORD_0`.
fn texture_image(
    root: &Value,
    buffers: &mut Buffers<'_, '_>,
    images: &mut Images,
    mesh: &mut Mesh,
    texture: Option<&Value>,
) -> Option<u32> {
    let texture = texture?;
    if index(texture, "texCoord").unwrap_or(0) != 0 {
        return None;
    }
    let source = index(array(root, "textures").get(index(texture, "index")?)?, "source")?;
    images.get(root, buffers, mesh, source)
}

/// The material's base colour factor and textures. Without a material, or with only a texture
/// (which [`crate::render`] doesn't sample yet) over a white factor: [`NEUTRAL_GREY`], so an
/// untextured render doesn't glare. V0.2 turns that white once the texture is drawn.
fn material_of(
    root: &Value,
    buffers: &mut Buffers<'_, '_>,
    images: &mut Images,
    mesh: &mut Mesh,
    material: Option<usize>,
) -> ([f32; 4], Material) {
    let grey = [NEUTRAL_GREY[0], NEUTRAL_GREY[1], NEUTRAL_GREY[2], 1.0];
    let Some(m) = material.and_then(|m| array(root, "materials").get(m)) else { return (grey, Material::default()) };
    let pbr = m.get("pbrMetallicRoughness");
    let factor = numbers::<4>(pbr.and_then(|p| p.get("baseColorFactor"))).map(|c| c.map(|v| v.clamp(0.0, 1.0)));
    let base_color_image = texture_image(root, buffers, images, mesh, pbr.and_then(|p| p.get("baseColorTexture")));
    let metallic_roughness_image =
        texture_image(root, buffers, images, mesh, pbr.and_then(|p| p.get("metallicRoughnessTexture")));
    let declared_texture = pbr.and_then(|p| p.get("baseColorTexture")).is_some();
    let color = match factor {
        Some(c) if !(declared_texture && c == [1.0; 4]) => c,
        _ => grey,
    };
    let material = Material {
        base_color_image,
        metallic_roughness_image,
        double_sided: m.get("doubleSided").and_then(Value::as_bool).unwrap_or(false),
    };
    (color, material)
}

fn append(
    mesh: &mut Mesh,
    colors: &mut HashMap<([u32; 4], Material), u16>,
    prim: &Primitive,
    m: &Mat4,
    limits: &Limits,
) -> Result<(), MeshError> {
    if mesh.positions.len().saturating_add(prim.positions.len()) > limits.max_vertices
        || mesh.triangles.len().saturating_add(prim.triangles.len()) > limits.max_triangles
    {
        return Err(MeshError::TooLarge);
    }
    let world: Vec<[f32; 3]> = prim
        .positions
        .iter()
        .map(|&[x, y, z]| {
            [
                m[0] * x + m[4] * y + m[8] * z + m[12],
                m[1] * x + m[5] * y + m[9] * z + m[13],
                m[2] * x + m[6] * y + m[10] * z + m[14],
            ]
        })
        .collect();
    if world.iter().flatten().any(|v| !v.is_finite()) {
        return Ok(()); // a degenerate transform or junk floats: skip this instance
    }
    let base = u32::try_from(mesh.positions.len()).map_err(|_| MeshError::TooLarge)?;
    let color = mesh.slot_index(colors, prim.color, prim.material)?;
    let count = world.len();
    // Normals and UVs stay parallel to positions across primitives: once any primitive brings
    // one, earlier and later vertices without it get the "none" value.
    if prim.normals.is_some() || !mesh.normals.is_empty() {
        mesh.normals.resize(mesh.positions.len(), [0.0; 3]);
        match &prim.normals {
            Some(normals) => mesh.normals.extend(normals.iter().map(|n| transform_normal(m, *n))),
            None => mesh.normals.resize(mesh.positions.len() + count, [0.0; 3]),
        }
    }
    if prim.uvs.is_some() || !mesh.uvs.is_empty() {
        mesh.uvs.resize(mesh.positions.len(), [0.0; 2]);
        match &prim.uvs {
            Some(uvs) => mesh.uvs.extend_from_slice(uvs),
            None => mesh.uvs.resize(mesh.positions.len() + count, [0.0; 2]),
        }
    }
    mesh.positions.extend(world);
    mesh.triangles.extend(prim.triangles.iter().map(|t| t.map(|i| i + base)));
    mesh.triangle_color.extend(std::iter::repeat_n(color, prim.triangles.len()));
    Ok(())
}

/// A normal through the upper-left 3×3 of `m`, re-normalized. Exact for rotations and uniform
/// scale (the case for every glTF exporter we know of); a non-uniform scale skews it slightly,
/// which only shades a little wrong.
fn transform_normal(m: &Mat4, [x, y, z]: [f32; 3]) -> [f32; 3] {
    unit_or_zero([m[0] * x + m[4] * y + m[8] * z, m[1] * x + m[5] * y + m[9] * z, m[2] * x + m[6] * y + m[10] * z])
}
