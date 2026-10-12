# STATUS

_Last updated: 2026-10-12_

## Current: AssetCraft Stage 0 (Navigator), step 1 done

**Plan:** [docs/assetcraft-plan.md](docs/assetcraft-plan.md) · **Plan mode spec:** [docs/storyline-builder.md](docs/storyline-builder.md) · **Study notes:** [docs/crafting-apps-notes.md](docs/crafting-apps-notes.md) · **Agent rules:** [AGENTS.md](AGENTS.md)

**Decided 2026-10-12**
- Storyline Builder is AssetCraft's Plan mode.
- AI providers are chosen in Preferences: Local, API or ArtCraft account (credits).
- Stage 0 is a Bridge-style navigator; `test_data/library/` holds 24 generated test files.
- Elo stays out of AssetCraft.
- This repo is `4waymedia/assetcraft` (renamed from ArtCraft-Library; history kept). Old ArtCraft material is in `docs/archive/`.

### Stage 0 checklist (steps from the plan)

- [x] 1. Workspace scaffold: `Cargo.toml` (edition 2024, Rust 1.95+), lints (no `unsafe`, no `unwrap`/`expect`/`panic` outside tests), stub crates `fs` L0, `probe` L1, `thumbs` L2, `engine` L5, `ui-egui` L6, apps `assetcraft` + `assetcraft-cli`, `xtask` (`ci`, `layers`), AGENTS.md, licences, NOTICE, ATTRIBUTION.md
- [ ] 2. `crates/fs`: `Services` trait, real + fake file system, bounded walks
- [ ] 3. `crates/probe`: magic-byte sniffing, header-only dimensions, file times
- [ ] 4. `crates/thumbs`: decode + downscale on a prioritized worker pool
- [ ] 5. `crates/engine`: `Session`, `CommandSpec` registry, navigation state
- [ ] 6. `crates/ui-egui` + desktop app: panels, craft-ui docking
- [ ] 7. `assetcraft-cli`: `list`, `probe`, `thumb`, `commands`
- [ ] 8. Tests over `test_data/library`; Stage 0 exit criteria

**Verified (Linux, Rust 1.97):** `cargo xtask ci` passes: fmt, clippy `-D warnings`, 12 tests, layers ok (8 crates). The layer check rejects an L0 crate that depends on `egui`.

**Next action:** Paul runs `cargo xtask ci` on Windows and commits (block in chat). Claude then does step 2 (`crates/fs`).

---

_Everything below is the superseded ArtCraft Library history._

## ArtCraft Library stage: 0 — Fork, build and spike

**Next action (Paul):** launch the app from `artcraft-wt\library`, open Apps → Library, and paste the `[Library spike]` console group back here.

### Stage 0 checklist

- [x] Upstream paths in the plan verified against `d4e4ac6`
- [x] Stage 0 patch written and type-checked (no new errors in touched files)
- [ ] Docs repo (this folder) committed and pushed (private)
- [x] Private clone `4waymedia/artcraft-private` created (not a fork), `upstream` push disabled
- [x] Library worktree at `artcraft-wt\library`; patch applied on `library/stage-0`
- [ ] App builds and runs from the private repo
- [ ] Library tab opens and shows the placeholder
- [ ] Console shows media files and folders from the live API
- [ ] `SearchUserMediaFiles` probe result recorded (see finding F2)

### Setup commands (PowerShell)

Layout (decided 2026-10-12): one private clone, one worktree per app.

```
F:\Script-Projects\
├── ArtCraft-Library\     # this docs repo
├── artcraft\             # private clone; main mirrors upstream; hosts worktrees
└── artcraft-wt\
    └── library\          # worktree: library/main -> library/stage-0
```

Done so far:

```powershell
Set-Location F:\Script-Projects
git clone -c core.autocrlf=false https://github.com/storytold/artcraft.git artcraft
Set-Location .\artcraft
git remote rename origin upstream
git remote set-url --push upstream DISABLED
git remote add origin https://github.com/4waymedia/artcraft-private.git
git push -u origin main

New-Item -ItemType Directory ..\artcraft-wt | Out-Null
git branch library/main main
git worktree add ..\artcraft-wt\library library/main
Set-Location ..\artcraft-wt\library
git switch -c library/stage-0
git apply ..\..\ArtCraft-Library\patches\stage0-library-tab.patch
```

