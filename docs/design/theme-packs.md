_2026-11-18 · Stage 4 (Storyboard) → suite · hybrid decided by Paul (D-M1a); TP decisions open; M1b building_

- **Scope:** theme packs. A user designs branded title pieces in EffectCraft (a card, a banner,
  later a lower third and an end card), saves them as templates in a pack beside a
  `.theme.json`, and AssetCraft fills them with the clip's words and the theme's colours and fonts.
  AssetCraft's own renderer keeps drawing the built-in looks.
- **Project goal it serves:** branding. A studio's look is designed once, in the suite's motion
  tool, and every Cut uses it with no rebuilding. This follows the suite-reuse rule: AssetCraft
  doesn't build what EffectCraft already does ([suite-reuse.md](suite-reuse.md)).
- **Immediate area of improvement:** today a title's look is one of the fixed built-in themes.
  Built-in themes can't hold a logo, a custom shape or a move of their own.

## 1. Decision D-M1a (Paul, 2026-11-18)

| Part                                                          | Engine                                 | Why                                                             |
| ------------------------------------------------------------- | -------------------------------------- | --------------------------------------------------------------- |
| Built-in looks (Bold, Neon Night, …: plate, gradient, decor, finish, fade) | AssetCraft's renderer (kept) | 74–96 ms per 1080p frame against EffectCraft's 115–125 ms; no size cost (suite-reuse §2b) |
| Theme-pack designs (anything a user makes in EffectCraft)     | EffectCraft, in-process (`crates/titles`) | Branding needs custom shapes, logos and moves; "Solid backgrounds only is too general" (Paul) |
| Banner moves: Stack, Laser Trace                              | EffectCraft: built-in designs made in code | Same path as packs, so a pack can replace them               |
| M4 (retire AssetCraft's renderer)                             | not planned                            | Revisit after R-U5 (an EffectCraft effects feature) lands      |

## 2. What EffectCraft gives us at `v0.24.1` (`582344d`)

| Need                          | EffectCraft                                                                                          | Where                                        |
| ----------------------------- | ---------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| A design file                 | `.ectemplate`: a stored ZIP with `manifest.json` (`format: "effectcraft-template"`, `version` 1, `comp`), `project.ecproj`, `poster.png`, `media/<item>/<file>` | `crates/engine/src/cmd/essential.rs:25`, `:878` |
| Made by the user              | File › Save as Template…, `essential.exportTemplate`                                                 | `crates/engine/src/home/templates.rs:7`      |
| Fields a user exposes         | Essential Graphics controls: Text, Color, Slider, Checkbox, Point, Angle, Dropdown, Media, Font, Scale; each has a `name` | `crates/project/src/essential.rs:25-100`     |
| Filling the fields (library)  | `essential::with_overrides(project, comp, &[Override{control, value}])`: sets each control's property, drops its keys, swaps media | `crates/project/src/essential.rs:660`        |
| Reading the ZIP (library)     | `effectcraft_lottie::zip::read_stored`                                                               | `crates/lottie/src/zip.rs:83`                |
| Rendering                     | `effectcraft_render::render_frame(&project, comp, tick, scale)`                                      | `crates/render/src/frames.rs:140`            |

Opening a template is engine code: `read_template` plus `upgrade_effects`, and the CLI's `--values`
(`apps/effectcraft-cli/src/template.rs`). AssetCraft takes only the library crates and does the
short read itself (finding R-U6).

## 3. A theme pack

```text
Documents/AssetCraft/Themes/acme-brand/          (TH9: user themes live in Themes/)
├── acme-brand.theme.json                         colours, fonts, background, motion (format v1)
└── designs/
    ├── card.ectemplate                           made in EffectCraft
    └── banner.ectemplate
```

```jsonc
// acme-brand.theme.json: one new optional field (TP1)
"designs": { "card": "designs/card.ectemplate", "banner": "designs/banner.ectemplate" }
```

### Filling a design: controls bound by name

AssetCraft fills a control when its name matches (case-insensitive). Every other control keeps the
value the designer gave it.

| Control name (EffectCraft) | Control type | AssetCraft fills it with                      |
| -------------------------- | ------------ | --------------------------------------------- |
| `Headline`                 | Text         | The clip's words (title card; banner text)    |
| `Caption`                  | Text         | The second line, if the clip has one          |
| `Background`               | Color        | `color.background`                            |
| `Surface`                  | Color        | `color.surface`                               |
| `Text`                     | Color        | `color.text`                                  |
| `Muted`                    | Color        | `color.muted`                                 |
| `Accent`                   | Color        | `color.accent`                                |
| `Accent 2`                 | Color        | `color.accent2`                               |
| `Headline Font`            | Font         | `type.display`                                |
| `Body Font`                | Font         | `type.body`                                   |
| `Logo`                     | Media        | The theme's `assets` entry with role `logo` (TP3) |

This works in both directions. A designer can lock a colour by naming the control something else
or leaving it out. One pack's designs follow its theme file, so changing a colour in
`.theme.json` re-brands every design in the pack.

### Size and timing

- The design's comp size is its native size. AssetCraft renders at `board size / comp size` and
  places the result with the board's crop (F2, `FrameSpec::place`), so a 1080 × 1920 design fills
  a vertical board, and a 16:9 design used on a vertical board is cropped by the board's fill and
  snap.
- Time is the clip's elapsed seconds, held at the design's last frame. The design's own duration
  is its move. The clip can be longer, and the end holds.
- The Viewer renders at 640 × 360 (6–9 ms) and caches each frame by clip, time and size. The export
  renders at full size (115–125 ms at 1080p).

### Hostile input (like themes and presets)

| Rule                                                                                       | On failure                        |
| ------------------------------------------------------------------------------------------ | --------------------------------- |
| File ≤ 64 MiB; `manifest.json` ≤ 64 KiB and `project.ecproj` ≤ 16 MiB                       | refused                           |
| `format` is `effectcraft-template`, `version` ≤ 1, `comp` names a comp                      | refused                           |
| Comp side 16–8192                                                                          | refused                           |
| Rendering runs behind `catch_unwind` (EffectCraft is a dependency: no unwrap in our code, and a panic in theirs can't take the app down) | the clip falls back to its built-in look and lists a problem |
| Media entries are read only when a Media control is filled (TP3), and only into the cache   | skipped                           |

## 4. Steps (one at a time)

| Step | What                                                                                                                         | Test                                                                       |
| ---- | ---------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| M1b  | `crates/titles`: `Design::read(bytes)` (the ZIP, manifest, project, the comp's controls), `Design::controls()`, `Fill` (words, palette, fonts) → `with_overrides`, `Design::render(fill, secs, w, h)`. Not used by the app yet | A design built in code and saved as a ZIP reads back; a Headline and an Accent fill change the frame; unnamed controls keep their values; bad files are refused (not a ZIP, wrong format, no comp, too big) |
| M1c  | Built-in designs made in code with the same controls: Stack and Laser Trace (Banner Build Moves mock)                         | Filmstrip proof against the mock; both fill from Bold and Neon Night       |
| TP1  | `.theme.json` `designs`; `storyboard.themes` lists each theme's designs (and why one didn't load)                             | Load rules; a missing design is a warning, not a refused theme             |
| TP2  | Title cards and banners draw through a design when their theme has one (Viewer cached at 640 × 360; export at full size); the Inspector picks the design; failure falls back to the built-in look | Viewer and export agree; fallback shows the problem                        |
| TP3  | `Logo` and other Media controls from the theme's `assets`                                                                    | A logo PNG fills the slot                                                  |
| TP4  | "Edit in EffectCraft…" opens the pack's design in EffectCraft (`effectcraft-cli` / control channel) and picks up the saved file | The file reloads after a save                                              |

## 5. Decisions for Paul

| ID  | Question                                         | Recommendation (Claude)                                                                 |
| --- | ------------------------------------------------ | --------------------------------------------------------------------------------------- |
| TD1 | What a pack is on disk                           | A folder in `Themes/` (theme file + `designs/`), not one archive. It's editable in place and shareable as a zip later |
| TD2 | The control names in §3                          | As listed. They mirror the theme file's own names, so a designer reads them once       |
| TD3 | First roles                                      | `card` and `banner`; `lowerThird` and `endCard` after TP2                               |

## 6. Findings

| ID   | Finding                                                                                     | Scope            | Goal it serves               | Fix                                                                                     | Owner       | Status      |
| ---- | ------------------------------------------------------------------------------------------- | ---------------- | ---------------------------- | --------------------------------------------------------------------------------------- | ----------- | ----------- |
| R-U6 | Opening a template (`read_template`, `upgrade_effects`, instancing) is engine and CLI code, not a library | EffectCraft crates | Theme packs in any suite app | Propose an `effectcraft-template` crate (read, upgrade, fill by control name, render) that AssetCraft and EffectCraft's CLI both use | EffectCraft | opportunity |
| R-U2 | (from suite-reuse) the `--values` schema isn't documented                                   | EffectCraft CLI  | Templates as theme designs   | The §3 name table is AssetCraft's half; propose it as the documented schema             | EffectCraft | opportunity |
