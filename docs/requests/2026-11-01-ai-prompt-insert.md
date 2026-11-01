# AssetCraft Feature Request: AI Prompt & Insert for Audio-Driven Storyboards

**Status:** Ready for repository discovery and implementation  
**Feature:** AI Prompt & Insert  
**Product:** AssetCraft (custom digital asset manager inspired by Adobe Bridge)  
**Goal:** Generate an editable storyboard from an existing audio timeline and user-provided creative direction using an AI-produced JSON document.

## 1. Instructions to Claude / Coding Agent

You are implementing this feature **inside the existing AssetCraft repository**. Do not assume the framework, file structure, timeline architecture, storage design, AI provider, or UI library.

1. Inspect the repository, application entry points, storyboard UI, audio import/analysis, clip/timeline models, serialization, undo/redo, and tests.
2. Summarize the current relevant architecture and identify reusable patterns and extension points.
3. Write a short implementation plan with files/modules to change. Then implement the feature incrementally in the repository, following existing conventions.
4. Reuse existing clip/channel/effect types where feasible; add versioned serialization or adapters where required. Do not break existing storyboard files.
5. Add automated tests and run relevant lint, type-check, build, and test commands; report results accurately.
6. Keep AI providers optional: prompt generation, copying, JSON paste/import, and local validation must work **without** configuring a cloud API.
7. Avoid replacing entire storyboards or modifying audio by default. Preserve user edits, include preview and explicit confirmation, and integrate with existing undo/redo if present.
8. If something is not supported in the current renderer, expose it as an unsupported capability or gracefully degrade—do not silently pretend an effect or asset exists.

### Deliverables

- Working **AI Prompt & Insert** panel integrated with the storyboard UI.
- Deterministic prompt builder that uses user input and actual audio timeline metadata.
- Copy/export prompt, paste/import AI JSON, validation feedback, preview/diff, and confirmed insertion.
- Editable themed composite clips with nested titles, graphics, effects, and motion where supported by the existing clip model.
- Separate overlay channel/content model.
- Versioned JSON schema, sample fixture, tests, and brief user/developer documentation.
- Implementation summary: changed files, schema decisions, test results, and known limitations.

## 2. Product Intent and Core Distinctions

AssetCraft already has a custom storyboard system that uses an audio file to generate a timeline and clips. This feature adds AI-assisted composition generation.

**Workflow:**

`Audio timeline + user inputs -> generated AI prompt -> AI JSON response -> schema validation -> preview -> insert editable storyboard content`

There are **two different visual concepts**:

1. **Independent overlays:** content on its own overlay channel (e.g., captions, floating bats, labels, warnings). An overlay can span multiple clips and is positioned on the master timeline.
2. **Themed composite clips:** timeline clips that **own editable nested elements**, potentially including backgrounds/media, titles, decorative graphics, effects, particles, and animation. Their child elements use **clip-relative** timing. They are not merely flattened images or videos.

An AI-created themed clip must remain editable: user can change its title, visual parameters, nested elements, asset prompts, and timing. Don't generate a single flattened render as the only representation.

## 3. User Experience: Prompt Panel

Provide a panel named **AI Prompt & Insert**, accessible from the existing storyboard interface using the repository's UI patterns. Preserve fields on panel close/open, preferably per storyboard/project.

### Required fields

| Field | UI | Description |
|---|---|---|
| Storyboard title | Text | Human-readable project/story title |
| Intro | Multiline text | Desired opening content and mood |
| Outro | Multiline text | Desired ending content and mood |
| Theme | Text or existing theme picker | Cohesive aesthetic, e.g. vintage haunted carnival |
| Story description | Multiline text | Narrative, scene concepts, setting, characters, pacing |
| Number of overlays | Nonnegative integer | Requested number of **independent** timeline overlays |

### Additional controls

| Field | UI | Suggested default |
|---|---|---|
| Number of themed clips | Nonnegative integer or Auto | Auto, driven by existing audio clip/segment layout |
| Number of scenes | Nonnegative integer or Auto | Auto; only if this distinction exists in the current storyboard |
| Include titles | Checkbox | On |
| Include graphics | Checkbox | On |
| Include effects | Checkbox | On |
| Include motion | Checkbox | On |
| Visual complexity | Simple / Moderate / Cinematic | Moderate |
| Theme consistency | Strict / Flexible | Strict |
| Creative freedom | Directed / Balanced / Imaginative | Balanced |
| Generate asset prompts | Checkbox | On |

