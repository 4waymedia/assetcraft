# Folders panel: System and My Folders

_2026-10-27 · Stage 1 follow-up (Organizer) · F1 built, F2–F4 planned_

- **Scope:** the Organizer's Folders panel, and the suite-wide list of folders a user pins.
- **Project goal it serves:** a navigator that feels like the user's own Finder / Explorer, and
  one list of working folders shared by every Crafting app.
- **Immediate area of improvement:** today the panel is Computer · Places · Favorites; it misses
  the OS sidebar, recent folders, network shares and the AssetCraft folder, and favorites belong
  to AssetCraft alone.

## Decisions (Paul, 2026-10-27)

| ID    | Decision                                                                                                   |
|-------|------------------------------------------------------------------------------------------------------------|
| FP-1  | Two sections: **System** (computer, the OS's favorites, extras) and **My Folders** (folders the user pins)  |
| FP-2  | System's favorites **mirror the OS sidebar**: Finder Favorites (macOS), Quick Access pins (Windows), GTK bookmarks (Linux) |
| FP-3  | System extras: **Recent** folders, the **AssetCraft** working folder, **Network** shares                   |
| FP-4  | My Folders is **shared by all Crafting apps**; each pin is set to **All Crafting apps** or **This app only** |

Defaults chosen by Claude (cheap to change): a new pin is *All Crafting apps*; AssetCraft's old
Favorites move into My Folders as *All Crafting apps*; Recent keeps the last 8 folders; when the
OS sidebar can't be read, System shows the standard list (Home, Desktop, Documents, Downloads,
Pictures, Music, Movies / Videos).

## Layout

```
SYSTEM
  Computer
    /            Macintosh HD            (drives; network ones go under Network)
  Favorites                              (the OS sidebar, else the standard list)
    Desktop  Documents  Downloads  …
  AssetCraft                             (the working folder, when it exists)
  Recent                                 (last 8 folders opened in AssetCraft)
  Network                                (mounted shares: smbfs / nfs / afp / cifs …, mapped drives)
MY FOLDERS                               (pins; ⇄ = shared with all Crafting apps)
  Reapers concepts ⇄
  Plates (this app only)
  + Add current folder
```

Right-click a pin: Open · **Share with all Crafting apps** / **AssetCraft only** · Remove · Reveal.
Right-click any other folder: Open · Pin to My Folders · Reveal.

## The shared store: `places.json`

One file for the whole suite, beside the shared cache's naming:

| OS      | Path                                                     |
|---------|----------------------------------------------------------|
| macOS   | `~/Library/Application Support/Crafting/places.json`     |
| Windows | `%APPDATA%\Crafting\places.json`                         |
| Linux   | `$XDG_CONFIG_HOME/crafting/places.json` (`~/.config/…`)  |

```json
{
  "version": 1,
  "pins": [
    { "path": "/Users/paul/Art/Reapers", "apps": "all" },
    { "path": "/Users/paul/Plates", "apps": ["assetcraft"] }
  ]
}
```

- `apps` is `"all"` or the ids of the apps that show the pin (`assetcraft`, `designcraft`,
  `filmcraft`, …). An app shows pins for `"all"` or its own id, and **keeps every other pin when
  it writes** (re-read, change, write atomically), so one app never drops another's pins.
- Hostile input: at most 1 MiB and 500 pins; a pin needs an absolute UTF-8 path; anything else is
  skipped, never fatal. An unreadable file is left alone (renamed `places.json.bad` before the
  next write).
- Optional fields reserved for later: `name` (a label), `added` (date).

## Steps

| Step | Delivers                                                                                       | Owner         | Status |
|------|------------------------------------------------------------------------------------------------|---------------|--------|
| F1   | System / My Folders layout; Recent (prefs, 8); AssetCraft folder; Network (`mount` on macOS, `/proc/mounts` on Linux, `net use` on Windows); Music in the standard list; Linux GTK bookmarks; `places.json` with per-pin scope; `nav.favorite.scope`; old Favorites carried over | Claude | built 2026-10-27 |
| F2   | macOS: read Finder Favorites (`~/Library/Application Support/com.apple.sharedfilelist/com.apple.LSSharedFileList.FavoriteItems.sfl3`, an NSKeyedArchiver plist of bookmarks) as hostile input; fall back to the standard list | Claude (Paul: test on the Mac) | planned |
| F3   | Windows: read Quick Access pins (shell namespace `{679f85cb-0220-4080-b29b-5540cc05aab6}`, without `unsafe` in our crates); fall back to the standard list | Claude (Paul: test on Windows) | planned |
| F4   | craft-libs proposal: a tiny `crafting-places` reader/writer so DesignCraft, FilmCraft and the other forks show the same My Folders | Claude drafts, Paul files | planned |
