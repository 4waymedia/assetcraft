# Themes: one look, every Crafting app

_2026-11-06 (revised 2026-11-07) · Stage 4 (Storyboard) → suite · spec T0′ approved by Paul, step 1a building_

- **Scope:** a suite-wide theme file (`.theme.json`): fonts, colours, backgrounds, imagery treatment and
  motion feel; 20 themes that ship with AssetCraft; per-app appliers that turn a theme into each app's
  own commands; agent-made themes.
- **Project goal it serves:** a Cut, its title cards, its EffectCraft motion pieces, its FilmCraft
  titles and its DesignCraft print pieces all share one look, chosen once or written by an agent from a
  brief ("dark, gilded, 1920s noir").
- **Immediate area of improvement:** today a look is `TextTheme` (`crates/storyboard/src/text.rs:68`):
  4 hard-coded themes (Bold, Minimal, Cinematic, Paper) painting a `Look` (`text.rs:82`: background,
  text, plate, sizes, tracking, uppercase) on title cards and overlays only, with the app's UI font.

> **2026-11-14 — drawing moves to EffectCraft's crates.** Paul, 2026-11-13: "I would prefer to use the
> same functions from effectCraft. I don't want to recreate things that are already a suite
> standard." The theme file and its commands stay; what draws a theme (steps 1c–1g's renderer, and
> step 2's planned moves) becomes an EffectCraft composition built by a new `crates/titles`. The split,
> spike numbers and migration steps M1–M8 are in [suite-reuse.md](suite-reuse.md).

## Decisions

| ID   | Decision                                                                                       | By / date          | Consequence                                                         |
|------|------------------------------------------------------------------------------------------------|--------------------|---------------------------------------------------------------------|
| TH1  | Themes are **standalone**: any theme works with any Style › Preset (`docs/storyboard-presets.md`); timing and look stay separate | Paul, 2026-11-06 | 20 presets × 20 themes, not 20 fixed templates                      |
| TH2  | The 20 shipped themes aim for **variety** (mood, era, genre, colour, type), not variants of one look | Paul, 2026-11-06 | Roster below spans 20 distinct directions                           |
| TH5  | **Look + Move + Place**: every element (title, transition, panel, callout) is a theme look, a named move from a short list, and a place. AssetCraft has no keyframe editor | Paul, 2026-11-07 ("I don't want to build everything… the most powerful tools we can within our framework") | Moves are a closed, data-driven list; `.motion.json` exchange deferred |
| TH6  | **EffectCraft is a door, not a parallel system**: the `.theme.json` file is the shared contract; "Build in EffectCraft…" renders a title as a clip with alpha that AssetCraft places like any asset | Paul, 2026-11-07 ("For advanced building, users can use effectCraft") | One CLI call, no motion format to keep in sync |
| TH7  | Start with **8 shipped themes** (the 4 current looks + 4), grow toward the 20 below | Paul, 2026-11-07 | Smaller font list (F1) first |
| TH8  | Moves are timed in **beats** when the board has a song, seconds otherwise | Paul, 2026-11-07 | A title lands on the music |
| TH9  | Users' theme files live in `Documents/AssetCraft/Themes/` beside Templates | Paul, 2026-11-07 | `storyboard.themes` lists built-ins + these |

## 1. What already exists (reuse)

| Piece                          | Where                                                                  | Use                                                          |
|--------------------------------|------------------------------------------------------------------------|--------------------------------------------------------------|
| `TextTheme` + `Look`           | `crates/storyboard/src/text.rs:68-110`                                  | Becomes the 4 **built-in** themes; `Look` is derived from a theme file |
| `TextStyle {theme, animation, position}` on clips | `text.rs:346`                                          | `theme` widens from an enum to a theme id (v1 ids still load) |
| AI prompt insert, `theme` input | `docs/design/ai-prompt-insert.md` §3–4                                 | The agent's theme brief and the theme file meet here          |
| Bundled fonts                  | storytold/craft-fonts @ `aced413`: Inter, Montserrat, Poppins, Lato, Nunito Sans, Open Sans, Roboto, Source Sans 3 / Serif 4, Merriweather, Playfair Display, Liberation, CJK and other scripts | Body and most title type |
| EffectCraft `batch`, `add_effect`, `layer.newText`, fonts | storytold/effectcraft @ `3768ff4` `docs/agents.md` | Theme → motion title comp                                    |

**Opportunity F1 (owner: Claude):** craft-fonts has no display faces (condensed, slab, script, retro,
horror). Variety needs ~10 OFL display families bundled with AssetCraft (`assets/fonts/`, one
`ATTRIBUTION.md` row each), or offered upstream to craft-fonts (owner: Paul). Licences checked on each
family's own repo before bundling.

