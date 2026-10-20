# Test data

`library/` is a small, generated asset tree for exercising the AssetCraft navigator by hand
and in tests. Every image is drawn procedurally by `make_library_fixtures.py` (seeded, so it is
reproducible): no photos, no third-party art. Released as CC0.

Regenerate (from this folder):

```powershell
python make_library_fixtures.py library
python make_model_fixtures.py library      # 3D samples in props/models/ (standard library only)
```

| Folder                      | Exercises                                                                    |
|-----------------------------|------------------------------------------------------------------------------|
| `characters/reaper/`        | Version stacks (`_v01`…`_v03`, `_final`), nested folder (`turnaround/`)      |
| `environments/`             | Aspect ratios (16:9, 21:9, 9:16), JPG / PNG / WebP                           |
| `environments/dupes/`       | Exact copy, resized + recompressed, brightened: near-duplicate finder        |
| `props/`                    | PNG with transparency, GIF                                                   |
| `props/models/`             | 3D: GLB (icosphere on a cube, two nodes and materials), glTF with a base64 data-URI buffer (torus), OBJ (quads, pentagon, negative indices), textured GLB (cube with normals, UVs, a base colour and a metallic-roughness PNG embedded as bufferViews) |
| `palettes/`                 | BMP, colour extraction                                                       |
| `storyboard/`               | TIFF (LZW), greyscale PNG sketches named by shot number                      |
| `environments/` (Stage 1)  | Camera raw: `clearing_raw_orient6.dng` (embedded JPEG preview, Orientation 6); HEIC `lantern_dusk.heic` |
| `palettes/` (Stage 1)      | PSD with an RLE composite: `reapers_palette_board.psd`                       |
| `edge-cases/`               | Unicode + spaces in names, uppercase extension, 64 px icon, 20:1 panorama, a file with a `.jpg` name that is not an image |

The three Stage 1 samples come from `make_stage1_samples.py` (same licence; the HEIC needs
`pip install pillow-heif`): `python make_stage1_samples.py library`.

28 files from the two generators above, about 1.7 MB.

### Stage 1: PDF and video samples

`make_pdf_video_samples.py` adds five small files to the existing `environments/` folder (a new
folder would change the folder-count tests). Regenerate with
`python make_pdf_video_samples.py library` (videos need `ffmpeg` on PATH).

| File                                         | Exercises                                                     |
|----------------------------------------------|---------------------------------------------------------------|
| `environments/location_breakdown.pdf`        | PDF first page (612 × 396 pt, vector shapes, Helvetica text)  |
| `environments/snow_clearing_flythrough.mp4`  | MP4 (`ftyp isom`), H.264, 160 × 90, 2 s                       |
| `environments/snow_clearing_flythrough.mov`  | QuickTime (`ftyp qt`), H.264                                  |
| `environments/soul_light_loop.webm`          | Matroska / WebM (EBML), VP8                                   |
| `environments/frozen_lake_pan.avi`           | AVI (`RIFF … AVI `), Motion JPEG                              |

Video poster frames need `filmcraft-cli`; without it the videos show their type icon.
35 files in all (3 of them 3D models in `props/models/`), about 1.9 MB.