Do **not** make the optional controls a prerequisite for the first end-to-end workflow. Use field descriptions that distinguish global title from titles embedded in themed clips. Show audio length and available timing data in a read-only context block.

### Panel actions and states

- **Generate Prompt**: builds deterministic, readable AI instructions from inputs and timeline context.
- **Copy Prompt** / **Export Prompt**: enables external use with Claude or any compatible LLM.
- **AI Response**: a JSON editor/paste area and **Import JSON** action. Optionally offer **Send to AI** *only if* AssetCraft already has an AI provider service or a separately configured provider adapter; don't hard-code one vendor.
- **Validate**: parse and verify AI response; show errors with JSON paths and actionable explanations.
- **Preview Changes**: inspect proposed new clips, nested elements, overlays, their timing, and unsupported asset/effect types.
- **Insert into Storyboard**: enabled only after validation, and requires user confirmation.
- **Cancel**: changes nothing.

Add loading, empty, failed-validation, unsupported-capability, and success states. Clearly communicate when the AI returned *instructions for an asset* rather than an actual asset file.

## 4. Audio Is the Source of Truth

Before building the prompt, inspect and extract **real** metadata from the existing storyboard/audio processing logic:

- Audio duration (milliseconds).
- Existing scene/clip boundaries, when available.
- Existing beat, phrase, marker, silence, or energy analysis **only if available**.
- Track/channel layout and insertion constraints.
- Project aspect ratio, frame rate/timebase, and resolution if supported.

**Do not invent beats, markers, duration, or analysis results.** If only duration exists, say so in the generated prompt and instruct the AI to use duration/clip boundaries without claiming beat alignment.

Time fields in AI JSON are **integer milliseconds**, using start-inclusive/end-exclusive intervals and nonnegative durations. Validate that clip placements fall within the audio timeline. Choose a documented policy for gaps/overlap based on existing track semantics; overlaps across legitimate independent channels are allowed. Child timings are relative to their themed parent clip.

## 5. Prompt Builder Contract

The prompt builder should be deterministic for the same inputs and timeline snapshot and should contain:

1. Role: AI storyboard director for AssetCraft.
2. User's exact creative inputs, clearly delimited as data.
3. Verified audio/timeline metadata and capabilities from the app.
4. Instruction to compose a coherent beginning, middle, and end with consistent visual style.
5. Rules for independent overlays versus nested themed clip elements.
6. Exact requested **independent overlay count**; titles inside themed clips **do not** count toward that number.
7. Requirements for elements to be editable and for every media/graphic concept to include a generative asset prompt or a valid known asset reference.
8. Requirement to use only supported effect/animation/type identifiers returned by the app's capability registry; for unsupported requests use a safe fallback and warnings rather than fabricating executable effects.
9. Strict JSON-only output following the versioned schema, with no Markdown code fences or explanatory prose.
10. Timing/uniqueness/validation constraints and clear handling of uncertainty.

**Security and trust:** user story descriptions and external AI output are untrusted data. Never execute code embedded in JSON; avoid accepting arbitrary file paths/URLs as executable or trusted assets; don't render raw HTML. Use normal JSON parsing, schema validation, and safe references to locally managed assets.

## 6. Suggested Versioned JSON Contract

The following is a **reference contract**, not an instruction to replace existing AssetCraft serialization. Prefer adapting this into current internal models with a stable import/export adapter.

