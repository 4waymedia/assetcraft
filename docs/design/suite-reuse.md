# Suite reuse: what AssetCraft takes from EffectCraft and craft-libs, and what it builds

_2026-11-14 · Stage 4 (Storyboard) → suite · decision proposed, spike done_

- **Scope:** every part of AssetCraft that draws, animates, sets type, encodes or muxes, set against
  the suite's shared Rust crates (`storytold/effectcraft`, `storytold/craft-libs`).
- **Project goal it serves:** one motion, type and media standard across the Crafting apps. A title
  made in AssetCraft is an EffectCraft composition, opens in EffectCraft, and renders the same in both.
- **Immediate area of improvement:** titles. Themes steps 1c–1g re-implemented, inside AssetCraft,
  what EffectCraft's crates already do. Paul, 2026-11-13: "I would prefer to use the same functions
  from effectCraft. I don't want to recreate things that are already a suite standard."

Sources read: `storytold/effectcraft` @ `53576d2` (v0.24.1, 2026-11-13), `storytold/craft-libs` @
`7bb6c90` (2026-11-13), AssetCraft @ `3e50ac6`.

## 1. The rule

AssetCraft builds what is unique to planning and assembling a Cut. It takes motion, type, compositing,
codecs, containers and fonts from the suite, pinned by git revision, the way it already takes
`soundcraft-audio-io`, `photocraft-psd` and `craft-ui` (`Cargo.toml:36`, `:55`, `:60`).

| Decision  | Meaning                                                                                       |
| --------- | --------------------------------------------------------------------------------------------- |
| Take      | Use the suite crate; write only the adapter                                                   |
| Keep      | AssetCraft's own: nothing in the suite does it                                                |
| Retire    | AssetCraft code that goes once the suite path matches it (tests prove it first)               |
| Upstream  | Something AssetCraft needs that belongs in a suite crate; proposed there, not copied here     |

## 2. Spike S0 (2026-11-14): an EffectCraft title comp built and rendered from outside EffectCraft

A scratch crate (outside both repos) depended on `effectcraft-project`, `-render`, `-keyframe`, `-time`
and `-color` by path at `53576d2`. It built a 1920×1080 comp: a shape-layer plate, a text layer
"ISTANBUL" (92 px, tracking 160, opacity keyed 0 → 100 over 0.6 s) and an Outer Glow layer style, then
rendered it with `effectcraft_render::render_frame(&project, comp, tick, scale)`
(`crates/render/src/frames.rs:140`).

| Measure                                 | Result                                                                  |
| --------------------------------------- | ----------------------------------------------------------------------- |
| Builds outside EffectCraft              | Yes; `build::layer`, `shape_rect`, `shape_fill`, `styles::add_style`, keyframes, `TextDoc` all public |
| Fade                                    | Correct: mid-fade at 0.3 s, full at 2.0 s                               |
| Frame time, 1920×1080                   | 254 ms first frame (font load), 59–114 ms after                         |
| Frame time, 640×360 (Viewer preview)    | 12 ms                                                                   |
| Cold release build (2-core container)   | 271 s; AssetCraft's own `assetcraft-animatic` cold release: 304 s       |
| Crates pulled in                        | 107, every licence inside the suite allowlist                           |
| Binary                                  | 23 MB release, unstripped                                               |
| Heaviest edge                           | `effectcraft-segment` (ML models, `ndarray`) arrives only through `effectcraft-effects` → finding R-U1 |

