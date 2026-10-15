"""Generate the Stage 1 PSD, camera raw (DNG) and HEIF samples for AssetCraft (CC0, procedural).

Writes into existing `<root>` folders (new folders would change folder-count tests):
- `palettes/reapers_palette_board.psd`: RGB PSD, RLE (PackBits) composite, written by hand from Adobe's public
  file-format layout (no layers section, so the composite is the image).
- `environments/clearing_raw_orient6.dng`: minimal DNG: IFD0 a 40 x 25 RGB thumbnail with DNGVersion and
  Orientation 6 (rotate 90 CW), a SubIFD with a 320 x 200 baseline JPEG preview, and a SubIFD
  with a 64 x 40 16-bit Bayer mosaic (never decoded by AssetCraft: the preview is used).
- `environments/lantern_dusk.heic`: 8-bit 4:2:0 HEIC (needs `pip install pillow-heif`; skipped with a note
  otherwise).

Usage (from this folder): python make_stage1_samples.py library
"""
import io, os, struct, sys
from PIL import Image, ImageDraw

root = sys.argv[1]


def scene(w, h, sky, ground, sun):
    im = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(im)
    for y in range(h):
        t = y / max(1, h - 1)
        c = tuple(int(sky[k] + (ground[k] - sky[k]) * t) for k in range(3))
        d.line([(0, y), (w, y)], fill=c)
    r = h // 6
    d.ellipse([w * 0.7 - r, h * 0.3 - r, w * 0.7 + r, h * 0.3 + r], fill=sun)
    d.polygon([(0, h), (w * 0.35, h * 0.45), (w * 0.7, h)], fill=(30, 35, 45))
    d.rectangle([w * 0.1, h * 0.75, w * 0.2, h * 0.95], fill=(200, 60, 40))
    return im


# ---- PSD -----------------------------------------------------------------------------------

def packbits(row):
    out, i, n = bytearray(), 0, len(row)
    while i < n:
        j = i
        while j + 1 < n and row[j + 1] == row[i] and j - i < 127:
            j += 1
        if j > i:  # run of j - i + 1 equal bytes
            out += bytes([257 - (j - i + 1), row[i]])
            i = j + 1
            continue
        j = i
        while j < n and j - i < 128 and not (j + 2 < n and row[j] == row[j + 1] == row[j + 2]):
            j += 1
        out += bytes([j - i - 1]) + bytes(row[i:j])
        i = j
    return bytes(out)


def write_psd(path, im):
    w, h = im.size
    planes = [im.getchannel(c).tobytes() for c in "RGB"]
    rows = [packbits(p[y * w:(y + 1) * w]) for p in planes for y in range(h)]
    out = bytearray(b"8BPS" + struct.pack(">H6xHIIHH", 1, 3, h, w, 8, 3))
    out += struct.pack(">I", 0)  # colour mode data
    out += struct.pack(">I", 0)  # image resources
    out += struct.pack(">I", 0)  # layer and mask information
    out += struct.pack(">H", 1)  # RLE
    out += b"".join(struct.pack(">H", len(r)) for r in rows)
    out += b"".join(rows)
    with open(path, "wb") as f:
        f.write(out)


# ---- DNG -----------------------------------------------------------------------------------

SHORT, LONG, ASCII, BYTE = 3, 4, 2, 1


def ifd_bytes(entries, base, next_ifd=0):
    """One little-endian IFD at file offset `base`; values over 4 bytes follow the IFD."""
    entries = sorted(entries)
    n = len(entries)
    extra_at = base + 2 + 12 * n + 4
    head, extra = bytearray(struct.pack("<H", n)), bytearray()
    for tag, kind, values in entries:
        if kind == ASCII:
            data = values.encode() + b"\0"
            count = len(data)
        else:
            fmt = {SHORT: "H", LONG: "I", BYTE: "B"}[kind]
            data = struct.pack("<" + fmt * len(values), *values)
            count = len(values)
        if len(data) <= 4:
            field = data.ljust(4, b"\0")
        else:
            field = struct.pack("<I", extra_at + len(extra))
            extra += data + (b"\0" if len(data) % 2 else b"")
        head += struct.pack("<HHI", tag, kind, count) + field
    head += struct.pack("<I", next_ifd)
    return bytes(head + extra)


