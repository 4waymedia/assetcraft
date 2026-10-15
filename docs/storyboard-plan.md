# Storyboard mode: implementation plan

_2026-10-15 · Stage 4 (Plan: Storyboard mode) · steps 1–2 done, step 3 first slice done_

- **Scope:** AssetCraft's Storyboard mode, built in egui from the Claude Design handoff
  ([design/storyboard-mode-handoff.md](design/storyboard-mode-handoff.md)). Covers storyline
  slices A and B of [storyline-builder.md](storyline-builder.md) §10, restated in the design's
  vocabulary.
- **Project goal it serves:** go from tagged Reapers assets to a timed, playable **Cut** timed to
  a music **track**, then hand it to FilmCraft.
- **Immediate area of improvement:** the model and its rules exist (`crates/storyboard`);
  nothing yet creates or edits a storyboard through `Session::execute`.

## Vocabulary

The handoff's terms are binding in UI and code: Organizer, Storyboard, Story, Asset, Clip,
Scene, Title card, Format › Preset, Track, Beat grid, Shape to music, Re-cut, Quick fill,
Shot map, Cut. They replace storyline-builder.md's **block** (now **Clip**) and the planned
crate name `boards` (now `storyboard`; "Board" is on the design's do-not-use list).

## Mapping the handoff to the house rules

| Handoff                                   | AssetCraft                                                                                         |
|-------------------------------------------|----------------------------------------------------------------------------------------------------|
| "Electron + React or Tauri + React"       | egui in `crates/ui-egui` (L6); no webview (AGENTS.md rule 1)                                        |
| `Clip.index`                              | position in `Storyboard::clips`, never stored (`crates/storyboard/src/model.rs`)                    |
| `durationSec` (float), `beats` stored     | `duration: Ticks` (FilmCraft's 254,016,000,000/s); beats derived via `Bpm::ticks_to_beats`          |
| `Scene.title`                             | the scene's first title card's description (`Storyboard::scene_title`)                             |
| HTML5 drag and drop, `dataTransfer = assetId` | egui drag payload `AssetId`; drop runs `storyboard.place {asset, clip}`                       |
| Re-cut reference algorithm                | `crates/storyboard/src/recut.rs` `recut()`; pure, returns a `RecutReport` the engine applies as one undoable command |
| Story "tags persist on files"             | until Stage 2 (XMP writes), a Story lives inside the storyboard document only                       |
| "Watch your Cut" playback                 | animatic: stills/poster frames on timing (storyline-builder §2 v1); music later                     |

## Steps

| Step | Delivers                                                                                                   | Status |
|------|------------------------------------------------------------------------------------------------------------|--------|
| 1    | `crates/storyboard` (L3): model, `Ticks`/`Bpm`, Re-cut, Quick fill, hostile-JSON validation with `Limits`   | done   |
| 2    | Engine: one open storyboard in `Session` (memory only); commands `app.mode`, `storyboard.new`, `storyboard.select`, `storyboard.place`, `storyboard.recut`, `storyboard.undo`, `storyboard.redo`, `storyboard.state`; undo/redo stack (100 steps, selection included). `storyboard.clip.add` / `.clip.set` moved to step 6 with the clip editor | done   |
| 3    | UI: Organizer \| Storyboard mode switch, toolbar chrome, Shot map 1a (112 px tiles, shot-type colours, § and ◆), Inspector 300 px, status bar | first slice done; Format › Preset menu and transport wait for steps 5 and 9 |
| 4    | Strip view (2b): Content panel, Viewer, full-width timeline at 44 px/s, drag asset → clip                    | —      |
| 5    | Quick fill (4a): NEXT card, click-to-place, REPLACING target, progress bar, Undo / Auto-fill rest / Clear; Re-cut on track change with toast; Watch your Cut (animatic) | —      |
| 6    | Scenes view (2a), Clip editor modal (1d), Manual start (3e)                                                  | —      |
| 7    | Save / open a storyboard document (needs decision D3)                                                        | —      |
| 8    | Tracks view (2c): text, shots and music tracks; waveform and beat grid (audio decode: opportunity O2)        | —      |
| 9    | Format › Preset picker (3c), Start storyboard (3b), Tag content → Init story (3a)                            | —      |
| 10   | OTIO export → FilmCraft (`crates/interchange`, slice B)                                                      | —      |
| Later (Stage 5) | AI build (3d), Shape to music proposal (1e), suggested angles                                     | —      |

## Decisions (Paul)

| ID | Question                                                                                         | Recommendation / decision                                                                                         | Needed by |
|----|--------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------|-----------|
| D1 | Adopt the handoff's vocabulary over storyline-builder's "block"                                   | **Decided (Paul, 2026-10-15): yes.** storyline-builder.md keeps its history; new code and UI use Clip/Scene/Storyboard | Done      |
| D2 | Storyboard ahead of Stages 1.5, 2 and 3                                                           | Yes, as asked; it needs none of them for steps 1–6. Stage 1 W1 (Windows CI re-run) still closes first   | Now       |
| D3 | Writing storyboard documents before Stage 2's write rules                                         | **Decided (Paul, 2026-10-15): agreed.** AssetCraft's own new files only (never the user's assets): atomic temp + rename; save-as never overwrites without asking | Step 7 |

## Opportunities

| ID | Scope                         | Opportunity                                                                                     | Fix                                                                                   | Owner  |
|----|-------------------------------|-------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------|--------|
| O1 | Re-cut, `syncToDownbeat`      | The handoff can lock synced starts to downbeats; step 1 locks title cards only                  | Add a downbeat pass once tracks carry a real beat grid (step 8)                       | Claude |
| O2 | Tracks view, Watch your Cut   | No audio decoding in AssetCraft yet                                                             | Study FilmCraft's audio path first (Call or Depend), then a permissive pure-Rust decoder | Claude |
| O3 | Beat grid                     | Beat detection doesn't exist in the suite                                                       | Hand-authored grid (`bpm` from zero) for v1, as storyline-builder §7 already decided  | Claude |
