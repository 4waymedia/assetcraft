# AssetCraft

Browse, structure, organize, plan and execute: an asset app for creative work, in the spirit
of Adobe Bridge. Built in pure Rust as a sibling of the storytold
[Crafting Apps](https://getartcraft.com/apps) (PhotoCraft, FilmCraft, LightCraft, PdfCraft,
DesignCraft, WordCraft).

- **Project goal it serves:** one place to find, organize and plan creative assets
  (concepts, plates, video, 3D meshes), and then to produce from them.
- **Immediate area of improvement:** the Crafting Apps have no Bridge. Each app opens its own
  files, and nothing sits above them to find, group, plan and hand off work.

**Status (2026-10-22):** Stages 0 and 1 done; Stage 4 Storyboard mode largely built; Stage 2 (Organize) and Stage 1.5
(API) under way; 3D viewer V0 in progress. The "Now" table in [STATUS.md](STATUS.md) has the details.

## Build (Windows, PowerShell)

```powershell
Set-Location F:\Script-Projects\ArtCrafting\assetcraft
cargo xtask ci                     # fmt, clippy, tests (default + heic), layers, deny
cargo run -p assetcraft --release -- "F:\Script-Projects\ArtCrafting\assetcraft\test_data\library"
cargo run -p assetcraft-cli -- --help
cargo run -p assetcraft-cli -- analyze "F:\Script-Projects\ArtCrafting\_dev-data\assetcraft\preset-tracks\vanguard-by-scott-buckley.mp3"
cargo run -p assetcraft-playback --release --example play -- "<an .mp3>" 30 10   # hear 10 s from 0:30
```

Watch your Cut plays the storyboard's track on the default output device (cpal). Windows and macOS
need nothing extra; Linux builds need the ALSA headers (`sudo apt install libasound2-dev`).

Needs Rust 1.95 or later (`rustup update stable`).

## Optional formats

### HEIC / HEIF (off by default)

HEIC photos (iPhone default) use the HEVC codec, which may be patent-encumbered in some
countries. Like LightCraft and PhotoCraft, AssetCraft leaves it out unless you opt in, in two
steps:

1. **Build with the `heic` feature.** Without it the decoder (`lightcraft-heif` over
   `heic-rs`) isn't compiled at all.
   ```powershell
   Set-Location F:\Script-Projects\ArtCrafting\assetcraft
   cargo run -p assetcraft --release --features heic
   ```
2. **Turn it on in the app:** View → Formats → **HEIC / HEIF thumbnails (HEVC)**. The choice is
   saved in your preferences (`formats.heic`) and thumbnails re-render straight away.

Until both are done, HEIC files show a type icon. In a build without the feature the menu
item is greyed, and its tooltip says to rebuild with `--features heic`. The switch is the
`formats.heic {"enabled": true}` command, so the menu, keys and future automation share it.

The CLI doesn't read the app's preferences; pass `--heic` per call instead:

```powershell
Set-Location F:\Script-Projects\ArtCrafting\assetcraft
cargo build -p assetcraft-cli --release --features heic
.\target\release\assetcraft-cli.exe thumb photo.heic --size 256 --out photo.png --heic
```

For anything you distribute, decide on HEVC licensing first; builds without the feature
contain no HEVC code.

### Video poster frames

Video thumbnails (MP4, MOV, MKV/WebM, AVI) come from FilmCraft's `filmcraft-cli`, run as a
separate program. Put it on `PATH` or set `ASSETCRAFT_FILMCRAFT_CLI` to its full path, then
restart AssetCraft. Without it, videos show a type icon.

## Video: decode, encode and export

AssetCraft contains no video codec of its own. Decoding (poster frames, hover scrub) is
FilmCraft's job through `filmcraft-cli` (above). Encoding — the storyboard's **Export Video…**
(plan step SF5, `docs/storyboard-plan.md` § Files) — follows the same clean-room rules as
everything else: no GPL/LGPL/AGPL code, nothing we can't take as source, and the choice is
stated here so a build never surprises anyone.

