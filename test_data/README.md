# Test data

`library/` is a small, generated asset tree for exercising the AssetCraft navigator by hand
and in tests. Every image is drawn procedurally by `make_library_fixtures.py` (seeded, so it is
reproducible): no photos, no third-party art. Released as CC0.

Regenerate (from this folder):

```powershell
python make_library_fixtures.py library
```

| Folder                      | Exercises                                                                    |
|-----------------------------|------------------------------------------------------------------------------|
| `characters/reaper/`        | Version stacks (`_v01`…`_v03`, `_final`), nested folder (`turnaround/`)      |
| `environments/`             | Aspect ratios (16:9, 21:9, 9:16), JPG / PNG / WebP                           |
| `environments/dupes/`       | Exact copy, resized + recompressed, brightened: near-duplicate finder        |
| `props/`                    | PNG with transparency, GIF                                                   |
| `palettes/`                 | BMP, colour extraction                                                       |
| `storyboard/`               | TIFF (LZW), greyscale PNG sketches named by shot number                      |
| `edge-cases/`               | Unicode + spaces in names, uppercase extension, 64 px icon, 20:1 panorama, a file with a `.jpg` name that is not an image |

24 files, about 1.7 MB.