```json
{
  "schema_version": "assetcraft.storyboard.ai.v1",
  "project": {
    "title": "Halloween Carnival",
    "theme": "Vintage Haunted Carnival",
    "audio_duration_ms": 30000
  },
  "themed_clips": [
    {
      "id": "themed_001",
      "name": "The Carnival Arrives",
      "start_ms": 0,
      "duration_ms": 8000,
      "description": "The carnival materializes out of fog.",
      "elements": [
        {
          "id": "bg_001",
          "type": "background",
          "start_ms": 0,
          "duration_ms": 8000,
          "asset_ref": null,
          "asset_prompt": "Wide shot of a vintage carnival emerging from fog at night; cinematic composition",
          "properties": {},
          "animations": [{ "type": "slow_zoom", "start_ms": 0, "duration_ms": 8000, "parameters": {} }]
        },
        {
          "id": "title_001",
          "type": "text",
          "start_ms": 1000,
          "duration_ms": 4500,
          "content": "WELCOME TO THE CARNIVAL",
          "properties": { "color": "#FF7518", "font_category": "horror_display" },
          "animations": [{ "type": "flicker", "start_ms": 0, "duration_ms": 1800, "parameters": {} }]
        },
        {
          "id": "fx_001",
          "type": "effect",
          "start_ms": 0,
          "duration_ms": 8000,
          "effect_type": "rolling_fog",
          "properties": { "density": 0.6, "speed": 0.2 },
          "animations": []
        }
      ],
      "transition_out": { "type": "fade", "duration_ms": 500 }
    }
  ],
  "overlays": [
    {
      "id": "overlay_001",
      "type": "text",
      "start_ms": 12000,
      "duration_ms": 3000,
      "content": "DARE TO ENTER?",
      "properties": { "placement": "bottom_center" },
      "animations": []
    }
  ],
  "warnings": []
}
```

**Notes:** This fixture demonstrates shape only; it intentionally contains one themed clip and one overlay and does **not** claim to fill the complete 30-second timeline. The real generator must honor the user's requested counts and follow the app's coverage rules. Sample identifiers (`slow_zoom`, `flicker`, `rolling_fog`, `fade`, `horror_display`) are placeholders unless those capabilities exist in the repository. Adapt, register, or safely map them to supported functionality.

### Required schema semantics

- `schema_version` required; reject or explicitly migrate unknown versions.
- Unique IDs across all imported entities, or deterministic safe remapping to avoid collisions with existing storyboard IDs.
- Parent clip timing is absolute, nested element timing is relative; each element must fit within its parent. Element animation timing is relative to the element unless the existing engine has a different documented convention.
- Independent overlay timing is absolute to the master audio timeline.
- Preserve requested overlay count, including `0`.
- Explicit media asset references or prompts; **prompts are not resolved media**. Missing asset references should show a placeholder/needs-generation state.
- Valid, finite animation/effect values with supported names and sensible parameter ranges.
- No destructive changes to existing audio or user-created content.
- `warnings` can communicate capability fallbacks; fatal validation issues must block insertion.

## 7. Timeline & Editing Behavior

The storyboard should visually keep independent overlays separate from themed composite clips. Adapt names to existing channels, but preserve meaning.

```text
THEMED CLIPS    [ Carnival Intro ][ Haunted Ride ][ Finale ]
OVERLAYS                [ Bats ]          [ Warning Text ]
SCENES          [ Main Scene A ][ Main Scene B ][ Main Scene C ]
AUDIO           [============= Original Audio =============]
```

A themed clip should be expandable (or inspectable in an equivalent inspector):

```text
▼ Carnival Intro (0:00–0:08)
  ├─ Background       [================]
  ├─ Title              [=========]
  ├─ Graphics             [=========]
  ├─ Fog effect       [================]
  └─ Lighting              [=======]
```

- Moving a parent themed clip moves its elements with it.
- Resizing a parent clip must use the current app's trim/retime semantics; never silently let children extend outside bounds.
- Titles, properties, graphic asset prompts, effects, and animations should be individually editable where supported.
- Independent overlays must be editable/movable separately from themed clips.
- Import changes should be reversible as one undoable operation when the host app supports undo.

## 8. Validation & Import Pipeline

Use a staged, **non-destructive** pipeline:

`AI JSON text -> parse -> schema version check -> structural validation -> semantic/timing checks -> capability checks -> asset reference resolution -> preview -> confirmation -> transaction/undoable import`

Validation should detect and explain:

- Invalid JSON, missing required fields, wrong types, out-of-range numerical values.
- Duplicate IDs and collisions with IDs already in the storyboard.
- Out-of-bounds parent clips, overlays, nested elements, and animations.
- Invalid counts (especially requested independent overlays).
- Unsupported channel/element/effect/animation/transition names.
- Invalid references, missing media, or unsafe paths/URLs.
- Unexpected collisions or overwrites against existing timeline content.

Where possible, distinguish **blocking errors**, **warnings**, and **placeholder assets**. Present insertion modes only if safely supported: **Insert as New Content** (required); **Replace Previously AI-Generated Content** (optional, explicit); **Cancel**. Do not add implicit whole-project replacement.

## 9. Secondary Workflow: Regenerate One Clip (Future-Friendly)

