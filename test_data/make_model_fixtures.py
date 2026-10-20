"""Generate procedural 3D model samples for AssetCraft (CC0, no third-party content).

Writes into <library>/props/models/:
  soul_orb_icosphere.glb   GLB: icosphere on a cube pedestal (two nodes, TRS transforms,
                           two baseColorFactor materials, u16 indices, BIN chunk)
  scythe_ring.gltf         glTF JSON with an embedded base64 data-URI buffer (torus)
  tombstone.obj            Wavefront OBJ: quads, a pentagon, negative indices, v/vt/vn faces
  rune_crate_textured.glb  GLB: a cube with per-face NORMAL and TEXCOORD_0, a 64x64 PNG base
                           colour and a 16x16 metallic-roughness PNG embedded as bufferViews
                           (the shape TRELLIS exports have), u16 indices, BIN chunk

Standard library only, deterministic output. Usage: python make_model_fixtures.py library
"""
import base64, json, math, os, struct, sys, zlib


def icosphere(subdiv):
    t = (1 + 5 ** 0.5) / 2
    verts = [(-1, t, 0), (1, t, 0), (-1, -t, 0), (1, -t, 0), (0, -1, t), (0, 1, t),
             (0, -1, -t), (0, 1, -t), (t, 0, -1), (t, 0, 1), (-t, 0, -1), (-t, 0, 1)]
    verts = [tuple(c / math.sqrt(sum(x * x for x in v)) for c in v) for v in verts]
    faces = [(0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11), (1, 5, 9), (5, 11, 4),
             (11, 10, 2), (10, 7, 6), (7, 1, 8), (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8),
             (3, 8, 9), (4, 9, 5), (2, 4, 11), (6, 2, 10), (8, 6, 7), (9, 8, 1)]
    for _ in range(subdiv):
        cache, out = {}, []

        def mid(a, b):
            key = (min(a, b), max(a, b))
            if key not in cache:
                m = [(verts[a][k] + verts[b][k]) / 2 for k in range(3)]
                n = math.sqrt(sum(x * x for x in m))
                verts.append(tuple(x / n for x in m))
                cache[key] = len(verts) - 1
            return cache[key]

        for a, b, c in faces:
            ab, bc, ca = mid(a, b), mid(b, c), mid(c, a)
            out += [(a, ab, ca), (b, bc, ab), (c, ca, bc), (ab, bc, ca)]
        faces = out
    return verts, faces


def cube():
    v = [(x, y, z) for x in (-1, 1) for y in (-1, 1) for z in (-1, 1)]
    q = [(0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)]
    return v, [t for a, b, c, d in q for t in ((a, b, c), (a, c, d))]


def torus(R, r, nu, nv):
    verts, faces = [], []
    for i in range(nu):
        u = 2 * math.pi * i / nu
        for j in range(nv):
            w = 2 * math.pi * j / nv
            verts.append(((R + r * math.cos(w)) * math.cos(u), r * math.sin(w), (R + r * math.cos(w)) * math.sin(u)))
    for i in range(nu):
        for j in range(nv):
            a, b = i * nv + j, ((i + 1) % nu) * nv + j
            c, d = ((i + 1) % nu) * nv + (j + 1) % nv, i * nv + (j + 1) % nv
            faces += [(a, b, c), (a, c, d)]
    return verts, faces


def pack(verts, faces):
    """Positions (f32) + u16 indices, each 4-byte aligned, plus accessor min/max."""
    pos = b"".join(struct.pack("<3f", *v) for v in verts)
    idx = b"".join(struct.pack("<3H", *f) for f in faces)
    idx += b"\0" * (-len(idx) % 4)
    lo = [min(v[k] for v in verts) for k in range(3)]
    hi = [max(v[k] for v in verts) for k in range(3)]
    return pos, idx, lo, hi


