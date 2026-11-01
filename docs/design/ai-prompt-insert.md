# AI Prompt & Insert: design review

_2026-11-01 · Stage 5 (Storyline Builder slice C, brought forward as a manual path) · proposed, not started_

- **Scope:** generating an editable storyboard from the analysed song plus the user's creative direction, through a prompt the user copies to any AI and a JSON answer pasted back, validated and inserted. Request: [docs/requests/2026-11-01-ai-prompt-insert.md](../requests/2026-11-01-ai-prompt-insert.md).
- **Project goal it serves:** the Storyline Builder promise, "plan the story in blocks, generate what you need, and assemble it into a finished video", with AI optional and no provider required.
- **Immediate area of improvement:** today `storyboard.generate` builds clips from the music alone; nothing turns a story idea, theme, titles and overlays into those clips.

Nothing here is built. This doc maps the request onto the code as it stands (`19061a3`), records what already exists, and proposes small steps for Paul to review **before building**.

## 1. What already exists (reuse, don't rebuild)

| Request concept               | Already in AssetCraft                                                                                                                         | Where                                         |
|-------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------|
| Real audio metadata           | Cue map: duration, BPM, beats, downbeats, sections with energy, drops and hits, fade fits. Richer than the request's "duration only" fallback | `crates/storyboard/src/cuemap.rs`, `analyse.rs` |
| Clip boundaries from audio    | `storyboard.generate` builds clips from the cue map and a pacing profile                                                                      | `generate.rs`, `engine/src/generate_cmd.rs`   |
| Scenes                        | `Scene { id, song_section }`; clips belong to scenes                                                                                          | `model.rs:228`                                |
| Independent overlay channel   | `text_clips` (track T1): `TextClip { start, duration, text, placement }`, absolute timing                                                     | `model.rs:306`                                |
| Titles inside clips           | `Clip.overlay` + `text_style` (`TextKind` Card / Overlay, `TextTheme`, `TextAnimation`, `TextPosition`)                                       | `model.rs:168`, `text.rs`                     |
| Transitions                   | `Transition`: Cut, Fade, Fade in, Fade out, Flash, Smash cut, Whip                                                                            | `model.rs:111`                                |
| Capability registry           | Those enums are the supported vocabulary: 5 animations, 4 themes, 5 positions, 7 transitions, the shot types                                 | `text.rs`, `model.rs`                         |
| One undoable import           | `EDIT` effect: whole-board snapshot, one undo step; a refused edit leaves no step                                                             | `engine/src/storyboard.rs`                    |
| Hostile-JSON handling         | `Limits`, size caps, `deny_unknown_fields`, `NewerVersion`                                                                                    | `validate.rs`, `cuemap.rs`                    |
| A plan for this feature       | Storyline Builder: blocks, prefilled directives, asset prompts, schema-validated AI storyline (slice C), providers with "None" = manual        | [storyline-builder.md](../storyline-builder.md) §5, §6, §10, §11 |

## 2. Gaps and conflicts

| ID | Impact | Issue                                                                                                                                           | Suggested fix                                                                                                                                                                       | Owner                         | Status   |
|----|--------|-------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------|----------|
| R1 | H      | Storyboards are not saved yet (plan decision D3), so acceptance #11 "persists through save and reopen" and #13 "old files still load" can't pass | Build D3 (save to `<working folder>/Projects/`, atomic, on the audit's write seam AR-1) before step 3, or defer #11 and #13                                                         | Claude (Paul: D-AI3)          | open     |
| R2 | H      | Nested themed-clip elements (background, title, fog, particles) don't exist: `Clip` is flat. AssetCraft renders no effects; it plans, FilmCraft and EffectCraft render | Elements as **plan data**: format v2 adds `elements: Vec<Element>` with `#[serde(default)]` (v1 files still load), shown and edited in the Inspector; fog, particles and motion are marked "directive, not rendered" (the request's own rule 8) | Paul (D-AI1), Claude          | open     |
| R3 | M      | The request uses integer milliseconds; the model uses exact `Ticks` (254,016,000,000 per second)                                                | Milliseconds only at the JSON boundary, through one conversion function                                                                                                            | Claude                        | proposed |
| R4 | M      | Overlays such as "floating bats" aren't text, but `text_clips` holds text only                                                                 | v1: text overlays; a non-text overlay imports as a warning plus a note. v2: an asset-prompt overlay kind                                                                           | Claude                        | proposed |
| R5 | L      | The request suggests `snake_case` fields and its own ids; house style is `camelCase` and numeric `ClipId` / `SceneId`                         | `camelCase` schema; AI ids are only for cross-references inside the answer and are remapped to fresh numeric ids on import ("deterministic safe remapping")                         | Claude                        | proposed |
| R6 | L      | `Storyboard` has no frame rate or aspect ratio yet                                                                                             | The prompt says "not set" truthfully; add when the S2e output settings land                                                                                                         | Claude                        | proposed |
| R7 | M      | The request is UI-first (a panel); the house rule is that every action is a command                                                            | Three commands (§4). The CLI and MCP get them free, so an AI agent can run the whole loop without copy and paste                                                                   | Claude                        | proposed |
| R8 | L      | "Send to AI" assumes a provider service; none exists yet                                                                                       | Step 5: the Stage 5 provider slice, local llama.cpp (`openai-compatible`) first                                                                                                     | Claude, later                 | proposed |
| R9 | M      | Two parallel story-planning features would drift                                                                                               | This becomes the Storyline Builder's **manual (copy and paste) provider path** for slice C, on the same blocks model; recorded in storyline-builder.md §10                          | Paul (D-AI2)                  | open     |