## 2. File format v1 (`themes/<id>.theme.json`)

```json
{
  "version": 1,
  "id": "noir-gold",
  "name": "Noir Gold",
  "tags": ["dark", "elegant", "1920s", "luxury"],
  "credit": { "author": "AssetCraft", "license": "own" },
  "color": {
    "background": "#0B0B0D", "surface": "#17171B", "text": "#F2EFE8",
    "muted": "#8A857A", "accent": "#C9A227", "accent2": "#7A1F1F"
  },
  "type": {
    "display": { "family": "Cinzel", "weight": 700, "tracking": 0.12, "uppercase": true },
    "body":    { "family": "Inter",  "weight": 400 },
    "scale":   { "card": 0.14, "overlay": 0.06, "caption": 0.035 }
  },
  "background": { "kind": "gradient", "stops": ["#0B0B0D", "#1C1408"], "angle": 90,
                  "grain": 0.08, "vignette": 0.35 },
  "plate": { "color": "#0B0B0D", "alpha": 180 },
  "space": { "margin": 0.06, "padding": 0.02 },
  "imagery": { "treatment": "contrast", "tint": "#C9A227", "tintAmount": 0.15 },
  "motion": { "titleIn": "fade-up", "ease": "easyEase", "beats": 2, "transition": "dip-to-black" },
  "assets": [{ "role": "texture", "path": "textures/paper-dark.png", "license": "CC0-1.0" }]
}
```

v1 as built (1a) reads `version`, `id`, `name`, `tags`, `credit`, `color`, `type`, `background`,
`plate`, `space` and `motion`; `imagery` and `assets` are refused until their steps (TH4).
Sizes are fractions of frame height (as `Look` today); colours `#RRGGBB`; `motion.titleIn` and
`motion.transition` use the existing `TextAnimation` and transition vocabularies, so nothing new is
invented for them.

**Load rules** (hostile input, like presets):

| Rule                                                                              | On failure            |
|-----------------------------------------------------------------------------------|-----------------------|
| ≤ 64 KiB, known fields only, `version` 1, `id` `[a-z0-9-]{1,40}`                   | refused               |
| Colours parse; `text` on `background` contrast ≥ 4.5:1 (WCAG AA)                   | refused for shipped themes, warning for user themes |
| Font family is bundled or installed; otherwise falls back to `body`, then Inter    | warning, never refused |
| Asset paths relative to the theme, inside it, licence in `CC0-1.0`, `CC-BY-4.0`, `OFL-1.1`, `own` | refused   |
| `motion` values in the known vocabularies                                          | refused               |
| Sizes 0.01–0.5, tracking −0.1–0.5, grain / vignette / tintAmount 0–1               | refused               |

## 3. Appliers (theme → each app's commands)

| App         | Theme becomes                                                                 | Through                              | Status        |
|-------------|-------------------------------------------------------------------------------|--------------------------------------|---------------|
| AssetCraft  | Title cards, overlays, strip colours, preview background                       | `Look::from_theme`; `storyboard.*`   | T1            |
| EffectCraft | Title comp: gradient solid, grain, vignette, display text with tracking, eased `titleIn` keys | one atomic `batch` (CLI `exec` or MCP) | T2 |
| FilmCraft   | Title clips, default transition, colour look on clips                          | `filmcraft-cli commands --json`      | T3, to map    |
| DesignCraft | Swatches; paragraph / character styles (display, body, caption)               | `designcraft-cli commands --json`    | T3, to map    |
| PhotoCraft  | Image treatment (tint, contrast) on stills and plates                          | `photocraft-cli commands --json`     | T3, to map    |

An applier is deterministic and re-runnable: same theme in, same commands out, previewable as a dry
run (the command list) before it runs. Agents write theme **files**, not command streams, so a theme
can be reviewed, versioned and re-applied.

## 4. The 20 shipped themes (proposal; Paul picks, TH2 variety)

