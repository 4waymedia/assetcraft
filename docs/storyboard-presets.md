# Storyboard presets: music-timed starting structures

_2026-10-15 · Stage 4 (Storyboard mode) · spec, decisions recorded, 20-track grid proposed from Audio Library_

- **Scope:** the Format › Preset menu of Storyboard mode, which becomes **Style › Preset**: 20 starting
  structures, each timed by hand to a real song's bars, sections and hits.
- **Project goal it serves:** turning a preset plus a folder of art into a Cut that already lands on the
  music, so Quick fill and Watch your Cut produce something worth showing.
- **Immediate area of improvement:** today a preset is just evenly weighted beats at a fixed tempo. Music
  timing needs a cue map per song, a preset file format, and audio playback for review.

## Decisions (Paul, 2026-10-15)

| ID  | Decision                                                                                       | Consequence                                                                            |
|-----|------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------|
| P1  | 20 starting presets: 5 each at 30 s, 60 s, 90 s and 2 min                                       | Catalog below                                                                          |
| P2  | **20 different songs**, one per preset, downloaded by Paul from Motion Array                    | 20 cue maps to draft and check                                                         |
| P3  | The toolbar menu groups them **Style › Preset**                                                 | Replaces the design's Format › Preset (the vocabulary row stays "Format › Preset" until P3 is reflected in the handoff copy) |

## Licensing: two kinds of track (recommendation R1, amended 2026-10-15)

| Kind                         | Example                                         | Redistribute?                         | Where the audio lives                                  |
|------------------------------|-------------------------------------------------|---------------------------------------|--------------------------------------------------------|
| **CC BY 4.0** (free)         | *The Long Dark*, Scott Buckley                  | Yes, with credit; changes must be noted | Paul's machine for now; may ship with AssetCraft later (decision R2) |
| **Subscription** (Motion Array) | Paul's downloads                             | No (third-party summaries; Paul to confirm, O6) | Paul's machine only, never in git                |

Every preset records its track's licence and credit, so AssetCraft can show the credit and, when the
Cut is exported (Stage 6), hand the user the text to paste into a video description. CC BY 4.0 also
asks that changes be indicated: a preset that uses part of a song (a 30 s cut of a 3-minute track) says
so in `credit.changes`.

