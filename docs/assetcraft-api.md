# AssetCraft API

_2026-10-14 · Stage 1.5 · status: proposed, not started_

- **Scope:** how programs (scripts, agents, the sibling apps, the future asset-hub layer) drive
  AssetCraft, headless or while the window is open.
- **Project goal it serves:** AssetCraft "browses, structures, organizes, plans and executes".
  Executing plans and the asset-hub layer both need AssetCraft to be callable, not just clickable.
- **Immediate area of improvement:** every action is already a command
  (`Session::execute(id, params)`, `crates/engine/src/lib.rs:211`) and `assetcraft-cli exec` runs
  one per process. Missing: a long-lived session, MCP, a live-app channel, and a policy for
  what a remote caller may run.
- **Why before Stage 2:** Stage 2 adds the first writes to user files. The effect classes and
  remote policy below must exist *before* any write is reachable from outside.

## One engine, five doors

```
                     ┌──────────────────────────── Session::execute(id, params) ──┐
 UI / keys ──────────┤  (exists)                                                  │
 assetcraft-cli exec ┤  (exists) one command per process                          │
 assetcraft-cli serve┤  NEW  stdio JSON lines, one long-lived headless session    │
 assetcraft-cli mcp  ┤  NEW  MCP over stdio (compact tool set, root-confined)     │
 assetcraft --control┤  NEW  loopback TCP + token, drives the open window         │
                     └── every remote door passes through api::policy first ──────┘
```

## Wire format (`serve` and `--control` share it)

One JSON object per line, UTF-8, ≤ 1 MiB per line.

```jsonc
// request
{"id": 1, "command": "nav.open", "params": {"path": "F:\\Art\\Reapers"}}
// success
{"id": 1, "ok": true, "result": { /* command's JSON */ }, "generation": 42}
// failure
{"id": 1, "ok": false, "error": {"code": "bad_params", "message": "nav.open: `path` is required"}}
```

| `error.code`  | From                          | Meaning                                         |
|---------------|-------------------------------|-------------------------------------------------|
| `unknown`     | `CmdError::Unknown`           | No command with that id                         |
| `bad_params`  | `CmdError::BadParams`         | Missing, wrong-typed or unknown parameter       |
| `fs`          | `CmdError::Fs`                | The file system refused                         |
| `unavailable` | `CmdError::Unavailable`       | Can't run right now                             |
| `denied`      | NEW `api::policy`             | Effect class not allowed on this door, or path outside `--root` |
| `protocol`    | NEW `api::wire`               | Line isn't valid JSON / too long / bad envelope |

Later (with Stage 2): `{"event": "nav.changed", "generation": 43}` pushed on `serve` and `--control`.

## Command effects (step 1)

`CommandSpec` gains `effect: Effect`. `app.commands` reports it; MCP maps it to
`readOnlyHint` / `destructiveHint`.

| Effect     | Commands today                                                                        | Remote default                    |
|------------|---------------------------------------------------------------------------------------|-----------------------------------|
| `Read`     | `app.state` `app.commands` `item.probe` `app.openInTargets` (+ new `folder.list` `item.thumbnail`) | allowed                |
| `View`     | `nav.*` (but `nav.changed`) `view.*` `select.*` `formats.heic` `cache.clear`         | allowed                           |
| `Launch`   | `file.open` `file.openIn` `file.reveal`                                               | refused unless `--allow-launch`   |
| `Run`      | `file.run` (executes programs)                                                        | **always refused remotely**       |
| `Internal` | `nav.changed` (fed by the watcher)                                                    | hidden and refused                |
| `Write`    | none yet (Stage 2: ratings, rename, move…)                                            | refused unless `--allow-write` and inside `--root` |

Recommendation: `file.run` never crosses a remote door. An agent that can run arbitrary programs
through a file browser is a bigger grant than anything else here; Paul can lift it later per-door
if a real recipe needs it.

## New read commands for callers (step 3)

The GUI commands move the user's view. Callers also need to look without moving it:

```jsonc
// folder.list  (Read) — doesn't change location, history or selection
{"path": "F:\\Art\\Reapers\\plates", "sort": "date", "ascending": false, "offset": 0, "limit": 200, "hidden": false}
→ {"path": "...", "total": 734, "entries": [{"name": "...", "kind": "file", "size": 1234, "modified": "2026-10-01T12:00:00Z", "format": "png", "width": 4096, "height": 2160}]}

// item.thumbnail  (Read) — through the same ThumbPool + disk cache as the UI
{"path": "F:\\Art\\Reapers\\plates\\p01.psd", "size": 256}          // size 32..512
→ {"mime": "image/png", "width": 256, "height": 144, "data": "<base64>"}
```

## MCP (step 5): compact mode, PdfCraft's model

`assetcraft-cli mcp --root <DIR> [--allow-launch]`: JSON-RPC 2.0 over stdio, written with
`serde_json` (no SDK dependency, keeps the tree pure Rust and small).

| Tool                   | Maps to                 | Hints              |
|------------------------|-------------------------|--------------------|
| `assetcraft_list`      | `folder.list`           | read-only          |
| `assetcraft_probe`     | `item.probe`            | read-only          |
| `assetcraft_thumbnail` | `item.thumbnail` → MCP `image` content | read-only |
| `assetcraft_state`     | `app.state`             | read-only          |
| `tool_search`          | filters `app.commands`  | read-only          |
| `tool_call`            | any allowed command     | from its `effect`  |

Every path parameter is resolved and must sit inside a `--root`; unknown keys are rejected.

Claude Desktop config (Windows) once built:

```json
{ "mcpServers": { "assetcraft": {
    "command": "F:\\Script-Projects\\ArtCrafting\\assetcraft\\target\\release\\assetcraft-cli.exe",
    "args": ["mcp", "--root", "F:\\Art\\Reapers"] } } }
```

## Live control channel (step 6): PhotoCraft's model

- `assetcraft --control <port>` (`0` = pick a free port). Off unless the flag is given.
- Binds `127.0.0.1` only. Writes `{port, token}` to `<config>/control.json` (owner-only), new token
  each launch. First line from a client must be `{"token": "..."}` or the socket is closed.
- The listener thread never touches `Session`: requests go to the UI thread over a channel,
  are applied in `update()` through the same `execute`, then `ctx.request_repaint()`. The window
  updates exactly as if the user clicked.

## Layering

| Crate        | Layer | Contents                                                         |
|--------------|-------|------------------------------------------------------------------|
| `api` (NEW)  | L6    | `wire` (envelope, errors), `policy` (effects, roots), `serve`, `mcp`, `control` listener. No UI crates. Replaces the planned `mcp` row |
| `engine`     | L5    | gains `Effect`, param schemas, `folder.list`, `item.thumbnail`   |
| apps         | —     | `assetcraft-cli serve|mcp`; `assetcraft --control` wires `api::control` to `ui-egui` through a plain channel (so `ui-egui` doesn't depend on `api`) |

## Steps (one at a time, each ends green on `cargo xtask ci`)

| #  | Step                                                         | Exit check                                                                 |
|----|--------------------------------------------------------------|----------------------------------------------------------------------------|
| 1  | `Effect` on every `CommandSpec`; `app.commands` shows it      | Test: every command has an effect; `file.run` is `Run`; `nav.changed` is `Internal` |
| 2  | `crates/api` `wire` + `policy` + `assetcraft-cli serve`       | Exit test pipes 6 requests (ok, unknown, bad JSON, denied `file.run`, …) and checks each line |
| 3  | `folder.list`, `item.thumbnail`                               | Every file in `test_data/library` lists and thumbnails or fails gracefully |
| 4  | Param schemas (JSON Schema per command), unknown keys rejected | `app.commands` emits schemas; test: each schema validates its own examples |
| 5  | `assetcraft-cli mcp` (compact)                                | Test drives `initialize` → `tools/list` → `tools/call` thumbnail; Paul adds it to Claude Desktop and lists a folder |
| 6  | `assetcraft --control`                                        | Test client with token opens a folder in a headless `Navigator`; wrong token is dropped; Paul drives the open window from PowerShell |
| 7  | README, STATUS, plan                                          | Docs match the code                                                         |

## Opportunities (later, owner: Claude unless noted)

- Events stream (`nav.changed`, `thumb.ready`) once Stage 2 gives callers something to react to.
- A typed Rust client crate for the asset-hub layer (owner: Paul decides when that layer starts).
- Ask DesignCraft upstream to add PhotoCraft-style tokens to its control channel (already logged in
  `crafting-apps-notes.md`).
