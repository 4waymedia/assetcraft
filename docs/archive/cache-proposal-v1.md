# Suite cache: one design for every Crafting app

_2026-10-16 · Stage 4 (Storyboard mode, analysis A2c) · proposal, decisions open_

- **Scope:** where and how the Crafting apps keep disposable, regenerable data (thumbnails, previews,
  waveforms, track analysis, rendered frames) on Windows, Linux, macOS and the web build.
- **Project goal it serves:** Bridge-style speed (Paul, 2026-10-16: "adobe bridge would create many
  cache files for edits, and thumbnails. We should implement the same"), without ever writing into
  the user's folders, and with one answer across the suite instead of one per app.
- **Immediate area of improvement:** AssetCraft's track analysis (A2c) needs a home now; its
  thumbnail cache already follows most of this design and moves onto it first.

## 1. What the suite has today

| ID | App / crate | What it does | Where | Cite |
|----|-------------|--------------|-------|------|
| S1 | craft-libs `craft-settings` | `paths::cache_dir(app_id)` = the platform cache folder + app id (`%LOCALAPPDATA%` on Windows so it does not roam), `data_dir(app_id)` for recovery files. Desktop only (`#[cfg(not(target_arch = "wasm32"))]`) | `dirs::cache_dir()/<app id>` | storytold/craft-libs `crates/settings/src/paths.rs:162–170`, `lib.rs:42–51`, 83bf4cb |
| S2 | EffectCraft disk cache | RAM-preview frames, layer buffers and mattes: `<folder>/v1/<kind>/<2 hex>/<32 hex>.ecc`, 128-bit content keys (a changed project changes its keys, so nothing goes stale), LZ4, magic + checksum per file (damaged = miss, deleted), LRU by modification time with a size limit, temp file + rename, background writes with a bounded queue. On the web build queued writes are dropped | user-set folder | storytold/effectcraft `crates/render/src/disk_cache.rs:1–60, 421`, 8f02f52 |
| S3 | FilmCraft Media Cache | Settings ▸ Media Cache: location (default `<data dir>/Media Cache`), **save next to media**, a separate database location, management never / older than N days / exceeds N GB; analysis results stored "In the Media Cache" or "In the project" | `<data dir>/Media Cache` | storytold/filmcraft `crates/engine/src/settings.rs:656–668, 949, 1604–1608`, 8309f21 |
| S4 | AssetCraft thumbnails | `<cache>/thumbs/<2 hex>/<32 hex>.png`; SipHash-1-3-128 key over renderer version, size bucket, file size, mtime and the first and last 64 KiB; 1 GiB LRU pruned to 80 % in the background; own-files-only listing; temp + rename | `%LOCALAPPDATA%\AssetCraft\thumbs`, `~/Library/Caches/AssetCraft/thumbs`, `$XDG_CACHE_HOME/assetcraft/thumbs` | `crates/thumbs/src/disk.rs:1–30`, `crates/ui-egui/src/prefs_file.rs:40–70` |

**Reading:** S2's layout and file rules plus S4's cheap source key already add up to a Bridge-style
cache. The gaps: every app picks its own root and folder name (S1 uses the app id, S4 a display
name); no app has a cache on the web; nothing is shared between apps, so the same track is
analysed again by every app that opens it.

## 2. Proposal

### 2.1 One layout

```
<cache root>/<owner>/v<N>/<kind>/<2 hex>/<32 hex>.<ext>
```

| Part | Rule |
|------|------|
| `<cache root>` | Windows `%LOCALAPPDATA%`, macOS `~/Library/Caches`, Linux `$XDG_CACHE_HOME` else `~/.cache` (S1's `dirs::cache_dir()`); a user-chosen folder in Settings overrides it (S3) |
| `<owner>` | The app id (`assetcraft`, `filmcraft`, …), or **`craft-shared`** for results any app can reuse (§2.3) |
| `v<N>` | Format version of that kind's files; a new format starts an empty folder, the old one is pruned whole |
| `<kind>` | `thumbs`, `waveforms`, `analysis`, `frames`, `layers`, … one folder per kind, each with its own budget |
| `<32 hex>` | 128-bit key: kind version + parameters + **source identity** (file size, mtime, first and last 64 KiB: S4's key) |
| `<ext>` | The payload type (`png`, `json`, `ecc`) |

**Files** (from S2 and S4): magic + format version + payload + checksum; a damaged, short or
unreadable file is a miss and is deleted; writes go to a unique temp file renamed into place (safe
across processes, so two apps can share a folder); only names of the cache's own shape are ever
listed or deleted.

### 2.2 Budgets and management

Per kind: a size budget with LRU eviction (S2, S4), optionally "older than N days" (S3). Settings ▸
Cache, the same page in every app: location, size used per kind, budget, **Clear**, and (later) the
shared cache's total. Clearing is always safe: everything is regenerable.

### 2.3 Shared results (Bridge's central cache)

App-neutral results go under `craft-shared` so the first app to compute them saves the work for the
rest: **track analysis** (AssetCraft, FilmCraft, SoundCraft), **waveform overviews**, **image
thumbnails**. App-specific results (EffectCraft frames, FilmCraft conformed media) stay under the
app. A shared kind's file format is versioned and documented in the interchange spec (storyboard
plan step 10).

### 2.4 Web build

No file system: the same keys and layout in the browser's **Origin Private File System** (OPFS,
reached from Rust/WASM through `web-sys`, no JS UI), with the browser's storage quota as the outer
budget and our LRU inside it. Until that lands, the web build keeps an in-memory cache only (what
EffectCraft does today, S2).

### 2.5 Cache beside the media (later, opt-in)

Bridge's "export cache to folders" and FilmCraft's "save next to media" (S3) write into the user's
folders, so for AssetCraft they wait for Stage 2's write rules and stay **off by default**.

### 2.6 Where the code lives

1. **C1** `assetcraft-cache` (L1, std only): `Cache::open(root, owner, kind, version, budget)`,
   `get(key) / put(key, bytes)`, `key(source identity, params)`, LRU prune, own-files-only; S4's
   thumbnail cache moves onto it (the old `thumbs/` folder is cleared once, nothing to migrate:
   it's all regenerable).
2. **C2** `analysis` and `waveforms` kinds for A2c (under `craft-shared` if D-C1 says so).
3. **C3** Settings ▸ Cache page and `cache.*` commands (`cache.info`, `cache.clear {kind?}`).
4. **C4** OPFS backend for the web build.
5. **C5** Offer the crate upstream as `craft-cache` in craft-libs, beside `craft-settings`, so
   every app shares one implementation (a PR from Paul's fork, in craft-libs' own style).

## 3. Decisions for Paul

| ID   | Question | Recommendation | Needed by |
|------|----------|----------------|-----------|
| D-C1 | Shared `craft-shared` folder for app-neutral results (analysis, waveforms, thumbnails), or each app keeps its own copy? | **Shared:** analyse a track once for the whole suite, like Bridge's central cache | A2c |
| D-C2 | Folder names: the app id (`assetcraft`, S1) or the display name (`AssetCraft`, S4 today)? | **App id**, as craft-settings already does; AssetCraft's thumbnails move with C1 | C1 |
| D-C3 | Web: OPFS now, or memory-only first? | **Memory-only first**, OPFS as C4; the desktop apps come first | C4 |
| D-C4 | Cache beside media (Bridge "export cache to folders") | **Later, opt-in, after Stage 2** (it writes into the user's folders) | Stage 2 |

## 4. Findings

| ID | Scope | Finding (opportunity) | Fix | Owner | Status |
|----|-------|-----------------------|-----|-------|--------|
| F-C1 | Suite | Each app picks its own cache root and naming (S1–S4) | One layout (§2.1) in a shared crate (C1 → C5) | Claude (C1), Paul (C5 upstream) | Open |
| F-C2 | Web | No persistent cache in any web build (S2 drops writes) | OPFS backend (C4) | Claude | Open |
| F-C3 | Suite | The same track or image is analysed again by every app | `craft-shared` kinds (D-C1) | Paul (decision) | Open |
