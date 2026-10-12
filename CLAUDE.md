# CLAUDE.md

Guidance for coding agents working in the ArtCraft Library workspace.

## What this is

A Bridge-style Library tab for ArtCraft, built in seven stages
(`docs/development-stages.md`). This folder is the workspace repo; the ArtCraft
source is the nested private clone at `artcraft/`.

- Inside `artcraft/`, **upstream's `AGENTS.md` / `CLAUDE.md` apply first.** This file
  adds Library-specific rules on top; it never overrides upstream conventions.
- Read `STATUS.md` before starting work: it names the current stage and next action.

## Working agreement with Paul

- **Never run git write commands** (commit, push, rebase, merge, checkout of changed
  files). Paul runs all git himself. Hand over a PowerShell block instead:
  ```powershell
  Set-Location F:\Script-Projects\ArtCraft-Library\artcraft
  git add -A
  git commit -m "Stage N: <what changed>"
  git push
  ```
  Read-only git (`status`, `diff`, `log`, `show`) is fine.
- Small steps. One stage at a time; within a stage, one step at a time. No rewrites.
- Lead with artifacts: file:line, diffs, API shapes, DDL. Prose second.
- Every finding comes with: scope, the goal it serves, the suggested fix, and who owns it.
  Frame gaps as opportunities; this is all in active development.
- Update `STATUS.md` at the end of every working session (checkboxes, findings, next action).

## Where Library code goes (keeps upstream rebases clean)

| What                         | Path (inside `artcraft/`)                                       |
|------------------------------|-----------------------------------------------------------------|
| Library page and components  | `frontend/apps/artcraft/app/src/pages/PageLibrary/`             |
| Reusable logic (Stage 1+)    | `frontend/libs/library/` (survives a move to `artcraft-native`) |
| Rust commands (Stage 4+)     | one file per command, `library_*` names, per upstream `AGENTS.md` |
| Local DB (Stage 4+)          | `library_v1.sqlite` with its own migrations folder              |

Shared upstream files get **one-line touches only**. The Stage 0 touch points:

| File                                                        | Touch                                  |
|-------------------------------------------------------------|----------------------------------------|
| `frontend/apps/artcraft/app/src/pages/Stores/TabState.ts`   | `"LIBRARY"` in `TabId`                 |
| `frontend/apps/artcraft/app/src/pages/MainApp.tsx`          | lazy import + `case "LIBRARY"`         |
| `frontend/apps/artcraft/app/src/config/appMenu.tsx`         | `AppId`, `ALL_APPS` entry, `goToApp`   |
| `frontend/apps/artcraft/app/src/components/signaled/TopBar/TopBar.tsx` | breadcrumb case            |

## Hard rules

- **No backend changes.** `artcraft-services` is private and out of scope.
- **Never edit the `tasks` SQLite migrations** or `TASK_DATABASE_VERSION` (a missed
  bump segfaults on Windows). Library data goes in `library_v1.sqlite`.
- **Use `@storyteller/api`** (`frontend/libs/api`) for API calls, not the app-local
  duplicate in `app/src/Classes/ApiManager/`, so Library logic can move into a lib.
- Stages 1 to 3 are frontend only: no Rust, no schema, no new Tauri permissions.
- Stage 1 is read-only: nothing may change server data.
- Before Stage 6, narrow the asset protocol scope from `**` to the library roots.
- Never push to `upstream`; its push URL is disabled on purpose.
- Rust (Stage 4+): `sqlx::query!` macros, `log` macros (no `println!`), two-space
  indentation in TS, refreshed `.sqlx` metadata.
- Markdown tables are space-padded so columns align in plain text (upstream rule).

## Verify

```powershell
# Type-check the app (inside artcraft/frontend/apps/artcraft)
npx tsc --noEmit -p tsconfig.json
```
Upstream already has ~185 `tsc -b` errors in `frontend/libs/*` (build-config and spec
files) as of `d4e4ac6`. Judge changes by **no new errors in touched files**, then by
the stage's exit criteria checked by hand in the running app.
