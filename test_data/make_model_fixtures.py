"""Generate procedural 3D model samples for AssetCraft (CC0, no third-party content).

Writes into <library>/props/models/:
  soul_orb_icosphere.glb   GLB: icosphere on a cube pedestal (two nodes, TRS transforms,
                           two baseColorFactor materials, u16 indices, BIN chunk)
  scythe_ring.gltf         glTF JSON with an embedded base64 data-URI buffer (torus)
  tombstone.obj            Wavefront OBJ: quads, a pentagon, negative indices, v/vt/vn faces

Standard library only, deterministic output. Usage: python make_model_fixtures.py library
"""
import base64, json, math, os, struct, sys


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
