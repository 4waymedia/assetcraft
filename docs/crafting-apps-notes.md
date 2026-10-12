# Crafting Apps: learning notes for AssetCraft

_2026-10-12 · read-only study of the storytold Crafting Apps, to decide what AssetCraft
reuses, copies, or builds new._

- **Project goal it serves:** AssetCraft is a place to **browse, structure, organize, plan
  and execute plans** over creative assets, as a sibling of the Crafting Apps.
- **Immediate area of improvement:** the suite has no Bridge. Every app opens its own
  files, and nothing sits above them to find, group, plan and hand work off.
- **Scope of these notes:** patterns and features worth building on, with file references.
  Elo is out of AssetCraft. The SDM idea is parked in its own section at the end.

## Repos studied

| Repo        | Rebuilds      | Commit    | Notes                                         |
|-------------|---------------|-----------|-----------------------------------------------|
| lightcraft  | Lightroom     | `701f78e` | Catalog, XMP, previews, MCP                   |
| filmcraft   | Premiere Pro  | `71438c6` | Media Browser, Project panel, interchange     |
| photocraft  | Photoshop     | `d43db3e` | Actions, batch, `serve`, PSD                  |
| designcraft | InDesign      | `17cf5b4` | Page/frame model, links, canvas, IDML/PDF     |
| pdfcraft    | Acrobat       | n/a       | Render, organize grid, compact MCP            |
| wordcraft   | Word          | `159b84f` | Flow model, DOCX, Zotero bridge               |
| craft-libs  | shared        | `f58bc1a` | `craft-ui`, `craft-atomic`, print, quit       |

Not studied yet: VectorCraft (Illustrator), EffectCraft (After Effects).

## 1. House rules every sibling follows (adopt as-is)

- **Rust only.** egui/eframe on wgpu; the web build is the same Rust compiled to WASM.
  No Tauri, Electron or JS.
- **Layout:** `apps/<name>`, `apps/<name>-cli`, `apps/<name>-web`, `crates/*`, `xtask`.
- **Layering enforced at build time:** `xtask/src/layers.rs` holds a crate-to-layer table, and
  UI crates (egui, eframe, winit, rfd, craft-ui) are banned below the UI layer.
  Reference: `lightcraft/xtask/src/layers.rs:35-96`. It runs under `cargo xtask ci`.
- **Everything is a command:** `CommandSpec { id, label, menu, shortcut, params, enabled, run, journal }`
  - LightCraft: `crates/engine/src/cmd/mod.rs:47`, with the `cmd!` macro at :120
  - FilmCraft: `crates/engine/src/commands.rs:23`
  - One entry point, `Session::execute(id, params)`. The UI, keys, CLI, control channel and MCP
    all dispatch through it.
- **Agent-drivable by default:**
  - an opt-in MCP server over stdio
  - an opt-in control channel on loopback only
  - every feature reachable headless, with an end-to-end test
- **Never crash:** production crate roots deny `unwrap`, `expect` and `panic`, and forbid
  `unsafe`. Input is treated as hostile.
- **Clean room:** no Adobe or Microsoft assets, no GPL code. Every asset needs an attribution
  entry (`cargo xtask assets`). Fonts come from `storytold/craft-fonts`. ArtCraft brand files
  are trademarked, so drop them in our own repo.
- **Shared UI:** `craft-ui` is pinned by git rev (`5c874d5` in FilmCraft and PhotoCraft).
- **Not public:** `craftrules`, the suite standards. Ask a maintainer before Stage 7.

## 2. Learnings by AssetCraft capability

