# AssetCraft

Browse, structure, organize, plan and execute: an asset app for creative work, in the spirit
of Adobe Bridge. Built in pure Rust as a sibling of the storytold
[Crafting Apps](https://getartcraft.com/apps) (PhotoCraft, FilmCraft, LightCraft, PdfCraft,
DesignCraft, WordCraft).

- **Project goal it serves:** one place to find, organize and plan creative assets
  (concepts, plates, video, 3D meshes), and then to produce from them.
- **Immediate area of improvement:** the Crafting Apps have no Bridge. Each app opens its own
  files, and nothing sits above them to find, group, plan and hand off work.

**Status:** Stage 1 (Every format, fast) done, pending a Windows check. See [STATUS.md](STATUS.md).

## Build (Windows, PowerShell)

```powershell
Set-Location F:\Script-Projects\assetcraft
cargo xtask ci                     # fmt, clippy, tests (default + heic), layers, deny
cargo run -p assetcraft --release -- "F:\Script-Projects\assetcraft\test_data\library"
cargo run -p assetcraft-cli -- --help
```

Needs Rust 1.95 or later (`rustup update stable`).

## Optional formats

### HEIC / HEIF (off by default)

HEIC photos (iPhone default) use the HEVC codec, which may be patent-encumbered in some
countries. Like LightCraft and PhotoCraft, AssetCraft leaves it out unless you opt in, in two
steps:

1. **Build with the `heic` feature.** Without it the decoder (`lightcraft-heif` over
   `heic-rs`) isn't compiled at all.
   ```powershell
   Set-Location F:\Script-Projects\assetcraft
   cargo run -p assetcraft --release --features heic
   ```
2. **Turn it on in the app:** View → Formats → **HEIC / HEIF thumbnails (HEVC)**. The choice is
   saved in your preferences (`formats.heic`) and thumbnails re-render straight away.

Until both are done, HEIC files show a type icon. In a build without the feature the menu
item is greyed, and its tooltip says to rebuild with `--features heic`. The switch is the
`formats.heic {"enabled": true}` command, so the menu, keys and future automation share it.

The CLI doesn't read the app's preferences; pass `--heic` per call instead:

```powershell
Set-Location F:\Script-Projects\assetcraft
cargo build -p assetcraft-cli --release --features heic
.\target\release\assetcraft-cli.exe thumb photo.heic --size 256 --out photo.png --heic
```

For anything you distribute, decide on HEVC licensing first; builds without the feature
contain no HEVC code.

### Video poster frames

Video thumbnails (MP4, MOV, MKV/WebM, AVI) come from FilmCraft's `filmcraft-cli`, run as a
separate program. Put it on `PATH` or set `ASSETCRAFT_FILMCRAFT_CLI` to its full path, then
restart AssetCraft. Without it, videos show a type icon.

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
| [docs/assetcraft-api.md](docs/assetcraft-api.md)           | Stage 1.5 API: serve, MCP, live control, command effects |
| [docs/storyline-builder.md](docs/storyline-builder.md)     | Plan mode spec, slices A–F, provider design           |
| [docs/crafting-apps-notes.md](docs/crafting-apps-notes.md) | What we learned from each sibling app, with file refs |
| [test_data/README.md](test_data/README.md)                 | The generated test library                            |
| [docs/archive/](docs/archive/)                             | The earlier ArtCraft Library plan (superseded)        |

## Stack (planned, following the siblings)

- Rust (edition 2024, 1.95+), egui/eframe on wgpu, WASM web build. No Tauri, Electron or JS.
- Every action is a command; opt-in MCP server and loopback control channel.
- `craft-ui` from [storytold/craft-libs](https://github.com/storytold/craft-libs) for docking
  and theming.

## Licence

MIT OR Apache-2.0 (to match the suite). Test images in `test_data/library` are generated
procedurally and released as CC0.