**Route 1 (decided, D-SF3): FilmCraft renders.** Export Video… writes the Cut as OTIO and opens
it in FilmCraft, which does the encoding. One place in the suite encodes, no licence question
in AssetCraft.

**Route 2 (planned, D-SF6): a native export when FilmCraft isn't installed.** What we can write
clean-room, and what plays where without installing anything (stock players: Windows Media
Player / Movies & TV, macOS QuickTime Player, GNOME Videos on Linux; every browser plays H.264
MP4 and AV1 WebM, Safari AV1 only on M3+):

| Route | File                               | Encoder / muxer (licence)                                                   | Windows | macOS QuickTime           | Linux | 90 s 1080p24 animatic | Status      |
|-------|------------------------------------|-----------------------------------------------------------------------------|---------|---------------------------|-------|-----------------------|-------------|
| C′    | `.mov` / `.mp4` · H.264 · PCM      | `openh264` (Cisco OpenH264, BSD-2, C++ compiled in); muxer pure Rust        | yes     | yes                       | yes   | ≈ 45–70 MB + 16 MB    | recommended |
| B     | `.mkv` / `.webm` · AV1 · FLAC/Opus | `rav1e` (BSD-2, Rust), `flacenc` (Apache-2) or libopus (BSD-3, C)           | Win 11  | macOS 14+ on M3+ only     | yes   | ≈ 25–45 MB            | option      |
| A     | `.avi` · Motion-JPEG · PCM         | none new (`image` encodes JPEG; RIFF/AVI and WAV written here)              | yes     | yes                       | yes   | ≈ 400–500 MB          | fallback    |
| C     | `.mp4` · H.264 · AAC via `ffmpeg`  | none linked: an `ffmpeg` found on `PATH` is run as a separate program       | yes     | yes                       | yes   | ≈ 45–70 MB            | optional    |
| —     | H.264 bundled from `x264`          | GPL                                                                         |         |                           |       |                       | out         |
| —     | VP8 / VP9 / HEVC                   | no clean-room encoder                                                       |         |                           |       |                       | out         |