| #  | Theme            | Mood / era                 | Palette (bg · text · accent)   | Display / body                    |
|----|------------------|----------------------------|--------------------------------|-----------------------------------|
| 1  | Bold             | punchy, default            | red · white · black            | Montserrat 900 / Inter            |
| 2  | Minimal          | quiet, modern              | near-black · off-white · grey  | Inter 300 / Inter                 |
| 3  | Cinematic        | film trailer               | black · warm white · amber     | Cinzel* / Source Serif 4          |
| 4  | Paper            | editorial, light           | off-white · ink · rust         | Playfair Display / Source Serif 4 |
| 5  | Noir Gold        | 1920s, luxury              | black · cream · gold           | Cinzel* / Inter                   |
| 6  | Neon Night       | synthwave, 1980s           | deep purple · white · magenta / cyan | Bungee* / Poppins           |
| 7  | Haunted Carnival | vintage horror             | oxblood · bone · mustard       | Creepster* / Lato                 |
| 8  | Field Notes      | documentary, nature        | moss · paper · ochre           | Merriweather / Nunito Sans        |
| 9  | Tech Grid        | product, SaaS              | navy · white · electric blue   | Space Grotesk* / Inter            |
| 10 | Retro Arcade     | 8-bit, playful             | black · white · lime / pink    | Press Start 2P* / Roboto          |
| 11 | Sunlit           | lifestyle, travel          | sand · charcoal · coral        | Poppins 600 / Nunito Sans         |
| 12 | Newsroom         | news, explainer            | white · black · signal red     | Oswald* / Source Sans 3           |
| 13 | Blueprint        | how-to, tutorial           | blueprint blue · white · yellow | IBM Plex Mono* / Inter           |
| 14 | Pastel Pop       | social, kids, upbeat       | lilac · plum · mint            | Righteous* / Nunito Sans          |
| 15 | Brutalist        | art, fashion               | concrete · black · safety orange | Archivo Black* / Inter          |
| 16 | Typewriter       | memoir, true story         | parchment · ink · faded red    | Special Elite* / Merriweather     |
| 17 | Epic Fantasy     | games, myth                | midnight · parchment · ember   | Cinzel Decorative* / Lato         |
| 18 | Chalkboard       | education, classroom       | slate green · chalk · yellow   | Caveat* / Nunito Sans             |
| 19 | Luxe Mono        | premium product            | white · black · none           | DM Serif Display* / Inter         |
| 20 | Monsoon          | moody, music video         | teal-black · mist · rain blue  | Bebas Neue* / Open Sans           |

\* not in craft-fonts: F1. 1–4 keep today's `TextTheme` ids, so existing boards keep their look.

## 4b. Elements and the title power-ups (2026-11-07)

| Element    | Lives                         | Look (theme)                         | Moves (named, beat-timed)                                 | Place                      |
|------------|-------------------------------|--------------------------------------|-----------------------------------------------------------|----------------------------|
| Title      | a Ttl clip or an overlay      | display / body fonts, colours, plate, background, spacing, sound | in + out: Fade · Rise · Zoom · Typewriter (today) + Slide · Pop · Wipe · Stagger | 5 positions + full card    |
| Transition | between two clips             | default kind + length, sound         | Fade · Dip (theme colour) · Slide · Push · Zoom · Whip-blur | between clips              |
| Panel      | a lane over a time range      | surface, accent bar, body font       | in / out + loop (crawl)                                   | top / bottom band          |
| Callout    | a point on a clip's frame     | accent, line weight, caption font    | draw-on in / out                                          | anchor (x, y) + label side |

Title power-ups, in order: (1) themes from files; (2) more moves, in and out, beat-snapped; (3) title +
subtitle, the accent bar on lower thirds; (4) background: theme · image · the next clip blurred;
(5) the theme's sound on the title's in. Panels start with one (ticker / headline bar), callouts
with one (pin + line + label). Anything fancier: EffectCraft (TH6).

## 5. Steps (one at a time; each ends with `cargo xtask ci` and a commit block)

