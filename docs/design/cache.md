# Suite cache and app storage: requirements for Windows, macOS, Linux and the web

_2026-10-16 · Stage 4 (Storyboard mode, analysis A2c) · spec v2.1: C1 built; D-C1, D-C2, D-C4 decided (supersedes
[archive/cache-proposal-v1.md](../archive/cache-proposal-v1.md))_

- **Scope:** where every Crafting app keeps the data it owns (settings, recovery, caches, and
  results shared between apps), and how cache entries are named, written, budgeted, cleared and
  driven by agents, on Windows, macOS, Linux and in the browser (WASM).
- **Project goal it serves:** Bridge-style speed (Paul, 2026-10-16: "adobe bridge would create many
  cache files for edits, and thumbnails. We should implement the same") under the suite's six
  rules, with one answer for every app rather than one per app ("We need to follow the same
  principles and structures. The app needs to run on the web as well").
- **Immediate area of improvement:** AssetCraft's track analysis (A2c) and thumbnails move onto it
  first; the crate is written so that any app can depend on it and it can go upstream into
  craft-libs.

## 1. The six rules, as cache requirements

| Rule (crafting apps)          | What it requires of a cache                                                                                                                   |
|-------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------|
| 01 Open source                | The layout and entry format are documented here and versioned, so any app (or a user) can read, clear or rebuild them                         |
| 02 Native, not wrapped        | Pure Rust on every target, the web included: `web-sys` for OPFS and IndexedDB, **no JavaScript glue** (most repos' AGENTS.md: "Rust only (no handwritten JS/TS). Never break wasm", e.g. designcraft `AGENTS.md:29` @c2609f9) |
| 03 Familiar from day one      | The names professionals know: **Media Cache** (Premiere, Bridge), **Disk Cache** (After Effects), **Purge**; a Settings ▸ Cache page with location, size, budget and Clear |
| 04 Your files, your machine   | A cache never writes into the user's folders by default, never leaves the machine, holds nothing that can't be rebuilt ("The cache is nobody's pictures", lightcraft `crates/preview/src/disk.rs:8` @f34c7a8) |
| 05 Agent-ready                | Every cache action is a command with JSON params (`cache.info`, `cache.clear`, …) on the CLI, MCP and control channel                        |
| 06 Also in your browser       | The same keys, layout and entry format in the browser's Origin Private File System, with IndexedDB and memory as fallbacks                    |

## 2. What the suite does today

Survey of all 15 storytold repos, 2026-10-16 (commits: artcraft d4e4ac6, cadcraft 9b2308b,
craft-libs 83bf4cb, deckcraft 1dc6972, designcraft c2609f9, effectcraft 8f02f52, figcraft d1d8c3f,
filmcraft 8309f21, gridcraft f1894c6, lightcraft f34c7a8, pdfcraft 9e5b70d, photocraft 2a952f1,
soundcraft 3a28a65, vectorcraft 8bc6532, wordcraft eae2421).

### 2.1 Desktop

| App                | Cache                                                                                                                          | Cite                                                                                       |
|--------------------|--------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------|
| craft-libs         | Policy: caches and models in the platform cache folder (`paths::cache_dir(app_id)`, `%LOCALAPPDATA%` so it doesn't roam), recovery in the data folder; config override `<APP>_CONFIG_DIR` (empty = isolated), portable marker. **No app depends on `craft-settings` yet** | `crates/settings/src/paths.rs:116-136, 162-170`, `docs/settings-storage.md:13-14`          |
| EffectCraft        | Disk Cache `<folder>/v1/{frames,layers,mattes}/<2 hex>/<32 hex>.ecc`; FNV-1a-128 content keys; magic `ECDC` + kind + LZ4 + FNV-64 checksum; temp `<key>.tmp.<pid>.<n>` + rename; 64-deep write queue; LRU by mtime, index rebuilt on open; 100 GB default; folder per OS, user-settable | `crates/render/src/disk_cache.rs:1-21, 274-290`; `crates/engine/src/prefs.rs:250-252`      |
| LightCraft         | Inside the library: `thumbs/<2 hex>/<32 hex>.lcthumb` + `CACHEDIR.TAG`, SipHash-1-3-128 keys, 2 GiB LRU to 80 %; `Smart Previews/` with `LCSP1` magic and a JSON header | `crates/preview/src/disk.rs:1-31`, `crates/engine/src/smart.rs:16-57`                      |
| FilmCraft          | Media Cache at `mediaCache.location` or `<data dir>/Media Cache` (roaming `%APPDATA%` on Windows); never / older than / exceeds size; `mediaCache.info`, `mediaCache.clean`; analysis "In the Media Cache" or "In the project" | `crates/engine/src/settings.rs:656-681, 949, 1603-1609`                                    |
| PdfCraft           | Clipboard images in the platform cache folder                                                                                  | `crates/ui-egui/src/image_pick.rs:70-86`                                                   |
| AssetCraft         | `<cache>/thumbs/<2 hex>/<32 hex>.png`, SipHash-1-3-128 key (renderer version, size bucket, file size, mtime, first and last 64 KiB), 1 GiB LRU to 80 %, own-files-only | `crates/thumbs/src/disk.rs:1-30`, `crates/ui-egui/src/prefs_file.rs:40-70`                 |
| ArtCraft Native    | Media in memory only (textures evicted past a budget); **`~/Artcraft` shared with the Tauri app** for credentials, settings and job state ("connecting in either app works in both") | `artcraft-native/src/backend/media_cache.rs:1-30`, `session.rs:35, 94`, `README.md` (Other providers) |
| Others             | PhotoCraft, DesignCraft, SoundCraft, VectorCraft, WordCraft, FigCraft, GridCraft, CADCraft, DeckCraft: no disk cache            | survey                                                                                      |

### 2.2 Web

| App          | Storage                                                                                                                 | Cite                                                                            |
|--------------|-------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------|
| EffectCraft  | OPFS → IndexedDB → memory; disk cache `effectcraft-cache/v1/<kind>/<32 hex>.ecc` in the desktop format; limit = setting capped at half the origin quota; `storage.info / persist / clear`. **Through JS glue** (`host.js`) | `apps/effectcraft-web/js/host.js:8-24`, `docs/web.md:75-95, 311-368`, `crates/engine/src/storage.rs:20-82` |
| LightCraft   | OPFS → IndexedDB via `web-sys`; `request_persistence`; `QuotaExceededError` → `NotSaved`, retried; one tab holds the library | `apps/lightcraft-web/src/backend.rs:1-4, 326-345`, `files.rs:260-272`          |
| FilmCraft    | OPFS for autosave, recovery and media copies (`web-sys`)                                                                 | `apps/filmcraft-web/src/opfs.rs:1-4`                                            |
| PhotoCraft   | Prefs in `localStorage`, presets in IndexedDB                                                                            | `apps/photocraft-web/src/web.rs:388-397`                                        |
| VectorCraft  | `localStorage` for prefs and recovery, with Web Locks                                                                    | `apps/vectorcraft-web/src/web.rs:495-524`                                       |
| WordCraft    | IndexedDB for user fonts                                                                                                 | `apps/wordcraft-web/src/web/font_store.rs:1-10`                                 |
| CADCraft, PdfCraft | eframe `persistence` (`localStorage`)                                                                              | `apps/cadcraft-web/src/main.rs:46-48`                                           |
| DesignCraft, GridCraft, FigCraft, SoundCraft, DeckCraft | No storage code                                                               | survey                                                                          |

### 2.3 What they already agree on

1. Caches are disposable; a damaged entry is a miss and is deleted (EffectCraft, LightCraft).
2. Content-addressed, versioned keys: an edit makes a new key, nothing is invalidated in place.
3. Temp file + rename, so no reader in any process sees a partial entry.
4. 128-bit hex names in 2-hex shard folders.
5. Size budget, LRU by modification time, index rebuilt from the folder on open.
6. Web: OPFS first, then IndexedDB, then memory, with `persist()` and `estimate()`.

### 2.4 Where they differ (the spec settles each, §3)

Root (cache folder vs roaming data folder vs inside the library), folder name (app id vs display
name), hash (FNV-1a vs SipHash), eviction target (100 % vs 80 %), command ids (`edit.purge`,
`cache.diskStats`, `mediaCache.*`, `storage.*`) and web plumbing (JS glue vs `web-sys` vs eframe).

## 3. Requirements

### 3.1 Storage classes

| Class          | Holds                                                     | Windows                          | macOS                                      | Linux                                              | Web (per origin)                         |
|----------------|-----------------------------------------------------------|----------------------------------|--------------------------------------------|----------------------------------------------------|------------------------------------------|
| **Settings**   | Prefs, layouts, presets; small, valuable                  | `%APPDATA%\<app>` (roams)        | `~/Library/Application Support/<app>`      | `$XDG_CONFIG_HOME/<app>` (`~/.config`)             | OPFS `settings/` → IndexedDB → memory    |
| **Data**       | Recovery, autosave, job state; valuable, machine-local    | `%LOCALAPPDATA%\<app>`           | `~/Library/Application Support/<app>`      | `$XDG_DATA_HOME/<app>` (`~/.local/share`)          | OPFS `data/`                             |
| **Cache**      | Thumbnails, previews, waveforms, analysis, frames; disposable, large | `%LOCALAPPDATA%\<app>\Cache` (never roams) | `~/Library/Caches/<app>`         | `$XDG_CACHE_HOME/<app>` (`~/.cache`)               | OPFS `cache/`                            |
| **Shared cache** | App-neutral results any Crafting app can reuse (§3.6)   | `%LOCALAPPDATA%\Crafting\Cache`  | `~/Library/Caches/Crafting`                | `$XDG_CACHE_HOME/crafting`                         | OPFS `shared/` when the apps share an origin, else the app's `cache/` |
| **User files** | Pictures, tracks, documents                               | never written by a cache (rule 04) | never                                    | never                                              | only what the user opened or imported    |

- **R1** Roots come from craft-libs' `paths` (`dirs::cache_dir()`, `data_local_dir()`,
  `config_dir()`), never hand-built per app (fixes the differences in §2.4).
- **R2** Resolution order for the cache root, the same as craft-settings' config rules
  (`paths.rs:116-136`): Settings ▸ Cache location → `<APP>_CACHE_DIR` (set but empty = **memory
  only**, for tests, demos and locked-down machines) → portable mode (`<exe dir>/<App>Data/Cache`,
  beside the `.app` on macOS) → the platform folder above → memory only if none can be created.
- **R3** Large caches never go in roaming `%APPDATA%` (opportunity G4 for FilmCraft's Media Cache
  default and EffectCraft's models).

### 3.2 Layout

```
<cache root>/v<N>/<kind>/<2 hex>/<32 hex>.crft        (+ CACHEDIR.TAG at <cache root>)
```

- **R4** `<kind>`: `thumbs`, `previews`, `waveforms`, `analysis`, `frames`, `layers`, `mattes`, …;
  each kind has its own format version `v<N>` and budget. A new format version starts an empty
  folder; the old one is deleted whole on the next prune.
- **R5** `CACHEDIR.TAG` (the Cache Directory Tagging spec, as LightCraft does) at every cache root
  on desktop, so backup tools skip it.
- **R5a** Every entry is a `.crft` file (§3.3's entry format); what the payload is (a PNG, JSON)
  is the kind's business, recorded in the kind's format version (C1, 2026-10-16: one extension
  keeps the own-files-only rule simple for every kind).
- **R6** Names stay short (≤ 60 characters under the root), which keeps Windows under `MAX_PATH`
  even below a long `%LOCALAPPDATA%`; names are lower-case hex, so case-insensitive file systems
  (Windows, macOS default) and case-sensitive ones agree.

### 3.3 Keys and entries

- **R7** Key: SipHash-1-3-128 with the suite's fixed keys (already in LightCraft and AssetCraft)
  over `kind`, the kind's format version, the producer's version (renderer, analyser), the
  parameters, and the **source identity**: file size, modification time, first and last 64 KiB.
  At most 128 KiB is read per key, so a 2 GB file costs what a 2 MB file does.
- **R8** Entry: magic `CRFT`, header version, kind tag, the source identity again, payload length,
  payload (optionally LZ4, `lz4_flex`, MIT), then a 64-bit checksum. On read the source identity
  is compared with the file's current one, so a key collision or a shared entry written by
  another app can never show the wrong picture or tempo.
- **R9** Every entry is **hostile input** (another process, another app or a damaged disk may have
  written it): length limits per kind, checksum, never executed, never trusted for paths. A bad
  entry is a miss and is deleted.

### 3.4 Writing and concurrency

- **R10** Write to `<32 hex>.tmp.<pid>.<n>` in the shard folder, then rename into place (atomic on
  NTFS, APFS, ext4 and in OPFS via `move()`); no fsync (disposable).
- **R11** Windows: a rename onto a file another process has open fails; the write is dropped and
  the next request recomputes (never an error to the user).
- **R12** Writes run off the UI thread through a bounded queue; a full queue drops the write
  (EffectCraft's rule), so caching never stalls the app.
- **R13** Several processes (two apps on the shared cache, two windows) may read and write at once:
  entries are immutable once named, so no locks are needed; pruning only deletes files older than
  the prune's own start.
- **R14** Own files only: list, count and delete only names of the cache's shape; never follow
  links (AssetCraft and LightCraft rule).

### 3.5 Budgets and management

- **R15** Budget per kind (defaults: thumbs 1 GiB, previews 4 GiB, waveforms 256 MiB, analysis
  256 MiB, frames 20 GiB in apps that render); LRU by modification time down to 80 %, a read
  refreshes the time at most once a day; optional "older than N days" (FilmCraft).
- **R16** Index rebuilt from the folder on open, on a background thread; the app works while it
  runs.
- **R17** Commands, the same ids in every app:

| Command          | Params                                         | Result                                                                  | Effect |
|------------------|------------------------------------------------|-------------------------------------------------------------------------|--------|
| `cache.info`     | `{}`                                           | `{root, rule, kinds: [{kind, version, bytes, files, budget}], shared?}` | READ   |
| `cache.clear`    | `{kind?: id, shared?: bool}`                   | `{freedBytes}` (dry run with `dryRun: true`)                            | APP    |
| `cache.budget`   | `{kind: id, bytes: n}`                         | `{kind, budget}`                                                        | APP    |
| `cache.location` | `{path: folder \| null}`                       | `{root, rule}` (null = platform default; contents are not moved)        | APP    |
| `storage.info`   | `{}` (web)                                     | `{backend: opfs\|indexeddb\|memory, usage, quota, persisted}`           | READ   |
| `storage.persist`| `{}` (web)                                     | `{persisted}` (asks the browser to keep the app's storage)              | APP    |

  Existing ids map onto these: EffectCraft `edit.purge {what: disk}` → `cache.clear {kind:
  frames}`, `cache.diskStats` → `cache.info`; FilmCraft `mediaCache.info / clean` → `cache.info /
  clear` (aliases kept in each app until its next major version).
- **R18** Settings ▸ Cache, the same page in every app: location (and the rule that chose it),
  size per kind against its budget, Clear per kind and Clear all, and the shared cache's size.

### 3.6 The shared cache

- **R19** Kinds that depend only on the source file and a published producer version go in the
  shared cache, so the first app to compute them saves every other app the work: **track
  analysis**, **waveform overviews**, **image thumbnails** at the standard sizes. App-specific
  results (EffectCraft frames, FilmCraft conformed media, LightCraft develop-dependent previews)
  stay in the app's cache.
- **R20** Each shared kind's format is specified in the interchange spec (storyboard plan step 10)
  and versioned; an app that reads a newer format than it knows treats it as a miss.
- **R21** Sandboxes: a Mac App Store build (App Sandbox), a Windows MSIX package (virtualised
  `%LOCALAPPDATA%`), a Flatpak or Snap (per-app `XDG_CACHE_HOME`) and a web build on its own origin
  cannot reach a common folder. There the shared kinds simply live in the app's own cache, same
  layout; nothing else changes. The shared cache is a speed-up, never a dependency.
- **R22** The shared cache's budget is the largest budget any app set; each app prunes only kinds
  it knows.

### 3.7 Web

- **R23** Backends in order: **OPFS** (sync access handles in a worker, `createWritable` on the
  main thread), then **IndexedDB** (one object store per kind, key = the file name), then
  **memory**; chosen once at start, reported by `storage.info`. All through `web-sys` from Rust.
- **R24** Budget = min(the setting, half of `navigator.storage.estimate()` quota) (EffectCraft's
  rule); `QuotaExceededError` drops the write (LightCraft's `NotSaved`) and prunes.
- **R25** `storage.persist` (`navigator.storage.persist()`) is offered, never forced; without it
  the browser may evict the cache, which is allowed.
- **R26** Several tabs: entries are immutable, so tabs share the cache safely; pruning takes a Web
  Lock (`navigator.locks`) so two tabs don't prune at once (VectorCraft and LightCraft use Web
  Locks and a single library owner).
- **R27** Only content the user opened lives there; nothing is uploaded (rule 04; "Nothing is
  uploaded anywhere", filmcraft `docs/web.md:6`).

### 3.8 Beside the media (later)

- **R28** Bridge's "export cache to folders" and FilmCraft's "save next to media" write into the
  user's folders: off by default, and in AssetCraft only after Stage 2's write rules (atomic, dry
  run, undo).

### 3.9 Tests every implementation passes

| Test                                    | What                                                                                       |
|-----------------------------------------|--------------------------------------------------------------------------------------------|
| Round trip per backend                  | put → get the same bytes (file system, OPFS in a headless browser, IndexedDB, memory)      |
| Hostile entries                         | truncated, wrong magic, wrong checksum, wrong source identity, oversized, a symlinked shard: all misses, deleted, no panic |
| Concurrency                             | two processes writing the same key; a reader during a write; a prune during reads          |
| Budget                                  | over budget → 80 % by LRU; a new format version folder empties the old                    |
| Roots                                   | each rule of R2, including empty `<APP>_CACHE_DIR` = memory only and portable mode         |
| Windows rename onto an open file        | the write is dropped, the next get recomputes                                              |

## 4. Where the code lives

A `craft-cache` crate, no UI and no app dependencies, built in AssetCraft first
(`crates/cache`, layer L1) and offered upstream to craft-libs beside `craft-settings` and
`craft-atomic`, so every app can depend on one implementation:

```rust
pub enum Root { Platform { app_id: &'static str }, Shared, Dir(PathBuf), Memory }
pub struct Cache { /* backend, root, kind, version, budget */ }
impl Cache {
    pub fn open(root: Root, kind: &'static str, version: u16, budget: u64) -> Cache; // never fails: memory as the last resort
    pub fn key(&self, source: &SourceId, params: &[u8]) -> Key;
    pub fn get(&self, key: &Key, source: &SourceId) -> Option<Vec<u8>>;
    pub fn put(&self, key: &Key, source: &SourceId, payload: &[u8]); // queued, may drop
    pub fn info(&self) -> KindInfo;
    pub fn clear(&self, dry_run: bool) -> u64;
}
pub struct SourceId { pub size: u64, pub mtime_ns: i64, pub head_tail: [u8; 16] }
// Backends: FsBackend (desktop), OpfsBackend / IdbBackend (wasm32, web-sys), MemBackend.
```

## 5. Phases

| Phase | Delivers                                                                                                           | Owner                    |
|-------|--------------------------------------------------------------------------------------------------------------------|--------------------------|
| C1    | **Built 2026-10-16.** `assetcraft-cache` (L1; desktop + memory backends; roots R1–R3, layout R4–R6, entries R7–R9, writes R10–R14, budgets R15–R16); AssetCraft thumbnails moved onto it (kind `thumbs` v1), the old `thumbs/` folder retired once in the background. Commands (R17) stay `cache.clear` / `app.state` until C3 | Claude                   |
| C2    | `analysis` and `waveforms` kinds for A2c, in the shared cache (R19–R22)                                            | Claude                   |
| C3    | Settings ▸ Cache page and the `cache.*` commands                                                                   | Claude                   |
| C4    | OPFS and IndexedDB backends (R23–R27), tested in a headless browser                                                | Claude                   |
| C5    | Upstream as `craft-cache` in craft-libs (a PR from Paul's fork, in craft-libs' style); sibling apps adopt it at their own pace | Paul (PR), each app      |

## 6. Decisions for Paul

| ID   | Question                                                                                                  | Recommendation                                                                                                   | Needed by |
|------|-----------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------|-----------|
| D-C1 | Shared cache for app-neutral results (§3.6)?                                                              | **Decided (Paul, 2026-10-16): yes**, with the sandbox fallback (R21); FilmCraft is the next app to read it ("I want to implement a very similar feature into FilmCraft. So, we want to share the cache") | C2 (A2c)  |
| D-C2 | Folder names: app id (`assetcraft`, craft-libs policy) or display name (`AssetCraft`, what most apps do on Windows and macOS)? | **Decided (Paul, 2026-10-16, "build C1" on the recommendations): display name on Windows and macOS, lower-case id on Linux** | C1        |
| D-C3 | Web backends OPFS → IndexedDB → memory, all from Rust through `web-sys` (no JS glue)?                    | **Yes**; EffectCraft's `host.js` storage becomes an upstream opportunity (G6)                                   | C4        |
| D-C4 | Shared cache folder name: `Crafting`?                                                                     | **Decided (Paul, 2026-10-16): `Crafting`** (`crafting` on Linux)                                                  | C2        |
| D-C5 | `cache.*` and `storage.*` as the suite's command ids, with the existing ids as aliases?                   | **Yes**                                                                                                          | C3        |
| D-C6 | Cache beside the media (R28)                                                                              | **Later, opt-in, after Stage 2**                                                                                 | Stage 2   |

## 7. Findings (opportunities in the sibling apps)

| ID | Scope        | Opportunity                                                                                       | Fix                                                                   | Owner                     | Status |
|----|--------------|---------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------|---------------------------|--------|
| G1 | Suite        | No shared cache crate; `craft-settings` has no dependents yet                                      | `craft-cache` (C1 → C5), built on `craft-settings` paths              | Claude (C1), Paul (C5)    | Open   |
| G2 | Web builds   | DesignCraft, GridCraft, FigCraft, SoundCraft and DeckCraft keep nothing between visits              | `craft-cache` OPFS backend + settings on OPFS                          | Each app                  | Open   |
| G3 | Docs         | Web READMEs say prefs are kept in `localStorage` but no code does it (designcraft, gridcraft, wordcraft `packaging/web/README.md:67`, filmcraft `:69`) | Fix the text or add the code                    | Each app                  | Open   |
| G4 | FilmCraft, EffectCraft | Large data in roaming `%APPDATA%` (Media Cache default; AI models)                       | Move to `%LOCALAPPDATA%` per craft-libs `docs/settings-storage.md:13` | FilmCraft, EffectCraft    | Open   |
| G5 | Suite        | Four namespaces for cache commands                                                                 | `cache.*` / `storage.*` with aliases (D-C5)                           | Each app                  | Open   |
| G6 | EffectCraft  | Web storage through `host.js`, against the suite's "Rust only, no handwritten JS" rule              | Replace with `craft-cache`'s `web-sys` OPFS backend                   | EffectCraft               | Open   |
