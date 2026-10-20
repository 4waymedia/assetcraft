#!/usr/bin/env python3
"""Vectorise an engraving-style app-icon drawing into the Crafting Apps' three-colour tile.

The drawing is a square image: a figure in ink and paper on a flat colour field (the corners may be
white or transparent; they are clipped away). Output: a 512-unit viewBox SVG with a rounded-square
clip (rx=112), the field as a full-bleed rect, then one paper path (the figure's silhouette) and one
ink path (the line work), each keyed to exactly one colour. Same layout as FilmCraft's
assets/app-icon/filmcraft.svg.

Tracing uses the vtracer CLI (MIT OR Apache-2.0; `cargo install vtracer`), a tool only: nothing of
it is bundled. Pixels are classified with numpy + Pillow.

  python3 tools/vectorize_icon.py SOURCE.png OUT.svg --field '#9c354b' [--size 2048] [--ink 0.42]
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image

INK = "#0b0b0c"
PAPER = "#efe9dc"
VIEW = 512
RX = 112


def hex_rgb(h: str) -> np.ndarray:
    h = h.lstrip("#")
    if not re.fullmatch(r"[0-9a-fA-F]{6}", h):
        raise SystemExit(f"error: not a #rrggbb colour: {h}")
    return np.array([int(h[i : i + 2], 16) for i in (0, 2, 4)], dtype=float)


def classify(img: Image.Image, field: np.ndarray, ink_level: float) -> tuple[np.ndarray, np.ndarray]:
    """Returns (figure, ink) boolean masks. figure = everything that is not field (or a white or
    transparent corner); ink = dark pixels inside the figure."""
    rgba = np.asarray(img.convert("RGBA")).astype(float)
    rgb, alpha = rgba[..., :3], rgba[..., 3]
    dist = np.sqrt(((rgb - field) ** 2).sum(-1))
    white = rgb.min(-1) > 248
    is_field = (dist < 60) | white | (alpha < 128)
    lum = (0.2126 * rgb[..., 0] + 0.7152 * rgb[..., 1] + 0.0722 * rgb[..., 2]) / 255.0
    figure = ~is_field
    ink = figure & (lum < ink_level)
    return figure, ink


def trace(mask: np.ndarray, work: Path, name: str, vtracer: str, speckle: int, segment: float) -> list:
    """Traces a black-on-white mask; returns path data in the mask's pixel coordinates."""
    png = work / f"{name}.png"
    svg = work / f"{name}.svg"
    Image.fromarray(np.where(mask, 0, 255).astype(np.uint8)).save(png)
    subprocess.run(
        [vtracer, "--input", str(png), "--output", str(svg), "--colormode", "bw", "--mode", "spline",
         "--filter_speckle", str(speckle), "--corner_threshold", "60", "--segment_length", str(segment),
         "--splice_threshold", "45", "--path_precision", "1"],
        check=True, stdout=subprocess.DEVNULL,
    )
    text = svg.read_text()
    out = []
    for el in re.findall(r"<path\b[^>]*>", text):
        d = re.search(r'\sd="([^"]+)"', el)
        if not d:
            continue
        t = re.search(r'transform="translate\(([-\d.]+),([-\d.]+)\)"', el)
        out.append((d.group(1), float(t.group(1)) if t else 0.0, float(t.group(2)) if t else 0.0))
    return out


NUM = re.compile(r"-?\d+(?:\.\d+)?")


def num(v: int) -> str:
    """A tenth-of-a-unit integer as the shortest SVG number: 12 -> 1.2, 5 -> .5, -5 -> -.5, 30 -> 3."""
    sign = "-" if v < 0 else ""
    q, r = divmod(abs(v), 10)
    if r == 0:
        return f"{sign}{q}"
    return f"{sign}{q if q else ''}.{r}"


def join(tokens: list[str]) -> str:
    """Letters and numbers with the fewest separators: none around letters, none before '-', none
    before '.' when the previous number already has a decimal point."""
    out: list[str] = []
    prev = ""
    for t in tokens:
        if t.isalpha():
            out.append(t)
            prev = ""
            continue
        if prev and not t.startswith("-") and not (t.startswith(".") and "." in prev):
            out.append(" ")
        out.append(t)
        prev = t
    return "".join(out)


