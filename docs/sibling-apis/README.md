# Sibling app APIs: what AssetCraft can call

_2026-10-15 · read from source at each app's latest release tag; nothing was built. Detail, with
`file:line` for every claim and the full command-id lists, is in the three files below._

| File                                                                         | Apps                                 |
|------------------------------------------------------------------------------|--------------------------------------|
| [photocraft-lightcraft-filmcraft.md](photocraft-lightcraft-filmcraft.md)     | PhotoCraft, LightCraft, FilmCraft    |
| [pdfcraft-wordcraft-designcraft.md](pdfcraft-wordcraft-designcraft.md)       | PdfCraft, WordCraft, DesignCraft     |
| [effectcraft-vectorcraft-soundcraft.md](effectcraft-vectorcraft-soundcraft.md) | EffectCraft, VectorCraft, SoundCraft |

- **Scope:** every surface an outside program can use (launch args, CLI, command registry, MCP,
  control channel, library crates, file formats).
- **Goal it serves:** AssetCraft drives the suite instead of re-implementing it (plan principle 2,
  "reuse before build"), and the Claude Design screens for the next stages call real APIs.
- **Live check:** counts come from a static read and may be off by a few ids. The authoritative
  list is `<app>-cli commands --json` (or `engine.commands` over the control channel).

## The shared shape (all nine apps)

Every app follows the same suite conventions, so one AssetCraft client can talk to all of them:

1. **Command registry.** Every action is a command id with JSON params, the same pattern as
   AssetCraft's own `Session::execute`.
2. **Headless CLI** (`<app>-cli`) that runs those commands without a window, plus an **MCP
   server on stdio** (headless, or bridged to the running app in most apps).
3. **Control channel**: opt-in (`--control <port>` at launch), JSON lines on loopback,
   envelope `{"id","method","params"}` → `{"id","ok","result"|"error"}`, with
   `engine.execute {command, params}` and `engine.commands`. This is the only way to drive an
   app that's **already open**.

## At a glance

| App         | Tag     | Commands (engine + UI) | MCP tools | Control auth          | Headless script mode                    | Hands files to a running app |
|-------------|---------|------------------------|-----------|-----------------------|-----------------------------------------|------------------------------|
| PhotoCraft  | v0.9.0  | 863 + 76               | 25        | 256-bit token         | `run`, `batch`, `droplet`, `serve`      | macOS only (Apple events)    |
| LightCraft  | v0.8.0  | 312 + 170              | 28 + one per command | **none**   | `run` / `render`                        | no                           |
| FilmCraft   | v0.8.0  | 702 + 211              | 17        | **none**              | `run <script.jsonl>`                    | no                           |
| PdfCraft    | v0.8.0  | 194 (163 as tools)     | 163       | token file (JSON-RPC 2.0) | `run --script steps.json`           | Windows only                 |
| WordCraft   | v0.7.0  | 617 + ~30              | 16        | key file              | repeated `run --cmd id={json}`          | no                           |
| DesignCraft | v0.8.0  | 459 + 106              | 24        | **none**              | `script FILE.jsonl` (headless or `--connect`) | no (one process, many docs) |
| EffectCraft | v0.10.0 | 731 + 111              | 33        | **none**              | CLI subcommands, e.g. `render-frame`    | no                           |
| VectorCraft | v0.11.0 | 743 + 285              | 32        | **none**              | MCP / control                           | no                           |
| SoundCraft  | v0.7.0  | 478 + 66               | 16        | **none**              | `app --port P <cmd>`                    | no                           |

All are MIT OR Apache-2.0. SoundCraft lists itself as a Crafting App, but the EffectCraft and
VectorCraft READMEs don't list it yet.

## What AssetCraft needs, and the surface that does it

| AssetCraft job                         | Best surface today                                                                                     | Stage |
|----------------------------------------|---------------------------------------------------------------------------------------------------------|-------|
| PSD thumbnail                          | `photocraft-psd` crate (in use); `photocraft-format::read_thumbnail` for `.pcraft`                      | 1 ✅   |
| Raw / HEIC thumbnail                   | `lightcraft-raw::embedded_preview_covering`, `lightcraft-heif::decode_thumbnail` (in use)               | 1 ✅   |
| PDF thumbnail + page size              | `pdfcraft-render` (in use); fast page size via `pdfcraft_cos::Document::open` + `pdfcraft_model::pages()[i].display_size()` | 1 ✅ |
| Video poster frame                     | `filmcraft-cli run` script (in use); in-process `filmcraft-media::open_path` + `video_frame` is faster  | 1 ✅   |
| SVG / AI / EPS thumbnail               | VectorCraft `file.place.info {path, thumbnail: 256}` → `thumbnailBase64`; `.vectorcraft` embedded preview via `vectorcraft-format::preview()` | new |
| EffectCraft comp / template poster     | `effectcraft-cli render-frame --project F.ecproj --comp C --time T --max-side 256 --out p.png --json`; `.ectemplate` ZIPs already contain `poster.png` | new |
| Audio duration + waveform              | `soundcraft-audio-io::probe` (sample rate, channels, length, BWF) and `peaks::Peaks::build`; AssetCraft draws the waveform itself | new |
| DOCX / IDML / `.designcraft` thumbnail | Spawn `wordcraft-cli render` / `designcraft-cli run --export` (smaller than linking their crates)       | new |
| Open selection in a running app        | Launch with `--control <port>`, then send: PhotoCraft `app.open`, LightCraft `library.import`, FilmCraft `file.open`/`file.import`, EffectCraft `file.open`, VectorCraft `app.open`, SoundCraft `session.open`, DesignCraft/WordCraft open commands | Open In v2 |
| Open in LightCraft **Develop**         | `library.import`, then `ui.set {"view":"detail","right":"edit"}`                                         | 3.5   |
| Camera Raw–compatible XMP              | LightCraft `crs.rs` + `sidecar.rs`, still inside `lightcraft-engine`                                    | 2 / 3.5 |
| Contact sheet / layout from selection  | DesignCraft `designcraft-cli script`: `frame.grid` → `file.place` per image → `object.fit` → save/export | 6     |
| Assemble PDF from selection            | PdfCraft `page.combine`, headless only (`run --script`)                                                 | 6     |
| Timeline to FilmCraft                  | OTIO / FCPXML / EDL (FilmCraft interchange crate; EffectCraft already uses it via `file.exportTimeline`) | 4     |

