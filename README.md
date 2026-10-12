# AssetCraft

Browse, structure, organize, plan and execute: an asset app for creative work, in the spirit
of Adobe Bridge. Built in pure Rust as a sibling of the storytold
[Crafting Apps](https://getartcraft.com/apps) (PhotoCraft, FilmCraft, LightCraft, PdfCraft,
DesignCraft, WordCraft).

- **Project goal it serves:** one place to find, organize and plan creative assets
  (concepts, plates, video, 3D meshes), and then to produce from them.
- **Immediate area of improvement:** the Crafting Apps have no Bridge. Each app opens its own
  files, and nothing sits above them to find, group, plan and hand off work.

**Status:** Stage 0 (Navigator) in progress. See [STATUS.md](STATUS.md).

## Build (Windows, PowerShell)

```powershell
Set-Location F:\Script-Projects\assetcraft
cargo xtask ci                     # fmt, clippy, tests, layers
cargo run -p assetcraft-cli -- --help
```

Needs Rust 1.95 or later (`rustup update stable`).

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