def document(parts, buffer_uri=None):
    """parts: list of (verts, faces, color, node) → (json dict, bin bytes)."""
    blob, views, accessors, meshes, materials, nodes = b"", [], [], [], [], []
    for verts, faces, color, node in parts:
        pos, idx, lo, hi = pack(verts, faces)
        views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(pos), "target": 34962})
        blob += pos
        views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(faces) * 6, "target": 34963})
        blob += idx
        accessors.append({"bufferView": len(views) - 2, "componentType": 5126, "count": len(verts), "type": "VEC3", "min": lo, "max": hi})
        accessors.append({"bufferView": len(views) - 1, "componentType": 5123, "count": len(faces) * 3, "type": "SCALAR"})
        materials.append({"pbrMetallicRoughness": {"baseColorFactor": list(color), "metallicFactor": 0.0}})
        meshes.append({"primitives": [{"attributes": {"POSITION": len(accessors) - 2}, "indices": len(accessors) - 1, "material": len(materials) - 1}]})
        nodes.append(dict(node, mesh=len(meshes) - 1))
    buffer = {"byteLength": len(blob)}
    if buffer_uri is not None:
        buffer["uri"] = buffer_uri(blob)
    doc = {"asset": {"version": "2.0", "generator": "AssetCraft make_model_fixtures.py"},
           "scene": 0, "scenes": [{"nodes": list(range(len(nodes)))}], "nodes": nodes, "meshes": meshes,
           "materials": materials, "buffers": [buffer], "bufferViews": views, "accessors": accessors}
    return doc, blob


def cube_faces():
    """A cube as 6 quads with their own corners: positions, outward normals, UVs, triangles."""
    faces = [((0, 0, 1), ((-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1))),
             ((0, 0, -1), ((1, -1, -1), (-1, -1, -1), (-1, 1, -1), (1, 1, -1))),
             ((1, 0, 0), ((1, -1, 1), (1, -1, -1), (1, 1, -1), (1, 1, 1))),
             ((-1, 0, 0), ((-1, -1, -1), (-1, -1, 1), (-1, 1, 1), (-1, 1, -1))),
             ((0, 1, 0), ((-1, 1, 1), (1, 1, 1), (1, 1, -1), (-1, 1, -1))),
             ((0, -1, 0), ((-1, -1, -1), (1, -1, -1), (1, -1, 1), (-1, -1, 1)))]
    pos, nrm, uv, tri = [], [], [], []
    for k, (n, corners) in enumerate(faces):
        base = len(pos)
        # Each face gets its own column of a 6-wide atlas strip.
        u0, u1 = k / 6, (k + 1) / 6
        for c, (u, v) in zip(corners, ((u0, 1), (u1, 1), (u1, 0), (u0, 0))):
            pos.append(c)
            nrm.append(n)
            uv.append((u, v))
        tri += [(base, base + 1, base + 2), (base, base + 2, base + 3)]
    return pos, nrm, uv, tri