## Opportunities (scope · fix · owner)

| #   | Finding                                                                                          | Scope            | Suggested fix                                                                                     | Owner                       |
|-----|--------------------------------------------------------------------------------------------------|------------------|---------------------------------------------------------------------------------------------------|-----------------------------|
| O1  | No app hands files to an already-open window on Windows (except PdfCraft); a second launch opens a second app | Open In | Each app: single-instance forwarder. AssetCraft meanwhile: launch with `--control <port>` and keep the port, then reuse it | Each app; AssetCraft (interim) |
| O2  | Control channels have no auth in LightCraft, FilmCraft, DesignCraft, EffectCraft, VectorCraft, SoundCraft | Security | Adopt PhotoCraft's token handshake (or PdfCraft's token file)                                       | Each app                    |
| O3  | Three different control handshakes (PhotoCraft token, PdfCraft JSON-RPC, WordCraft key) and port clashes (VectorCraft and SoundCraft both default to 7979) | Every integration | A small shared `craft-control` client crate with one handshake and a port file per app; until then AssetCraft assigns explicit ports | craft-libs; AssetCraft (interim) |
| O4  | PhotoCraft's control `app.open` only accepts paths under `--automation-read-root`; path-taking `file.*` commands are refused over control/MCP | Open In PhotoCraft | AssetCraft launches PhotoCraft with the folder as read root; PhotoCraft could add an approved-roots open | AssetCraft; PhotoCraft |
| O5  | WordCraft's control open replaces the current document **without the save prompt**              | Open In WordCraft | Route control opens through the guarded path. AssetCraft: don't use WordCraft control-open until fixed; launch a new window instead | WordCraft; AssetCraft |
| O6  | LightCraft v0.8.0 still pins quick-xml 0.38.4 (RUSTSEC-2026-0194/0195)                         | Stage 1 deps     | LightCraft bumps quick-xml; AssetCraft moves its pin                                                | LightCraft, then Claude     |
| O7  | `crs.rs`/`sidecar.rs` still inside `lightcraft-engine`                                          | Stages 2, 3.5    | Split into a small crate (e.g. `lightcraft-xmp-develop`); `crs.rs` only needs `lightcraft-develop` + `serde_json` | LightCraft (ask), else AssetCraft carries a fork |
| O8  | No one-line thumbnail command in PhotoCraft, FilmCraft, WordCraft, DesignCraft; no waveform PNG in SoundCraft; EffectCraft `render-frame` uses the playhead, not `poster_time` | Thumbnails | A `thumbnail {path, maxSide}` command per app (suite convention); EffectCraft: `--poster` flag | Each app |
| O9  | `pdfcraft-render` has no cheap page-size query                                                  | PDF metadata     | Add `page_sizes()`; AssetCraft can use `pdfcraft-cos`/`-model` directly now                          | PdfCraft; Claude (interim)  |
| O10 | AssetCraft pins are one release behind: PhotoCraft 0.8.2 → 0.9.0, LightCraft 0.7.1 → 0.8.0, PdfCraft 0.7.1 → 0.8.0 (PdfCraft's patch set is the same five crates, re-pointed) | Stage 1 deps | Bump all three pins together, run `cargo xtask ci`                                       | Claude                      |
| O11 | WordCraft and DesignCraft crates have no version numbers, so they can only be git deps by tag   | DOCX/IDML thumbnails | Prefer spawning their CLIs; ask for versions if we link later                                   | AssetCraft; those apps      |
| O12 | Open In catalog has six apps; EffectCraft, VectorCraft (and SoundCraft) aren't in it            | Open In          | Add them to `crates/engine/src/open_in.rs` (installer dirs and binaries are in the detail files)   | Claude                      |

## Recommendation for the next build

- **Before the Claude Design screens are built:** do O10 (bump the pins) and O12 (add EffectCraft,
  VectorCraft, SoundCraft to Open In). Both are small and keep Stage 1 current.
- **New thumbnail formats** (SVG/AI/EPS, EffectCraft, audio) fit a "Stage 1.5" before Stage 2, if
  your designs show them. Otherwise they can wait.
- **A shared control client** (O3) belongs in AssetCraft's engine as `crates/craft-control` until
  craft-libs has one. It is what "Open In v2" (open in the *running* app) and Stage 6 recipes need.