## 3. Schema v1 (proposed)

`schemaVersion: "assetcraft.storyboard.ai.v1"`, `camelCase`, integer milliseconds, start inclusive and end exclusive. Mapped onto **existing** types only, so step 1 needs no format change:

| AI JSON                                     | Becomes                                                                                       |
|---------------------------------------------|-----------------------------------------------------------------------------------------------|
| `project.title`                             | `Storyboard.name` (only when the board is new)                                                 |
| `scenes[] {id, name, startMs, endMs}`       | `Scene` (+ `song_section` when it matches a cue-map section)                                   |
| `themedClips[] {id, sceneId, startMs, durationMs, shotType, description, title?, textStyle?, transitionIn?, transitionOut?, assetPrompts[]}` | `Clip`; `title` → `overlay`; `assetPrompts` → `description` / `notes` until v2 elements |
| `overlays[] {id, kind: "text", startMs, durationMs, text, position}` | `TextClip` on T1                                                     |
| `warnings[]`                                | shown in the preview; never fatal                                                             |

Validation runs as the request lays out: parse → version → structure → timing (inside the song, clips in order, children inside parents) → counts (overlays exactly as asked, including 0; titles inside clips don't count) → vocabulary (unknown animation, theme, position, transition or shot type → blocking error, or a documented fallback with a warning) → ids (unique, no dangling `sceneId`) → asset references (only library paths already known; URLs and other paths refused). Size cap 1 MiB, at most 2,000 clips and 2,000 overlays. Errors carry a JSON path, e.g. `themedClips[3].durationMs: ends at 61,200 ms, the song is 60,000 ms`.

## 4. Commands (proposed)

| Command                   | Effects | Params → result                                                                                                                         |
|---------------------------|---------|-----------------------------------------------------------------------------------------------------------------------------------------|
| `storyboard.ai.prompt`    | READ    | `{title, intro, outro, theme, story, overlays, themedClips?, scenes?, options?}` → `{prompt, context: {durationMs, bpm?, sections, beats?, vocabulary}}`; deterministic for the same inputs and board |
| `storyboard.ai.validate`  | READ    | `{json}` → `{ok, errors: [{path, message}], warnings, preview: {clips, scenes, overlays, unsupported}}`; changes nothing               |
| `storyboard.ai.import`    | EDIT    | `{json, mode: "new" \| "replaceAi", dryRun?}` → the preview, or the applied change as **one** undo step; refused unless `validate` passes |

`replaceAi` replaces only content an earlier AI import created (marked on the clip); there is no whole-project replace.

## 5. Steps (small, each ends with `cargo xtask ci` and a commit block)

| Step | Delivers                                                                                                                                       | Exit test                                                                                   | Needs     |
|------|------------------------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------|-----------|
| 1    | Engine only: schema v1 types, parser and validator (L3 `storyboard` crate), prompt builder from the real board and cue map, the three commands, a fixture | Acceptance #1–7, 10, 12, 14; malformed JSON, duplicate ids, oversized child timing, zero overlays; CLI round trip | —         |
| 2    | The **AI Prompt & Insert** panel (Start screen and Inspector): fields kept per board, Generate / Copy, paste, Validate with paths, preview diff, Insert with confirmation | UI test: generate, paste the fixture, validate, insert, Ctrl+Z restores                      | Step 1    |
| 3    | D3 save / load                                                                                                                                 | #11 and #13                                                                                 | AR-1      |
| 4    | Format v2 nested elements (R2) and asset-prompt overlays (R4)                                                                                  | #8, #9; v1 files load unchanged                                                             | D-AI1, 3  |
| 5    | Regenerate one clip; "Send to AI" through the Stage 5 provider (local first)                                                                  | Request §9; provider job polling                                                            | Stage 5   |

## 6. Decisions for Paul

| ID     | Decision                                                                                                   | Recommendation                                                                                                   |
|--------|------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------|
| D-AI1  | Themed-clip elements: plan data, edited here and rendered by FilmCraft / EffectCraft, or a preview compositor in AssetCraft? | Plan data. AssetCraft stays the planner; a compositor duplicates EffectCraft                         |
| D-AI2  | Make this the Storyline Builder's copy-and-paste path for slice C, not a separate feature?                  | Yes: one block model, one schema, one validator                                                                  |
| D-AI3  | D3 save / load before or after step 1?                                                                      | After: steps 1 and 2 work on the open board, and D3 sits on the audit's write seam (wave 1)                      |