def png_rgb(width, height, pixel):
    """A deterministic RGB PNG (standard library only); pixel(x, y) -> (r, g, b)."""
    raw = b"".join(b"\0" + b"".join(bytes(pixel(x, y)) for x in range(width)) for y in range(height))

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def textured_cube_document():
    """One primitive with POSITION, NORMAL, TEXCOORD_0, a base colour PNG and a metallic-roughness PNG."""
    pos, nrm, uv, tri = cube_faces()
    palette = [(178, 92, 48), (92, 120, 64), (70, 90, 140), (160, 150, 60), (120, 70, 130), (90, 90, 90)]

    def base_pixel(x, y):
        face = min(x * 6 // 64, 5)
        r, g, b = palette[face]
        rune = (x % 11 == y % 11) or (x + y) % 16 == 0
        return (min(r + 60, 255), min(g + 60, 255), min(b + 60, 255)) if rune else (r, g, b)

    base_png = png_rgb(64, 64, base_pixel)
    mr_png = png_rgb(16, 16, lambda x, y: (0, 140 + 6 * (x % 4), 40 if x < 8 else 200))   # G roughness, B metalness
    blob = b""
    views, accessors = [], []

    def add(data, target=None):
        nonlocal blob
        view = {"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)}
        if target:
            view["target"] = target
        views.append(view)
        blob += data + b"\0" * (-len(data) % 4)
        return len(views) - 1

    def vec(values, n, kind):
        data = b"".join(struct.pack("<%df" % n, *v) for v in values)
        lo = [min(v[k] for v in values) for k in range(n)]
        hi = [max(v[k] for v in values) for k in range(n)]
        accessors.append({"bufferView": add(data, 34962), "componentType": 5126, "count": len(values), "type": kind, "min": lo, "max": hi})
        return len(accessors) - 1

    a_pos, a_nrm, a_uv = vec(pos, 3, "VEC3"), vec(nrm, 3, "VEC3"), vec(uv, 2, "VEC2")
    idx = b"".join(struct.pack("<3H", *t) for t in tri)
    accessors.append({"bufferView": add(idx, 34963), "componentType": 5123, "count": len(tri) * 3, "type": "SCALAR"})
    a_idx = len(accessors) - 1
    v_base, v_mr = add(base_png), add(mr_png)
    doc = {"asset": {"version": "2.0", "generator": "AssetCraft make_model_fixtures.py"},
           "scene": 0, "scenes": [{"nodes": [0]}],
           "nodes": [{"name": "crate", "mesh": 0, "rotation": [0, 0.2588190, 0, 0.9659258]}],
           "meshes": [{"primitives": [{"attributes": {"POSITION": a_pos, "NORMAL": a_nrm, "TEXCOORD_0": a_uv}, "indices": a_idx, "material": 0}]}],
           "materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}, "metallicRoughnessTexture": {"index": 1}, "metallicFactor": 1.0, "roughnessFactor": 1.0}}],
           "textures": [{"source": 0, "sampler": 0}, {"source": 1, "sampler": 0}],
           "samplers": [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}],
           "images": [{"bufferView": v_base, "mimeType": "image/png"}, {"bufferView": v_mr, "mimeType": "image/png"}],
           "buffers": [{"byteLength": len(blob)}], "bufferViews": views, "accessors": accessors}
    return doc, blob


def write_glb(path, doc, blob):
    j = json.dumps(doc, separators=(",", ":")).encode()
    j += b" " * (-len(j) % 4)
    blob += b"\0" * (-len(blob) % 4)
    total = 12 + 8 + len(j) + 8 + len(blob)
    with open(path, "wb") as f:
        f.write(b"glTF" + struct.pack("<II", 2, total))
        f.write(struct.pack("<I", len(j)) + b"JSON" + j)
        f.write(struct.pack("<I", len(blob)) + b"BIN\0" + blob)


def main(root):
    out = os.path.join(root, "props", "models")
    os.makedirs(out, exist_ok=True)

    orb = icosphere(2)
    doc, blob = document([
        (*orb, (0.35, 0.65, 0.95, 1.0), {"name": "orb", "translation": [0, 1.9, 0], "scale": [0.9, 0.9, 0.9]}),
        (*cube(), (0.42, 0.40, 0.38, 1.0), {"name": "pedestal", "scale": [0.8, 0.5, 0.8], "rotation": [0, 0.3826834, 0, 0.9238795]}),
    ])
    write_glb(os.path.join(out, "soul_orb_icosphere.glb"), doc, blob)

    doc, blob = textured_cube_document()
    write_glb(os.path.join(out, "rune_crate_textured.glb"), doc, blob)

    doc, _ = document([(*torus(1.0, 0.28, 32, 12), (0.8, 0.78, 0.72, 1.0), {"name": "ring", "rotation": [0.3826834, 0, 0, 0.9238795]})],
                      buffer_uri=lambda b: "data:application/octet-stream;base64," + base64.b64encode(b).decode())
    with open(os.path.join(out, "scythe_ring.gltf"), "w", newline="\n") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")

    # Tombstone: a box with a pentagonal (pointed) top profile, extruded in z.
    profile = [(-0.6, 0.0), (0.6, 0.0), (0.6, 1.2), (0.0, 1.6), (-0.6, 1.2)]
    lines = ["# AssetCraft test tombstone (CC0)", "o tombstone"]
    for z in (-0.15, 0.15):
        lines += [f"v {x:.3f} {y:.3f} {z:.3f}" for x, y in profile]
    lines += ["vt 0 0", "vn 0 0 1", "g faces", "usemtl stone"]
    lines.append("f " + " ".join(f"{i + 6}/1/1" for i in range(5)))           # front pentagon (v/vt/vn)
    lines.append("f " + " ".join(str(i + 1) for i in reversed(range(5))))     # back pentagon
    for i in range(5):
        a, b = i + 1, (i + 1) % 5 + 1
        lines.append(f"f {a}//1 {b}//1 {b + 5}//1 {a + 5}//1")                   # sides (v//vn)
    lines += ["v -0.8 -0.1 -0.4", "v 0.8 -0.1 -0.4", "v 0.8 -0.1 0.4", "v -0.8 -0.1 0.4", "f -4 -3 -2 -1"]  # base slab, negative indices
    with open(os.path.join(out, "tombstone.obj"), "w", newline="\n") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "library")
