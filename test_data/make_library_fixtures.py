"""Generate procedural, asset-free test images for AssetCraft (CC0, no third-party content)."""
import math, os, random, sys
from PIL import Image, ImageDraw, ImageFilter

root = sys.argv[1]
random.seed(20261012)

def grad(w, h, c1, c2, vertical=True):
    im = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(im)
    n = h if vertical else w
    for i in range(n):
        t = i / max(1, n - 1)
        c = tuple(int(c1[k] + (c2[k] - c1[k]) * t) for k in range(3))
        if vertical: d.line([(0, i), (w, i)], fill=c)
        else: d.line([(i, 0), (i, h)], fill=c)
    return im

def figure(im, x, y, s, cloak=(18, 18, 24), eye=(120, 200, 255)):
    d = ImageDraw.Draw(im)
    d.polygon([(x, y - s), (x - s * 0.55, y + s), (x + s * 0.55, y + s)], fill=cloak)
    d.ellipse([x - s * .28, y - s * 1.25, x + s * .28, y - s * .7], fill=cloak)
    d.ellipse([x - s * .12, y - s * 1.02, x - s * .04, y - s * .94], fill=eye)
    d.ellipse([x + s * .04, y - s * 1.02, x + s * .12, y - s * .94], fill=eye)
    d.line([(x + s * .6, y - s * 1.3), (x + s * .6, y + s)], fill=(70, 60, 50), width=max(2, int(s * .05)))
    d.arc([x + s * .1, y - s * 1.5, x + s * 1.1, y - s * .9], 180, 300, fill=(200, 200, 210), width=max(2, int(s * .05)))

def snow(im, n):
    d = ImageDraw.Draw(im); w, h = im.size
    for _ in range(n):
        x, y, r = random.randint(0, w), random.randint(0, h), random.random() * 2.5 + .5
        d.ellipse([x - r, y - r, x + r, y + r], fill=(235, 240, 255))

def glow(im, x, y, r, c):
    g = Image.new("RGB", im.size, (0, 0, 0)); d = ImageDraw.Draw(g)
    d.ellipse([x - r, y - r, x + r, y + r], fill=c)
    g = g.filter(ImageFilter.GaussianBlur(r * .6))
    return Image.fromarray if False else Image.blend(im, Image.composite(g, im, g.convert("L")), .9)

def label(im, text):
    d = ImageDraw.Draw(im); d.rectangle([0, im.height - 22, im.width, im.height], fill=(0, 0, 0))
    d.text((6, im.height - 18), text, fill=(230, 230, 230))
    return im

def save(rel, im, **kw):
    p = os.path.join(root, rel); os.makedirs(os.path.dirname(p), exist_ok=True); im.save(p, **kw); return p

out = []
# characters/reaper: concept versions (version stacks), turnaround views
for v, hue in [(1, (40, 40, 70)), (2, (30, 50, 80)), (3, (25, 45, 90))]:
    im = grad(1024, 1024, hue, (5, 5, 10)); figure(im, 512, 600, 260); snow(im, 120)
    out.append(save(f"characters/reaper/reaper_concept_v0{v}.png", label(im, f"reaper concept v0{v}")))
im = grad(1024, 1024, (25, 45, 90), (5, 5, 10)); figure(im, 512, 600, 260, eye=(150, 230, 255)); snow(im, 120)
out.append(save("characters/reaper/reaper_concept_final.png", label(im, "reaper concept FINAL")))
for view, dx in [("front", 0), ("side", -60), ("back", 0)]:
    im = Image.new("RGB", (800, 1200), (200, 200, 205)); figure(im, 400 + dx, 700, 300, eye=(200, 200, 205) if view == "back" else (120, 200, 255))
    out.append(save(f"characters/reaper/turnaround/reaper_{view}.jpg", label(im, f"turnaround {view}"), quality=90))
