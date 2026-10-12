# Storyline Builder (AssetCraft Plan mode)

_Concept by Paul, 2026-10-12. Review notes and decisions are marked **Review** and **Decided**._

> **Promise:** plan the story in blocks, generate what you need, and assemble it into a
> finished video.

A visual editor that turns an idea, transcript, or reusable template into a sequence of
production-ready blocks. Users can arrange the storyline manually, ask AI to create it, or
combine both approaches. Each block describes what appears, how long it lasts, and what is
needed to produce it.

- **Decided:** this is AssetCraft's **Plan mode**, not its own app. It needs AssetCraft's
  library, entities (characters) and version stacks.
- **Decided:** AI is optional and chosen in Preferences, per capability: **Local**, **API**,
  or **ArtCraft account** (spends the user's ArtCraft credits). See section 11.

## 1. Three ways to start

| Starting mode    | User experience                                                                      |
|------------------|--------------------------------------------------------------------------------------|
| Build manually   | Drag blocks from an asset palette into the storyline                                 |
| Generate with AI | Provide a transcript or idea, audience, style and target duration; AI proposes the sequence |
| Use a template   | Choose a trailer theme with predefined timing, music cues, placeholders, transitions |

All three modes open the same editor. AI-created storylines remain fully editable.

## 2. Storyline editor

- **Asset palette:** block types plus the AssetCraft library (browse, keywords, collections).
- **Storyline canvas:** ordered cards with thumbnails, descriptions, durations, production status.
- **Block inspector:** requirements, prompts, references, editing controls.
- **Preview player:** plays the assembled sequence with music and text.
- **Timing view:** cuts, overlapping layers, transitions, music cues.

Drag to reorder, duplicate, split, replace or delete. Select several blocks and ask AI to
revise that section ("make the opening more mysterious", "shorten this to eight seconds").

**Review, preview player scope:** full video playback with compositing and audio mixing
is FilmCraft's whole engine.
- **v1:** an animatic, meaning stills or poster frames on timing, plus music and text. Add a
  "Preview in FilmCraft" action (export, then open over `--bridge`) for full-fidelity checks.
- **Later:** depend on FilmCraft's `media` and `render` crates for real in-app playback.

## 3. Block structure

A block is a story moment and can contain several assets. An intro could combine a
character video, a text overlay, background music and a glow effect.

| Field                  | Purpose                                                             |
|------------------------|---------------------------------------------------------------------|
| Block ID and title     | Stable identity and readable label                                  |
| Story purpose          | Hook, introduction, explanation, conflict, reveal, closing          |
| Duration               | Intended time on the storyline                                      |
| Description            | What the viewer sees or hears                                       |
| Requirements           | Mandatory content, actions, framing, wording, references            |
| Assets                 | Images, video, text, audio, characters, effects used in the block   |
| Timing rules           | Fixed, flexible, or aligned to a music cue                          |
| Entry / exit           | Incoming and outgoing transitions                                   |
| Continuity references  | Characters, environments, palette, lighting, visual style           |
| Source reference       | Transcript passage or timestamp supporting the block                |
| Status                 | Draft, ready to generate, generating, needs review, approved        |

A block's duration is separate from an asset's duration. A five-second block might hold a
seven-second source clip trimmed to fit, plus a two-second text overlay.

**Review, how this maps:**
- Block duration vs asset trim is OTIO's track placement vs `source_range`.
- Use exact rational time like FilmCraft's `Tick` (254,016,000,000 per second) so nothing drifts.
- Source references can come from FilmCraft's Whisper transcripts, which have word timestamps
  (`transcript.generate`, `transcript.set`), or from SRT/VTT files.

## 4. Asset types

| Palette item   | What it contains                                                     |
|----------------|----------------------------------------------------------------------|
| Image          | Uploaded or generated still, crop, optional camera movement          |
| Video          | Uploaded or generated clip, source trim, playback speed              |
| B-roll         | Supporting image or video with a defined narrative purpose           |
| Text           | Exact wording, typography, placement, animation                      |
| Character      | Reusable identity, reference images, appearance requirements         |
| Effect         | Glow, particles, distortion, colour treatment                        |
| Transition     | Cut, dissolve, wipe, fade, or custom, between blocks                 |
| Voiceover      | Script, voice selection, delivery, audio                             |
| Music / sound  | Track, sound cue, timing, fades, levels                              |

How these relate:
- A character is reusable across blocks. It's an AssetCraft **entity**.
- B-roll is a *role* given to an image or video, not a separate kind of file.
- Effects attach to assets or blocks. Transitions connect blocks.

**Review:** keep transitions and effects to a small vocabulary that FilmCraft can build.
Anything it can't build becomes a marker with a note, and the adapter reports what it
couldn't map instead of dropping it.

## 5. Prefilled AI directives

Every asset has an editable prompt, prefilled from its type, the block description, the
requirements and the project settings.

| Prompt field            | Content                                                     |
|-------------------------|-------------------------------------------------------------|
| Task                    | Generate, animate, revise, select or transform              |
| Main directive          | What the AI should produce                                  |
| Required content        | Details that must appear                                    |
| Constraints             | Restrictions and details to avoid                           |
| References              | Character sheets, images, previous shots, source material   |
| Output settings         | Duration, aspect ratio, resolution, format                  |
| Continuity instructions | Details that must match other assets                        |
| Acceptance criteria     | What makes the result usable                                |

Example video directive:

> Create a shot of the established Reaper character walking through a snow-covered clearing
> toward a faint blue soul light. Use the attached character reference. Preserve the cloak,
> proportions, staff, and eye color. Use soft cinematic lighting and a slow forward camera
> movement. Produce a seven-second clip in 16:9 with a stable opening and ending to allow
> trimming into a five-second block. No text overlays.

Store the editable template **and** the exact directive submitted, with references,
provider settings and generated versions. For text and effects, the directive can produce
structured editing instructions the app applies directly. Exact titles stay editable text
layers.

**Review:** keep three records per generation.
- The template.
- The rendered directive plus its hash.
- Provider, model, settings and seed, plus the outputs.

"Regenerate only what changed" then becomes a diff of directive hashes. Each output links
to its block and directive through **lineage**.

## 6. AI storyline creation and revision

The storyline AI receives:
- transcript, script or concept
- audience and objective
- target duration and aspect ratio
- template, music and style
- available assets and character references
- required messages and closing action

It returns structured blocks with descriptions, durations, asset requirements and
prefilled prompts.

Users can ask AI to:
- generate the whole storyline
- fill empty placeholders
- rewrite a selected block
- suggest missing B-roll
- adapt to a different duration
- create alternative openings or endings
- preserve selected blocks while revising the rest

Lock controls protect approved wording, assets, block order or timing.

**Review:**
1. **AI edits are patches, not rewrites.** The model returns operations (`insert_block`,
   `set_duration`, `rewrite_description`, …). The app rejects any operation that touches a
   locked field, applies the rest as one undo step, and keeps LightCraft's
   inverse-op pattern.
2. **Locks are per field** (wording, assets, order, timing), not per block.
3. **Schema-validated output:** define the block format with `serde` + `schemars`, send
   the JSON schema, and reject anything invalid. It's the Rust version of the Pydantic and
   Instructor approach. Locally, llama.cpp's JSON-schema or grammar mode constrains output
   directly.

## 7. Music-timed templates

Each template bundles:
- narrative structure and block placeholders
- a music track and an authored cue map
- shot requirements and duration limits
- text slots with length limits
- transitions and text animations
- visual defaults and output formats

Templates use locked timing for tightly choreographed trailers, or flexible timing for
explainers. Reordering a locked block flags any broken music alignment and offers a timing
fix.

**Review:** v1 cue maps are authored by hand. Beat detection comes later.

## 8. Production workflow

1. Create or generate the storyline.
2. Review descriptions, requirements and text.
3. Preview a rough storyboard using stills and placeholders.
4. Generate selected assets, one at a time or in a batch.
5. Review results and regenerate only what needs changes.
6. Assemble the timeline, titles, effects and audio.
7. Preview and export.

Show estimated cost and supported output settings before generating. A local provider
estimates **GPU time** where an API or ArtCraft provider estimates money or credits.
Generated assets stay attached to their blocks for comparing versions and restoring an
earlier one.

## 9. FilmCraft integration

Keep the storyline and template format independent of FilmCraft. An adapter turns approved
blocks into editor commands for placement, trims, titles, transitions, effects, audio and
export. The app owns the creative workflow, FilmCraft handles assembly, and the rendering
engine can change later.

**Review, two adapter paths:**
- **OTIO** (`filmcraft/crates/interchange/src/otio.rs`) carries cuts, trims, gaps and markers.
  One `Clip.1` goes in per layer, with `source_range` for the trim, `Gap.1` for empty slots and
  `Marker.2` for notes. Missing media imports offline. OTIO also opens in Resolve.
- **A command script** (`filmcraft-cli run -`, or `--bridge` to the running app) covers
  titles, transitions, effects and export.

## 10. First release, cut into slices

Original first-release list:
- manual drag-and-drop blocks
- AI storyline from a transcript or idea
- image, video, text and audio assets
- editable prefilled prompts and shared character references
- one music-timed trailer template
- storyboard preview before video generation
- individual asset generation and replacement
- FilmCraft assembly and export

**Review, cut into slices that each work on their own:**

| Slice | Delivers                                                                                   | Proves                   |
|-------|--------------------------------------------------------------------------------------------|--------------------------|
| A     | Manual blocks: canvas, inspector, reorder/split/duplicate, image/video/text/audio from the library | The block model  |
| B     | Animatic preview (stills, music, text) + OTIO export that opens in FilmCraft               | The FilmCraft round trip |
| C     | AI storyline from transcript or idea (schema-validated) + patch-based revise + locks       | AI that stays editable   |
| D     | Prefilled directives, character references, generation through the provider adapter       | Generation workflow      |
| E     | One music-timed template with an authored cue map; locked-timing warnings                  | Templates                |
| F     | FilmCraft assembly beyond OTIO: titles, transitions, export through the command adapter    | Finished video           |

## 11. Providers (Preferences)

One trait, many providers, chosen per capability in Preferences:

```rust
trait Provider {
  fn capabilities(&self) -> &[Capability];            // Text, Image, Video, Mesh3d, Voice, Music
  fn estimate(&self, req: &GenRequest) -> Estimate;   // money, credits, or GPU time
  fn submit(&self, req: GenRequest) -> Result<JobId>;
  fn poll(&self, job: JobId) -> JobState;             // queued | running(progress) | done | failed
  fn fetch(&self, job: JobId) -> Result<Vec<Output>>; // files + provider metadata
}
```

```toml
# assetcraft settings: providers.toml (no secrets in this file)
[defaults]
text   = "local-llama"
image  = "comfyui"
video  = "artcraft"
mesh3d = "trellis"

[[provider]]
id   = "local-llama"
kind = "openai-compatible"
url  = "http://127.0.0.1:8080/v1"

[[provider]]
id   = "comfyui"
kind = "comfyui"
url  = "http://127.0.0.1:8188"

[[provider]]
id      = "trellis"
kind    = "cli"
command = "trellis {input} -o {output}"

[[provider]]
id   = "artcraft"
kind = "artcraft-account"   # signs in, spends ArtCraft credits

[[provider]]
id   = "some-api"
kind = "http-api"
key  = "keyring:some-api"
```

Rules:
- API keys and tokens live in the OS keyring, never in settings files or the catalog.
- Every capability works with "None" selected, which means manual assets only.
- **ArtCraft account provider: confirm with the ArtCraft team before shipping.** Their
  backend (`artcraft-services`) is private. The desktop client code shows how the endpoints
  behave, but using their API and credits from a third-party app needs their OK. That
  conversation also fits the plan to offer AssetCraft to them.