def rescale(paths, scale: float) -> str:
    """vtracer's absolute M/C/L/Z path data, scaled into the 512 viewBox and rounded to 0.1, then
    written compactly: relative commands from the rounded points (so rounding never accumulates),
    repeated commands omitted, minimal separators."""
    tokens: list[str] = []
    cx = cy = sx = sy = 0
    for d, tx, ty in paths:
        toks = re.findall(r"[MLCZmlcz]|-?\d+(?:\.\d+)?", d)
        cmd, vals, groups = None, [], []
        for t in toks + ["Z"]:
            if t.isalpha():
                if t.islower():
                    raise SystemExit("error: vtracer wrote relative path commands; expected absolute")
                if cmd:
                    groups.append((cmd, vals))
                cmd, vals = t, []
            else:
                axis = len(vals) % 2
                vals.append(round((float(t) + (tx if axis == 0 else ty)) * scale * 10))
        last = None
        for c, v in groups:
            if c == "Z":
                if tokens and tokens[-1] != "z":
                    tokens.append("z")
                cx, cy, last = sx, sy, None
                continue
            per = {"M": 2, "L": 2, "C": 6}[c]
            for i in range(0, len(v) - per + 1, per):
                pts = v[i : i + per]
                rel = [pts[j] - (cx if j % 2 == 0 else cy) for j in range(per)]
                letter = c.lower()
                if c == "M":
                    sx, sy = pts[0], pts[1]
                if letter != last or c == "M":
                    tokens.append(letter)
                tokens.extend(num(n) for n in rel)
                last = "l" if c == "M" else letter
                cx, cy = pts[-2], pts[-1]
    text = join(tokens)
    return text[0].upper() + text[1:] if text else text


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("source")
    ap.add_argument("out")
    ap.add_argument("--field", required=True, help="the app colour, #rrggbb (also used to key the field)")
    ap.add_argument("--key", help="colour of the field in the drawing, if it differs from --field")
    ap.add_argument("--size", type=int, default=2048, help="trace resolution in px (2048 canonical, 1024 small)")
    ap.add_argument("--ink", type=float, default=0.42, help="luminance below which a figure pixel is ink (0..1)")
    ap.add_argument("--speckle", type=int, default=3, help="drop specks smaller than this many px (vtracer)")
    ap.add_argument("--segment", type=float, default=6, help="max spline segment length in px (vtracer)")
    a = ap.parse_args()

    vtracer = shutil.which("vtracer")
    if not vtracer:
        print("error: vtracer not found (cargo install vtracer)", file=sys.stderr)
        return 1
    if not (256 <= a.size <= 4096):
        print("error: --size must be 256..4096", file=sys.stderr)
        return 1
    field = hex_rgb(a.field)
    key = hex_rgb(a.key) if a.key else field
    src = Image.open(a.source)
    if src.width != src.height:
        print(f"error: the drawing must be square, got {src.width}x{src.height}", file=sys.stderr)
        return 1
    img = src.convert("RGBA").resize((a.size, a.size), Image.LANCZOS)
    figure, ink = classify(img, key, a.ink)
    scale = VIEW / a.size
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        paper_d = rescale(trace(figure, work, "paper", vtracer, a.speckle, a.segment), scale)
        ink_d = rescale(trace(ink, work, "ink", vtracer, a.speckle, a.segment), scale)
    fhex = "#" + "".join(f"{int(c):02x}" for c in field)
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {VIEW} {VIEW}">'
        f'<defs><clipPath id="t"><rect width="{VIEW}" height="{VIEW}" rx="{RX}"/></clipPath></defs>'
        f'<g clip-path="url(#t)"><rect width="{VIEW}" height="{VIEW}" fill="{fhex}"/>'
        f'<path fill="{PAPER}" fill-rule="evenodd" d="{paper_d}"/>'
        f'<path fill="{INK}" fill-rule="evenodd" d="{ink_d}"/></g></svg>'
    )
    Path(a.out).write_text(svg)
    print(f"{a.out}: {len(svg) // 1024} KB, figure {figure.mean():.0%} of the tile, ink {ink.mean():.0%}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
