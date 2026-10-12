# CLAUDE.md

Guidance for coding agents working on AssetCraft.

## What this is

AssetCraft is a pure-Rust, Bridge-style asset app that browses, structures, organizes, plans
and executes. It's a sibling of the storytold Crafting Apps.
- Read `STATUS.md` first. It names the current stage and the next action.
- The plan is `docs/assetcraft-plan.md`.
- Sibling patterns and file references are in `docs/crafting-apps-notes.md`.

## Working agreement with Paul

- **Never run git write commands** (commit, push, rebase, merge, checkout of changed files).
  Paul runs all git himself, so hand over a PowerShell block instead:
  ```powershell
  Set-Location F:\Script-Projects\AssetCraft
  git add -A
  git commit -m "Stage N: <what changed>"
  git push
  ```
  Read-only git (`status`, `diff`, `log`, `show`) is fine.
- Small steps: one stage at a time, and one step at a time within a stage. No rewrites.
- Lead with artifacts (file:line, diffs, API shapes, schemas), then prose.
- Every finding names its scope, the goal it serves, the suggested fix and who owns it.
  Frame gaps as opportunities.
- Update `STATUS.md` at the end of every working session.

## House rules (from the Crafting Apps; follow from day one)

- **Rust only.** egui/eframe on wgpu; the web build is the same Rust compiled to WASM. Never
  add Tauri, Electron, a webview or JS.
- **Layering is enforced** by `cargo xtask layers`. UI crates (egui, eframe, winit, rfd,
  craft-ui) are only allowed in the UI layer and the apps.
- **Everything is a command:** a `CommandSpec { id, label, menu, shortcut, params, enabled, run }`
  registry. One `Session::execute(id, params)` entry point serves the UI, keys, CLI, control
  channel and MCP.
- **Never crash:** production crate roots deny `unwrap`, `expect` and `panic`, and forbid
  `unsafe`. Treat every file and input as hostile.
- **Clean room:** no Adobe or Microsoft assets or code, and no GPL dependencies. Every bundled
  asset gets an `ATTRIBUTION.md` entry.
- **Read before write:** nothing changes files on disk before Stage 2. After that, every write
  is atomic, every batch operation has a dry run, and undo works.
- **AI is optional.** Providers come from Preferences (Local, API, ArtCraft account). Every
  feature works with none selected. Secrets live in the OS keyring, never in files.
- **Elo is out of scope** for AssetCraft.
- Markdown tables are space-padded so columns line up in plain text.

## Verify

```powershell
cargo xtask ci        # fmt, clippy, tests, layers (once scaffolded in Stage 0)
```
Test against `test_data/library`. Every edge case there must load or fail gracefully.
