# AssetCraft for agents

_2026-10-22 · Stage 1.5 (API) · v1: Part 1 marks what works today and what is planned; Part 2 is in force now_

- **Scope:** how an agent (Claude Code, a script, PipelineCraft) drives AssetCraft (Part 1), and how a
  coding agent develops it (Part 2). The binding code rules stay in [../AGENTS.md](../AGENTS.md), and
  the working agreement with Paul in [../CLAUDE.md](../CLAUDE.md). This page points to them and doesn't
  repeat them.
- **Project goal it serves:** one place an agent reads first, in the same layout as filmcraft
  `docs/agents.md` @ `256151ae` and designcraft / effectcraft `docs/agents.md`, so moving between
  Crafting Apps costs nothing.
- **Immediate area of improvement:** most of Part 1 is planned (Stage 1.5 steps C1–C8 in
  [cli.md](cli.md) §4). Today an agent drives AssetCraft through `assetcraft-cli commands` and `exec`.

---

## Part 1: driving AssetCraft

### 1. Three surfaces, one command set

| Surface         | Start                                              | Use it for                                        | Status          |
|-----------------|----------------------------------------------------|---------------------------------------------------|-----------------|
| CLI             | `assetcraft-cli exec <id> '<json>'`                | Scripts, one-off checks, CI                        | **works** (`exec`, `commands [filter]`, `describe`; C1 built 2026-10-22); `run`, `inspect`, `key=value` planned (C2–C3) |
| MCP             | `assetcraft-cli mcp`                               | Agents, headless or bridged                        | planned (C6), [mcp.md](mcp.md) |
| Control channel | `assetcraft --control <port>`                      | Driving and looking at the running window          | planned (C5), [control-protocol.md](control-protocol.md) |

All three call `Session::execute(id, params)`, the same path the menus and keys use. There's nothing an agent can do that the user can't, and nothing the user can do that an agent can't.

### 2. Discover

```powershell
Set-Location F:\Script-Projects\ArtCrafting\assetcraft
cargo run -q -p assetcraft-cli -- commands                 # table: id, enabled, effects, menu, shortcut, params
cargo run -q -p assetcraft-cli -- commands storyboard      # only ids / labels containing "storyboard"
cargo run -q -p assetcraft-cli -- commands --json          # the live list (cli.md §3.3)
cargo run -q -p assetcraft-cli -- describe nav.back        # one command, with enabled / disabledReason now
cargo run -q -p assetcraft-cli -- exec app.commands        # same list, as a command
```

Read each command's `enabled` and `effects` before you call it:
- `enabled: false` comes with `disabledReason`, the same words the command would answer with
  ("no storyboard is open (File → New Storyboard)", "nothing to go back to"); a command whose only
  preconditions are its parameters is always enabled, and its handler reports a bad parameter;
- `read` is safe to repeat;
- `write-app-state` changes the session or prefs;
- `write-user-file` touches the user's files (Stage 2+);
- `confirm` means a person should agree first;
- `remoteAllowed: false` is refused over MCP and the control channel.

### 3. Run

```powershell
Set-Location F:\Script-Projects\ArtCrafting\assetcraft
$cli = '.\target\release\assetcraft-cli.exe'
& $cli exec app.state --dir test_data\library
& $cli exec item.probe '{\"path\":\"F:\\Art\\shot.png\"}'
& $cli exec select.set '{\"paths\":[\"F:\\Art\\a.png\",\"F:\\Art\\b.png\"]}' --dir F:\Art   # one process = one session
& $cli exec file.renamePreview '{\"template\":\"{folder}_{seq:3}\",\"paths\":[\"F:\\Art\\a.png\"]}'
```

- **One session per process:** `exec` starts a fresh session each time, so selection doesn't carry over between calls. Pass `paths` explicitly, or use `run` / MCP / the control channel once they land.
- **Exit codes:** 0 ok, 1 the command failed, 2 usage error.

### 4. Writing files (Stage 2 onward)