Next: run the app from the worktree (two terminals):

```powershell
Set-Location F:\Script-Projects\artcraft-wt\library
.\script\artcraft\windows_frontend_dev.ps1    # terminal 1
.\script\artcraft\windows_rust_dev.ps1        # terminal 2
```

When the exit criteria pass:

```powershell
Set-Location F:\Script-Projects\artcraft-wt\library
git add -A
git commit -m "Stage 0: Library tab spike"
git push -u origin library/stage-0
```

Weekly upstream sync (from `F:\Script-Projects\artcraft`):

```powershell
git fetch upstream
git switch main; git merge --ff-only upstream/main; git push origin main
# then in each worktree: git rebase main
```

Add another app later: `git branch <app>/main main; git worktree add ..\artcraft-wt\<app> <app>/main`

## Findings (opportunities)

| ID | Finding                                                                                         | Scope   | Suggested fix                                                                                                    | Owner         |
|----|-------------------------------------------------------------------------------------------------|---------|------------------------------------------------------------------------------------------------------------------|---------------|
| F1 | Tabs are opened from the Apps menu (`config/appMenu.tsx`), not a top-bar entry                  | Stage 0 | Done in patch: `AppId`, `ALL_APPS` entry and `goToApp` list, plus a TopBar breadcrumb. Update plan step 4 wording | Claude (done) |
| F2 | `MediaFilesApi.SearchUserMediaFiles` (`/v1/media_files/search_session`) exists and nothing calls it | Stage 3 | Spike probes it with `"reaper"`. If it returns your files, Stage 3 search goes server-side and the risk row shrinks | Paul to run; Claude updates plan |
| F3 | Two `MediaFilesApi` classes: `libs/api` and `app/src/Classes/ApiManager`                        | 1 to 6  | Use `@storyteller/api` only (spike does), so Library logic can move to `frontend/libs/library`                   | Claude        |
| F4 | Windows dev runs two scripts; frontend uses `VITE_ENVIRONMENT_TYPE=production`, port 5173       | Stage 0 | Use both scripts as in step 4; the Unix single launcher doesn't apply                                           | Paul          |
| F5 | ~185 pre-existing `tsc -b` errors in `frontend/libs/*`                                          | All     | Treat as baseline; gate on "no new errors in touched files"                                                     | Claude        |

## Open decisions

| Decision                                                   | Recommendation                                     | Needed by |
|------------------------------------------------------------|----------------------------------------------------|-----------|
| Repo layout                                                | Decided: one clone + worktree per app              | Done      |
| Library: tab inside ArtCraft, or standalone app            | Tab first (patch is a tab); revisit at Stage 1     | Stage 1   |
| Local metadata: SQLite only, or SQLite + JSON sidecars     | SQLite only in Stage 4; sidecars on export (Stage 5) | Stage 4 |
| Cloud + local: one merged view or two sections             | Two sections first, merge later if it earns it     | Stage 4   |
| Deal type (OSS credit / license / role)                    | Decide before any code is shared                   | Stage 6   |

## Direction change (2026-10-12)

ArtCraft (credit-based AI studio) is not the hub. New target: **AssetCraft**, a pure-Rust
Bridge-style sibling of the storytold Crafting Apps (browse, structure, organize, plan,
execute). Elo stays out. The ArtCraft clone/worktree and Stage 0 patch are superseded; keep or
delete at will. Study notes: [docs/crafting-apps-notes.md](docs/crafting-apps-notes.md).

## Log

- 2026-10-12: Workspace docs created. Stage 0 patch prepared against `d4e4ac6`.
- 2026-10-12: Private clone + Library worktree set up; Stage 0 patch applied cleanly.
- 2026-10-12: Studied LightCraft, FilmCraft, PhotoCraft, DesignCraft, PdfCraft, WordCraft, craft-libs; notes written.
- 2026-10-12: AssetCraft plan, Storyline Builder spec and test_data/library written.
