# AssetCraft: development plan

_2026-10-12 · supersedes `archive/artcraft-library-stages.md` (the ArtCraft Library plan, kept for history)._

- **What it is:** a pure-Rust sibling of the storytold Crafting Apps for **browsing,
  structuring, organizing, planning and executing plans** over creative assets. It does what
  Adobe Bridge does, plus a Plan mode (the [Storyline Builder](storyline-builder.md)) and
  automation that drives the sibling apps.
- **Project goal it serves:** one place to find, organize and plan Reapers art (concepts,
  plates, video, trellis.cpp meshes), and then to produce from it.
- **Immediate area of improvement:** the suite has no Bridge. Every app opens its own files,
  and nothing sits above them.
- **Out of scope:** Elo, and editing pixels, video or documents. Editing stays in
  PhotoCraft, FilmCraft and the rest.
- **Reference:** [crafting-apps-notes.md](crafting-apps-notes.md) has the file-level
  learnings each stage draws on.

## Principles

1. **Read before write.** Browse first; nothing changes on disk until Stage 2.
2. **Reuse before build.** Call the siblings' CLIs first. Depend on their small standalone
   crates second. Build new only where the suite has a gap.
3. **Follow the house rules** from day one, so offering AssetCraft to the ArtCraft team is
   a packaging step, not a rewrite:
   - Rust only, egui + wgpu, WASM web build
   - everything is a command, with an opt-in loopback MCP server and control channel
   - never crash, clean room, layering checked by `xtask`
4. **Every stage ends with exit criteria** you can check by hand in the running app and with
   `cargo xtask ci`.
5. **AI is optional.** Providers are chosen in Preferences (Local, API, or ArtCraft
   account), and every feature works with none selected.

## Repository layout (from LightCraft, the closest sibling)

```
assetcraft/
├── apps/
│   ├── assetcraft/          # desktop (eframe)
│   ├── assetcraft-cli/      # headless: list, probe, thumbs, exec, mcp
│   └── assetcraft-web/      # WASM build (later stage)
├── crates/
│   ├── fs/        L0  directory listing behind a Services trait (real FS, fake FS for tests)
│   ├── probe/     L1  file type, dimensions, basic metadata
│   ├── thumbs/    L2  decode + downscale, job pool (disk cache in Stage 1)
│   ├── engine/    L5  Session, CommandSpec registry, nav/view/select commands
│   └── ui-egui/   L6  panels, grid, preview, craft-ui docking
├── test_data/library/       # 24 generated images (see test_data/README.md)
├── xtask/                   # ci: fmt, clippy, test, layers
├── AGENTS.md · CLAUDE.md · README.md · STATUS.md · ROADMAP.md
└── LICENSE-MIT · LICENSE-APACHE · NOTICE · ATTRIBUTION.md
```

## Stages

| Stage | Name                         | Delivers (short)                                                     |
|-------|------------------------------|----------------------------------------------------------------------|
| 0     | Navigator                    | Bridge-style read-only file browser for common image formats         |
| 1     | Every format, fast           | Disk thumbnail cache; PSD, raw, PDF, video, 3D thumbnails; folder watching |
| 2     | Organize                     | XMP ratings, labels, keywords; culling mode; batch rename; near-duplicates |
| 3     | Structure and find           | Catalog op log, collections, smart collections, search, version stacks, entities |
| 4     | Plan: storyline A + B        | Manual blocks, animatic preview, OTIO export into FilmCraft          |
| 5     | Plan: AI + generation, C + D | Provider preferences (Local, API, ArtCraft); AI storyline; directives |
| 6     | Execute                      | Recipes, any-CLI step, hot folders; storyline E + F (templates, FilmCraft assembly) |
| 7     | Connect and offer            | Lineage graph, "used in", shot coverage; packaging; pitch to the ArtCraft team |

### Stage 0: Navigator

A read-only file browser that feels like Bridge. No catalog, no writes, common image formats
only.

**Features**
- **Folders panel:** drives, home, Desktop/Pictures/Downloads, Favorites (add/remove), and an
  expandable folder tree loaded lazily.
- **Path bar:** breadcrumb you can click, an editable path field, and back, forward and up
  (Alt+←, Alt+→, Alt+↑).
- **Content panel:** thumbnail grid with a size slider, plus a details list (name, type, size,
  dimensions, modified). Folders come first. Sort by name, date, size or type. Show or hide
  hidden files.
- **Preview panel:** large preview of the selection, fit to the panel.
- **Metadata panel:** file name, path, size, dimensions, format, created and modified dates.
- **Selection:** click, Ctrl-click, Shift-click, Ctrl-A; arrow keys move; Enter opens a folder;
  Space shows a full-screen preview.
- **Open:** opens the file in its system default app. "Reveal in Explorer."
- **Formats:** JPG, PNG, WebP, GIF, BMP, TIFF using the `image` crate. Anything else shows a
  type icon.
- **Workspace:** panels dock through `craft-ui`. Layout, last folder, favorites and thumbnail
  size persist.

**Commands** (every UI action goes through `Session::execute`)
- `nav.open {path}`, `nav.back`, `nav.forward`, `nav.up`, `nav.refresh`
- `nav.favorite.add {path}`, `nav.favorite.remove {path}`
- `view.mode {grid|list}`, `view.thumbSize {px}`, `view.sort {by, ascending}`, `view.hidden {bool}`
- `select.set {paths}`, `select.all`, `select.none`
- `file.open {path}`, `file.reveal {path}`

**CLI**
- `assetcraft-cli list <dir> [--json]`
- `assetcraft-cli probe <file> --json`
- `assetcraft-cli thumb <file> --size 256 --out x.png`
- `assetcraft-cli commands`

