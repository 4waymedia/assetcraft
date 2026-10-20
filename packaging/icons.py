#!/usr/bin/env python3
"""Regenerate every derived app icon from assets/app-icon/assetcraft.svg (the canonical artwork; a
512-unit full-bleed tile with rx=112). assetcraft-small.svg is a lighter trace of the same drawing
and is copied, not generated.

Needs: resvg on PATH (`cargo install resvg`) and Pillow (`pip install pillow`). Pillow packs the
Windows .ico and the macOS .icns on any OS (FilmCraft's packaging/icons.sh needs macOS iconutil for
the .icns; this script runs the same on Windows). The outputs are committed, so builds never need
these tools.

  python packaging/icons.py
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
DIR = ROOT / "assets" / "app-icon"
SVG = DIR / "assetcraft.svg"
SMALL = DIR / "assetcraft-small.svg"
APP_ID = "ai.storyteller.assetcraft"
VIEWBOX = 'viewBox="0 0 512 512"'
# macOS icons follow Apple's grid: the tile body is 824 of 1024 px with a transparent margin, so the
# Dock shows it at the same size as other apps. 512 * 1024 / 824 = 636.27 units, margin 62.14.
MAC_VIEWBOX = 'viewBox="-62.14 -62.14 636.27 636.27"'
HICOLOR = [16, 24, 32, 48, 64, 128, 256, 512]
ICO = [16, 20, 24, 32, 40, 48, 64, 128, 256]
ICNS = [16, 32, 64, 128, 256, 512, 1024]


def render(resvg: str, svg: Path, size: int, out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([resvg, "-w", str(size), "-h", str(size), str(svg), str(out)], check=True,
                   stdin=subprocess.DEVNULL)


def main() -> int:
    resvg = shutil.which("resvg")
    if not resvg:
        print("error: resvg not found (cargo install resvg)", file=sys.stderr)
        return 1
    for f in (SVG, SMALL):
        if not f.is_file():
            print(f"error: missing {f}", file=sys.stderr)
            return 1
    text = SVG.read_text(encoding="utf-8")
    if VIEWBOX not in text:
        print(f"error: unexpected viewBox in {SVG}", file=sys.stderr)
        return 1
    with tempfile.TemporaryDirectory() as tmp_s:
        tmp = Path(tmp_s)
        mac = tmp / "mac.svg"
        mac.write_text(text.replace(VIEWBOX, MAC_VIEWBOX, 1), encoding="utf-8")

        render(resvg, SVG, 1024, DIR / "assetcraft-1024.png")
        # Runtime window/Dock icon on macOS.
        render(resvg, mac, 512, DIR / "assetcraft-macos-512.png")

        # Linux hicolor theme. Windows and Linux use the full-bleed tile.
        for s in HICOLOR:
            render(resvg, SVG, s, DIR / "hicolor" / f"{s}x{s}" / "apps" / f"{APP_ID}.png")
        scalable = DIR / "hicolor" / "scalable" / "apps" / f"{APP_ID}.svg"
        scalable.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(SMALL, scalable)

        # Windows .ico: one hand-rendered PNG per size (no downscaling inside the .ico).
        ico = []
        for s in ICO:
            render(resvg, SVG, s, tmp / f"ico-{s}.png")
            ico.append(Image.open(tmp / f"ico-{s}.png").convert("RGBA"))
        ico[-1].save(DIR / "assetcraft.ico", format="ICO", sizes=[(s, s) for s in ICO],
                     append_images=ico[:-1])

        # macOS .icns, from the Apple-grid tile.
        icns = []
        for s in ICNS:
            render(resvg, mac, s, tmp / f"icns-{s}.png")
            icns.append(Image.open(tmp / f"icns-{s}.png").convert("RGBA"))
        icns[-1].save(DIR / "assetcraft.icns", format="ICNS", append_images=icns[:-1])

    print(f"icons written to {DIR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