# environments: plates in several aspect ratios and formats
envs = [("snow_clearing_plate", (1920, 1080), (60, 80, 120), (220, 230, 245), "jpg"),
        ("soul_light_forest", (1920, 1080), (5, 10, 25), (20, 40, 60), "jpg"),
        ("frozen_lake_wide", (2560, 1080), (90, 110, 150), (200, 215, 235), "webp"),
        ("ruins_vertical", (1080, 1920), (30, 25, 35), (90, 80, 90), "png")]
for name, (w, h), c1, c2, ext in envs:
    im = grad(w, h, c1, c2); d = ImageDraw.Draw(im)
    for i in range(14):
        x = random.randint(0, w); th = random.randint(h // 4, h // 2)
        d.polygon([(x, h - th), (x - 60, h), (x + 60, h)], fill=(15, 25, 20))
    if "soul" in name: im = glow(im, w // 2, h // 2, 120, (80, 160, 255))
    snow(im, 300)
    kw = {"quality": 88} if ext in ("jpg", "webp") else {}
    out.append(save(f"environments/{name}.{ext}", label(im, name), **kw))
# near-duplicates of one plate (resized + recompressed + slightly brighter)
base = Image.open(os.path.join(root, "environments/snow_clearing_plate.jpg"))
out.append(save("environments/dupes/snow_clearing_plate_copy.jpg", base.copy(), quality=88))
out.append(save("environments/dupes/snow_clearing_plate_small.jpg", base.resize((960, 540)), quality=70))
out.append(save("environments/dupes/snow_clearing_plate_bright.jpg", Image.eval(base, lambda p: min(255, int(p * 1.08))), quality=85))
# props & palette swatches (small sizes, transparency, gif, bmp, tiff)
im = Image.new("RGBA", (512, 512), (0, 0, 0, 0)); d = ImageDraw.Draw(im)
d.line([(256, 40), (256, 480)], fill=(70, 60, 50, 255), width=16); d.arc([150, 20, 420, 200], 180, 330, fill=(210, 210, 220, 255), width=14)
out.append(save("props/scythe_transparent.png", im))
im = Image.new("RGB", (256, 256), (10, 10, 20)); im = glow(im, 128, 128, 60, (80, 160, 255))
out.append(save("props/soul_orb_small.gif", im.convert("P", palette=Image.ADAPTIVE)))
im = Image.new("RGB", (600, 120)); d = ImageDraw.Draw(im)
for i, c in enumerate([(18, 18, 24), (25, 45, 90), (120, 200, 255), (220, 230, 245), (70, 60, 50)]):
    d.rectangle([i * 120, 0, i * 120 + 119, 119], fill=c)
out.append(save("palettes/reapers_palette.bmp", im))
im = grad(1200, 800, (120, 90, 60), (30, 20, 15), vertical=False); figure(im, 600, 480, 200)
out.append(save("storyboard/shot_010_intro.tiff", label(im, "shot 010 intro"), compression="tiff_lzw"))
for n, txt in [(20, "walk to light"), (30, "reveal")]:
    im = Image.new("L", (1280, 720), 235); d = ImageDraw.Draw(im)
    d.rectangle([40, 40, 1240, 680], outline=40, width=4); d.text((60, 60), f"SHOT {n:03d}: {txt}", fill=20)
    d.ellipse([580, 260, 700, 380], outline=40, width=3); d.line([(640, 380), (640, 560)], fill=40, width=3)
    out.append(save(f"storyboard/shot_{n:03d}_sketch.png", im))
# edge cases: unicode name, spaces, uppercase extension, tiny + huge-aspect, zero-byte broken file
im = grad(400, 400, (200, 80, 80), (40, 10, 10)); out.append(save("edge-cases/Réaper ünïcode name.png", im))
im = grad(64, 64, (0, 255, 0), (0, 60, 0)); out.append(save("edge-cases/TINY_ICON.JPG", im, quality=90))
im = grad(4000, 200, (255, 200, 0), (120, 0, 120), vertical=False); out.append(save("edge-cases/panorama_20to1.jpg", im, quality=80))
open(os.path.join(root, "edge-cases/broken_not_an_image.jpg"), "wb").write(b"this is not a jpeg")
out.append(os.path.join(root, "edge-cases/broken_not_an_image.jpg"))
print(len(out), "files")
