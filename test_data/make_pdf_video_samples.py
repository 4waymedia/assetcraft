"""Generate the Stage 1 PDF and video samples for AssetCraft (CC0, procedural, no third-party art).

Writes into the existing `<root>/environments/` folder (new folders would change folder-count tests):
- `location_breakdown.pdf`: one 612 x 396 pt page (half-letter, landscape) of vector shapes and
  Helvetica text, written byte by byte below (no PDF library).
- `snow_clearing_flythrough.mp4` (H.264, `isom` brand), `snow_clearing_flythrough.mov`
  (H.264, `qt  ` brand), `soul_light_loop.webm` (VP8, Matroska/EBML) and `frozen_lake_pan.avi`
  (Motion JPEG, RIFF AVI): 160 x 90, 12 fps, 2 s. Frames are drawn with Pillow and encoded by
  `ffmpeg` (must be on PATH; the videos are skipped with a note otherwise).

Usage (from this folder): python make_pdf_video_samples.py library
"""
import os, shutil, subprocess, sys
from PIL import Image, ImageDraw

root = sys.argv[1]
out_dir = os.path.join(root, "environments")
os.makedirs(out_dir, exist_ok=True)


# ---- PDF -----------------------------------------------------------------------------------

def make_pdf(path):
    content = b"\n".join([
        b"0.86 0.90 0.96 rg 0 0 612 396 re f",            # pale sky
        b"0.10 0.16 0.12 rg 0 0 612 120 re f",            # ground
        b"0.25 0.45 0.80 rg 430 250 m 470 250 470 290 450 310 c 430 290 430 250 430 250 c f",
        b"0.08 0.08 0.10 rg 120 120 m 160 220 l 200 120 l f",  # figure
        b"0.75 0.20 0.15 RG 4 w 40 40 532 316 re S",      # frame
        b"BT /F1 28 Tf 0 0 0 rg 60 340 Td (Location breakdown) Tj ET",
        b"BT /F1 14 Tf 0.2 0.2 0.2 rg 60 316 Td (Snow clearing - day 3) Tj ET",
    ])
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 396] /Contents 4 0 R "
        b"/Resources << /Font << /F1 5 0 R >> >> >>",
        b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for n, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % n + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objects) + 1, xref)
    with open(path, "wb") as f:
        f.write(out)


make_pdf(os.path.join(out_dir, "location_breakdown.pdf"))
written = ["location_breakdown.pdf"]


# ---- video ---------------------------------------------------------------------------------

W, H, FPS, FRAMES = 160, 90, 12, 24


def frame(n):
    t = n / (FRAMES - 1)
    im = Image.new("RGB", (W, H))
    d = ImageDraw.Draw(im)
    for y in range(H):
        k = y / (H - 1)
        d.line([(0, y), (W, y)], fill=(int(60 + 160 * k), int(80 + 150 * k), int(120 + 125 * k)))
    d.rectangle([0, 70, W, H], fill=(225, 232, 245))
    x = int(20 + 110 * t)
    d.polygon([(x, 40), (x - 10, 72), (x + 10, 72)], fill=(18, 18, 24))
    d.ellipse([x - 5, 30, x + 5, 40], fill=(18, 18, 24))
    d.ellipse([120, 12, 140, 32], fill=(120, 200, 255))
    return im.tobytes()


VIDEOS = [
    ("snow_clearing_flythrough.mp4", ["-c:v", "libx264", "-pix_fmt", "yuv420p", "-f", "mp4"]),
    ("snow_clearing_flythrough.mov", ["-c:v", "libx264", "-pix_fmt", "yuv420p", "-f", "mov"]),
    ("soul_light_loop.webm", ["-c:v", "libvpx", "-b:v", "200k", "-f", "webm"]),
    ("frozen_lake_pan.avi", ["-c:v", "mjpeg", "-q:v", "5", "-pix_fmt", "yuvj420p", "-f", "avi"]),
]

ffmpeg = shutil.which("ffmpeg")
if ffmpeg is None:
    print("ffmpeg not found: video samples skipped")
else:
    raw = b"".join(frame(n) for n in range(FRAMES))
    for name, codec in VIDEOS:
        cmd = [ffmpeg, "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24",
               "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-", "-map_metadata", "-1",
               "-fflags", "+bitexact", "-flags:v", "+bitexact", *codec, os.path.join(out_dir, name)]
        subprocess.run(cmd, input=raw, check=True)
        written.append(name)

print(len(written), "files:", ", ".join(written))