| What                                  | Where it lives                                                                                | In git? |
|---------------------------------------|-----------------------------------------------------------------------------------------------|---------|
| The audio files                       | `F:\Script-Projects\ArtCrafting\_dev-data\assetcraft\preset-tracks\` (Paul's machine)          | No (R2) |
| Track facts, licence and credit       | the preset file                                                                               | Yes     |
| Cue map (bars, sections, hits)        | the preset file: timing data, not audio                                                       | Yes     |
| Clip structure and briefs             | the preset file                                                                               | Yes     |

**R2 (Paul to decide by PR4): bundle the CC BY tracks?** Recommendation: not in git. A WAV is tens of
megabytes, and every version would stay in the history forever. Ship them in the release package
instead (Stage 7), with an `ATTRIBUTION.md` row each, and keep them local until then.

When a preset is applied, AssetCraft looks for `track.fileHint` in the preset-tracks folder (set in
Preferences later), else the user picks their own copy. A preset with no track file still works: it
times to its cue map silently. Owner: Paul (confirm licences, R2); Claude (build the relink flow).

## Credits in the Cut (decision C1, Paul, 2026-10-15)

Music and assets made by someone else get a credit; anything made in the Crafting apps doesn't. The
goal is that, once everything is produced in-house, the credit list is simply empty.

| Rule  | What                                                                                                  |
|-------|-------------------------------------------------------------------------------------------------------|
| C1.1  | Every track and every asset carries a `license`: `own` (made by Paul or in the Crafting apps), `CC-BY-4.0`, `CC-BY-3.0`, `FreeToUse`, `Motion-Array`, … |
| C1.2  | A Cut's credit list = the track plus every clip asset whose licence isn't `own`, in Cut order, each with its verbatim credit text and a `Changes:` line when the preset uses part of a track |
| C1.3  | On export (Stage 6): `<cut name>-credits.txt` beside the video (the text to paste into a video description) and, optionally, an on-screen credit frame after the end card |
| C1.4  | Empty credit list → no credits file, no credit frame                                                  |
| C1.5  | Licences AssetCraft won't put in a preset: `CC-BY-ND-*` (no syncing to video) and `*-NC-*` (non-commercial); these tracks stay in the folder but the preset loader refuses them |

Defaults until Stage 2 (XMP) can tag files: Organizer assets count as `own`; a track's licence comes from
its preset file. Opportunity O7 (owner: Claude, Stage 2): an XMP rights field (`xmpRights:UsageTerms`,
`dc:rights`) so a third-party image or clip carries its licence and credit like a track does.

The working credit list for Paul's downloads is `_dev-data\assetcraft\preset-tracks\copyright-claims.md`
(one section per file, credits verbatim, licence and preset fit per track). As of 2026-10-15: 26 tracks,
21 usable (CC BY), 2 to check (Free To Use), 2 refused (CC BY-ND, CC BY-NC-SA). It stays outside the
repo with the audio; each preset copies its own track's credit.

## Preset file (draft schema, `presets/<style>/<id>.preset.json`)

Times are in **bars and beats** from the first downbeat, never seconds, so every cut lands on the grid
and a preset survives a re-export of the song with a different lead-in.

```json
{
  "version": 1,
  "id": "trailer-60",
  "style": "Trailer",
  "name": "Dark build · 1:00",
  "lengthSec": 60.0,
  "track": {
    "title": "<song title>",
    "artist": "<artist>",
    "source": "Scott Buckley",
    "sourceUrl": "https://www.scottbuckley.com.au/2023/02/new-library-track-the-long-dark/",
    "license": "CC-BY-4.0",
    "credit": {
      "text": "Music: The Long Dark by Scott Buckley https://www.scottbuckley.com.au\nLicense: Creative Commons — Attribution 4.0 International — CC BY 4.0",
      "changes": "Cut to 1:00 (bars 1–24 of the original)"
    },
    "fileHint": "the-long-dark.wav",
    "durationSec": 60.0,
    "bpm": 92.0,
    "beatsPerBar": 4,
    "firstDownbeatSec": 0.42,
    "sections": [
      { "name": "Intro",  "startBar": 1,  "bars": 4 },
      { "name": "Build",  "startBar": 5,  "bars": 8 },
      { "name": "Drop",   "startBar": 13, "bars": 8 },
      { "name": "Outro",  "startBar": 21, "bars": 2 }
    ],
    "hits": [
      { "bar": 13, "beat": 1, "kind": "drop" },
      { "bar": 20, "beat": 3, "kind": "riser-end" }
    ]
  },
  "scenes": [
    {
      "title": "Cold open",
      "section": "Intro",
      "clips": [
        { "type": "TTL", "beats": 4, "brief": "Studio logo over black", "sync": true },
        { "type": "EST", "beats": 6, "brief": "Where are we?", "angles": ["High wide", "Drone push-in"] },
        { "type": "INS", "beats": 2, "brief": "The detail that matters", "transitionOut": "Flash" },
        { "type": "CU",  "beats": 4, "brief": "First look at the hero" }
      ]
    }
  ]
}
```

**Load rules** (hostile input, like `Storyboard::from_json`):

| Rule                                                                     | On failure                   |
|--------------------------------------------------------------------------|------------------------------|
| Size, counts and string lengths within `Limits`; known fields only        | refused                      |
| `license` is a known id (`CC-BY-4.0`, `CC-BY-3.0`, `FreeToUse`, `Motion-Array`, `own`) and a CC BY track has `credit.text` | refused |
| `license` is `CC-BY-ND-*` or `*-NC-*` (C1.5)                              | refused                      |
| `bpm` 20–400, `beatsPerBar` 2–12, `firstDownbeatSec` ≥ 0, `durationSec` ≤ 6 h | refused                  |
| Sections ascending, non-overlapping, inside the track                     | refused                      |
| Each scene's clip beats add up to its section's bars × `beatsPerBar`       | refused (the point of a preset) |
| Every hit lands on a clip start                                           | warning in the preset report |
| Tempo is constant through the song                                        | v1 limit (opportunity O4)    |

**Applying** turns beats into `Ticks` at the track's tempo plus `firstDownbeatSec`, fills `Track.beat_grid`
and `song_sections` from the cue map, sets `sync_to_downbeat` where marked, and records one undo step (the
engine part, `storyboard.applyPreset`, is built and tested; it reads a catalog in code today).

## Catalog: 5 styles × 4 lengths (proposal, Paul to confirm the styles)

| Style            | 30 s           | 60 s           | 90 s           | 2 min          |
|------------------|----------------|----------------|----------------|----------------|
| Trailer          | song 1         | song 2         | song 3         | song 4         |
| Teaser / promo   | song 5         | song 6         | song 7         | song 8         |
| Social reel      | song 9         | song 10        | song 11        | song 12        |
| Character intro  | song 13        | song 14        | song 15        | song 16        |
| Music montage    | song 17        | song 18        | song 19        | song 20        |

Rough clip counts to aim for (cut density rises with tempo and energy): 30 s ≈ 10–16 clips, 60 s ≈ 18–26,
90 s ≈ 26–36, 2 min ≈ 32–48. Each preset opens with a title card on a downbeat and closes on the song's
final hit or ring-out.

## Track candidates (from YouTube Audio Library, 2026-10-15)

Paul's shortlist pages: [dramatic](https://www.audiolibrary.com.co/moods/dramatic),
[happy](https://www.audiolibrary.com.co/moods/happy),
[inspirational](https://www.audiolibrary.com.co/moods/inspirational) (page 1 of each). Audio Library is
an aggregator: it shows no licence name, and for these tracks it says to download from the artist's
own site. **The artist's site is the source of truth** for each licence.

**Licence status key:** ✅ verified on the artist's site · ◐ secondary source only · ? not checked yet.
Only CC BY tracks (✅, or ◐ once confirmed) may become presets that ship; a track with custom
"free for YouTube with credit" terms can only be a local preset, like Motion Array.

| Artist                 | Licence                                                    | Status | Source                                                                                     |
|------------------------|------------------------------------------------------------|--------|--------------------------------------------------------------------------------------------|
| Scott Buckley          | CC BY 4.0 ("All music is licensed under Creative Commons Attribution 4.0") | ✅ | [scottbuckley.com.au library](https://www.scottbuckley.com.au/library/instrumentation/synth-atmospherics/page/4) |
| Kevin MacLeod          | CC BY 4.0, credit "Kevin MacLeod (incompetech.com)"; no standalone redistribution of the raw files | ◐ | [licenseorg.com summary](https://licenseorg.com/guide/music-audio/incompetech); confirm on incompetech.com |
| Sakura Girl, Scandinavianz, Declan DP, Roa Music, Next Route & MEZCLA, ZiMPL, ArtMyLife, Damtaro, KV, Aylex, Hotham, Jay Someday, Johny Grimes | not stated on Audio Library | ? | each artist's own site (owner: Claude to look up once Paul has picked by ear) |

**Proposed grid (Paul to replace by listening; moods are Audio Library's tags, tempo and length unknown):**

| Style            | Mood source   | 30 s                         | 60 s                          | 90 s                          | 2 min                        |
|------------------|---------------|------------------------------|-------------------------------|-------------------------------|------------------------------|
| Trailer          | dramatic      | Escape Velocity (Buckley ✅) | The Long Dark (Buckley ✅)     | Balefire (Buckley ✅)          | Vanguard (Buckley ✅)         |
| Character intro  | dramatic      | Sneaky (MacLeod ◐)           | A Dragon's Lullaby (Buckley ✅) | Reawakening (Buckley ✅)      | Cirrus (Buckley ✅)           |
| Social reel      | happy         | Teddy Bear (Sakura Girl ?)   | Pacific (Scandinavianz ?)      | Coconut Cafe (Declan DP ?)    | Beachside (Roa Music ?)      |
| Teaser / promo   | happy         | Jaunty Gumption (MacLeod ◐)  | Vacation (Sakura Girl ?)       | Vice Versa (Scandinavianz ?)  | Horizon (Next Route & MEZCLA ?) |
| Music montage    | inspirational | In The Zone (ZiMPL ?)        | Elevation (Damtaro ?)          | Triumph Of Victory (ArtMyLife ?) | Geography (Aylex ?)       |

Spares: Three Sheets To The Wind (Buckley ✅), Sunflower (Sakura Girl ?), Dopamine (KV ?), Take A Walk
(Hotham ?), Weightless (Jay Someday ?), I Cant Make You Love Me (Johny Grimes ?), Sunfade (Roa Music ?).

Notes:
- 8 of the 9 dramatic tracks are Scott Buckley, so Trailer and Character intro lean on one composer.
  Fine for a start; the dramatic pages 2–24 have other artists if variety matters.
- A 2-min preset needs a track at least 2:00 long; a 30 s preset can use part of a longer track
  (CC BY allows edits; the preset notes it in `credit.changes`).
- Ambient and orchestral tracks (most of the dramatic row) cut on sections and swells; the dance and
  electronic tracks (happy, inspirational) cut on bars and drops, and suit denser clip counts.

## Workflow

| Step | Who    | What                                                                                                     |
|------|--------|----------------------------------------------------------------------------------------------------------|
| 1    | Paul   | Confirm or change the 5 styles; pick 20 songs with a **steady tempo** and clear sections; download WAVs into `_dev-data\assetcraft\preset-tracks\`, named `<style>-<length>.wav` |
| 2    | Paul   | Connect that folder to the Claude session                                                                 |
| 3    | Claude | Draft each cue map: tempo, first downbeat, bar grid, section boundaries and hits, by beat and onset analysis on a working copy (nothing stored or committed) |
| 4    | Claude | Draft each preset's scenes and clips against its cue map; `assetcraft-cli` validates every file           |
| 5    | Paul   | Review by ear in AssetCraft (needs track playback, below) and adjust bars, hits and briefs               |

## Code steps (each one small, each its own patch)

| Step | Delivers                                                                                                   | Status |
|------|------------------------------------------------------------------------------------------------------------|--------|
| PR1  | Preset mechanics: `storyboard.presets` (READ), `storyboard.new {format, preset}`, `storyboard.applyPreset` (EDIT, one undo step), apply = replace structure, keep name/track/Story, time to track | built and tested in the sandbox, not handed over (its in-code catalog is the design's placeholder list) |
| PR2  | Preset file format + loader + validation (schema above) in `crates/storyboard`; catalog read from `presets/`; Style › Preset menu in the toolbar | next |
| PR3  | Track audio (plan step 8, pulled forward): decode + playback in Watch your Cut on an audio clock, track relink by `fileHint` | after PR2 |
| PR4  | The 20 preset files, drafted from Paul's songs                                                             | after the songs arrive |

## Opportunities

| ID | Scope            | Opportunity                                                                | Fix                                                                | Owner  |
|----|------------------|----------------------------------------------------------------------------|--------------------------------------------------------------------|--------|
| O4 | Cue maps         | Songs with tempo changes or rubato don't fit a single `bpm`                 | Tempo map (list of `{bar, bpm}`) in format version 2               | Claude |
| O5 | Beat detection   | Cue maps are drafted offline by Claude and checked by ear                   | In-app beat grid detection (plan O3) once audio decoding exists     | Claude |
| O6 | Licence          | Exact Motion Array terms for timing data derived from a track               | Paul confirms with Motion Array's terms or support                  | Paul   |
