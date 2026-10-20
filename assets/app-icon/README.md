# AssetCraft app icon

_2026-10-20 · AssetCraft app icon · artwork chosen (elephant), not yet wired into the app_

- **Scope:** AssetCraft's app icon, built to the Crafting Apps icon standard (FilmCraft's
  `assets/app-icon/`, the owl template), with the same files, layout and sidecars.
- **Project goal it serves:** AssetCraft looks like part of the suite in the Dock, taskbar and Start
  menu, next to FilmCraft, DesignCraft, EffectCraft and SoundCraft.
- **Immediate area of improvement:** the icon isn't used by the app yet. The next step embeds
  `assetcraft.ico` in the Windows exe and sets the runtime window icon (see "How the icon reaches each OS").

**Creature:** an African elephant, head and shoulders, ears spread to the tile's edges and the trunk
curling off the bottom: "an elephant never forgets", the library that remembers every asset. Chosen
over a raven on 2026-10-20: it reads in paper-on-colour like the rest of the suite, keeps its shape
at 16–32 px, and isn't a third bird after FilmCraft's owl and SoundCraft's bird.

## Palette

Exactly three colours:

| Colour                        | Hex       | Used for                               |
|-------------------------------|-----------|----------------------------------------|
| Ink                           | `#0b0b0c` | line work and the figure's contour     |
| Paper                         | `#efe9dc` | the figure (the elephant)              |
| AssetCraft garnet (app colour) | `#9d354b` | the full-bleed field behind the figure |

The nearest suite colour is EffectCraft's magenta (`#e0368f`); garnet is darker and still reads as a
different red at 32 px.

## Geometry and style

- A 512-unit viewBox tile, rounded square `rx=112`, with the app colour full bleed (no frame or border).
- An engraving-style portrait in the owl-template framing: the head fills the tile, the eye at about
  30 % of the height, and the shoulders and trunk running off the bottom edge.
- macOS renders (`assetcraft.icns`, `assetcraft-macos-512.png`) put the tile in Apple's grid (824 of
  1024 px, transparent margin). Windows, Linux and the web use the full-bleed tile.

## Provenance

The owner made it in ArtCraft as a 1254 px engraving-style image. The original is kept outside the
repo at `_dev-data\assetcraft\app-icon\source-elephant.png` (the raven option beside it as
`source-raven.png`). It was vectorised with [`tools/vectorize_icon.py`](../../tools/vectorize_icon.py),
which keys the drawing to the three-colour palette (field by colour distance; ink below 47 %
luminance, the midpoint between ink and paper, so stroke widths hold) and traces each layer with
vtracer (MIT OR Apache-2.0, a tool only, nothing bundled). FilmCraft used craftrules'
`vectorize_tile.py` with potrace; craftrules isn't available here, and potrace is GPL, so AssetCraft
uses its own script. Licence: [LICENSE.txt](LICENSE.txt) (MIT OR Apache-2.0).

```text
python tools/vectorize_icon.py source-elephant.png assets/app-icon/assetcraft.svg       --field #9d354b --size 2048 --ink 0.47 --speckle 4 --segment 8
python tools/vectorize_icon.py source-elephant.png assets/app-icon/assetcraft-small.svg --field #9d354b --size 1024 --ink 0.47 --speckle 3 --segment 8
```

## Files

| File                                                                         | What                                                                  |
|------------------------------------------------------------------------------|-----------------------------------------------------------------------|
| `assetcraft.svg`                                                             | canonical artwork, traced at 2048 px (about 1.3 MB)                   |
| `assetcraft-small.svg`                                                       | lighter trace at 1024 px (about 630 KB), for when size matters        |
| `assetcraft-1024.png`                                                        | 1024 px render                                                        |
| `assetcraft-macos-512.png`                                                   | runtime Dock icon on macOS                                            |
| `assetcraft.icns`                                                            | macOS bundle icon (`CFBundleIconFile`)                                |
| `assetcraft.ico`                                                             | Windows icon, 16–256 px (one render per size)                         |
| `hicolor/<size>/apps/ai.storyteller.assetcraft.png`, `hicolor/scalable/…svg` | Linux icon theme                                                      |
| `*.attribution`                                                              | one sidecar per file; rows in [`ATTRIBUTION.md`](../../ATTRIBUTION.md) |

## How the icon reaches each OS (next step, not wired yet)

- **Windows:** `apps/assetcraft/build.rs` embeds `assetcraft.ico` as the exe's resource icon
  (Explorer, Start menu, pinned taskbar), as FilmCraft's `apps/filmcraft/build.rs` does; the runtime
  icon covers the window, taskbar and Alt-Tab.
- **macOS:** the window icon set at startup (`ViewportBuilder::with_icon`, from
  `assetcraft-macos-512.png`) is the Dock icon when the app runs unbundled.
- **Linux:** the runtime icon (hicolor 256 px) plus `with_app_id("ai.storyteller.assetcraft")`, matched
  to a `.desktop` file.

## Regenerate

`python packaging/icons.py` (needs `resvg` on PATH and Pillow; runs on Windows, macOS and Linux, and
packs the `.ico` and `.icns` with Pillow, so no `iconutil` or xtask is needed). To change the artwork,
re-run `tools/vectorize_icon.py` on a new drawing, then the script. Every derived file needs its
`.attribution` sidecar and a row in `ATTRIBUTION.md`.