def write_dng(path, preview):
    pw, ph = preview.size
    buf = io.BytesIO()
    preview.save(buf, "JPEG", quality=85)
    jpeg = buf.getvalue()
    thumb = preview.resize((40, 25))
    thumb_px = thumb.tobytes()
    rw, rh = 64, 40
    mosaic = bytearray()
    for y in range(rh):
        for x in range(rw):
            r, g, b = preview.getpixel((x * pw // rw, y * ph // rh))
            v = [[r, g], [g, b]][y % 2][x % 2]
            mosaic += struct.pack("<H", v * 16)  # 12-bit values

    # Layout: header, IFD0, preview IFD, raw IFD (each <= 1 KiB), then pixel data.
    ifd0_at, prev_at, raw_at, data_at = 8, 1024, 2048, 3072
    thumb_at = data_at
    jpeg_at = thumb_at + len(thumb_px)
    mosaic_at = jpeg_at + len(jpeg) + (len(jpeg) % 2)
    ifd0 = ifd_bytes([
        (254, LONG, [1]), (256, LONG, [40]), (257, LONG, [25]), (258, SHORT, [8, 8, 8]),
        (259, SHORT, [1]), (262, SHORT, [2]), (271, ASCII, "AssetCraft"), (272, ASCII, "Test Camera"),
        (273, LONG, [thumb_at]), (274, SHORT, [6]), (277, SHORT, [3]), (278, LONG, [25]),
        (279, LONG, [len(thumb_px)]), (284, SHORT, [1]), (330, LONG, [prev_at, raw_at]),
        (50706, BYTE, [1, 4, 0, 0]), (50707, BYTE, [1, 1, 0, 0]), (50708, ASCII, "AssetCraft Test Camera"),
    ], ifd0_at)
    prev = ifd_bytes([
        (254, LONG, [1]), (256, LONG, [pw]), (257, LONG, [ph]), (258, SHORT, [8, 8, 8]),
        (259, SHORT, [7]), (262, SHORT, [6]), (273, LONG, [jpeg_at]), (277, SHORT, [3]),
        (278, LONG, [ph]), (279, LONG, [len(jpeg)]), (284, SHORT, [1]),
    ], prev_at)
    raw = ifd_bytes([
        (254, LONG, [0]), (256, LONG, [rw]), (257, LONG, [rh]), (258, SHORT, [16]),
        (259, SHORT, [1]), (262, SHORT, [32803]), (273, LONG, [mosaic_at]), (277, SHORT, [1]),
        (278, LONG, [rh]), (279, LONG, [len(mosaic)]), (284, SHORT, [1]),
        (33421, SHORT, [2, 2]), (33422, BYTE, [0, 1, 1, 2]), (50717, LONG, [4095]),
    ], raw_at)
    assert len(ifd0) <= prev_at - ifd0_at and len(prev) <= 1024 and len(raw) <= 1024
    out = bytearray(b"II*\0" + struct.pack("<I", ifd0_at))
    for at, block in [(ifd0_at, ifd0), (prev_at, prev), (raw_at, raw), (thumb_at, thumb_px),
                      (jpeg_at, jpeg), (mosaic_at, bytes(mosaic))]:
        out += b"\0" * (at - len(out))
        out += block
    with open(path, "wb") as f:
        f.write(out)


write_psd(os.path.join(root, "palettes", "reapers_palette_board.psd"), scene(320, 200, (40, 70, 130), (230, 190, 140), (255, 230, 120)))
write_dng(os.path.join(root, "environments", "clearing_raw_orient6.dng"), scene(320, 200, (90, 120, 170), (220, 230, 245), (255, 250, 220)))
try:
    import pillow_heif
    pillow_heif.register_heif_opener()
    scene(256, 160, (10, 20, 40), (60, 40, 30), (255, 180, 60)).save(
        os.path.join(root, "environments", "lantern_dusk.heic"), quality=80, chroma=420)
except ImportError:
    print("pillow-heif not installed: lantern_dusk.heic not written")
