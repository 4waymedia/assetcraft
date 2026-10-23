# MCP server

_2026-10-22 · Stage 1.5 (API) · spec v1: not built yet (step C6 in [cli.md](cli.md) §4)_

- **Scope:** `assetcraft-cli mcp`, the Model Context Protocol server agents use to drive AssetCraft
  headless or, with `--bridge`, the running window. The tool set, annotations, errors and file
  confinement are covered here.
- **Project goal it serves:** an agent that already knows FilmCraft, EffectCraft, DesignCraft or
  SoundCraft can drive AssetCraft with the same tools and the same client config (suite "FilmCraft
  #28" conventions), and the tool hints come from AssetCraft's declared command effects.
- **Immediate area of improvement:** today `assetcraft-cli mcp` exits with "unknown command `mcp`"
  (`ArtCrafting\cli-matrix\assetcraft.mcp-help.err`, 2026-10-22).

Suite sources (2026-10-22): filmcraft `docs/agents.md:82-132` @ `256151ae` (the newest conventions),
effectcraft `docs/mcp.md:223-251` @ `2c08f675`, designcraft `docs/mcp.md` @ `5aeb4dd7` (file
confinement), soundcraft `docs/mcp.md` @ `8198d6e`.

## 1. Register it

```jsonc
// Claude Code: .mcp.json in the repo, or `claude mcp add assetcraft -- <cmd>`
{
  "mcpServers": {
    "assetcraft": {
      "command": "F:\\Script-Projects\\ArtCrafting\\assetcraft\\target\\release\\assetcraft-cli.exe",
      "args": ["mcp", "--dir", "F:\\Art",
               "--automation-read-root", "F:\\Art",
               "--automation-write-root", "F:\\Art\\_out"]
    }
  }
}
```

- **Transport:** newline-delimited JSON-RPC 2.0 on stdio. Logs go to stderr, never stdout.
- **Protocol versions:** 2025-06-18, 2025-03-26 and 2024-11-05. 2026-07-28 (`_meta`, `resultType`) is honoured as in filmcraft.
- **Modes:**
  - **Headless** (default): an in-process `Session` opened at `--dir`.
  - **Bridged:** `--bridge 127.0.0.1:PORT` forwards to `assetcraft --control PORT` ([control-protocol.md](control-protocol.md)). The `ui_*` tools work only when bridged; headless they return an error that says how to connect.

## 2. Tools

Commands aren't one tool each: the core set is the suite's, and it's the same in every app.

| Tool             | Arguments                                   | What                                                                  | Hints (read, destructive, idempotent, openWorld) |
|------------------|---------------------------------------------|-----------------------------------------------------------------------|--------------------------------------------------|
| `command_list`   | `{filter?}`                                 | `commands --json` (suite shape, effects, `enabled`)                    | read                                             |
| `command_run`    | `{id, params?}`                             | One command via `Session::execute`; the result is the command's JSON   | from the command's effects, checked per call      |
| `command_batch`  | `{steps: [{id, params?}], stop_on_error?}`  | `{completed, failed, results}`; `failed > 0` sets `isError`             | the union of the steps' effects                   |
| `doc_inspect`    | `{}`                                        | `app.state` (location, selection, view, items) plus storyboard state when open, and `session.inspect`'s change report (C3b) | read                                     |
| `render_preview` | `{path, max_side?, out?}`                   | A thumbnail PNG (`thumb.render`): base64 ≤ 1 MiB, or a **new** file at `out` | read; a writer when `out` is given        |
| `ui_screenshot`, `ui_elements`, `ui_click`, `ui_key`, `ui_type` | as the control methods | Bridged only                                               | `ui_screenshot` read; the rest are not read-only  |

- **Resources:** `assetcraft://state` (`app.state`) and `assetcraft://commands`, both `application/json`.
- **Prompts:** none.
- **Annotations:** every tool has a `title` and the four hints. A test checks that each command-backed tool's hints match the command's declared `Effects` (`crates/engine/src/effect.rs`):
  - `read` gives `readOnlyHint: true`;
  - `destructive` gives `destructiveHint`;
  - `idempotent` gives `idempotentHint`;
  - `external-network` gives `openWorldHint`.
- **Refused here (AC):** `command_run` refuses commands with `remoteAllowed: false` (`file.run`). Without `--automation-write-root` it refuses every `write-user-file` command (decision D-A4 in cli.md); `file.renamePreview` and other previews still work.

## 3. Errors (suite)

| Case                                   | Response                                                         |
|----------------------------------------|------------------------------------------------------------------|
| Unknown tool argument                  | JSON-RPC `-32602`, naming the accepted keys                       |
| Command failed (`CmdError`)            | Tool result with `isError: true` and the message                  |
| `command_batch` with `failed > 0`      | `isError: true`; `results` holds every step                       |
| A line that isn't JSON                 | `-32700` with `id: null`; the server keeps serving                |
| Panic caught at the tool boundary      | `isError: true`, `internal error: …` (never-crash rule)           |

## 4. Long work

Long commands emit `notifications/progress` when the request carries `_meta.progressToken`: today the waveform decode, later batch rename and sidecar writes. `notifications/cancelled` stops the job, rolls back what it changed (the batch-rename rollback in T2), and sends no response (filmcraft `docs/agents.md:105-132`).

## 5. Confining file access (suite text, designcraft `docs/mcp.md:153-219`)

- `--automation-read-root <dir>` and `--automation-write-root <dir>` can each be given more than once. The flags are the same in every app, so one client config works across the suite.
- **A root you leave out grants nothing.** A path outside every read root is refused before any I/O. With no write root, no tool writes a user file.
- **Path matching:** symlinks are resolved, then paths are compared folder by folder (`F:\Art` doesn't admit `F:\Artwork`). Windows device names (`CON`, `\\.\`) are refused.
- **Not with `--bridge`:** combining the roots with `--bridge` is a usage error. The running app enforces its own rules.
- **One gate (AC):** every path any command touches goes through one engine gate (`Session` services). A test checks that commands don't open files around it, the designcraft `io` module rule.

## 6. Tests (step C6 exit)

- **Claude Code:** `claude mcp add` lists the tools, and read-only tools are marked read-only (plan Stage 1.5 exit).
- **Scripted session:** a stdin/stdout transcript test covers `initialize`, `tools/list` (names, the hints match the effects), `command_run nav.open` then `doc_inspect`, a bad argument (`-32602`), a non-JSON line (`-32700`, still serving), `file.run` refused, and a write refused without roots.