Reuse modes: **Depend** (git dependency pinned by rev) · **Copy** (copy the pattern into our
own crate) · **Call** (drive the sibling's CLI or MCP) · **Build** (nothing to reuse).

### Browse

| Feature                              | Learn from                                                                      | Mode   |
|--------------------------------------|---------------------------------------------------------------------------------|--------|
| Folder browsing behind a `Services` trait (fake FS in tests, works on web) | FilmCraft `crates/engine/src/media_browser.rs` (~l.700-770)   | Copy   |
| Favorites, drives, recent, back/forward/up, saved prefs | FilmCraft `media_browser.rs`: `MediaBrowserPrefs` l.83, `BrowserState` l.118 | Copy |
| Browse in place without importing    | LightCraft `crates/engine/src/cmd/browse.rs` (`library.browse`, `local.rs`)     | Copy   |
| Bounded, symlink-safe walks, never on the UI thread | LightCraft `crates/engine/src/walk.rs`                           | Copy   |
| Thumbnail disk cache (content hash, `<2hex>/<32hex>.jpg`, LRU prune to 80%) | LightCraft `crates/preview/src/disk.rs`, `hash.rs`, `lru.rs` | Depend or Copy |
| Prioritized job pool where scrolling replaces queued work | LightCraft `crates/preview/src/pool.rs`                      | Copy   |
| Video hover scrub, poster frame, nearest-cached-frame fallback | FilmCraft `crates/ui-egui/src/frames.rs`, `panels/media_browser.rs` l.583-705 | Copy |
| Video probe + poster frame           | FilmCraft `MediaPool::probe_file` (`engine/src/media_pool.rs:239`) or `filmcraft-cli probe <file>` | Call first, Depend later |
| PSD thumbnail (resource 1036) or merged composite | PhotoCraft `crates/psd` (standalone: `thiserror`, `flate2`)       | Depend |
| PDF page-1 thumbnail                 | PdfCraft `PageRenderer` (`crates/render/src/raster.rs:421`); capped pool in `ui-egui/src/combine_grid.rs` | Depend |
| PDF metadata (cheap)                 | PdfCraft `organize::page_count` / `organize::info` (cos only)                   | Depend |
| DOCX page-1 render, core props       | WordCraft `engine/src/io_ext.rs:40 render_png`; `Document.core`                 | Call   |
| Raw, HEIF, TIFF decode               | LightCraft `crates/raw`, `heif`, `tiff`                                         | Depend |
| Grid ↔ list views, virtualized rows, card stacks for multi-page files | PdfCraft `canvas.rs` `organize_grid` (:4195), `combine_grid.rs` | Copy |

### Structure (metadata and the data model)

| Feature                              | Learn from                                                                      | Mode   |
|--------------------------------------|---------------------------------------------------------------------------------|--------|
| Append-only op log + snapshot, CRC per line, atomic replace, compaction | LightCraft `crates/catalog/src/journal.rs`              | Copy   |
| `apply()` returns the inverse op (undo/redo, replay) | LightCraft `crates/catalog/src/lib.rs` (`enum Op` :76)           | Copy   |
| Pluggable store (FS, memory, OPFS for web), one-process lock | LightCraft `catalog/src/store.rs`, `lock.rs`             | Copy   |
| Extensible item metadata as string keys, so new features need no schema bump | FilmCraft `ProjectItem.metadata` (`crates/project/src/lib.rs:296`); Freeform keys at `engine/src/project_panel.rs:502` | Copy |
| Asset links: path + hash, `ok/modified/missing` status, relink folder | DesignCraft `crates/engine/src/cmd/links.rs`. It embeds bytes; **we shouldn't** | Copy |
| Versioned JSON file envelope with single-step migrations, refuse `TooNew` | FilmCraft `crates/format/src/lib.rs` (now schema v15)     | Copy   |
| Crash-safe writes                    | `craft-atomic` (`atomic_write`, `write_new`)                                    | Depend |

### Organize

| Feature                              | Learn from                                                                      | Mode   |
|--------------------------------------|---------------------------------------------------------------------------------|--------|
| Ratings 0–5, pick/reject flag, 5 colour labels | LightCraft `crates/catalog/src/model.rs` `Photo` l.220                  | Copy   |
| Hierarchical keywords (`a\|b`), synonyms, rename/merge as one undo | LightCraft `crates/catalog/src/keywords.rs`               | Copy   |
| **XMP sidecars, read + write, merge preserving other tools' fields** | LightCraft `crates/meta` (`xmp.rs`, `xmp_merge.rs`); policy in `engine/src/sidecar.rs`; `docs/xmp-interop.md` | Depend |
| Search tokens (`rating:>3 flag:pick kw:x`) | LightCraft `crates/catalog/src/query.rs` (`token_matches` :115)           | Copy   |
| Smart collections (`all/any/none`, nested groups, typed field table) | LightCraft `crates/catalog/src/rules.rs` (`FIELDS` :63) | Copy |
| Stacks (ordered, top first, collapsed) | LightCraft `model.rs` `Stack` l.22; FilmCraft Freeform stacks                  | Copy   |
| Batch rename tokens, no overwrite, moves sidecars, undo moves files back | LightCraft `crates/engine/src/rename.rs`               | Copy   |
| List columns derived from all metadata keys; 10 view presets | FilmCraft `project_panel.rs` l.281-287, `project.viewPreset.*`    | Copy   |
| Auto-tag candidates (opt-in models, never bundled) | LightCraft `faces`, `geocode` (offline), `segment` (SAM); FilmCraft `speech` (Whisper), `scene_detect.rs` | Later |

XMP is the interop contract: a rating or keyword set in AssetCraft also shows up in LightCraft,
and in Bridge or Lightroom if those are installed.

### Plan

| Feature                              | Learn from                                                                      | Mode   |
|--------------------------------------|---------------------------------------------------------------------------------|--------|
| Freeform board: positions, stacks, named arrangements in item metadata | FilmCraft Freeform (`project.freeform.*`, `project_panel.rs`) | Copy |
| Canvas transform (zoom 0.05–40, `zoom_at`, fit to bounds) | DesignCraft `crates/ui-egui/src/canvas.rs:22-170`                  | Copy   |
| Snapping, smart guides, align/distribute, equal spacing | DesignCraft `crates/tools/src/snap.rs`, `object.align/distribute`    | Copy   |
| Page/frame model for printed boards (pages, margins, columns, frames with fitting and crop, captions) | DesignCraft `crates/doc` (`Item` `item.rs:930`, `Graphic` :444) | Depend or Copy |
| Docking and workspaces               | `craft-ui::docking` (as used in FilmCraft `ui-egui/src/dock_shared.rs`)         | Depend |
| Storyboard → timeline                | FilmCraft `crates/interchange/src/otio.rs`: Clip + `source_range` per panel, `Gap` for blanks, `Marker.2` for notes, missing media imports offline | Build (write OTIO) |
| Storyboard → printable layout        | Write a `.designcraft` file (zip of `document.json` + assets) or a `designcraft-cli script` (`frame.grid`, `file.place`, `object.caption`, `file.exportPdf`) | Call |

### Execute

| Sibling     | Headless entry                                   | Live app entry                          | Auth                |
|-------------|--------------------------------------------------|-----------------------------------------|---------------------|
| PhotoCraft  | `photocraft-cli serve` (stdio JSON lines), `droplet`, `batch --actions` | `--control <port> --control-token-file` | Token handshake     |
| FilmCraft   | `filmcraft-cli exec` / `run -` / `mcp`           | `filmcraft --control`, CLI `--bridge`   | Loopback only       |
| LightCraft  | `lightcraft-cli`, MCP (headless)                 | `--control PORT`                        | Loopback only       |
| DesignCraft | `designcraft-cli run` / `script` / `mcp`         | `mcp --connect PORT`                    | **None**            |
| PdfCraft    | `pdfcraft-cli run` / `mcp --root DIR`            | n/a found                               | Root-confined       |
| WordCraft   | `wordcraft-cli`, MCP with read/write roots       | `--connect`                             | Read/write roots    |

- **Recommended way to run plans:** AssetCraft keeps one long-lived headless session per
  sibling (PhotoCraft `serve` is the model). A plan becomes a list of `{command, params}`
  steps sent to that session, with read and write roots limited to the project folder.
- **PhotoCraft action JSON:** `["id", {params}]` or `{"command","params"}` steps; named
  actions in `{"version":1,"actions":[…]}` (`crates/engine/src/actions_cmds.rs:32`).
- **MCP design to copy:** PdfCraft's compact mode (`crates/automation/src/mcp.rs:41`). It
  lists a small core set plus `tool_search` and `tool_call`, rejects unknown keys, and
  marks tools read-only or destructive.

## 3. Gaps AssetCraft fills itself (opportunities)

| Gap                                                   | Suggested action                                                              | Owner      |
|-------------------------------------------------------|-------------------------------------------------------------------------------|------------|
| No file-system watcher anywhere in the suite          | Build a `watch` crate on `notify`; feed it into the job pool                   | AssetCraft |
| FilmCraft thumbnails are in memory only               | Reuse the LightCraft-style disk cache for video poster frames                  | AssetCraft |
| `photocraft-psd` reads the whole file to get a thumbnail | Contribute a `read_resources()` that stops after the resources section     | Upstream PR |
| No XMP reader in PdfCraft (waiting on its M2 `model`) | Use LightCraft `meta` for PDF XMP packets                                       | AssetCraft |
| No DOCX embedded-thumbnail reader in WordCraft        | Read `docProps/thumbnail.*` from the zip first, then fall back to a render      | AssetCraft |
| DesignCraft control channel has no auth               | AssetCraft's client supports both styles; suggest PhotoCraft's token upstream   | Upstream issue |
| A Bridge for the suite                                | AssetCraft itself                                                              | Paul       |

## 4. Draft AssetCraft crate layout (to confirm in Stage 0)

| Layer | Crates                                                                              |
|-------|-------------------------------------------------------------------------------------|
| L0    | `geom`, `hash`                                                                      |
| L1    | `meta` (XMP/EXIF via LightCraft `meta`), `probe` (per-format adapters)              |
| L2    | `catalog` (op log, assets, keywords, collections, rules), `thumbs` (cache + pool), `watch` |
| L3    | `boards` (moodboard, storyboard, shot list), `interchange` (OTIO, DesignCraft script) |
| L4    | `bridge` (sibling sessions: serve, CLI, control, MCP clients)                       |
| L5    | `engine` (commands, `Session::execute`)                                             |
| L6    | `ui-egui` (craft-ui docking), `mcp`                                                 |
| Apps  | `assetcraft`, `assetcraft-cli`, `assetcraft-web`                                    |

Recommendation: start with LightCraft as the template. It's closest to AssetCraft and the
most self-contained.

## 5. Parked: SDM refactor candidates (separate from AssetCraft)

- **PdfCraft is a poor fit for replacing the model.** Its rule is "the object graph is the
  document model" (`crates/cos/README.md`), and text lives only in content streams. An SDM
  could only be an export or projection: a new L4 crate next to `export`, built from
  `render::PageText` and the structure tree.
- **WordCraft is the better fit.**
  - Each paragraph is one `String` with `runs: Vec<Run{len, props}>` laid over it, which is
    already a contiguous track (`crates/doc/src/para.rs`).
  - Start/end marker objects (bookmarks, comments, fields, permissions) are range tracks.
  - Revisions are an id into a document table, which is an attribute-as-id.
  - `crates/formats/src/model.rs` already has a small Markdown-like block vocabulary
    (`Kind`: Normal, Heading 1–6, Title, Quote, Code, Rule) with `to_doc` / `from_doc`.
- **Lowest-risk first step when that lane opens:** add SDM as one more format next to
  `formats/model.rs` before touching `doc::Paragraph`. Costs to plan for:
  - positions are byte offsets per paragraph (`doc/src/lib.rs:169-214`)
  - the layout cache keys on `Paragraph.rev`

## 6. Open questions

1. Workspace rename: `ArtCraft-Library` → `AssetCraft`, with docs moving into the new repo.
2. Study VectorCraft and EffectCraft too? They matter for SVG/AI and motion asset thumbnails.
3. First real asset folder for Stage 0 testing, e.g. the Reapers art and trellis.cpp outputs.
4. Dependency policy: git dependencies on sibling crates (fast, but coupled to their revs),
   or calling their CLIs (loose, but slower). Recommendation: call CLIs first, then depend on
   a few small, standalone crates (`photocraft-psd`, `lightcraft-meta`, `lightcraft-preview`).
