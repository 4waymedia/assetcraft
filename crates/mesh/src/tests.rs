use super::*;
use crate::gltf::relative_path;

const CUBE_OBJ: &str = "# cube\nv -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\n\
f 1 2 3 4\nf 5 8 7 6\nf 1 5 6 2\nf 2 6 7 3\nf 3 7 8 4\nf 5 1 4 8\n";

fn no_files(_: &str, _: usize) -> Result<Vec<u8>, String> {
    Err("no external files in this test".into())
}

/// One triangle, as little-endian f32 positions + u16 indices (padded to 4).
fn triangle_bin() -> Vec<u8> {
    let mut bin = Vec::new();
    for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        bin.extend_from_slice(&v.to_le_bytes());
    }
    for i in [0u16, 1, 2, 0] {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    bin
}

fn triangle_json(uri: Option<&str>, extra: &str) -> String {
    let uri = uri.map(|u| format!(r#","uri":"{u}""#)).unwrap_or_default();
    format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
        "nodes":[{{"mesh":0,"translation":[5,0,0]}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1,"material":0}}]}}],
        "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.8,0.1,0.1,1]}}}}],
        "buffers":[{{"byteLength":42{uri}}}],
        "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":6}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
                     {{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}]{extra}}}"#
    )
}

fn glb(json: &str, bin: &[u8]) -> Vec<u8> {
    let mut j = json.as_bytes().to_vec();
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let mut b = bin.to_vec();
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
    let total = 12 + 8 + j.len() + 8 + b.len();
    let mut out = b"glTF".to_vec();
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&j);
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&b);
    out
}

