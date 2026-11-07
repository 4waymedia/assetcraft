# AssetCraft roadmap

_2026-11-06 · Stage 4 (Storyboard mode) shipping, Stages 1.5 and 2 under way · the stable index; `STATUS.md` is the working log_

- **Scope:** every milestone of AssetCraft in one place — shipped, building, planned, and the
  opportunities and ideas collected from the plan docs, the status log and the audits — with the
  decision each one waits on. Facts come from the docs linked in each row; nothing here is new.
- **Project goal it serves:** anyone (Paul on the Mac or the PC, a reviewer, an agent) can see what
  AssetCraft does today and what comes next without reading the log.
- **Immediate area of improvement:** the roadmap lived in four places (the plan's stages, the
  "Now" table, the storyboard plan's step tables, the audit's waves). This file indexes them; when
  a row changes, update it here and in its source doc in the same commit.

Status words: **shipped** (committed, on main), **building** (code delivered, being tested or
committed), **next** (the step after the one building), **planned** (specced, ordered, not started),
**idea** (logged, not specced), **decided** / **open** (for decisions).

## 1. Shipped

| Milestone | What it delivers | Date · commit | Doc |
|-----------|------------------|---------------|-----|
| Stage 0 Navigator | Bridge-style read-only browser: folders, Computer, favorites, recent, back/forward/up, grid/list, sort, selection, thumbnails for common images, Preview panel, Metadata panel, prefs saved atomically | 2026-10-12 → 10-15 | `docs/assetcraft-plan.md` § Stage 0 |
| Stage 1 Every format, fast | Disk thumbnail cache (suite cache design), PSD, camera raw, HEIC (opt-in `heic` feature), AVIF, PDF, video poster frames via `filmcraft-cli`, 3D (GLB / glTF / OBJ), folder watcher with incremental updates | 2026-10-15 | `docs/assetcraft-plan.md` § Stage 1, `docs/design/cache.md` |
| Stage 1.5 step 1, 2a | Command registry with suite effects (`read` / `write-app-state` / `write-user-file`), `app.commands` with `enabled` + `disabledReason` (C1), `assetcraft-cli exec` / `commands`, docs `cli.md`, `mcp.md`, `control-protocol.md`, `agents.md` | 2026-10-22 `e2c9397`, `bde947f` | `docs/assetcraft-plan.md` § Stage 1.5 |
| Stage 2 T1 | Tools › Batch Rename… preview (`file.renamePreview`): tokens, presets, never-overwrite plan; nothing renamed yet | `e0bde69` | § Stage 2 |
| App icon | Elephant, to the FilmCraft icon standard; Windows / macOS / Linux sizes | `7383a5e`, `82a0649` | `STATUS.md` § App icon |
| 3D viewer V0 | Bounded GLB / glTF / OBJ parser (`crates/mesh`), CPU rasterizer, orbit / zoom / pan, shaded / wireframe / normals / UV-checker, backgrounds, turntable, `assetcraft-cli info`, `asset.meshInfo` | 2026-10-27 `0887ed8` … `bd054c2` | `claude/modelcraft-plan.md` (project doc), `STATUS.md` § 3D viewer V0 |
| 3D viewer V0.6–V0.7 | Vertex-clustered light copy for drags / turntable / playback (no triangle skipping); Metadata shows Polygons and Textures | 2026-11-05 `237837d` | `README.md` § 3D assets |
| Stage 4 Storyboard mode | Model with Re-cut and Quick fill (`crates/storyboard`), Shot map, Strip view (Content · Viewer · Inspector · timeline), Quick fill, text clips (cards, overlays, themes, intros), Start screen (song → preset → length, output settings), generate from a track's cue map, Watch your Cut with the song (cpal), undo / redo | 2026-10-15 → 10-27 | `docs/storyboard-plan.md` steps 1–5, TC1, S1–S2, B1–B9 |
| Track analysis | Beats, bars, sections, hits, tempo and fits in pure Rust (`crates/analysis`), Analyse / Analyse all in the app, cue-map sidecars, W1 working folder with Copy & analyse | 2026-10-25 | `docs/track-analysis.md` |
| Folders panel F1–F3, FP-6 | Computer · Favorites · Craft tabs; My Folders in the suite-wide `places.json`; macOS Finder favorites; Windows Quick Access pins (CFB `DestList`, no COM); a favorite click navigates the Computer tree | 2026-10-27 → 11-04 `cb1b0fa`, `7a0ba7e` | `docs/design/folders-panel.md` |
| EffectCraft E1 | EffectCraft as an Open In target for `.ecproj` (E2, thumbnails through `effectcraft-cli render-frame`, is next) | `2ec05ef` | `STATUS.md` § EffectCraft |
| Audit wave 0 | SB-9, AR-2, AR-4, AR-5, V3-1, AR-9, W0-1, W0-2, CI1 a–d: bugs and cheap wins with tests | 2026-11-01 → 11-04 | `docs/status/audit-wave0.md` |
| SB-3D1 steps 1–3c | Clip motion: `Clip.motion` (turntable, push in / pull out, pan, rise, custom; eased; re-timed by Re-cut), `storyboard.motion`, Inspector MOTION section, Viewer playback from a full-quality frame cache rendered ahead of the playhead, Return builds the whole cache, A/V sync on the audio clock (SB-M1–M3) | 2026-11-04 → 11-05 `f224267` … `115a004` | `docs/design/clip-motion-handoff.md` |
| R3 toolchain pin | `rust-toolchain.toml` = 1.99.0 so Mac, PC and CI lint alike | 2026-11-04 | `rust-toolchain.toml` |
| W2a | In-app floating scroll bars (Trellis-style) | 2026-11-05 `7dfa9d2` | `crates/ui-egui/src/theme.rs` |