The plate and accent bar landed in the wrong place and the glow was faint: the spike's coordinates
(shape positions are relative to the layer's centre) and default style values, not a crate gap.

## 3. The split

### 3.1 Titles and motion: take from EffectCraft

| Capability                         | AssetCraft today                                                     | EffectCraft @ `53576d2`                                                                 | Decision          |
| ---------------------------------- | -------------------------------------------------------------------- | --------------------------------------------------------------------------------------- | ----------------- |
| Easing, in/out moves, duration     | `storyboard/src/text.rs` `TextAnimation`, `TextEffect`, `TextExit`, `Timing::frame_at` | `effectcraft-keyframe` `Keyframe` (temporal ease, hold), `crates/keyframe/src/lib.rs:67` | Take; Retire ours |
| Text layout, tracking, wrap        | `animatic/src/text.rs` (156 lines, `ab_glyph`), `ui-egui/src/text_view.rs` | `effectcraft-text` (shaping via harfrust, per-glyph geometry)                         | Take; Retire ours |
| Typewriter, Erase, Decode          | `AnimFrame.reveal` in `text.rs`                                      | text animators + range / expression selectors, `project/src/build.rs:163-230`; Character Offset `:252`, `:312` | Take              |
| Plate, accent bar, rules, frame, corners | `storyboard/src/decor.rs` `card_decor`, `text_decor`           | shape layers: `shape_rect`, `shape_fill`, `shape_stroke`, `build.rs:387-463`            | Take; Retire ours |
| Patterns (grid, dots, scanlines)   | `decor.rs` `Pattern`                                                 | `shape_repeater` `build.rs:534`; effect `ec.generate.grid`                              | Take; Retire ours |
| Gradient, radial, grain, vignette  | `text.rs` `Backdrop::color_at`, `grain_at`                           | `shape_gradient_fill` `build.rs:487`; effects `ec.noise.addgrain`, `ec.stylize.ccvignette` | Take; Retire ours |
| Shadow, outline, glow              | `decor.rs` `finish_passes`                                           | layer styles `dropShadow`, `stroke`, `outerGlow`, `project/src/styles.rs:28-38`; effect `ec.stylize.glow` | Take; Retire ours |
| Laser beams                        | (planned B1, `DecorLine`)                                            | `shape_stroke` + Trim Paths `build.rs:527`                                               | Take; never build |
| Stack pieces                       | (planned B2–B3, `Piece`)                                             | shape layers with position keys; seeded offsets in code                                  | Take; never build |
| Compositing a frame                | `animatic/src/draw.rs` (198 lines), `lib.rs` card and overlay paths  | `effectcraft_render::render_frame`                                                       | Take for titles   |
| Save for EffectCraft               | none                                                                 | `craft-ecproj-format` (craft-libs) + `effectcraft-project` serde                         | Take (TH6 door)   |
| Templates with fill-in fields      | none                                                                 | `.ectemplate` + Essential properties, `render --template --values`                       | Take later        |

### 3.2 Media: take from craft-libs

| Capability                          | AssetCraft today                                                         | craft-libs @ `7bb6c90`                                                    | Decision                       |
| ----------------------------------- | ------------------------------------------------------------------------ | ------------------------------------------------------------------------- | ------------------------------ |
| H.264 encode                        | `crates/video` over `rusty_h264-encoder` (D-SF6, `docs/storyboard-plan.md:223`) | `craft-video` `h264` (encode under `src/h264/encode/`), plus ProRes, VP9, AV1, HEVC | Take after a speed and quality check against D-SF6's numbers |
| MP4 / MOV writing                   | `crates/video/src/mux.rs` (563 lines, written here)                      | `craft-media` `isobmff/mux.rs`, `matroska/mux.rs`                          | Take; Retire ours              |
| AAC encode                          | `rusty_aac`                                                               | `craft-audio` (AAC encoder)                                               | Take with the H.264 switch      |
| Alpha video (EffectCraft title clips) | none                                                                    | `craft-video` ProRes 4444, VP9 with alpha in WebM                          | Take                           |
| Decoding inset videos in the export | not built (open follow-up)                                               | `craft-media` demuxers + `craft-video` decoders                            | Take; builds the open follow-up |
| MP4 / MOV duration                  | `probe/src/duration.rs:16` (own `mvhd` reader)                            | `craft-media` ISO BMFF demuxer                                             | Take; Retire ours               |
| Fonts                               | `assets/fonts/` (Inter ×3, JetBrains Mono, OFL texts) in the repo       | `craft-fonts` repo + `craft-font-catalog`, `craft-fonts-build` (house rule: font files live in `storytold/craft-fonts`) | Take; move ours out             |

### 3.3 Keep: AssetCraft's own

| Capability                                                         | Where                                                             | Why it stays                                              |
| ------------------------------------------------------------------ | ----------------------------------------------------------------- | --------------------------------------------------------- |
| The storyboard document: Cut, shot types, sections, insets, voice-over | `storyboard/src/model.rs`, `inset.rs`                          | The planning model is AssetCraft's product                 |
| Generating a board from a song: cue maps, presets, templates       | `generate.rs`, `template.rs`                                      | Nothing in the suite plans a Cut                           |
| Music fit: chorus loop, structured fit, crossfades                 | `storyboard` music modes                                          | Editorial logic, not a codec                               |
| Voice over music: ducking, speech spans                            | `audio/src/mix.rs`, `program.rs`                                  | Mix rules specific to a Cut                                |
| The theme file and its commands                                    | `storyboard/src/theme.rs`, `engine/src/text_cmd.rs`, `theme_cmd.rs` | The user's contract; see R-U3 for moving the schema to the suite |
| **The title builder (new):** theme + clip words + length → an EffectCraft comp | new crate `crates/titles`                               | The one adapter between our schema and EffectCraft's model |
| Library: browsing, thumbnails, analysis, cache                     | `fs`, `thumbs`, `analysis`, `cache`, `watch`                      | AssetCraft's original job                                  |
| The Inspector, Strip and Viewer                                    | `ui-egui`                                                         | The UI only draws; it shows frames the titles crate renders |

## 4. Findings

| ID   | Finding                                                                                          | Scope            | Goal it serves                    | Fix                                                                                                      | Owner          | Status   |
| ---- | ------------------------------------------------------------------------------------------------ | ---------------- | --------------------------------- | -------------------------------------------------------------------------------------------------------- | -------------- | -------- |
| R-T1 | Titles re-implement EffectCraft's keyframes, text, shapes and styles                             | AssetCraft titles | One motion standard               | New `crates/titles` builds an EffectCraft comp; retire `decor.rs`, `Backdrop`, `finish_passes`, `animatic/text.rs` after parity tests | Claude | proposed |
| R-T2 | Planned B1–B5 (lines, glows, pieces) would add a third drawing system                            | themes step 2    | Don't rebuild the standard        | Build Stack and Laser Trace as comp recipes in `crates/titles`                                          | Claude         | proposed |
| R-M1 | Own MP4 muxer and third-party H.264/AAC beside the suite's codecs                                | export           | One media standard; alpha clips   | Spike `craft-video` + `craft-media` against D-SF6 (speed, PSNR, size); switch if it matches              | Claude         | proposed |
| R-M2 | Inset video isn't decoded in the export                                                          | export           | Finished Cuts                     | Decode with `craft-media` + `craft-video` (comes with R-M1)                                              | Claude         | open     |
| R-F1 | Font files live in AssetCraft's repo; the house rule puts them in `craft-fonts`                  | fonts            | Suite house rules; theme fonts (F1) | Use `craft-fonts` through `craft-fonts-build`; check Inter and JetBrains Mono are there first          | Claude         | proposed |
| R-U1 | Embedding `effectcraft-render` pulls `effectcraft-segment` (ML models) through `effectcraft-effects` | EffectCraft crates | Light embedders               | Propose a feature on `effectcraft-effects` that leaves out Roto/face models                              | EffectCraft    | opportunity |
| R-U2 | Template values (`render --template --values`) have no documented schema                         | EffectCraft CLI  | Templates as theme moves           | Document the `--values` schema                                                                           | EffectCraft    | opportunity |
| R-U3 | The theme file is meant to be suite-wide (`themes.md` scope) but lives in AssetCraft             | themes           | One theme file for every app       | Propose a `craft-theme` crate in craft-libs once the schema settles                                      | Paul (suite)   | opportunity |

## 5. Migration, one step at a time

Each step ends green (`cargo xtask ci`) and changes no user file format.

| Step | What                                                                                                                         | Proves                                                                 |
| ---- | ---------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| M1   | `crates/titles`: pin EffectCraft by rev; `title_comp(look, timing, words, size) -> (Project, ItemId)` for a Card with plate, words, fade | A test renders Bold and Neon Night cards; words, colours and the fade's alpha at set times match today's renderer within tolerance |
| M2   | The export draws title cards and overlays through `crates/titles`                                                             | Export tests pass; filmstrip proof matches `themes-1g-motion.png`      |
| M3   | The Viewer shows the same frames (640×360, cached per clip and time)                                                           | Viewer and export frames agree                                         |
| M4   | Map every theme field (backdrop, decor, finish, motion) to comp properties; retire `decor.rs`, `Backdrop`, `finish_passes`, `animatic/text.rs` | All 1d–1g tests pass on the new path                             |
| M5   | Stack and Laser Trace as comp recipes (`motion.bannerIn` / `bannerOut`)                                                       | Filmstrip proof against the Banner Build Moves mock                    |
| M6   | "Open in EffectCraft…": save the title comp as `.ecproj`                                                                      | The file opens in EffectCraft and renders the same frame               |
| M7   | R-M1 spike, then the switch if it matches D-SF6                                                                               | Export tests; file sizes and PSNR recorded beside D-SF6                |
| M8   | R-F1 fonts                                                                                                                    | Titles and UI set in the same faces from `craft-fonts`                 |

### Theme fields → EffectCraft comp

| Theme field                     | EffectCraft property                                                        |
| ------------------------------- | --------------------------------------------------------------------------- |
| `background` solid / gradient / radial | shape layer: `shape_rect` + `shape_fill` or `shape_gradient_fill` (radial flag) |
| `background.grain`              | effect `ec.noise.addgrain`                                                   |
| `background.vignette`           | effect `ec.stylize.ccvignette`                                               |
| `plate`                         | shape layer behind the words                                                 |
| `decor.accentBar`, `rules`, `frame`, `corners` | shape rects and strokes in the accent colour                  |
| `decor.pattern`                 | `shape_repeater` or effect `ec.generate.grid`                                |
| `type.display` family, size, tracking, uppercase | `TextDoc` `font`, `size`, `tracking`, `all_caps`            |
| `type.display.finish`           | layer style `dropShadow` / `stroke` / `outerGlow`                             |
| `motion.titleIn` fade / rise / zoom | keyframes on `transform/opacity`, `position`, `scale`                    |
| `motion.titleIn` typewriter, decode | text animator with a range selector; Character Offset for decode       |
| `motion.titleEffect` push / drift / pulse | scale or position keys; pulse as an expression                      |
| `motion.titleOut`               | the in's keys mirrored at the end of the duration                            |
| `motion.titleSeconds`           | layer out point                                                              |