| Step | What                                                                                           | Owner                     | Status  |
|------|------------------------------------------------------------------------------------------------|---------------------------|---------|
| T0′  | This spec, revised for TH5–TH9 and §4b                                                          | Claude                    | done    |
| 1a   | `crates/storyboard/src/theme.rs`: `Theme` model + hostile-input loader (rules above, `space` added); the 4 looks as built-in themes equal to today's `Look`s; `Theme::look()`; `storyboard.themes {}` lists built-ins + `Themes/*.theme.json` with load problems | Claude | building |
| 1c   | Timed text (Paul, 2026-11-08: "theme, transition in, duration, effect, transition out, position"): `TextStyle` gains `durationMs` (up from the clip's start, in + out included; none = the whole clip), `effect` (None · Slow push · Drift up · Pulse, while up) and `exit` (Cut · Fade out · Sink · Zoom out · Erase); `TextStyle::frame_at` runs in → effect → out → hidden; `storyboard.text.style {duration, effect, exit}`; the Inspector's TEXT rows in that order with a preview of the whole run | Claude | built 2026-11-08 |
| 1b-i | 4 more shipped themes (TH7) from §4 as built-ins: Noir Gold (#5), Neon Night (#6), Field Notes (#8), Blueprint (#13); ids `noir-gold` · `neon-night` · `field-notes` · `blueprint`; drawn in Inter until F1; the Inspector's picker shows a swatch per theme | Claude | built 2026-11-10 |
| 1b-ii| Title cards and overlays take any theme id, a user's `Themes/*.theme.json` included (the clip's `theme` becomes an id; unknown id → Bold, with a note) | Claude | — |
| 1d   | The base drawn (Paul, 2026-11-10: "more than just a solid color and font size… lets get the base working"): a card's `background` gradient (2–4 stops, angle; 90° = top to bottom), grain and vignette, and `decor.accentBar` (under a card's title, down an overlay plate's edge); one colour function (`Backdrop::color_at`) for the export (per pixel, steady grain) and the Viewer (vertex-colour mesh; grain export-only); all 8 built-ins given depth; a stop under AA contrast is a warning | Claude | built 2026-11-10 |
| 1e   | Decor (`crates/storyboard/src/decor.rs`): `decor.rules` (none · below · both · double), `corners`, `frame`, `pattern` (none · grid · dots · scanlines) and `background.kind: radial`; one list of rectangles per card (`card_decor`, steady) and per text (`text_decor`, fades with the words; rules replace a card's accent bar, an overlay keeps its bar); both renderers draw the same list; patterns bounded at 4096 shapes. Built-ins: Paper frame · Noir Gold radial + rules both + corners · Neon Night scanlines + frame · Field Notes dots + rule below · Blueprint grid + corners + frame. Paper fibre deferred (grain stands in) | Claude | built 2026-11-10 |
| 1f   | Text finish: `type.display.finish` = none · shadow · outline · glow; drawn as extra passes of the same words under them (`finish_passes`, `decor.rs`) by the export and the Viewer alike — shadow down-right in black, outline a 12-step ring in the tone opposite the text, glow three faint rings in the accent. Built-ins: Bold, Minimal, Field Notes shadow · Neon Night glow. Two-tone title + subtitle moves to step 4 (it needs the subtitle line) | Claude | built 2026-11-13 |
| 1g   | Theme motion: a theme file's `motion` = `titleIn` · `titleEffect` · `titleOut` · `titleSeconds` (none = whole clip), checked like the rest; a clip's In · Duration · Effect · Out are its own choices and an empty one follows the theme (`TextStyle::timing()` resolves; `storyboard.text.style` takes `"theme"` to go back and replies with the moves in effect + `fromTheme`); Inspector menus open with "Theme (…)" and show "Zoom · theme"; Duration = Theme · Whole clip · Set time. Built-ins: Bold, Minimal Fade/–/Fade · Cinematic, Noir Gold Fade/Push/Fade · Paper Rise/–/Sink · Neon Night Zoom/Pulse/Zoom · Field Notes Rise/Drift/Fade · Blueprint Typewriter/–/Erase. Template titles and the credits card follow the theme; a preset keeps its own in | Claude | built 2026-11-13 |
| 2    | Moves: Slide · Pop · Wipe · Stagger for in and out (the out lane exists since 1c), beat-snapped durations (TH8), the theme's `motion` gains `titleOut`, `titleEffect`; the Start screen's TITLE CARDS panel | Claude | — |
| 3    | Transitions drawn in the Viewer and the export (SF5-3b) + Dip · Slide · Push · Zoom · Whip-blur | Claude | — |
| 4    | Title + subtitle; background choice                                                             | Claude | — |
| 5    | Title sound (theme `sound`; CC0 picks, F2)                                                      | Claude, Paul picks sounds | — |
| 6    | "Build in EffectCraft…" (TH6): text + length + theme → `effectcraft-cli` → a clip with alpha     | Claude | — |
| 7    | Ticker panel; pin callout                                                                       | Claude | — |
| T3   | Map FilmCraft, DesignCraft, PhotoCraft commands to theme fields                                 | Claude | — |
| T4   | Pick / rename the themes (§4); approve F1 font list                                             | Paul   | open |
| T5   | `theme.fromBrief` in the AI prompt insert: brief → validated `.theme.json`                      | Claude | — |

## 6. Decisions for Paul

| ID   | Question                                                                 | Recommendation                                                     |
|------|--------------------------------------------------------------------------|--------------------------------------------------------------------|
| TH3  | Bundle display fonts in AssetCraft (F1) or offer them to craft-fonts first? | AssetCraft first (ships sooner); offer upstream after              |
| TH4  | Theme textures and backgrounds: generated (gradients, grain) only in v1, or ship CC0 images too? | Generated only in v1: no licences to track, tiny files |
