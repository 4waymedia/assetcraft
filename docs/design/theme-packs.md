_2026-11-18 · Stage 4 (Storyboard) → suite · hybrid decided by Paul (D-M1a); TD1 decided; M1b, M1c-a (Laser Trace) and M1c-b (Stack) built; TP1 next_

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
Documents/AssetCraft/Themes/acme-brand/          a pack is a folder (TD1, Paul 2026-11-18)
├── theme.json                                    colours, fonts, background, motion, assets (format v1)
├── designs/       card.ectemplate, banner.ectemplate      made in EffectCraft
├── images/        logo.png, texture-paper.jpg             PNG · JPEG · WebP
├── sounds/        title-sting.wav, whoosh.wav, bed.mp3    WAV · MP3
├── video/         intro.mp4, loop-bg.mov, outro.mp4       MP4 · MOV · WebM
└── fonts/         BrandSans-Bold.ttf, OFL.txt             TTF · OTF, OFL-1.1 only
```

A loose `<id>.theme.json` in `Themes/` (today's format) still loads; it simply has no folders.

```jsonc
// theme.json: two new optional fields
"designs": { "card": "designs/card.ectemplate", "banner": "designs/banner.ectemplate" },   // TP1
"assets": [                                                                                 // TP3 (TH4 revised)
  { "role": "logo",            "path": "images/logo.png",        "license": "own" },
  { "role": "texture",         "path": "images/texture-paper.jpg", "license": "CC0-1.0", "credit": "…" },
  { "role": "titleSound",      "path": "sounds/title-sting.wav", "license": "own" },
  { "role": "transitionSound", "path": "sounds/whoosh.wav",      "license": "CC-BY-4.0", "credit": "Name, URL" },
  { "role": "musicBed",        "path": "sounds/bed.mp3",         "license": "own" },
  { "role": "intro",           "path": "video/intro.mp4",        "license": "own" },
  { "role": "backgroundVideo", "path": "video/loop-bg.mov",      "license": "own" },
  { "role": "outro",           "path": "video/outro.mp4",        "license": "own" },
  { "role": "font",            "path": "fonts/BrandSans-Bold.ttf", "license": "OFL-1.1" }
]
```

| Role              | Folder    | Used by                                                                    | Step |
| ----------------- | --------- | -------------------------------------------------------------------------- | ---- |
| `logo`            | `images/` | A design's `Logo` control; a corner bug on overlays (later)                | TP3  |
| `texture`         | `images/` | A design's `Texture` control; a card background under the theme's colour   | TP3  |
| `backgroundImage` | `images/` | A design's `Background Image` control; a card's background                 | TP3  |
| `backgroundVideo` | `video/`  | A design's `Background Video` control; a looping card background           | TP3  |
| `intro`, `outro`  | `video/`  | Offered as the Cut's first and last clip (Start screen, templates)         | TP5  |
| `titleSound`      | `sounds/` | Played on a title's in (themes step 5)                                     | TP5  |
| `transitionSound` | `sounds/` | Played on transitions (themes step 3)                                      | TP5  |
| `musicBed`        | `sounds/` | Offered as the song on the Start screen                                    | TP5  |
| `font`            | `fonts/`  | Display and body families named in `type`, loaded for this theme only (F1) | TP4  |

Asset rules (adding to themes.md §2's load rules; the clean-room rule applies to user packs as it
does to ours):

| Rule                                                                                                     | On failure                         |
| -------------------------------------------------------------------------------------------------------- | ---------------------------------- |
| `path` is relative, inside the pack, in the role's folder, with an allowed extension                     | that asset refused, theme loads    |
| `license` is `own`, `CC0-1.0`, `CC-BY-4.0` (with `credit`) or `OFL-1.1` (fonts only)                     | that asset refused                 |
| Files on disk but not listed in `assets` are ignored (no licence, not used)                              | listed under the theme's notes     |
| Sizes: image ≤ 32 MiB and ≤ 8192 px a side, sound ≤ 64 MiB, video ≤ 512 MiB, font ≤ 16 MiB              | that asset refused                 |
| Each asset is opened by the decoder that already reads it in the Organizer (thumbs, media, audio)       | that asset refused                 |
| A `CC-BY-4.0` asset's `credit` goes into the Cut's credits card and the export's notes when it's used    | —                                  |

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
| `Logo`                     | Media        | The pack's `logo` asset (TP3)                 |
| `Texture`                  | Media        | The pack's `texture` asset (TP3)              |
| `Background Image`         | Media        | The pack's `backgroundImage` asset (TP3)      |
| `Background Video`         | Media        | The pack's `backgroundVideo` asset (TP3)      |

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
| M1c-a | **Built 2026-11-18.** `titles::builtin`: **Laser Trace** (`banner-laser-trace`) as an EffectCraft comp made in code, 1920 × 1080, 6 s. The mock's curves are sampled per frame into keys: beams on P0 → A → B with trimmed trails, the traced edge kept as the border (trim paths), the fill and bar wiped by keyed rectangle sizes, the wipe's leading edge, the words decoded by a Character Offset animator whose range selector locks letters left to right, the detail line in the accent (Fill Color animator), the fold by the layer's scale about the banner's centre. Controls: Headline, Caption, Accent (linked to the bar, every beam's glow and the detail line), Surface, Headline Font | Built: in / hold / out pixels; the Neon accent reaches bar, border and detail line; fewer letters ink less; `AC_FILMSTRIP=<folder>` writes gold and neon filmstrips |
| M1c-b | **Built 2026-11-19.** **Stack** (`banner-stack`), from plain layers, no precomps: the mock's seeded 16 × 4 grid cut into 22 pieces of 2–4 cells; each piece is a null carrying the drop (sampled per frame: ease-in fall, sideways shift, bounce) with its own copy of the plate and bar and of whichever lines of words its rows touch, each masked to its cells (masks are on every layer but cameras, lights and nulls), plus a white cell overlay for the landing flash and seam tint; seams are the masks' expansion (−1 px, then +0.5 px once closed so neighbouring edges overlap). Once the seams close one still, whole banner replaces the pieces, so the hold costs one copy. Out: a band per row on a null whose scale and position squash it into what's below, bottom row first, with a white flash. Controls as Laser Trace's, each linked to every copy (Headline 14 properties, Accent 37) | Built: nothing before the first piece, part-way at 0.9 s, the hold matches Laser Trace's plate, bar and words, rows going at 4.75 s, gone by 5.9 s; the accent and headline reach every piece; filmstrips; `AC_TIMING=1` prints frame times |
| TP1  | `.theme.json` `designs`; `storyboard.themes` lists each theme's designs (and why one didn't load)                             | Load rules; a missing design is a warning, not a refused theme             |
| TP2  | Title cards and banners draw through a design when their theme has one (Viewer cached at 640 × 360; export at full size); the Inspector picks the design; failure falls back to the built-in look | Viewer and export agree; fallback shows the problem                        |
| TP3  | `assets` in `theme.json` (rules above); `Logo`, `Texture`, `Background Image`, `Background Video` Media controls filled from them | A logo PNG fills the slot; an unlisted or unlicensed file is refused with a note |
| TP5  | Pack sounds and clips: title and transition sounds, intro / outro clips and the music bed offered on the Start screen | Each role used once; credits carry CC-BY lines |
| TP4  | Pack fonts (`fonts/`, OFL-1.1) loaded for the theme's `type`; then "Edit in EffectCraft…" opens the pack's design in EffectCraft (`effectcraft-cli` / control channel) and picks up the saved file | The file reloads after a save                                              |

## 5. Decisions for Paul

| ID  | Question                                         | Recommendation (Claude)                                                                 |
| --- | ------------------------------------------------ | --------------------------------------------------------------------------------------- |
| TD1 | What a pack is on disk                           | **Decided (Paul, 2026-11-18):** a folder with a `theme.json` and folders for designs, images, sounds, video clips and fonts (§3) |
| TD2 | The control names in §3 (now with `Texture`, `Background Image`, `Background Video`) | As listed. They mirror the theme file's own names, so a designer reads them once |
| TD3 | First roles                                      | `card` and `banner`; `lowerThird` and `endCard` after TP2                               |

## 6. Findings

| ID   | Finding                                                                                     | Scope            | Goal it serves               | Fix                                                                                     | Owner       | Status      |
| ---- | ------------------------------------------------------------------------------------------- | ---------------- | ---------------------------- | --------------------------------------------------------------------------------------- | ----------- | ----------- |
| R-U6 | Opening a template (`read_template`, `upgrade_effects`, instancing) is engine and CLI code, not a library | EffectCraft crates | Theme packs in any suite app | Propose an `effectcraft-template` crate (read, upgrade, fill by control name, render) that AssetCraft and EffectCraft's CLI both use | EffectCraft | opportunity |
| R-T4 | Laser Trace has no corner flares, and its glow is a wide 50 % stroke rather than a blur, so it reads lighter than the mock | M1c-a | The mock's look | Flares as radial-gradient ellipses keyed per corner; the glow as the layer style `outerGlow` linked to Accent | Claude | opportunity |
| R-T5 | Stack's drop frames cost 150–190 ms at 1080p and 40–75 ms at 640 × 360 (hold 25–40 ms), against Laser Trace's 25–60 ms: each of ~60 masked copies is drawn full-frame before its mask | M1c-b | Smooth Viewer playback | TP2's per-frame Viewer cache covers playback; upstream, EffectCraft could clip a layer's drawing to its masks' bounds | Claude (cache) / EffectCraft (clip) | opportunity |
| R-T6 | During Stack's line clear a faint hairline shows where row bands meet (the plate is 90 % opaque, so their 0.5 px overlap draws twice) | M1c-b | The mock's look | Draw the plate's rows at 100 % with the band's opacity at 90 %, or one band layer per row boundary | Claude | opportunity |
| R-U2 | (from suite-reuse) the `--values` schema isn't documented                                   | EffectCraft CLI  | Templates as theme designs   | The §3 name table is AssetCraft's half; propose it as the documented schema             | EffectCraft | opportunity |
