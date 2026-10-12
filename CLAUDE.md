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
- **When a tool can't do something** (e.g. writing into `.cargo/`, building on Windows), don't stop:
  hand Paul the exact PowerShell commands to run, then carry on.
- **Every command block starts with `Set-Location <full path>`** so Paul knows where to run it.
- Small steps: one stage at a time, and one step at a time within a stage. No rewrites.
- Lead with artifacts (file:line, diffs, API shapes, schemas), then prose.
- Every finding names its scope, the goal it serves, the suggested fix and who owns it.
  Frame gaps as opportunities.
- Update `STATUS.md` at the end of every working session.

## Code rules

The code rules live in [AGENTS.md](AGENTS.md): Rust only, enforced layers, everything is a
command, never crash, read before write, clean room, optional AI. Read it before writing code.

## Verify

```powershell
cargo xtask ci        # fmt, clippy, tests, layers (once scaffolded in Stage 0)
```
Test against `test_data/library`. Every edge case there must load or fail gracefully.