H.264 is the most *compatible* codec, AV1 the best *compressor* (≈ 30–50 % smaller at the same
quality, slower to encode); Motion-JPEG stores every frame whole and is an order of magnitude
larger. The one gap in an H.264 export is the audio track — there is no clean-room AAC encoder
(`fdk-aac` is under Fraunhofer's own licence) — so the sound rides as PCM or Opus:

| MP4/MOV · H.264 + …                         | Windows      | macOS QuickTime | Linux | Size for 90 s stereo        |
|---------------------------------------------|--------------|-----------------|-------|-----------------------------|
| PCM in `.mov` (what Canon/Sony cameras write) | yes          | yes             | yes   | +16 MB (16-bit 44.1 k)      |
| PCM in `.mp4` (`ipcm`, ISO 23003-5)          | newer builds | macOS 14+       | yes   | +16 MB                      |
| Opus (libopus, BSD-3, C)                     | yes          | no              | yes   | +1 MB                       |
| AAC                                          | yes          | yes             | yes   | +1.5 MB — no clean encoder  |

So the universal clean-room file is **`.mov` · H.264 · PCM**; `.mp4` with `ipcm` is the second
flavour once QuickTime's floor is confirmed. Shipping an H.264 *encoder* in a public build is
patent-pool territory (Via LA; most base patents have expired) — a check for the project owner,
not something the code decides. Decode stays with FilmCraft either way.

## 3D assets

The viewer and the storyboard read 3D files in pure Rust (`crates/mesh`, no GPU, no threads;
every count and size in the file is checked before anything is allocated — a bad file is an
error, never a crash).

| Format               | Read                                                                                     | Not read (reported, never guessed)                                     |
|----------------------|------------------------------------------------------------------------------------------|------------------------------------------------------------------------|
| `.glb`               | meshes, node transforms, materials (base colour + textures), normals, `TEXCOORD_0`       | sparse accessors, Draco / meshopt compression, samplers, other UV sets |
| `.gltf` + `.bin`     | as GLB; external buffers and images resolved beside the file                             | same                                                                   |
| `.obj`               | positions, faces, per-face material slot                                                 | `vn` / `vt`, `.mtl`                                                    |
| `.fbx` `.stl` `.usd` `.blend` | type icon only                                                                   | — (ModelCraft territory; `docs/modelcraft-plan.md`)                    |

Limits (defaults, `assetcraft_mesh::Limits`): 2 M vertices, 2 M triangles, 128 MiB of buffers,
16 MiB of glTF JSON, 96 MiB of encoded images. The Organizer's viewer draws up to 500 k
triangles at full quality; above 120 k triangles a vertex-clustered *light copy* (≤ 60 k
triangles, `crates/mesh/src/simplify.rs`) is built once and drives drags, the turntable and
storyboard playback, so interaction never skips triangles. The Metadata panel shows
**Polygons** and **Textures** once the model has loaded; `assetcraft-cli info <file>` and
`asset.meshInfo` give the same facts headless (vertices, triangles, materials, images, bounds,
warnings).

What is drawn: smooth or flat shading under a three-point rig, bilinear base-colour textures,
wireframe, normals and UV-checker modes, a choice of background. Not yet: the
metallic-roughness map with an environment term, vertex colours, alpha (3D viewer V1, wgpu).
In the storyboard a model clip carries a camera motion (turntable, push in, rise…) as data on
the clip, rendered at full quality ahead of the playhead (Return builds the whole cache).

## What it will do

| Verb      | Highlights                                                                                     |
|-----------|------------------------------------------------------------------------------------------------|
| Browse    | Bridge-style folder navigator, thumbnails for every Craft format (PSD, raw, video, PDF, DOCX, 3D) |
| Structure | Catalog with undo, entities (characters, locations, props), version stacks, lineage            |
| Organize  | XMP ratings, labels and keywords (shared with LightCraft), collections, smart collections, batch rename |
| Plan      | [Storyline Builder](docs/storyline-builder.md): blocks, animatic preview, OTIO export into FilmCraft |
| Execute   | Recipes that drive the sibling apps headlessly, any-CLI steps, hot folders                     |

AI is optional. Each capability uses a provider chosen in Preferences: **Local**, **API**, or
**ArtCraft account**.

## Docs

| Doc                                                        | What it covers                                        |
|------------------------------------------------------------|-------------------------------------------------------|
| [docs/assetcraft-plan.md](docs/assetcraft-plan.md)         | Stages 0–7, Stage 0 in detail, feature backlog        |
| [docs/storyline-builder.md](docs/storyline-builder.md)     | Plan mode spec, slices A–F, provider design           |
| [docs/crafting-apps-notes.md](docs/crafting-apps-notes.md) | What we learned from each sibling app, with file refs |
| [docs/storyboard-plan.md](docs/storyboard-plan.md)         | Storyboard mode plan (Stage 4)                        |
| [docs/track-analysis.md](docs/track-analysis.md)           | Music analysis, cue maps, Fit to length               |
| [docs/design/cache.md](docs/design/cache.md)               | Suite cache design                                    |
| [docs/cli.md](docs/cli.md)                                 | `assetcraft-cli`: shipped and target surface          |
| [docs/control-protocol.md](docs/control-protocol.md)       | Loopback control channel (spec)                       |
| [docs/mcp.md](docs/mcp.md)                                 | MCP server (spec)                                     |
| [docs/agents.md](docs/agents.md)                           | Driving and developing AssetCraft as an agent         |
| [test_data/README.md](test_data/README.md)                 | The generated test library                            |
| [docs/archive/](docs/archive/)                             | Superseded plans and the ArtCraft Library history     |

## Stack (planned, following the siblings)

- Rust (edition 2024, 1.95+), egui/eframe on wgpu, WASM web build. No Tauri, Electron or JS.
- Every action is a command; opt-in MCP server and loopback control channel.
- `craft-ui` from [storytold/craft-libs](https://github.com/storytold/craft-libs) for docking
  and theming.

## Licence

MIT OR Apache-2.0 (to match the suite). Test images in `test_data/library` are generated
procedurally and released as CC0.
