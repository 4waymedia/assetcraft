# Storyboard presets: music-timed starting structures

_2026-10-15 · Stage 4 (Storyboard mode) · spec, decisions recorded, catalog proposed_

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

## Licensing: the songs stay out of the repo (recommendation R1)

Third-party summaries of the Motion Array licence mark **redistribution** as not allowed, and say the
licence generally doesn't cover *new* projects after a subscription is cancelled. I'm not a lawyer;
check Motion Array's own terms before anything ships. Until then:

| What                                  | Where it lives                                                                                | In git? |
|---------------------------------------|-----------------------------------------------------------------------------------------------|---------|
| The audio files                       | `F:\Script-Projects\ArtCrafting\_dev-data\assetcraft\preset-tracks\` (Paul's machine only)     | No      |
| Track facts (title, artist, source link, length, BPM) | the preset file                                                               | Yes     |
| Cue map (bars, sections, hits)        | the preset file: timing data, not audio                                                       | Yes     |
| Clip structure and briefs             | the preset file                                                                               | Yes     |

When a preset is applied, AssetCraft asks for the track: it looks for `track.fileHint` in the preset-tracks
folder (set in Preferences later), else the user picks their own copy. A preset with no track file still
works: it times to its cue map silently. This keeps AssetCraft offerable to the ArtCraft team without
shipping anyone's licensed music. Owner: Paul (confirm the licence); Claude (build the relink flow).

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
    "source": "Motion Array",
    "sourceUrl": "https://motionarray.com/…",
    "fileHint": "trailer-60.wav",
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