fn b64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in bytes.chunks(3) {
        let n =
            (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= c.len() {
                s.push(A[((n >> (18 - 6 * k)) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[test]
fn obj_cube_parses_and_renders_a_centered_opaque_shape() {
    let mesh = parse_obj(CUBE_OBJ.as_bytes(), &Limits::default()).unwrap();
    assert_eq!((mesh.positions.len(), mesh.triangle_count()), (8, 12));
    let image = render(&mesh, View::default(), &RenderOptions::new(64, 64)).unwrap();
    assert_eq!(image.rgba.len(), 64 * 64 * 4);
    let alpha = |x: usize, y: usize| image.rgba[(y * 64 + x) * 4 + 3];
    assert_eq!(alpha(32, 32), 255, "the cube covers the centre");
    assert_eq!(alpha(0, 0), 0, "background stays transparent");
    // Three faces are visible from the 3/4 view and are shaded differently.
    let mut greys: Vec<u8> = image.rgba.as_chunks::<4>().0.iter().filter(|p| p[3] == 255).map(|p| p[0]).collect();
    greys.sort_unstable();
    greys.dedup();
    assert!(greys.len() >= 3, "{greys:?}");
    let cropped = image.crop_to_content(0.02);
    assert!(cropped.width < 64 && cropped.height < 64 && cropped.width > 20);
    assert_eq!(cropped.rgba.len(), (cropped.width * cropped.height * 4) as usize);
}

#[test]
fn obj_handles_negative_and_slashed_indices_and_skips_junk() {
    let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nf -3/1/1 -2/1/1 -1/1/1\nf 1 2 99\nf a b c\nv nan 0 0\nfoo\n";
    let mesh = parse_obj(text.as_bytes(), &Limits::default()).unwrap();
    assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
    assert_eq!(mesh.positions[3], [0.0; 3], "a NaN vertex keeps its slot as the origin");
    assert_eq!(parse_obj(b"v 0 0 0\n", &Limits::default()), Err(MeshError::Empty));
    assert_eq!(parse_obj(&[0xFF, 0xFE, 0x00], &Limits::default()), Err(MeshError::Empty));
    let tiny = Limits { max_vertices: 4, ..Limits::default() };
    assert_eq!(parse_obj(CUBE_OBJ.as_bytes(), &tiny), Err(MeshError::TooLarge));
}

#[test]
fn glb_with_bin_chunk_applies_node_transform_and_material() {
    let bytes = glb(&triangle_json(None, ""), &triangle_bin());
    let mesh = parse_glb(&bytes, &Limits::default(), &mut no_files).unwrap();
    assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
    assert_eq!(mesh.positions[1], [6.0, 0.0, 0.0], "translation applied");
    assert_eq!(mesh.colors[mesh.triangle_color[0] as usize], [0.8, 0.1, 0.1, 1.0]);
    let image = render(&mesh, View::default(), &RenderOptions::new(32, 32)).unwrap();
    assert!(image.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0 && p[0] > p[1]), "drawn in red");
}

#[test]
fn gltf_with_data_uri_and_external_buffers() {
    let uri = format!("data:application/octet-stream;base64,{}", b64(&triangle_bin()));
    let mesh = parse_gltf(triangle_json(Some(&uri), "").as_bytes(), &Limits::default(), &mut no_files).unwrap();
    assert_eq!(mesh.triangle_count(), 1);

    let mut asked = Vec::new();
    let mut resolve = |path: &str, len: usize| {
        asked.push((path.to_string(), len));
        Ok(triangle_bin())
    };
    let json = triangle_json(Some("sub/my%20mesh.bin"), "");
    assert_eq!(parse_gltf(json.as_bytes(), &Limits::default(), &mut resolve).unwrap().triangle_count(), 1);
    assert_eq!(asked, vec![("sub/my mesh.bin".to_string(), 42)]);

    for evil in ["../secret.bin", "/etc/passwd", "C:/x.bin", "http://example.com/x.bin", "a/../../b", "%2e%2e/x"] {
        let mut called = false;
        let mut resolve = |_: &str, _: usize| {
            called = true;
            Ok(Vec::new())
        };
        let r = parse_gltf(triangle_json(Some(evil), "").as_bytes(), &Limits::default(), &mut resolve);
        assert!(matches!(r, Err(MeshError::External(_))), "{evil}: {r:?}");
        assert!(!called, "{evil} never reaches the resolver");
    }
    assert_eq!(relative_path("./a\\b.bin").as_deref(), Some("a/b.bin"));
}

#[test]
fn hostile_gltf_is_an_error_not_a_panic() {
    let limits = Limits::default();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("magic only", b"glTF".to_vec()),
        ("v1", [b"glTF".as_slice(), &1u32.to_le_bytes(), &20u32.to_le_bytes()].concat()),
        (
            "chunk past end",
            [b"glTF".as_slice(), &2u32.to_le_bytes(), &100u32.to_le_bytes(), &999u32.to_le_bytes(), b"JSON"].concat(),
        ),
        ("not json", glb("{nope", &[])),
        ("json array", glb("[1,2]", &[])),
        ("short bin", glb(&triangle_json(None, ""), &[0; 8])),
        (
            "huge count",
            glb(
                &triangle_json(None, "").replace(r#""count":3,"type":"VEC3""#, r#""count":4000000000,"type":"VEC3""#),
                &triangle_bin(),
            ),
        ),
        (
            "past view",
            glb(
                &triangle_json(None, "").replace(r#""count":3,"type":"VEC3""#, r#""count":4,"type":"VEC3""#),
                &triangle_bin(),
            ),
        ),
        (
            "bad stride",
            glb(
                &triangle_json(None, "").replace(r#""byteLength":36}"#, r#""byteLength":36,"byteStride":4}"#),
                &triangle_bin(),
            ),
        ),
        (
            "giant buffer",
            glb(&triangle_json(None, "").replace(r#""byteLength":42"#, r#""byteLength":99999999999"#), &triangle_bin()),
        ),
        (
            "draco",
            glb(&triangle_json(None, r#","extensionsRequired":["KHR_draco_mesh_compression"]"#), &triangle_bin()),
        ),
    ];
    for (name, bytes) in cases {
        let r = parse_glb(&bytes, &limits, &mut no_files);
        assert!(r.is_err(), "{name}: {r:?}");
    }
    // Out-of-range indices are dropped; with nothing left the mesh is empty.
    let mut bin = triangle_bin();
    bin[36..38].copy_from_slice(&999u16.to_le_bytes());
    assert_eq!(parse_glb(&glb(&triangle_json(None, ""), &bin), &limits, &mut no_files), Err(MeshError::Empty));
}

#[test]
fn node_cycles_and_instancing_are_bounded() {
    let cyclic = triangle_json(None, "").replace(r#"{"mesh":0,"translation":[5,0,0]}"#, r#"{"mesh":0,"children":[0]}"#);
    let r = parse_glb(&glb(&cyclic, &triangle_bin()), &Limits::default(), &mut no_files);
    assert!(r.is_ok_and(|m| m.triangle_count() <= 65), "depth cap stops the cycle");
    let small = Limits { max_node_visits: 10, ..Limits::default() };
    let wide = triangle_json(None, "").replace(r#""nodes":[0]}"#, r#""nodes":[0,0,0,0,0,0,0,0,0,0,0,0]}"#);
    assert_eq!(parse_glb(&glb(&wide, &triangle_bin()), &small, &mut no_files), Err(MeshError::TooLarge));
    let few = Limits { max_vertices: 6, ..Limits::default() };
    assert_eq!(parse_glb(&glb(&wide, &triangle_bin()), &few, &mut no_files), Err(MeshError::TooLarge));
}

#[test]
fn render_is_bounded_and_views_stay_in_range() {
    let mesh = parse_obj(CUBE_OBJ.as_bytes(), &Limits::default()).unwrap();
    assert_eq!(render(&mesh, View::default(), &RenderOptions::new(0, 10)), Err(MeshError::TooLarge));
    assert_eq!(render(&mesh, View::default(), &RenderOptions::new(9000, 10)), Err(MeshError::TooLarge));
    // 4096² at 4× would be 268 M samples: supersampling drops until it fits.
    let big = RenderOptions { supersample: 4, ..RenderOptions::new(4096, 4096) };
    assert_eq!(render(&mesh, View::default(), &big).map(|i| i.rgba.len()), Ok(4096 * 4096 * 4));
    // A pixel-test budget of zero draws nothing and still returns an image.
    let none = RenderOptions { max_pixel_tests: 0, ..RenderOptions::new(16, 16) };
    assert!(render(&mesh, View::default(), &none).unwrap().rgba.iter().all(|b| *b == 0));
    let v = View::default().orbit(100.0, 100.0).zoomed(1e9);
    assert!(v.yaw.abs() <= std::f32::consts::PI && v.pitch == View::MAX_PITCH && v.zoom == View::ZOOM_RANGE.1);
    assert_eq!(View::default().orbit(f32::NAN, 0.0).yaw, View::THREE_QUARTER.yaw);
    // Straight down still renders.
    let top = render(&mesh, View::default().orbit(0.0, 9.0), &RenderOptions::new(32, 32)).unwrap();
    assert_eq!(top.rgba[(16 * 32 + 16) * 4 + 3], 255);
}

/// `prims` copies of the triangle primitive (all sharing accessors 0 and 1, alternating two
/// materials) in one mesh, placed by `instances` nodes.
fn shared_accessor_json(prims: usize, instances: usize) -> String {
    let prim: Vec<String> =
        (0..prims).map(|k| format!(r#"{{"attributes":{{"POSITION":0}},"indices":1,"material":{}}}"#, k % 2)).collect();
    let nodes: Vec<String> = (0..instances).map(|k| format!(r#"{{"mesh":0,"translation":[{k},0,0]}}"#)).collect();
    let roots: Vec<String> = (0..instances).map(|k| k.to_string()).collect();
    triangle_json(None, "")
        .replace(r#""nodes":[0]}"#, &format!(r#""nodes":[{}]}}"#, roots.join(",")))
        .replace(r#""nodes":[{"mesh":0,"translation":[5,0,0]}]"#, &format!(r#""nodes":[{}]"#, nodes.join(",")))
        .replace(
            r#""primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]"#,
            &format!(r#""primitives":[{}]"#, prim.join(",")),
        )
        .replace(
            r#""materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.8,0.1,0.1,1]}}]"#,
            r#""materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.8,0.1,0.1,1]}},{"pbrMetallicRoughness":{"baseColorFactor":[0.1,0.8,0.1,1]}}]"#,
        )
}

/// The same file with accessor 1 pointing at indices `0,0,0` (a degenerate, dropped triangle).
fn degenerate(json: &str, bin: &[u8]) -> (String, Vec<u8>) {
    let json = json
        .replace(r#""byteLength":42"#, r#""byteLength":48"#)
        .replace(r#"{"buffer":0,"byteOffset":36,"byteLength":6}"#, r#"{"buffer":0,"byteOffset":42,"byteLength":6}"#);
    let mut bin = bin.to_vec();
    bin.truncate(42);
    bin.extend_from_slice(&[0u8; 6]);
    (json, bin)
}

#[test]
fn primitives_sharing_an_accessor_count_against_the_whole_budget() {
    let bin = triangle_bin();
    // 20 primitives × 1 triangle, one instance: the cache itself goes over 10 triangles.
    let tight = Limits { max_triangles: 10, ..Limits::default() };
    let json = shared_accessor_json(20, 1);
    assert_eq!(parse_glb(&glb(&json, &bin), &tight, &mut no_files), Err(MeshError::TooLarge));
    // Every accessor read is charged (3 positions + 3 indices per primitive), even when the
    // primitive turns out empty: 9 × 6 > 20 + 3 × 10.
    let few = Limits { max_vertices: 20, max_triangles: 10, ..Limits::default() };
    let (empty9, zeros) = degenerate(&shared_accessor_json(9, 1), &bin);
    assert_eq!(parse_glb(&glb(&empty9, &zeros), &few, &mut no_files), Err(MeshError::TooLarge));
    let (empty8, zeros) = degenerate(&shared_accessor_json(8, 1), &bin);
    assert_eq!(parse_glb(&glb(&empty8, &zeros), &few, &mut no_files), Err(MeshError::Empty), "8 × 6 ≤ 50");
    let ok = parse_glb(&glb(&json, &bin), &Limits::default(), &mut no_files).unwrap();
    assert_eq!(ok.triangle_count(), 20);
    assert_eq!(ok.colors.len(), 2, "colours are deduplicated by value");
}

#[test]
fn instances_times_primitives_are_a_bounded_amount_of_work() {
    let bin = triangle_bin();
    let small = Limits { max_node_visits: 50, ..Limits::default() };
    // 30 primitives placed by 2 nodes: 30 reads + 2 × (1 + 30) placements > 50.
    let busy = shared_accessor_json(30, 2);
    assert_eq!(parse_glb(&glb(&busy, &bin), &small, &mut no_files), Err(MeshError::TooLarge));
    assert_eq!(parse_glb(&glb(&busy, &bin), &Limits::default(), &mut no_files).map(|m| m.triangle_count()), Ok(60));
    // Primitives without triangles aren't cached, so each instance of them costs one unit:
    // 30 reads + 15 instances ≤ 50.
    let (empty, zeros) = degenerate(&shared_accessor_json(30, 15), &bin);
    assert_eq!(parse_glb(&glb(&empty, &zeros), &small, &mut no_files), Err(MeshError::Empty));
}

// ---- V0.1: normals, texture coordinates, materials and images --------------------------------

/// The generated textured cube (`test_data/make_model_fixtures.py`).
fn rune_crate() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test_data/library/props/models/rune_crate_textured.glb"))
        .unwrap()
}

/// A triangle with NORMAL and TEXCOORD_0 (float), a base colour PNG in the BIN chunk and
/// `doubleSided`, in the layout TRELLIS exports use. Positions 0..36, normals 36..72,
/// UVs 72..96, indices 96..102 (+2 pad), image at 104.
fn textured_triangle(image: &[u8], image_json: &str, material_extra: &str) -> (String, Vec<u8>) {
    let mut bin = Vec::new();
    for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        bin.extend_from_slice(&v.to_le_bytes());
    }
    for v in [0.0f32, 0.0, 2.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0] {
        bin.extend_from_slice(&v.to_le_bytes()); // unnormalized, and one zero normal
    }
    for v in [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0] {
        bin.extend_from_slice(&v.to_le_bytes());
    }
    for i in [0u16, 1, 2, 0] {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    bin.extend_from_slice(image);
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
        "nodes":[{{"mesh":0,"rotation":[0,0.7071068,0,0.7071068]}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":2,"TEXCOORD_0":3}},"indices":1,"material":0}}]}}],
        "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}},"metallicRoughnessTexture":{{"index":0}},"metallicFactor":1,"roughnessFactor":1}}{material_extra}}}],
        "textures":[{{"source":0}}],
        "images":[{image_json}],
        "buffers":[{{"byteLength":{len}}}],
        "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":96,"byteLength":6}},
                       {{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":24}},
                       {{"buffer":0,"byteOffset":104,"byteLength":{ilen}}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
                     {{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}},
                     {{"bufferView":2,"componentType":5126,"count":3,"type":"VEC3"}},
                     {{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}]}}"#,
        len = bin.len(),
        ilen = image.len(),
    );
    (json, bin)
}

#[test]
fn normals_uvs_and_embedded_images_are_read_and_transformed() {
    let png = b"\x89PNG\r\n\x1a\nfake-but-bounded";
    let (json, bin) = textured_triangle(png, r#"{"bufferView":4,"mimeType":"image/png"}"#, r#","doubleSided":true"#);
    let mesh = parse_glb(&glb(&json, &bin), &Limits::default(), &mut no_files).unwrap();
    assert_eq!(mesh.normals.len(), 3);
    assert_eq!(mesh.uvs, vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    // +Z rotated 90° about Y is +X; the unnormalized [0,0,2] came out unit length.
    let n = mesh.normals[0];
    assert!((n[0] - 1.0).abs() < 1e-5 && n[1].abs() < 1e-5 && n[2].abs() < 1e-5, "{n:?}");
    assert_eq!(mesh.normals[2], [0.0; 3], "a zero normal stays 'none'");
    assert_eq!(mesh.images.len(), 1, "the image is copied once though two textures use it");
    assert_eq!(mesh.images[0].mime, "image/png");
    assert_eq!(mesh.images[0].bytes, png);
    let m = mesh.material_of(0);
    assert_eq!(m, Material { base_color_image: Some(0), metallic_roughness_image: Some(0), double_sided: true });
    assert_eq!(
        mesh.colors[0],
        [NEUTRAL_GREY[0], NEUTRAL_GREY[1], NEUTRAL_GREY[2], 1.0],
        "grey until V0.2 samples the texture"
    );
    assert_eq!(mesh.materials.len(), mesh.colors.len());
    assert!(mesh.cost() >= mesh.image_bytes() + 3 * 12 + 3 * 8);
    // Still renders, pixel-identical to before V0.1 (the texture isn't sampled yet).
    let image = render(&mesh, View::default(), &RenderOptions::new(24, 24)).unwrap();
    assert!(image.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
}

#[test]
fn images_are_bounded_and_bad_image_references_leave_the_geometry() {
    let png = b"\x89PNG\r\n\x1a\nfake";
    let cases: [(&str, &str, Limits); 5] = [
        (
            "over the image limit",
            r#"{"bufferView":4,"mimeType":"image/png"}"#,
            Limits { max_image_bytes: 4, ..Limits::default() },
        ),
        ("missing bufferView", r#"{"bufferView":9,"mimeType":"image/png"}"#, Limits::default()),
        ("external file refused by the resolver", r#"{"uri":"tex.png"}"#, Limits::default()),
        ("escaping uri", r#"{"uri":"../tex.png"}"#, Limits::default()),
        ("non-base64 data uri", r#"{"uri":"data:image/png,raw"}"#, Limits::default()),
    ];
    for (name, image_json, limits) in cases {
        let (json, bin) = textured_triangle(png, image_json, "");
        let mesh = parse_glb(&glb(&json, &bin), &limits, &mut no_files).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(mesh.triangles.len(), 1, "{name}");
        assert!(mesh.images.is_empty(), "{name}");
        assert_eq!(mesh.material_of(0).base_color_image, None, "{name}");
        assert_eq!(
            mesh.colors[0],
            [NEUTRAL_GREY[0], NEUTRAL_GREY[1], NEUTRAL_GREY[2], 1.0],
            "{name}: untextured → grey"
        );
    }
    // A data-URI image works and takes its MIME type from the URI.
    let (json, bin) = textured_triangle(png, &format!(r#"{{"uri":"data:image/jpeg;base64,{}"}}"#, b64(png)), "");
    let mesh = parse_glb(&glb(&json, &bin), &Limits::default(), &mut no_files).unwrap();
    assert_eq!(mesh.images[0].mime, "image/jpeg");
    assert_eq!(mesh.images[0].bytes, png);
    // A texture on another UV set is not used (we read TEXCOORD_0 only).
    let (json, bin) = textured_triangle(png, r#"{"bufferView":4,"mimeType":"image/png"}"#, "");
    let json = json.replace(r#""baseColorTexture":{"index":0}"#, r#""baseColorTexture":{"index":0,"texCoord":1}"#);
    let mesh = parse_glb(&glb(&json, &bin), &Limits::default(), &mut no_files).unwrap();
    assert_eq!(mesh.material_of(0).base_color_image, None);
    assert_eq!(mesh.material_of(0).metallic_roughness_image, Some(0));
}

#[test]
fn attributes_that_do_not_match_positions_are_dropped_not_fatal() {
    let png = b"\x89PNG\r\n\x1a\nfake";
    let (json, bin) = textured_triangle(png, r#"{"bufferView":4,"mimeType":"image/png"}"#, "");
    // NORMAL with 2 elements instead of 3, TEXCOORD_0 pointing at the indices (SCALAR u16).
    let json = json
        .replace(
            r#"{"bufferView":2,"componentType":5126,"count":3,"type":"VEC3"}"#,
            r#"{"bufferView":2,"componentType":5126,"count":2,"type":"VEC3"}"#,
        )
        .replace(r#""TEXCOORD_0":3"#, r#""TEXCOORD_0":1"#);
    let mesh = parse_glb(&glb(&json, &bin), &Limits::default(), &mut no_files).unwrap();
    assert_eq!(mesh.triangles.len(), 1);
    assert!(mesh.normals.is_empty() && mesh.uvs.is_empty());
}

#[test]
fn mixed_primitives_keep_normals_and_uvs_parallel_to_positions() {
    // The textured triangle plus a second, bare triangle instance of the plain mesh.
    let png = b"\x89PNG\r\n\x1a\nfake";
    let (json, bin) = textured_triangle(png, r#"{"bufferView":4,"mimeType":"image/png"}"#, "");
    let json = json
        .replace(r#""scenes":[{"nodes":[0]}]"#, r#""scenes":[{"nodes":[0,1]}]"#)
        .replace(r#""nodes":[{"mesh":0,"rotation":[0,0.7071068,0,0.7071068]}]"#, r#""nodes":[{"mesh":0},{"mesh":1,"translation":[3,0,0]}]"#)
        .replace(
            r#""meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":2,"TEXCOORD_0":3},"indices":1,"material":0}]}]"#,
            r#""meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":2,"TEXCOORD_0":3},"indices":1,"material":0}]},{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}]"#,
        );
    let mesh = parse_glb(&glb(&json, &bin), &Limits::default(), &mut no_files).unwrap();
    assert_eq!(mesh.positions.len(), 6);
    assert_eq!(mesh.normals.len(), 6);
    assert_eq!(mesh.uvs.len(), 6);
    assert_eq!(mesh.normals[3..], [[0.0; 3]; 3], "the bare primitive has 'none' normals");
    assert_eq!(mesh.uvs[3..], [[0.0; 2]; 3]);
    assert_eq!(mesh.colors.len(), 2, "textured and untextured slots are distinct");
    assert_eq!(mesh.material_of(0).base_color_image, Some(0));
    assert_eq!(mesh.material_of(1), Material::default());
    // Reverse order: the bare mesh first, then the textured one, still parallel.
    let json = json.replace(r#""scenes":[{"nodes":[0,1]}]"#, r#""scenes":[{"nodes":[1,0]}]"#);
    let mesh = parse_glb(&glb(&json, &bin), &Limits::default(), &mut no_files).unwrap();
    assert_eq!((mesh.normals.len(), mesh.uvs.len()), (6, 6));
    assert_eq!(mesh.normals[..3], [[0.0; 3]; 3]);
    assert!(mesh.normals[3..].iter().all(|n| n[2] > 0.9 || *n == [0.0; 3]), "{:?}", &mesh.normals[3..]);
}

#[test]
fn the_textured_cube_fixture_has_everything_v0_2_will_draw() {
    let mesh = parse_glb(&rune_crate(), &Limits::default(), &mut no_files).unwrap();
    assert_eq!(mesh.positions.len(), 24);
    assert_eq!(mesh.triangles.len(), 12);
    assert_eq!(mesh.normals.len(), 24);
    assert_eq!(mesh.uvs.len(), 24);
    assert!(mesh.normals.iter().all(|n| (n[0] * n[0] + n[1] * n[1] + n[2] * n[2] - 1.0).abs() < 1e-4));
    assert_eq!(mesh.images.len(), 2);
    assert!(mesh.images.iter().all(|i| i.mime == "image/png" && i.bytes.starts_with(b"\x89PNG")));
    assert_eq!(
        mesh.material_of(0),
        Material { base_color_image: Some(0), metallic_roughness_image: Some(1), double_sided: false }
    );
    let image = render(&mesh, View::default(), &RenderOptions::new(48, 48)).unwrap();
    assert!(image.rgba.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count() > 200);
}

// ---- V0.2: shading, textures, display modes, background ---------------------------------------

fn pixels(image: &Image) -> impl Iterator<Item = &[u8; 4]> {
    image.rgba.as_chunks::<4>().0.iter()
}

fn solid(opts: RenderOptions) -> RenderOptions {
    RenderOptions { background: Background::Solid([20, 20, 20]), ..opts }
}

/// A square in the XY plane (two triangles, four corners) with per-corner normals that lean
/// left on the left and right on the right, so smooth shading makes a gradient across it.
fn leaning_quad() -> Mesh {
    let lean = |x: f32| {
        let n = [x * 0.8, 0.0, 0.6];
        let l = (n[0] * n[0] + n[2] * n[2]).sqrt();
        [n[0] / l, 0.0, n[2] / l]
    };
    Mesh {
        positions: vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        triangle_color: vec![0, 0],
        colors: vec![[1.0; 4]],
        normals: vec![lean(-1.0), lean(1.0), lean(1.0), lean(-1.0)],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        materials: vec![Material { base_color_image: Some(0), ..Material::default() }],
        images: vec![ImageData { mime: "image/png".into(), bytes: vec![1, 2, 3] }],
    }
}

const FRONT: View = View { yaw: 0.0, pitch: 0.0, zoom: 1.0 };

#[test]
fn smooth_shading_makes_a_gradient_and_flat_does_not() {
    let mut mesh = leaning_quad();
    mesh.colors[0] = [0.4, 0.4, 0.4, 1.0];
    let opts = RenderOptions::new(32, 32);
    let smooth = render(&mesh, FRONT, &opts).unwrap();
    let flat = render(&mesh, FRONT, &RenderOptions { smooth: false, ..opts }).unwrap();
    let row = |img: &Image| (8..24).map(|x| img.rgba[(16 * 32 + x) * 4]).collect::<Vec<u8>>();
    let (s, f) = (row(&smooth), row(&flat));
    let distinct = |v: &[u8]| v.iter().collect::<std::collections::BTreeSet<_>>().len();
    assert!(distinct(&s) > 4, "smooth: a gradient across the face {s:?}");
    assert_eq!(distinct(&f), 1, "flat: one shade across the face {f:?}");
}

#[test]
fn textures_are_sampled_bilinearly_with_wrap_and_multiply_the_colour() {
    let mesh = leaning_quad();
    // 2 × 1: red on the left, blue on the right.
    let tex = Texture::new(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
    let opts = RenderOptions {
        smooth: false,
        lighting: Lighting { ambient: 1.0, key: 0.0, fill: 0.0, rim: 0.0, exposure: 1.0 },
        ..RenderOptions::new(32, 32)
    };
    let img = render_with(&mesh, &[Some(tex.clone())], FRONT, &opts).unwrap();
    // Along the middle row: a pure red pixel (the red texel's centre, u = 0.25), a pure blue
    // one to its right (u = 0.75), and a blend half-way between them (u = 0.5).
    let px = |x: usize| img.rgba[(16 * 32 + x) * 4..(16 * 32 + x) * 4 + 3].to_vec();
    let red = (0..32).find(|x| px(*x)[0] > 230 && px(*x)[2] < 40).expect("a pure red pixel");
    let blue = (0..32).rev().find(|x| px(*x)[2] > 230 && px(*x)[0] < 40).expect("a pure blue pixel");
    assert!(red < blue, "red {red} left of blue {blue}");
    let mid = px((red + blue) / 2);
    assert!(mid[0] > 100 && mid[2] > 100, "the middle blends: {mid:?}");
    // Shifting every uv by a whole number changes nothing (wrap).
    let mut shifted = mesh.clone();
    for uv in &mut shifted.uvs {
        uv[0] += 3.0;
        uv[1] -= 2.0;
    }
    assert_eq!(render_with(&shifted, &[Some(tex.clone())], FRONT, &opts).unwrap(), img);
    // A missing texture, or one that would not decode, draws the colour alone (white here).
    let plain = render_with(&mesh, &[None], FRONT, &opts).unwrap();
    assert!(px(red) != plain.rgba[(16 * 32 + red) * 4..(16 * 32 + red) * 4 + 3]);
    assert_eq!(render(&mesh, FRONT, &opts).unwrap(), plain);
    // The colour multiplies the texture.
    let mut tinted = mesh.clone();
    tinted.colors[0] = [0.0, 1.0, 0.0, 1.0];
    let green = render_with(&tinted, &[Some(tex)], FRONT, &opts).unwrap();
    assert!(
        pixels(&green).filter(|p| p[3] > 0).all(|p| p[0] < 8 && p[2] < 8),
        "red × green = black, blue × green = black"
    );
    assert!(Texture::new(2, 1, vec![0; 7]).is_none() && Texture::new(0, 1, vec![]).is_none());
}

#[test]
fn display_modes_and_backgrounds_render_as_specified() {
    let mesh = parse_glb(&rune_crate(), &Limits::default(), &mut no_files).unwrap();
    let base = RenderOptions::new(48, 48);
    // Normals: the cube is rotated 30° about Y, so some face normal has z ≈ 0.87 → blue ≈ 238.
    let normals = render(&mesh, View::default(), &RenderOptions { mode: DisplayMode::Normals, ..base }).unwrap();
    assert!(pixels(&normals).any(|p| p[3] == 255 && p[2] > 225 && (p[1] as i32 - 128).abs() < 6), "a +Z-ish face");
    assert_eq!(
        render(
            &mesh,
            View::default(),
            &RenderOptions {
                mode: DisplayMode::Normals,
                lighting: Lighting { exposure: 0.0, ..Lighting::STUDIO },
                ..base
            }
        )
        .unwrap(),
        normals,
        "normals are unlit"
    );
    // Wireframe: edge pixels in the edge colour, the rest dimmed.
    let wire = render(&mesh, View::default(), &RenderOptions { mode: DisplayMode::Wireframe, ..base }).unwrap();
    assert!(pixels(&wire).any(|p| p[3] == 255 && p[0] > 200 && p[2] > 230), "edge pixels");
    assert!(pixels(&wire).filter(|p| p[3] == 255).any(|p| p[0] < 90), "dimmed surface pixels");
    // UV checker: light and dark cells; without uvs, magenta.
    let checker = render(&mesh, View::default(), &RenderOptions { mode: DisplayMode::UvChecker, ..base }).unwrap();
    let lum: Vec<u32> =
        pixels(&checker).filter(|p| p[3] == 255).map(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])).collect();
    assert!(lum.iter().max().unwrap() - lum.iter().min().unwrap() > 200, "cells differ");
    let mut bare = mesh.clone();
    bare.uvs.clear();
    let magenta = render(&bare, View::default(), &RenderOptions { mode: DisplayMode::UvChecker, ..base }).unwrap();
    assert!(pixels(&magenta).filter(|p| p[3] == 255).all(|p| p[0] > p[1] + 40 && p[2] > p[1] + 40), "magenta");
    // Background: transparent corners by default; solid everywhere with alpha 255.
    let shaded = render(&mesh, View::default(), &base).unwrap();
    assert_eq!(shaded.rgba[3], 0);
    let on_solid = render(&mesh, View::default(), &solid(base)).unwrap();
    assert!(pixels(&on_solid).all(|p| p[3] == 255));
    assert_eq!(&on_solid.rgba[..4], &[20, 20, 20, 255]);
    // Exposure 0 is black; the untextured call equals an empty texture list.
    let dark = render(
        &mesh,
        View::default(),
        &RenderOptions { lighting: Lighting { exposure: 0.0, ..Lighting::STUDIO }, ..base },
    )
    .unwrap();
    assert!(pixels(&dark).filter(|p| p[3] > 0).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
    assert_eq!(render_with(&mesh, &[], View::default(), &base).unwrap(), shaded);
}
