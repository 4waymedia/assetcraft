# ArtCraft Library — Bridge-style Asset Browser: Development Stages

Oct 11, 2026 · @Paul Gemignani

## Goal and scope

Build a Library page inside ArtCraft that browses, organizes and finds generated assets the way Adobe Bridge does, in seven stages: six to build it in your own repository, and one to package it and offer it to ArtCraft as a finished product. Each stage ships something usable on its own, and each later stage builds only on what the stage before it proved.

- **Project goal it serves:** a single place to manage Reapers art: character concepts, environment plates, video tests and, from Stage 5, 3D meshes from trellis.cpp.
- **Immediate area of improvement:** ArtCraft can generate assets but has no way to organize them. The backend already supports folders, tags, ratings and prompt metadata, but no page exposes them.
- **In scope:** a new Library page in the Tauri app (`frontend/apps/artcraft`), with Rust commands only once local files come in (Stage 4).
- **Out of scope:** changes to the ArtCraft backend (`artcraft-services` is a separate, private repo), the egui `artcraft-native` app, and editing pixels. Editing stays in the existing tools.

**Guiding principles**

1. Read before write: show assets before letting anyone change them.
2. Reuse before build: call the existing APIs and components, and add new ones only when nothing fits.
3. Cloud first, local second: the cloud library needs no Rust or schema changes, so it is the easiest part for ArtCraft to accept.
4. Every stage ends with exit criteria you can check by hand in the running app.

## Codebase baseline

Most of the Library already exists as unused API calls, so Stages 1 to 3 are frontend work. These are the parts each stage builds on, checked against the repo on 2026-10-11.

| Area | What exists | Where | Used by |
| --- | --- | --- | --- |
| App shell | Tauri 2 app; React, Vite, Zustand, Nx monorepo | `crates/desktop/artcraft`, `frontend/apps/artcraft` | All stages |
| Page registry | Tabs are a `TabId` union; `MainApp.tsx` lazy-loads one page per tab in a `switch` | `pages/Stores/TabState.ts`, `pages/MainApp.tsx` | Stage 1 |
| Media listing | `ListUserMediaFiles`: paged, filters by media class, type and engine category. No search term | `libs/api/src/lib/MediaFilesApi.ts` | Stages 1, 3 |
| Folders | Create, nest, rename, star, color code, cover image, move and remove media | `libs/api/src/lib/FoldersApi.ts` | Stages 1, 2 |
| Tags | Bulk add and set, list by tag, list untagged | `libs/api/src/lib/TagsApi.ts` | Stages 2, 3 |
| Ratings, bookmarks | Per-user ratings and bookmarks | `UserRatingsApi.ts`, `UserBookmarksApi.ts` | Stage 2 |
| Prompt metadata | `BatchGetPrompts` returns prompt, model and settings per file | `libs/api/src/lib/PromptsApi.ts` | Stage 1 |
| Grid and preview UI | `generation-list`, `gallery-modal`, `lightbox-modal` (has a `TagsSection`), `viewer-3d` | `frontend/libs/components/` | Stages 1, 2, 5 |
| Local data | `~/Artcraft/` with downloads, assets, weights, state; one SQLite table, `tasks` | `core/state/data_dir/`, `_database/sql/artcraft_migrations/` | Stage 4 |
| Tauri permissions | `dialog`, `http`, `opener`, `upload`, `log`. No filesystem plugin | `crates/desktop/artcraft/capabilities/default.json` | Stage 4 |

Two gaps shape the plan. Your own library has no server-side text search, so Stage 3 searches on the client over loaded pages and prompt text. And nothing reads local folders yet, so Stage 4 adds Rust commands and its own SQLite table.

## Stages at a glance

Stages 0 to 3 are frontend only and give you a working cloud Library; Stages 4 and 5 add local files and the 3D pipeline; Stage 6 packages it and offers it to ArtCraft.

_(Roadmap diagram lives in the original doc: 7 stages, 6 gates.)_

Each gate is the stage's exit criteria in short; the full checklists are in the stage sections below.

## Stage 0: Fork, build and spike

This stage proves you can build and run ArtCraft from source and add an empty Library tab, before writing any real features.

**Features**

- A private copy of ArtCraft that builds and runs on your machine.
- An empty `LIBRARY` tab that appears in the app.
- A throwaway call to `ListUserMediaFiles` and `ListAllFolders` that logs real data from your account.

**Steps**