- **Preview first:** every `write-user-file` command has a preview, either `dryRun: true` or a `…Preview` command. Run it first and show it to the person.
- **Same templates as LightCraft:** batch rename templates use LightCraft's tokens (`{name}` `{num}` `{seq:3}` `{folder}` `{ext}`).
- **Metadata stays in sidecars:** ratings, labels and metadata go to `IMG_0001.xmp` sidecars only, merged so other apps' fields survive. Image bytes are never rewritten (decision, Paul, 2026-10-21).
- **Undo is a command:** use it, rather than reversing changes by hand.

### 5. Looking at the UI

- **Today:** headless UI tests in `crates/ui-egui/src/tests.rs` drive `Navigator::ui` frame by frame. To see a frame without a GPU, use `crates/ui-egui/src/screenshot.rs`:
  `$env:AC_SCREENSHOT='F:\shot.png'; cargo test -p assetcraft-ui-egui screenshot -- --ignored`
- **Planned (C5b):** `ui.elements`, then `ui.click`, then `ui.screenshot` over the control channel, then compare. This is the suite loop (filmcraft `docs/agents.md` §3).

---

## Part 2: developing AssetCraft

### 6. Read first, in this order

1. [../AGENTS.md](../AGENTS.md): the code rules (Rust only, layers, everything is a command, never crash, read before write, clean room, optional AI).
2. [../CLAUDE.md](../CLAUDE.md): the working agreement (Paul runs all git; PowerShell blocks start with `Set-Location`; small steps; lead with artifacts).
3. [../STATUS.md](../STATUS.md): the current stage, the next step, the open decisions.
4. [assetcraft-plan.md](assetcraft-plan.md), then the plan doc for the area you touch (`storyboard-plan.md`, `track-analysis.md`, this API set).
5. [crafting-apps-notes.md](crafting-apps-notes.md) and [sibling-apis/](sibling-apis/): what to reuse from sibling apps (cite repo, file:line, commit).

### 7. Work loop

1. **Orient:** STATUS.md names the step. If it doesn't, ask Paul for one, and only one.
2. **Plan the tests:** the behaviour, the hostile inputs (`test_data/library/edge-cases`), and undo where it applies.
3. **Implement the smallest change** that passes. No rewrites.
4. **Run `cargo xtask ci`:** fmt, clippy `-D warnings`, tests (default and `heic`), layers, deny. Paul's Windows toolchain can be newer than the sandbox, so his run decides.
5. **Hand back:** a patch or files, plus a PowerShell block (`Set-Location <full path>`, apply, `cargo xtask ci`, `git add`, `git commit`). Never run git write commands.
6. **Record:** update STATUS.md with the step, its status, findings (ID, fix, owner, status) and decisions with date and who made them.

### 8. Decision rules

| Decide yourself                                                         | Ask Paul                                                                                  |
|-------------------------------------------------------------------------|-------------------------------------------------------------------------------------------|
| Names, file layout and tests inside the current step                    | Anything that writes the user's files in a new way, or changes where it writes             |
| Following an existing pattern or a sibling convention                   | A new dependency, especially licence, size, or anything GPL-adjacent (never GPL/LGPL/AGPL) |
| Fixing a bug you found, as its own small item                           | Scope: a new stage, a skipped step, a dropped feature                                       |
| Wording of docs and help texts                                          | Suite-facing contracts: command ids, the wire format, MCP tool names, bundle id             |

### 9. Definition of done (one step)

- [ ] **It's a command:** registered with an effects preset and a one-line `params` string. It has a menu entry, or is listed in `NOT_IN_MENUS`.
- [ ] **No crash paths:** no `unwrap` / `expect` / `panic!` / `unsafe` outside tests. Every number from disk or params is bounded.
- [ ] **Writes are safe:** a preview (`dryRun` or a `…Preview` command), atomic writes, never an overwrite, and an undo step.
- [ ] **Tests:** engine tests for the behaviour and hostile input, plus a headless UI test when the UI changed. Checked against `test_data/library`.
- [ ] **Credits and docs:** a NOTICE line for adapted sibling code, an ATTRIBUTION.md row for any bundled asset, docs updated (this set when commands change).
- [ ] **Green and recorded:** `cargo xtask ci` passes on Windows, and STATUS.md is updated.

### 10. Reporting

- **Say how you know:** "passes on Linux, Paul to run on Windows" vs "passes". "Measured" vs "estimated".
- **Open items name a fix and an owner,** framed as opportunities.
