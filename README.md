# ArtCraft Library

A Bridge-style asset browser for [ArtCraft](https://github.com/storytold/artcraft):
browse, organize and find generated assets (images, video, and later local files
and trellis.cpp 3D meshes) from one Library tab.

- **Project goal it serves:** one place to manage Reapers art across cloud and disk.
- **Immediate area of improvement:** ArtCraft's backend already has folders, tags,
  ratings and prompt metadata, but no page shows them.

## Layout

This repository is the **workspace**: plan, status, agent guidance and patches.
The ArtCraft source lives in a separate private clone, nested and git-ignored, so
upstream files (their `README.md`, `CLAUDE.md`, `AGENTS.md`) are never touched.

```
ArtCraft-Library/              # this repo (private): docs + patches
├── README.md                  # you are here
├── CLAUDE.md                  # guidance for coding agents in this workspace
├── STATUS.md                  # current stage, checklists, findings, next action
├── docs/
│   └── development-stages.md  # the 7-stage plan (source of truth for scope)
├── patches/
│   └── stage0-library-tab.patch
└── artcraft/                  # private clone of storytold/artcraft (git-ignored)
    └── ...                    # remotes: origin = your private repo, upstream = storytold
```

## Stages

| Stage | Name                       | Kind              | Status      |
|-------|----------------------------|-------------------|-------------|
| 0     | Fork, build and spike      | Frontend          | Patch ready |
| 1     | Read-only cloud library    | Frontend          | Not started |
| 2     | Organize                   | Frontend          | Not started |
| 3     | Find                       | Frontend          | Not started |
| 4     | Local folders              | Rust + SQLite     | Not started |
| 5     | Asset pipeline             | Rust + Three.js   | Not started |
| 6     | Package and offer          | Release + pitch   | Not started |

Full scope, steps and exit criteria: [docs/development-stages.md](docs/development-stages.md).
Live progress: [STATUS.md](STATUS.md).

## Quick start (Windows, PowerShell)

Prerequisites: Rust, Node 20+ (upstream tests with v24.13.0), Tauri CLI 2.x, `gh`.

```powershell
# From F:\Script-Projects\ArtCraft-Library, after the clone is set up (see STATUS.md)
Set-Location .\artcraft

# Terminal 1: frontend dev server (Vite, port 5173)
.\script\artcraft\windows_frontend_dev.ps1

# Terminal 2: Tauri Rust app
.\script\artcraft\windows_rust_dev.ps1
```

Then open **Apps → Library** in the running app.

## Licensing note

ArtCraft is dual-licensed MIT / Apache-2.0. Anything pushed to a public place can
be used without a deal, so both repos stay private until Stage 6 step 1 (deal
terms) is decided.