1. Clone `storytold/artcraft`, push it to a new private repository, and follow `_docs/dev_setup.md`. Run the app from `frontend/apps/artcraft` and the Tauri crate.
2. Add `"LIBRARY"` to the `TabId` union in `pages/Stores/TabState.ts`.
3. Create `pages/PageLibrary/PageLibrary.tsx`, lazy-load it in `MainApp.tsx`, and add a `case "LIBRARY"` branch.
4. Add a tab entry in the top bar next to Moodboard.
5. In the page, call the two APIs once and log the results. Note the shape of `MediaFile` and the folder objects.

**Requirements**

- Rust toolchain, Node and npm, and the Tauri 2 prerequisites for Windows.
- A signed-in ArtCraft account with some generations and at least one folder. Create a folder in the web app if none exist.
- No Rust changes and no schema changes.

**Exit criteria**

- [ ] App builds and runs from your private repository.
- [ ] The Library tab opens and shows a placeholder.
- [ ] The console shows your media files and folders from the live API.

## Stage 1: Read-only cloud library

This stage gives you Bridge's three main panels over your cloud assets, with no way to change anything yet.

**Features**

- **Folders panel (left):** the folder tree from `ListAllFolders` and `ListSubfolders`, with stars, color codes and cover images. Two fixed entries at the top: All media and Unfiled (`ListMediaFilesWithoutFolder`).
- **Content panel (center):** a thumbnail grid with a size slider, a list view, sort by date, and paging.
- **Inspector panel (right):** a large preview, then file details (type, size, created date, model, provider) and the prompt from `BatchGetPrompts`.
- Keyboard: arrow keys move the selection; Space opens the existing lightbox.

**Steps**

1. Create a `libraryStore` in Zustand: selected folder, selected file, view mode, thumbnail size, sort order.
2. Build `FolderTree` with lazy loading of subfolders when a node is expanded.
3. Build `ContentGrid` on the `generation-list` components and `ListFolderMediaFiles`. Use the thumbnail URL templates rather than full files.
4. Virtualize the grid so a folder with 1,000+ items stays smooth.
5. Build `Inspector`. Fetch prompts in one `BatchGetPrompts` call per page and cache them by media token.
6. Wire Space to `lightbox-modal`.
7. Remember panel widths and the last folder in `localStorage`.

**Requirements**

- Stage 0 complete.
- Uses only existing APIs; no backend, Rust or schema changes.
- Media types covered: images and video. 3D files show a placeholder icon until Stage 5.

**Exit criteria**

- [ ] Every folder and subfolder in your account appears and opens.
- [ ] The grid shows the right files per folder, with paging, at 60 fps on scroll.
- [ ] Selecting a file shows its prompt and model.
- [ ] Nothing in this stage can change server data.

## Stage 2: Organize

This stage lets you change your library: folders, tags and ratings, one file or many at once.

**Features**

