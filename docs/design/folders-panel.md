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
| FP-5  | (2026-10-27, after trying F1: "still very clunky") **Three tabs** in the Folders panel: **Computer** (each hard drive, network drives), **Favorites** (the OS's favorite folders; Recent below), **Craft** (the AssetCraft folder and the folders the user pins). Supersedes FP-1's single System / My Folders list |

Defaults chosen by Claude (cheap to change): a new pin is *All Crafting apps*; AssetCraft's old
Favorites move into My Folders as *All Crafting apps*; Recent keeps the last 8 folders; when the
OS sidebar can't be read, System shows the standard list (Home, Desktop, Documents, Downloads,
Pictures, Music, Movies / Videos).

## Layout (FP-5)

```
 Computer   Favorites   Craft          ← tab strip (underline on the open tab)
 ─────────────────────────────
 Drives                                 Computer tab
   🖥 Computer
   ▭ Macintosh HD                       (/ and /Volumes/Macintosh HD are one entry)
   ▭ USB Stick
 Network
   🌐 Media                             (smbfs / nfs / afp / cifs …, mapped drives)

 System                                 Favorites tab
   📁 Desktop  Documents  Downloads …   (the OS sidebar, else the standard list)
 Recent
   📁 last 8 folders opened

   📁 AssetCraft                        Craft tab (the working folder)
 My Folders                         +   (+ pins the current folder)
   📁 Reapers concepts            ⚭     (⚭ = shared with all Crafting apps)
   📁 Plates                            (AssetCraft only)
```

Rows are Finder-style: full-width highlight, a glyph, names that end in "…" when long, a
disclosure chevron shown on hover or when open (sub-folders load only when opened). Group
headings (Drives, Network, System, Recent, My Folders) collapse with a click.

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
| F2   | macOS: the app reads the user's own Finder Favorites (`~/Library/Application Support/com.apple.sharedfilelist/com.apple.LSSharedFileList.FavoriteItems.sfl3`, else `.sfl2`; an NSKeyedArchiver plist of bookmarks, `crates/fs/src/finder.rs`) as hostile input at startup and on Refresh; falls back to the standard list; `assetcraft-cli sidebar` prints what it read | Claude (Paul: check against Finder) | built 2026-10-27, to verify on the Mac |
| F3   | Windows: read Quick Access pins (shell namespace `{679f85cb-0220-4080-b29b-5540cc05aab6}`, without `unsafe` in our crates); fall back to the standard list | Claude (Paul: test on Windows) | planned |
| F4   | craft-libs proposal: a tiny `crafting-places` reader/writer so DesignCraft, FilmCraft and the other forks show the same My Folders | Claude drafts, Paul files | planned |