## 2. Building and next

| Order | Milestone | What it delivers | Status | Waits on | Doc |
|-------|-----------|------------------|--------|----------|-----|
| 1 | W2b window chrome | The app's own 30 px title bar: icon · context · menus inline · ⎯ ▢ ✕, drag / double-click / edge resize via `ViewportCommand`; macOS keeps its traffic lights | shipped `c814bb4` (look check open) | — | `STATUS.md` § SB-3D1 (W2b) |
| 2 | SF1a Save / Save As | `storyboard.save` / `saveAs`, atomic `.storyboard.json`, dirty flag, Ctrl+S, title `Name •`, "Save changes?" before close; D-SF5: New doesn't ask | shipped `81cd6bc` | — | `docs/storyboard-plan.md` § Files |
| 4 | SF1b Save as FilmCraft… / Export via FilmCraft… | OTIO written to FilmCraft's reader conventions (one video track, `Gap`, `Marker.2`, song as audio, motion in an `assetcraft` namespace); Export Video = FilmCraft renders (D-SF3), greyed without FilmCraft | **next** | D-SF3 decided | § Files SF1b |
| 4 | SF2 Open / Recent | `storyboard.open` with the document limits, a newer format refused, missing assets reported and kept, Open Recent ▸ (10, in prefs), Ctrl+O; double-click in the Organizer later | **built 2026-11-06** | — | § Files SF2 |
| 5 | SF4 Export shot list + stills | CSV / Markdown shot list, every hero as PNG at the Viewer's framing, contact sheet; pure Rust | planned | — | § Files SF4 |
| 6 | SF3 Send to (App) | `storyboard.sendTo {app}` — FilmCraft via OTIO + `filmcraft-cli run`; EffectCraft after a look at its registry | planned | D-SF4 decided | § Files SF3 |
| 3 | SF5 native video export | `.mp4` / `.mov` · H.264 · AAC in pure Rust (D-SF6 chosen on the spike, 2026-11-06): `crates/video` encoder + muxer, `crates/animatic` frame compositor, `storyboard.exportVideo` job, File › Export Video… with settings, picker and progress | **built 2026-11-06** (SF5-1…5; Paul's look check + a real board on the PC) | patent note for a public build (Paul) | `docs/storyboard-plan.md` § Files SF5, D-SF6 |
| 8 | ST1–ST4 storyboard templates | `template` flag + Save as Template…; bundled Review · Tutorial · 5-minute instructions; File › New from Template ▸; user templates in `<working>/Templates/` | **ST1–ST3b + ST5 credits clip built 2026-11-06** (File menu + the Start screen's TEMPLATES row with a points / steps count and the song as the track; "Add the credits clip at the end" with an editable saved template); **SR1 resize built 2026-11-06** (`storyboard.resize`, Duration editable, a drag handle on every Strip card); **SR2a inset model + commands and SR2a′ (30 s titled step sections with a shot pattern, template cuts + INSET row on the Start timeline, D-SR2 length from the steps) built 2026-11-06** (Paul's mock-ups: a section inset lane for b-roll, interviews, a host or a graphic over the section's shots); **SR3 chorus loop built 2026-11-06** (every chorus span looped under the Cut with beat crossfades; `music: chorus | firstChorus | whole`); **SR2b built 2026-11-06** (Strip INSET + AUDIO rows, drop on a slot, a dropped MP4 / MOV sizes its section); **SR2c built** (Inspector INSET section; the inset drawn in its corner in the Viewer and the export, Swap); **SR3b built 2026-11-07** (structured music: the intro under the opening, a chorus starting on every section's title, the outro under the close — the default; the Strip's INSET lane with a click menu); **SR2d built 2026-11-07** (the Start screen's INSET panel); **SR4a built 2026-11-07** (an inset video's sound over its section, the music ducked, Music / Voice per section — export and Watch your Cut); next SR4b the voiceover track (Song · Voiceover · Song + voiceover · None), then the Start screen's INSET / AUDIO TRACK / TITLE CARDS / NUMBERING / SHOT PATTERN panels, SR2e the preset controls (preview strip, Cut pace, Transitions, Title cards, Shape) on the template path — Paul: templates are presets + title → section + an always-present inset lane (D-SR1), SR3 chorus loop + ducking with Music / Voice per section, SR4 the Start screen's Sections list and Length: Unknown; formats × styles (Vlog · Voice over · Slides) spec in `docs/design/template-formats.md`; ST4 (AI prompts) planned | — | `docs/storyboard-plan.md` § Templates, § Sections |
| 9 | SB-3D1 steps 4–5 | Set from Viewer (orbit / pan in the Storyboard Viewer, `storyboard.motion.capture`), Apply to all model / still shots | planned | — | `docs/design/clip-motion-handoff.md` |
| 10 | T-OS1 CI matrix | GitHub Actions on ubuntu / macos / windows + a `wasm32` check, so a first compile on another OS never fails in Paul's hands again (finder `is_absolute`, `/x` paths, label casing were all this class) | planned (steps 1–4 specced) | — | `STATUS.md` § Folders panel (T-OS1) |

## 3. Planned, by stage

| Stage | Milestone | What it delivers | Status | Waits on | Doc |
|-------|-----------|------------------|--------|----------|-----|
| 1.5 | 2b control channel, C2, C3b | `assetcraft --control <port>` loopback JSON lines (D-A1: no auth without a token file), `assetcraft-cli serve`, `mcp` over stdio, `session.inspect` for the automation audit AU1–AU10 | planned | D-A2, D-A3, D-A4, D-API1 open | `docs/control-protocol.md`, `docs/mcp.md` |
| 2 | T2 `file.rename` | Apply the rename plan with undo and `.xmp` sidecar moves — the first user-file write outside the working folder; needs the write seam (audit wave 1) | planned | audit D5 (write seam shape) | `docs/assetcraft-plan.md` § Stage 2, audit AR-1 |
| 2 | T3–T4 metadata | Metadata templates (creator, copyright, keywords), append / replace to `.xmp` sidecars; ratings and labels shared with LightCraft | planned | T2 | § Stage 2 |
| 2 | Culling, near-duplicates, colour search | From the feature backlog | idea | — | § Feature backlog |
| 3 | Structure and find | Catalog op log, collections, smart collections, search, version stacks, entities, folder schema checker, review states | planned | — | § Stage 3 |
| 3.5 | Develop (Camera Raw style) | Ctrl+R opens the selection in an embedded raw editor on LightCraft's develop crates; `lc:settings` sidecars (audit D1–D3), upstream PRs `lightcraft-crs`, `lightcraft-source` | planned (audit wave 3) | D1, D2, D3 open | `docs/audit/2026-10-28-audit.md` § DV |
| 4 | Storyboard steps 6, 8, 9 | Scenes view, clip editor modal, Tracks view with waveform lane (A3) and beat grid, B4b range re-cut with Tone, presets PR2 | planned | — | `docs/storyboard-plan.md` |
| 4 | AI Prompt & Insert | Prompt-driven themed clips as plan data (schema v1 on existing types, 3 commands, 5 steps) | reviewed, not started | D-AI1, D-AI2, D-AI3 open | `docs/design/ai-prompt-insert.md` |
| 5 | AI + generation | Provider preferences (Local, API, ArtCraft account; secrets in the OS keyring), AI storyline, directives; storyline slices C–D | planned | — | `docs/storyline-builder.md` §10–11 |
| 6 | Execute | Recipes (multi-app pipelines with dry run and resume), any-CLI steps (trellis.cpp, ffmpeg, Blender headless), hot folders, sheet builder → DesignCraft, delivery packages | planned | — | § Stage 6 |
| 7 | Connect and offer | Lineage graph, "used in" across FilmCraft / DesignCraft, shot coverage, packaging, the proposal to the ArtCraft team | planned | — | § Stage 7 |
| 3D | V1 wgpu viewport | GPU viewport, metallic-roughness with an environment term, thumbnails from the viewer renderer, `asset.meshReport`; V3-2 look/raster split, V3-3 structure-preserving `Document` (shared with ModelCraft, audit D4) | planned (after SF1–SF2) | D-V3, audit D4 open | `claude/modelcraft-plan.md` (project doc) |
| Audit | Waves 1–4 | 1: write seam (`Writer` trait), sidecar writer + file undo, one place for app folders, STATUS split · 2: low-end smoothness and memory (SB-1–4, SB-8, V3-4/5/10/11) · 3: Develop steps · 4: registry / Session shape, cue-map compatibility | planned | D5–D8 | `docs/audit/2026-10-28-audit.md` § Work plan |
| Web | WASM build | `cargo xtask wasm` checks `mesh` today; the egui web build and a server to run it are a T-OS1 job | idea | — | `xtask/src/main.rs` |

## 4. Opportunities and ideas (collected)

Logged where they came up; moved here so they are not lost. Each has an owner for the spec and
needs Paul's priority before it becomes a planned row.

| ID | Scope | Opportunity | Suggested fix / first step | Owner | Status |
|----|-------|-------------|----------------------------|-------|--------|
| O1 | Re-cut | `syncToDownbeat`: lock synced starts to downbeats once tracks carry a real beat grid | Downbeat pass in Re-cut after step 8 | Claude | idea |
| O3 | Beat grid | No beat detection in the suite at the time; hand-authored `bpm` was v1 | Superseded by `crates/analysis` (A2); keep for the Tracks view | Claude | done in spirit |
| O4 | Text clips | Free-floating `textClips` track overlaps per-clip overlays | Per-clip overlays stay the one way; `textClips` only for text spanning clips (Tracks view) | Claude | idea |
| O5 | Text clips | Text animates in only | Add an outro (fade / drop) to `TextAnimation` | Claude | idea |
| O6 | Text clips | Words edited nowhere yet | Text field in the clip editor (step 6) | Claude | planned with step 6 |
| O7 | Custom sequences (Paul, 2026-10-16) | A saved, reusable group of clips with effects and text; picked on the Start screen or dropped into a board; also an export item from FilmCraft / EffectCraft | Check what FilmCraft / EffectCraft export; spec a sequence document in the interchange crate | Paul (priority), Claude (spec) | idea |
| O8 | Themed composite shots (Paul, 2026-10-16) | A **theme** as a shot type: shots, titles, background, effects, fonts and edit styles that travel together (e.g. Family: paper titles, warm grade, crossfades) | Plan data here, rendered by FilmCraft / EffectCraft (ties to D-AI1) | Paul, Claude | idea |
| O9 | Storyboard in FilmCraft (Paul, 2026-10-16) | After AssetCraft's Storyboard mode is finished: the track-driven Start screen, presets and the storyboard-over-audio timeline become part of FilmCraft | A proposal to the ArtCraft team with the storyboard crate as the shared piece | Paul | idea |
| SUITE-1 | Favorites across the Crafting apps (Paul, 2026-10-20) | FilmCraft's media browser doesn't show AssetCraft's favorites | One shared favorites file (done as `places.json`, F1); FilmCraft reads it — propose upstream | Claude (spec), Paul (decide) | half done |
| F4 | Folders panel | The three-tab panel and `places.json` could live in `craft-libs` for every sibling | Proposal to craft-libs with the code as it stands | Claude | idea |
| V-RING | 3D viewer | A ring cache per turn (like the storyboard's frame cache) for a smooth turntable on dense models in the Organizer | Reuse `MotionPlayer`'s cache keyed by yaw | Claude | idea (V0.6 made it less urgent) |
| SF5-6 | Video export (Paul, 2026-11-06: "we need a progress or queue") | A queue of exports (several boards, sizes or file types) with a Jobs panel: each job's progress, cancel per job, the analyser's batches in the same list | `Exporter` holds a `VecDeque` of plans instead of one; `storyboard.exportVideo` enqueues; a Jobs window under View | Claude | planned |
| D-SF2 | Storyboard files | Autosave to `<name>.storyboard.json.autosave` every 30 s of changes, FilmCraft-style | After SF1 has been used for a week | Claude | idea |
| S2e | Storyboard | Output settings (aspect, resolution, frame rate) stored on the board, used by SF5 and the FilmCraft export | Build with SF1b / SF5 | Claude | planned |
| AN-W2 | Working folder | Undo the last music import; a Preferences row for the working folder | Small; after D-WF1 confirmed | Claude | idea |
| MC | ModelCraft (sibling app) | Clean up, re-model, rig and animate TRELLIS meshes; AssetCraft's `crates/mesh` grows into the shared structure-preserving model (audit D4) | Project doc `claude/modelcraft-plan.md` § 9 to close out | Paul, 3D cowork | idea |
| PC | PrepressCraft (sibling app) | Print prep / prepress for the suite | Project doc `claude/prepresscraft-plan.md` | Paul | idea |
| SUB | Submission readiness | What AssetCraft needs before it is offered to the ArtCraft team (D-S2–D-S5) | Project doc `claude/assetcraft-submission-readiness.md` | Paul | open |
| CACHE | Suite cache | D-C3, D-C5, D-C6 (shared analyses in the cache, redo vs migrate — audit D6) | `docs/design/cache.md` | Paul | open |
| DOC-FEAT | README (Paul, 2026-11-05) | Dedicated feature lists for the Organizer and for Storyboard mode, beside the three screenshots | Two README sections, each a checklist of shipped items with the command ids, built from § What it does today | Claude | planned |
| RATE | Organize (Paul, 2026-11-05: "build out the rating system") | Stars, colour labels and keywords on assets, shared with LightCraft through `.xmp` sidecars (`lightcraft-meta`), shown on the cards and in Metadata, filterable | Stage 2 T3 after the write seam (audit wave 1): `asset.rate {paths, stars}`, `asset.label`, `asset.keywords` with undo; read-only display first | Claude (Paul: D5 write seam) | planned |
| COLL | Organize (Paul, 2026-11-05: "collection tags") | Collections as tags: a named set an asset can belong to across folders, with a Collections list in the Folders panel; smart collections (saved searches such as `rating:>3 kw:reaper`) after | Start as `<working>/Collections/<name>.json` (atomic, versioned) + a `collection:` keyword in the sidecar so LightCraft sees it; Stage 3 adds the catalog and smart collections | Claude | planned |
| W2-SNAP | Window chrome | With our own title bar, Windows 11 Snap Layouts on hover of ▢ are lost | Re-enable via `WM_NCHITTEST` later if missed | Claude | idea |

## 5. Open decisions (index)

| ID | Decision | Where | Blocks |
|----|----------|-------|--------|
| D-SF6 | Native video export route — **decided on the spike: C″ pure-Rust H.264 + AAC**; the patent note for a public build stays with Paul | `docs/storyboard-plan.md` § Files | — |
| D-SF1–D-SF5 | File name and place, autosave, FilmCraft renders, send-to transport, New doesn't ask — built on the recommendations; confirm or change | `docs/storyboard-plan.md` § Files | — |
| D-AI1–D-AI3 | AI Prompt & Insert: plan data vs compositor; storyline slice C path; save before/after | `docs/design/ai-prompt-insert.md` | AI Prompt & Insert |
| D-A2–D-A4, D-API1 | Stage 1.5 control channel and API shape | `docs/assetcraft-plan.md` § Stage 1.5, `docs/control-protocol.md` | 2b, C2 |
| D-WF1 (a), (c) | Working folder defaults (built as recommended) | `crates/engine/src/working_cmd.rs` | — |
| D-V3 | 3D viewer: metallic-roughness map in V1 | project doc `claude/modelcraft-plan.md` | V1 |
| D-C3, D-C5, D-C6 | Suite cache | `docs/design/cache.md` | — |
| Audit D1–D8 | Develop sidecars, upstream PRs, JPEG XL, ModelCraft model, write seam, analyses in cache, toolchain pin (done, R3), Python cue maps | `docs/audit/2026-10-28-audit.md` § Decisions | waves 1–4 |
| B9a | Storyboard B9 follow-up | `docs/storyboard-plan.md` | — |
| D-S2–D-S5 | Submission readiness | project doc | Stage 7 |

## How to update this file

- A milestone moves between sections when its status word changes; keep its row text, change the
  status and date.
- New ideas go to § 4 with an ID, the scope, the first step and an owner — never only into
  `STATUS.md`.
- `STATUS.md` keeps the narrative and the "Now" table; this file keeps the index. When they
  disagree, `STATUS.md` is newer: fix this file in the next session.