- Multi-select with click, Shift-click, Ctrl-click and Ctrl-A.
- Drag files onto a folder to move them (`MoveMediaFiles`), and drag folders to nest them (`AddSubfolders`).
- Folder actions from a right-click menu: new, rename, delete, star, color, set cover.
- Tags (Bridge's keywords): add and remove in the inspector, and bulk-tag a selection (`BulkAddTags`, `BulkSetTags`).
- Ratings 1 to 5 on keys 1 to 5, and bookmarks as Bridge's color labels.
- Delete a file, using the existing `media_file_delete_command`.

**Steps**

1. Add selection state to `libraryStore` and a selection count bar.
2. Add drag and drop between `ContentGrid` and `FolderTree`.
3. Build a folder context menu that calls `FoldersApi`.
4. Move the `TagsSection` from `lightbox-modal` into a shared component and reuse it in the inspector.
5. Add bulk tagging for a selection.
6. Map keys 1 to 5 to `UserRatingsApi`. First confirm its value range matches five stars.
7. Update the UI first and roll back on API failure, with a toast for errors.
8. Ask for confirmation before any delete.

**Requirements**

- Stage 1 complete.
- Check that ratings and bookmarks are scoped to your own files, not only to public media. Test on one file first.
- Still frontend only.

**Exit criteria**

- [ ] Moving 20 files between folders works and survives an app restart.
- [ ] Bulk tags appear on every selected file.
- [ ] Ratings show on grid thumbnails.
- [ ] A failed API call rolls back and shows an error.

## Stage 3: Find

This stage adds Bridge's filter panel, search and saved collections, so you can find an asset across hundreds of generations.

**Features**

- **Filter panel:** facets for media type, model, provider, tag, rating and date range, each with a count, combined with AND.
- **Search:** text search across prompt, tags and file name.
- **Smart collections:** saved filter sets, for example "Reapers, rated 4+, video".
- **Compare:** view two to four selected files side by side, the same idea as Bridge's Review mode.

**Steps**

1. Send media type and class filters to the server (`filter_media_classes`, `filter_media_type`), since the API supports them.
2. Use `ListMediaFilesWithTag` for a single tag filter on the server.
3. Filter model, provider, rating and date on the client over loaded pages, and show how many pages are loaded.
4. Build a client-side search index over prompt text and tags, filled as pages load. A small library such as MiniSearch fits.
5. Store smart collections as JSON under `~/Artcraft/settings/` or in `localStorage`.
6. Build a compare view with synced zoom and pan.

**Requirements**

- Stage 2 complete, so tags and ratings exist to filter on.
- Client-side search covers only loaded pages. Add a "load all" action for full search, and note the trade-off in the UI.
- If server-side search for your own library is needed later, raise it with ArtCraft's maintainers, because that API lives in their private backend.

**Exit criteria**

- [ ] Searching a prompt word finds the matching files within 200 ms on 1,000 loaded items.
- [ ] Facet counts update when filters change.
- [ ] A saved collection reopens with the same results after a restart.

## Stage 4: Local folders

This stage makes the Library a true Bridge: it browses folders on your disk next to the cloud library, starting with `~/Artcraft/downloads`.

**Features**

- A Local section in the folders panel. It shows the ArtCraft downloads folder by default, plus folders you add with a picker.
- The same grid, inspector, tags and ratings for local files, stored locally.
- Thumbnails generated once and cached.
- Live updates when files are added, renamed or deleted on disk.
- Upload a local file to the cloud library, and download a cloud file to a chosen local folder.

**Steps**

1. Add Rust commands, one file per command as `AGENTS.md` requires: `library_add_root`, `library_list_dir`, `library_scan_root`, `library_get_thumbnail`, `library_set_tags`, `library_set_rating`.
2. Keep library data in a new SQLite file, `library_v1.sqlite`, with its own migrations folder. Tables: `roots`, `files` (path, size, modified time, content hash, media class), `tags`, `file_tags`, `ratings`.
3. Scan on a background thread, following the pattern in `core/threads/`. Emit progress as Tauri events.
4. Generate thumbnails with the `image` crate, cached under `~/Artcraft/state/thumbnails/` and keyed by content hash. Use ffmpeg for video frames if it is on the PATH.
5. Watch roots with the `notify` crate and emit change events to the frontend.
6. Show local files with `convertFileSrc`. The asset protocol is already on in `tauri.conf.json`.
7. Add a `LocalSource` beside the cloud source in the frontend, so the grid and inspector work with both.

**Requirements**

- Stage 3 complete, so the UI is stable before a second data source is added.
- A separate database file rather than a change to `tasks`. A change there means bumping `TASK_DATABASE_VERSION`, and a missed bump segfaults on Windows at startup.
- Narrow the asset protocol scope from `**` to the library roots before you share the build.
- Write the Rust to their conventions from the start, since the maintainers will review it in Stage 6: one command per file, `sqlx::query!` macros, `log` macros.

**Exit criteria**

- [ ] Adding a folder with 2,000 images scans in under a minute and browses smoothly.
- [ ] A file copied into a watched folder appears without a restart.
- [ ] Local tags and ratings survive a restart.
- [ ] Upload to the cloud shows the file in the cloud library.

## Stage 5: Asset pipeline

This stage connects the Library to the rest of your Reapers workflow: 3D previews, trellis.cpp outputs, sending assets into ArtCraft's tools, and batch export.

**Features**

- 3D previews: orbit glTF/GLB meshes and Gaussian splats in the inspector, with rendered grid thumbnails.
- A trellis.cpp output folder as a watched root, with each mesh linked to the image it came from.
- Send to tool: open a selection in 2D, 3D scene, Image to Video or Moodboard.
- Batch rename with patterns, for example `reaper_{tag}_{nnn}`.
- Export a selection to a folder, with a JSON or XMP sidecar holding prompt, model, tags and rating.

**Steps**

1. Reuse `viewer-3d` in the inspector. It already loads glTF through `GLTFLoader` and splats through Spark's `SplatMesh`.
2. Render 3D thumbnails once in an offscreen Three.js canvas and save them to the Stage 4 thumbnail cache.
3. Add a `source_file` link in `library_v1.sqlite` so a mesh points to its source image. Fill it by matching trellis.cpp output names.
4. Add send-to actions that set the target tab's store and switch tabs with `setActiveTab`. The existing `PageDrawWithGalleryDrop` shows how a page accepts gallery items.
5. Add `library_batch_rename` and `library_export` Rust commands with a dry-run preview.
6. Write sidecars as JSON first; add XMP only if another tool needs to read them.

**Requirements**

- Stage 4 complete for local meshes and the thumbnail cache.
- Agree a trellis.cpp output naming scheme first, so the mesh-to-image link is reliable.
- Batch rename and export change files on disk, so they need a preview and an undo log.

**Exit criteria**

- [ ] A trellis.cpp GLB shows as a thumbnail and orbits in the inspector.
- [ ] Each mesh shows a link to its source image.
- [ ] Sending an image to the 3D scene opens it there.
- [ ] Export writes files and matching sidecars, and a dry run changes nothing.

## Stage 6: Package and offer to ArtCraft

This stage turns the finished fork into something ArtCraft can say yes to: a gated feature, reviewable code, a demo and a written proposal.

**Features**

- **Feature flag:** a Settings toggle that shows the Library tab, off by default. `useStoryboardPageEnabled` in `ui-settings-modal` already gates the Storyboard page this way.
- **Reviewable code:** one pull request per stage, each rebased on current `main` and passing their checks.
- **Demo:** a two-minute video of the Reapers workflow, plus a signed Windows build they can install.
- **Proposal:** a short doc covering the problem, what the Library does, its architecture, the maintenance it adds, and what you are asking for.

**Steps**

1. Decide the deal before anyone sees the code: an open-source contribution with credit, a paid license or contract, or a pitch for a role or contracting work. Get a lawyer's view if money is involved.
2. Keep the code in a private repository until then. A GitHub fork of a public repo is public, so clone and push to a new private repo instead.
3. Audit against `AGENTS.md`: one Tauri command per file, `sqlx::query!` macros, `log` macros instead of `println!`, two-space indentation, refreshed `.sqlx` metadata.
4. Add tests: Vitest for stores and filters, Rust tests for each command and the `library_v1.sqlite` migrations.
5. Add the Settings toggle and check the app with it off matches upstream exactly.
6. Split the branch into one PR per stage, so they can accept Stages 0 to 3 even if they decline the Rust stages.
7. Record the demo and write the proposal.
8. Contact the maintainers through Discord or the founders, show the demo first, then share code under the agreed terms.

**Requirements**

- Stages 0 to 5 complete and used on real Reapers assets for at least two weeks.
- A clean rebase on the latest upstream `main` within a week of the pitch.
- The asset protocol scope narrowed from `**`, so the security review has nothing to flag.

**Exit criteria**

- [ ] With the toggle off, the app behaves exactly like upstream.
- [ ] Every PR builds and passes tests on its own.
- [ ] Demo, build and proposal are ready to send.
- [ ] The deal terms are decided before any code is shared.

## Risks, open decisions and next action

Building the whole product before pitching it carries one main risk: ArtCraft's plans can change during the build. A demo video after Stage 1 tests their interest without sharing code.

| Risk | Effect | Fix | Stage |
| --- | --- | --- | --- |
| ArtCraft builds its own library, or moves to the egui `artcraft-native` app | The offer lands on a product that no longer needs it | Watch their commits and `ROADMAP.md` weekly; keep Library logic in a shared lib (`frontend/libs/library`) so it can move; send a short demo video after Stage 1 to stake the idea without sharing code | 1 to 6 |
| Code shared before terms are set | Code under MIT/Apache can be used with no deal | Private repo; show video and builds first; share source only after Stage 6 step 1 | All |
| Upstream UI changes often | Rebase conflicts in `MainApp.tsx` and the top bar | Keep all Library code in `pages/PageLibrary/`; touch shared files in one line each; rebase weekly | 0 to 6 |
| No server-side search on your own library | Search misses unloaded pages | Client index plus a "load all" action; list a search parameter in the proposal as a backend ask | 3 |
| Ratings API may target public media | Ratings fail or show to others | Test on one file in Stage 2 step 6; fall back to tags such as `rating:4` | 2 |
| SQLite version bump missed | Windows segfault at startup | Separate `library_v1.sqlite`; never edit the `tasks` migration | 4 |
| Asset protocol scope is `**` | Any local file can be served to the webview | Narrow to library roots before Stage 6 | 4 |
| Disk changes in rename and export | Lost or misnamed files | Dry run, undo log, confirm dialog | 5 |

**Open decisions**

- Decided: build all six stages in your own repository, then offer the finished Library to ArtCraft (Stage 6).
- Local metadata format: SQLite only, or SQLite plus JSON sidecars next to files from Stage 4?
- Should cloud and local appear as one merged view, or as two sections in the folders panel?

**Recommended next action:** create a private repository from a clone of ArtCraft (not a GitHub fork, which would be public) and start Stage 0 this week.