Design the clip/prompt modules so a subsequent feature can support:

`Select themed clip -> AI: Regenerate Clip -> natural-language edit request -> new clip JSON -> validate -> preview -> replace selected clip`

Regeneration should preserve the selected clip's identity, position, and duration by default and only change nested creative content unless the user explicitly opts to change timing. **This can be deferred** until after the core prompt/import feature is functional; don't let it block the MVP.

## 10. Component Boundaries (Adapt to Repository)

| Logical component | Responsibility |
|---|---|
| PromptPanel | Inputs, persistence, generation, JSON paste/editor, progress/error states |
| TimelineContextAdapter | Reads real audio duration, segments, markers, capability IDs |
| PromptBuilder | Deterministic prompt text and constraints |
| AIProviderAdapter (optional) | Local/cloud model invocation through existing provider architecture |
| AIStoryboardSchema | Versioned JSON types and parser/schema validator |
| AIStoryboardValidator | Timing, counts, IDs, asset references, capabilities |
| ImportPreview | Proposed tracks/clips/elements and warnings/diff |
| StoryboardImporter | Safe model mapping, insertion, state update, undo integration |

Keep UI separate from prompt construction and validation logic. Prefer reusable functions and existing project serialization/commands over duplicate state representations.

## 11. Implementation Phases

**Phase 1 — Inspect and document:** find existing timeline, channel, composite clip support, effect registry, UI conventions, and import pathway. Report compatibility issues.

**Phase 2 — Functional MVP:** required panel inputs; deterministic prompt; copy/export; paste JSON; versioned parser/validator; preview; confirmed import into existing timeline. Ensure no cloud AI connection is required.

**Phase 3 — Composite UX:** render and edit themed clip child elements; separate overlay channel; property inspector; save/load support and undo behavior.

**Phase 4 — Optional integrations:** direct AI provider submission (if supported), asset-generation handoff, themed clip templates, selected-clip AI regeneration.

Do not overbuild Phase 4 before the core import path and data integrity are tested.

## 12. Acceptance Criteria / Test Cases

1. With a 60-second audio track and user inputs, **Generate Prompt** includes verified `60000` ms duration and does not invent beats/markers.
2. The panel accepts title, intro, outro, theme, story description, and a nonnegative independent overlay count.
3. Requesting **0 overlays** creates no overlays; titles nested inside themed clips do not increase the overlay count.
4. Requesting **5 overlays** leads the prompt to demand exactly five independent overlay entities, and importing four fails count validation (or explicitly requests user-approved adjustment).
5. User can generate and copy the prompt without a configured API key or model connection.
6. Valid model JSON can be pasted, validated, previewed, and inserted into the project.
7. Invalid JSON or timestamps beyond audio duration prevent insertion and show clear messages.
8. Generated themed clips contain separately editable nested text/graphic/effect objects supported by the repository; unresolved visual asset prompts are displayed as placeholders, not fabricated media.
9. Moving a themed clip preserves child-relative timing; independent overlays remain independently movable.
10. Importing does not overwrite the original audio or unrelated existing clips and is reversible where undo/redo exists.
11. Imported storyboard state persists through existing save/reopen workflows.
12. Unsupported effects are either rejected or explicitly mapped to documented supported fallbacks; the user sees the warning.
13. Existing storyboard project files continue to load unchanged.
14. Core prompt builder, parser, validator, and import adapter have automated tests, including malformed JSON, duplicate IDs, oversized child timing, zero overlays, and valid fixture import.
15. Existing build, lint, and test commands pass, or any failures are listed with evidence and scope.

## 13. Open Design Decisions to Resolve from the Repo

Do not block on questions that the code can answer. Inspect first and document these decisions:

- Is a themed clip already a group/composition in the current data model, or is a new nested model necessary?
- Are scenes and themed clips distinct track types, or can a themed clip inhabit the primary visual track?
- Which fonts, graphics, transitions, effect names, and animation parameters does the renderer genuinely support?
- Is the timeline millisecond-based, frame-based, or dual representation? Where should canonical conversion occur?
- How are project-local/generated assets referenced, stored, and marked unresolved?
- How does the existing command/undo and persistence system handle import transactions?
- Which (if any) model providers are already integrated?

**Default engineering principle:** the AI proposes creative structured content; **AssetCraft remains the authority** for timing, asset identity, supported behavior, editability, and final insertion.