**Steps**
1. Scaffold the workspace from LightCraft's layout: `Cargo.toml` (edition 2024,
   `rust-version = "1.95"`), lints, `xtask` with `layers.rs` and `ci`, licence files, AGENTS.md.
2. `crates/fs`: a `Services` trait (`list_entries`, `volumes`, `home_dir`, `known_folders`) with
   real and fake implementations, modelled on FilmCraft's `media_browser.rs`. Walks are
   bounded and symlink-safe, and never run on the UI thread.
3. `crates/probe`: type sniffing by magic bytes, not by extension, so
   `broken_not_an_image.jpg` is caught. Also dimensions from the image header without a full
   decode, and file times.
4. `crates/thumbs`: decode and downscale on a prioritized worker pool, where scrolling replaces
   queued jobs (LightCraft `preview/src/pool.rs` pattern). Memory LRU only in Stage 0.
5. `crates/engine`: `Session`, a `CommandSpec` registry with the `cmd!` macro, and navigation
   state (history, selection, favorites, prefs).
6. `crates/ui-egui` + `apps/assetcraft`: the panels above, with `craft-ui` docking pinned by rev.
7. `apps/assetcraft-cli`: `list`, `probe`, `thumb`, `commands`.
8. Tests: engine tests against the fake FS, plus probe and thumb tests over `test_data/library`
   covering every edge case there.

**Exit criteria**
- [ ] Browse from a drive root to `test_data/library` and into every subfolder using only
      the keyboard.
- [ ] All 23 images show thumbnails. The broken file shows an "unreadable" badge and nothing
      crashes.
- [ ] A folder of 2,000 images scrolls smoothly, and the first thumbnails appear in under a
      second.
- [ ] Back, forward and up, favorites, sort and view mode all work, and settings persist
      across a restart.
- [ ] `assetcraft-cli list test_data/library --json` matches what the UI shows.
- [ ] `cargo xtask ci` passes: fmt, clippy, tests, layers. No `unwrap` or `unsafe` in
      production crates.

### Stage 1: Every format, fast
- **Disk thumbnail cache** keyed by content hash (LightCraft `preview/src/disk.rs` pattern).
- **Folder watcher** (`notify`): new, renamed and deleted files update live. This is a suite
  gap, so it's new work.
- **Thumbnails by format:**
  - PSD via `photocraft-psd`
  - raw and HEIF via the LightCraft crates
  - PDF via `pdfcraft-render`
  - video poster frames via `filmcraft-cli probe`
  - GLB/glTF/OBJ through a small wgpu renderer
- **"Open in…"** a sibling app (PhotoCraft, FilmCraft, LightCraft, PdfCraft, DesignCraft,
  WordCraft) when it's installed.
- **Exit:** a mixed folder shows correct thumbnails for every type, and a file copied in
  appears without a refresh.

### Stage 2: Organize
- XMP sidecar ratings, labels and keywords via `lightcraft-meta`, merged without disturbing
  other tools' fields. They round-trip with LightCraft.
- Culling mode, batch rename (LightCraft `rename.rs` rules), and a near-duplicate finder
  using content hash plus perceptual hash.
- **Exit:**
  - rate a file in AssetCraft and the rating shows in LightCraft
  - batch rename can be undone
  - the files in `environments/dupes` group together

### Stage 3: Structure and find
- Catalog as an append-only op log plus snapshots (LightCraft `catalog/journal.rs`).
- Collections and smart collections, the search syntax (`rating:>3 kw:reaper`), version
  stacks (`_v01`…`_final`) and entities (characters, locations, props).
- **Exit:**
  - `characters/reaper` stacks into one versioned item
  - a smart collection survives a restart
  - search returns results in under 200 ms on 10,000 items

### Stages 4 to 7
- **4 and 5:** Storyline Builder slices A–D. See
  [storyline-builder.md](storyline-builder.md) §10–11.
- **6:** Recipes (multi-app pipelines with dry run and resume), any-CLI steps (trellis.cpp,
  ffmpeg, Blender headless), hot folders, and storyline slices E–F.
- **7:** Lineage graph, "used in" across FilmCraft and DesignCraft projects, shot coverage,
  delivery packages, packaging, and a proposal to the ArtCraft team. That proposal also covers
  the ArtCraft-account provider.

## Feature backlog → stage

| Feature                         | Stage | Size |
|---------------------------------|-------|------|
| 3D thumbnails + viewer          | 1     | M    |
| Folder watcher                  | 1     | M    |
| Culling mode                    | 2     | S    |
| Near-duplicate finder           | 2     | S    |
| Colour search + palettes        | 2     | S    |
| Version stacks                  | 3     | S–M  |
| Entities                        | 3     | M    |
| Folder schema checker           | 3     | M    |
| Review states + pinned notes    | 3     | M    |
| Animatic playback               | 4     | M    |
| Provider preferences            | 5     | M    |
| Recipes + any-CLI step          | 6     | L    |
| Hot folders                     | 6     | M    |
| Sheet builder (→ DesignCraft)   | 6     | S–M  |
| Delivery packages               | 6     | S–M  |
| Lineage graph + "used in"       | 7     | M–L  |
| Shot coverage                   | 7     | M    |

## Open decisions

| Decision                                     | Recommendation                                        | Needed by |
|----------------------------------------------|-------------------------------------------------------|-----------|
| Repo                                         | Decided: this repo, renamed to `assetcraft`           | Done      |
| Study VectorCraft and EffectCraft            | Yes, before Stage 1 (SVG/AI and motion thumbnails)    | Stage 1   |
| Sibling crates: git dependencies or CLIs     | CLIs first; depend on small standalone crates when stable | Stage 1 |
| ArtCraft-account provider terms              | Ask the ArtCraft team before building it              | Stage 5   |
