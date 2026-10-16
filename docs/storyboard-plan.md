# Storyboard mode: implementation plan

_2026-10-15 · Stage 4 (Plan: Storyboard mode) · steps 1, 2, 4, 5, G1, TC1, A1 and music playback done, step 3 first slice done; A2 or step 6 next_

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
| 4    | Strip view (2b): Content panel, Viewer, full-width timeline at 44 px/s, drag asset → clip                    | done; Effects / Titles tabs are placeholders, Scenes and Tracks toggles wait for steps 6 and 8 |
| 5    | Quick fill (4a): NEXT card, click-to-place, REPLACING target, progress bar, Undo / Auto-fill rest / Clear; Re-cut on track change with toast; Watch your Cut (animatic) | done; the animatic plays the track's music (2026-10-15, [track-analysis.md §9c](track-analysis.md)) |
| TC1  | Text clips: Card \| Overlay per clip, themes, intro animations, overlay positions; click the kind chip to switch ([Text clips](#text-clips-tc1)) | done   |
| 6    | Scenes view (2a), Clip editor modal (1d), Manual start (3e)                                                  | —      |
| 7    | Save / open a storyboard document (needs decision D3)                                                        | —      |
| 8    | Tracks view (2c): text, shots and music tracks; waveform and beat grid (audio decode: opportunity O2)        | —      |
| 9    | Style › Preset picker (3c; music-timed presets, see [storyboard-presets.md](storyboard-presets.md)), Start storyboard (3b), Tag content → Init story (3a)                            | —      |
| 10   | OTIO export → FilmCraft (`crates/interchange`, slice B)                                                      | —      |
| Later (Stage 5) | AI build (3d), Shape to music proposal (1e), suggested angles                                     | —      |

## Text clips (TC1)

_2026-10-15 · Paul: "Text clip types: separate clip, overlay … each text clip should allow the user to click to switch the type."_

| Kind    | What it is                                                                 | Stored as                                              |
|---------|----------------------------------------------------------------------------|--------------------------------------------------------|
| Card    | A clip of its own: the theme's background, the text animated in, centred   | `shotType: "TTL"`, text in `description` (as before)   |
| Overlay | The text over the clip's shot, at a position, in the theme's style         | any other shot type with `overlay: "<text>"`           |

- **Every text clip has a style:** `textStyle {theme, animation, position}`, left out of the
  document when it is the default (Bold, Fade in, Lower third · left).
  - Themes: Bold, Minimal, Cinematic, Paper.
  - Animations: None, Fade in (0.5 s), Rise (0.5 s), Zoom (0.6 s), Typewriter (1.2 s).
  - Positions (Overlay only): Lower third · left / right, Center, Top, Bottom.
  - Looks and frames are data in `crates/storyboard/src/text.rs` (`TextTheme::look`,
    `TextAnimation::frame`); the UI paints them.
- **Switching keeps the clip's id, start and length**, so the Cut never shifts; one switch is one
  undo step.
  - Card → Overlay: the clip becomes a Wide shot ("Shot under the title") that Quick fill can fill.
  - Overlay → Card: the shot's images are dropped (a Card holds text only); the message says how
    many, and Ctrl+Z brings them back.
- **Scene titles** come from the scene's first clip with text, so a title switched to an Overlay
  still names its scene.
- **Commands:** `storyboard.text.kind {clip, kind: card|overlay}` and `storyboard.text.style {clip,
  theme?, animation?, position?}` (both EDIT; a no-op records no undo step).
- **UI:** a `CARD ⇄` / `OVERLAY ⇄` chip on Shot map tiles and Strip clips switches on click; the
  Inspector's TEXT section has Card | Overlay, Theme, Animation, Position and a looping preview;
  the Viewer plays the intro during Watch your Cut. Generated boards style their cards by pacing:
  Calm Minimal + Fade, Cinematic Cinematic + Fade, Energetic Bold + Zoom, Trailer Bold + Rise.

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
| O2 | Tracks view, Watch your Cut   | ~~No audio decoding in AssetCraft yet~~ Done 2026-10-15: decode via `soundcraft-audio-io` (A1), playback via cpal (PR3) | Waveform lane comes with A3                                                            | Claude |
| O3 | Beat grid                     | Beat detection doesn't exist in the suite                                                       | Hand-authored grid (`bpm` from zero) for v1, as storyline-builder §7 already decided  | Claude |
| O4 | Text clips, `textClips` track | The model's free-floating `textClips` (T1 track) overlaps with per-clip Overlays and nothing writes it | Keep per-clip Overlays as the one way for now; use `textClips` only for text that spans several clips (Tracks view, step 8), or retire it with the format bump at step 7 | Claude |
| O5 | Text clips, outro             | Text animates in only; it leaves with the clip's transition                                       | Add an outro (fade / drop) to `TextAnimation` if Paul wants text to leave before the cut | Claude |
| O6 | Text clips, editing           | The words are edited nowhere yet (Card text comes from the generator or the sample)               | Text field in the clip editor (step 6), running `storyboard.clip.set`                  | Claude |
