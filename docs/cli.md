# AssetCraft CLI (`assetcraft-cli`)

_2026-10-22 · Stage 1.5 (API) · spec v1: §2 is shipped, §3 is the target, built in steps C1–C8_

- **Scope:** the command-line surface of AssetCraft: subcommands, flags, parameter syntax, output,
  exit codes and file safety. It sits beside [control-protocol.md](control-protocol.md) (the running
  app), [mcp.md](mcp.md) (agents over stdio) and [agents.md](agents.md) (how to drive and develop
  the app).
- **Project goal it serves:** anything the UI can do is scriptable, in the same shape as the other
  Crafting Apps, so one script, agent or PipelineCraft recipe can drive every app in the suite (plan
  Stage 1.5, AGENTS.md rule 3: everything is a command).
- **Immediate area of improvement:** today the CLI runs one command per process (`exec`), with no
  `describe`, `run`, `inspect`, `mcp` or `serve`. Also, `commands --json` doesn't yet have the suite
  shape (`menu` as a path, `enabled`).

## 1. The suite ethos this follows

Surveyed 2026-10-22 from designcraft `5aeb4dd7`, effectcraft `2c08f675`, filmcraft `256151ae` and
soundcraft `8198d6e`, plus the help and command lists captured in `ArtCrafting\cli-matrix\`.
FilmCraft is the newest template (effectcraft adopted its conventions as "FilmCraft #28").

| Rule                                                                                   | Where the suite does it                                         |
|----------------------------------------------------------------------------------------|-----------------------------------------------------------------|
| Every CLI run is a headless engine; the CLI never has logic the UI lacks               | all four                                                        |
| Discovery: `commands [filter] [--json]` and `describe <id>`                            | all four                                                        |
| `exec <id> [key=value … \| '<json>']`: values parse as JSON when they can              | filmcraft `apps/filmcraft-cli/src/main.rs:113-116`              |
| Batch: `run <script.jsonl \| ->`, one `{"id","params"}` per line, `--keep-going`       | filmcraft, designcraft (`script`), soundcraft (`script`)        |
| `mcp`: MCP server on stdio                                                              | all four                                                        |
| The running app: global `--bridge 127.0.0.1:PORT`                                       | filmcraft, effectcraft (`--connect` in designcraft, soundcraft) |
| JSON on stdout, pretty by default, `--compact` for one line; diagnostics on stderr as `<app>-cli: …` | filmcraft                                       |
| Exit codes: 0 ok, 1 a command failed, 2 usage error                                    | filmcraft `main.rs:118-140`, effectcraft `main.rs:299-301`      |
| A closed stdout pipe is not an error; the work still finishes                          | all four                                                        |
| `--version` / `-V` print `<app>-cli X.Y.Z`                                              | designcraft, effectcraft, filmcraft                             |
| Unknown flags are refused before anything runs                                         | effectcraft `main.rs:225-230`                                   |
| File access for agents confined with `--automation-read-root` / `--automation-write-root` | designcraft `apps/designcraft-cli/src/automation_args.rs:1-47`, soundcraft |

AssetCraft adds one rule the siblings don't have, from AGENTS.md rule 5 ("batch operations have a
dry run"): **every command that writes the user's files can be previewed without writing**, and
the CLI exposes that as `--dry-run` (§3.4).

## 2. Shipped today (`main` @ `0887ed8`)

| Subcommand                                         | What                                                      | Engine path                     |
|----------------------------------------------------|-----------------------------------------------------------|---------------------------------|
| `list <dir> [--json] [--sort k] [--desc] [--hidden]` | A folder as the navigator shows it                      | `view.*`, `nav.open`, `app.state` |
| `probe <file> [--json]`                            | Type, dimensions, size, times, header status              | `item.probe`                    |
| `thumb <file> --out <png> [--size px] [--force] [--heic]` | A thumbnail PNG (atomic write, never overwrites without `--force`) | `assetcraft_thumbs::render` (the only direct call) |
| `analyze <audio>… [--json]`                        | Duration, rate, channels, peak, RMS                       | `track.summary`                 |
| `commands [--json]`                                | Every command                                             | `app.commands`                  |
| `exec <id> [<json>] [--dir <folder>]`              | One command; pretty JSON result                           | `Session::execute`              |
| `help [<cmd>]`, `-h`, `--help`, `-V`, `--version`  | Help to stdout, exit 0                                    | —                               |

Already in line with the suite: exit codes 0/1/2, stdout/stderr split, closed-pipe handling,
`--` ends flags, non-UTF-8 arguments refused with exit 2 (`apps/assetcraft-cli/src/main.rs:1-12`,
`args.rs`).

## 3. Target surface

```text
assetcraft-cli [GLOBAL OPTIONS] <SUBCOMMAND> [ARGS]

SUBCOMMANDS
  exec <id> [key=value … | '<json>']   run one command; prints its result as JSON
  run <script.jsonl | ->               run commands, one {"id","params"} per line (# comments ok)
  commands [filter] [--json]           list commands (id, label, menu, shortcut, params, effects, enabled)
  describe <id>                        one command as JSON, with `enabled` / `disabledReason` now
  inspect                              app.state as JSON (location, selection, view, items)
  list <dir> [--json] [--sort k] [--desc] [--hidden]
  probe <file> [--json]
  thumb <file> --out <png> [--size px] [--force] [--heic]
  analyze <audio>… [--json]
  mcp                                  MCP server on stdio (headless, or --bridge to the live app)
  serve --control <port|0>             headless control server (control-protocol.md)
  help [<subcommand>] | --help | -V | --version | version

GLOBAL OPTIONS
  --dir <folder>                       open this folder first (headless; default: Pictures, else home)
  --bridge <127.0.0.1:PORT>            send commands to the running app (`assetcraft --control PORT`)
  --compact                            one-line JSON output
  --dry-run                            exec / run: preview writes, change nothing (§3.4)
  --keep-going                         run: report failing lines and continue
  --automation-read-root <dir>         mcp / serve: files agents may read (repeatable)
  --automation-write-root <dir>        mcp / serve: files agents may write (repeatable)
```

### 3.1 Parameters

- `exec <id> '<json>'`: one JSON object, as today.
- `exec <id> key=value …` (FilmCraft rule):
  - each value is parsed as JSON when it can be (`3.5`, `true`, `[1,2]`, `{"a":1}`), otherwise it's a string;
  - dotted keys nest (`cut.seconds=60` → `{"cut":{"seconds":60}}`);
  - a repeated key is a usage error.
- Unknown parameter keys are **refused**, and the error names the accepted keys. That matches the strict apps (effectcraft, filmcraft, designcraft's `engine.execute`), and AssetCraft's handlers already reject bad values.
- Paths in parameters are taken as given, and relative paths resolve against the working directory once, at the CLI boundary (the `nav.open` rule).

### 3.2 Output

| Case                                         | stdout                                   | stderr                              | Exit |
|----------------------------------------------|------------------------------------------|-------------------------------------|------|
| Success                                      | Result JSON (pretty; `--compact` → one line) | nothing                         | 0    |
| Command failed (`CmdError::Fs`, `Unavailable`) | nothing                                | `assetcraft-cli: <id>: <message>`   | 1    |
| Unknown id, bad params, unknown flag         | nothing                                  | message + usage or "see `commands`" | 2    |
| `run`, a line fails, no `--keep-going`        | results so far, one JSON per line        | `line N: …`                         | 1    |
| `run --keep-going`                           | every result; failures as `{"line","error"}` | summary                          | 1 if any failed |
| Closed pipe (`… \| head`)                     | dropped                                  | —                                   | 0 (work completes) |

`list`, `commands` and `analyze` keep their human tables without `--json`; everything else prints JSON.

### 3.3 `commands --json` and `describe` (suite shape)

```json
{
  "id": "file.renamePreview",
  "label": "Batch Rename",
  "menu": ["Tools"],
  "shortcut": null,
  "params": "{template, start?: 0.., paths?: [path]} …",
  "enabled": true,
  "disabledReason": null,
  "effects": { "classes": ["read"], "destructive": false, "undoable": false, "idempotent": true,
               "confirm": false, "source": "declared", "remoteAllowed": true }
}
```

Changes from today:
- `menu` becomes a path array, like filmcraft and effectcraft.
- `enabled` / `disabledReason` are added (plan Stage 1.5 step 2).
- `effects` is as built on 2026-10-15.
- `commands <filter>` matches the id or the label, case-insensitively.

### 3.4 Dry runs and file safety (AssetCraft rule)

- A command whose effects include `write-user-file` must have a preview path:
  - either a `dryRun: true` parameter that returns exactly what would happen,
  - or a separate read-only preview command (`file.renamePreview` for `file.rename`).
- `--dry-run` on `exec` / `run`:
  - sets `dryRun: true` on such commands;
  - routes `file.rename` to its preview;
  - refuses with exit 2 any write command that has no preview;
  - leaves read and app-state commands unchanged.
- The CLI never overwrites a user file without an explicit flag (`thumb --force`, as today), and every write is atomic: temp file, then rename.
- `--automation-write-root`:
  - When it is set, `write-user-file` commands outside the roots are refused before they run.
  - Without it, `mcp` and `serve` refuse every `write-user-file` command. The local CLI (`exec`, `run`) is the user at their own shell and is not confined.

### 3.5 Version

`assetcraft-cli 0.0.1 (assetcraft-engine)` today. Target: `assetcraft-cli X.Y.Z` on line one (suite shape), with the build commit on line two when known.

## 4. Build steps

| Step | Delivers                                                                                         | Plan link              | Owner  | Status |
|------|--------------------------------------------------------------------------------------------------|------------------------|--------|--------|
| C1   | `commands --json` suite shape (`menu` array, `enabled`, `disabledReason`), `commands <filter>`, `describe <id>` | Stage 1.5 step 2 | Claude | next   |
| C2   | `exec key=value`, global `--dir` / `--compact`, `inspect`, strict unknown-key errors                 | new                    | Claude |        |
| C3   | `run <jsonl \| ->`, `--keep-going`                                                                 | new                    | Claude |        |
| C4   | `--dry-run` and the write-command preview rule (test: every `write-user-file` command has a preview) | AGENTS.md rule 5     | Claude | with T2 (`file.rename`) |
| C5   | `crates/craft-control-proto` + `craft-control-server`; `assetcraft --control`, `serve`            | Stage 1.5 steps 3–4    | Claude |        |
| C6   | `mcp` (stdio), `--bridge`                                                                          | Stage 1.5 step 5       | Claude |        |
| C7   | `--automation-read-root` / `--automation-write-root`, one path gate in the engine                  | new (designcraft rule) | Claude |        |
| C8   | `thumb.render`, `asset.ref` commands; CLI `thumb` becomes `exec thumb.render`                       | Stage 1.5 step 6       | Claude |        |

## 5. Decisions

| ID   | Question                                   | Options                                                    | Recommendation                                                                                                         | Status |
|------|--------------------------------------------|------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------|--------|
| D-A1 | Control-channel auth (see control-protocol.md §4) | (a) none, like the suite; (b) token always (plan text); (c) none by default, token when `--control-token-file` is given | **(c)**: suite clients work unchanged, and PipelineCraft and the manifest get a token | Paul   |
| D-A2 | Flag for the running app                    | `--bridge` (filmcraft, effectcraft) / `--connect` (designcraft, soundcraft) | **`--bridge`**: the two newest apps, and the MCP conventions come from them                          | Paul   |
| D-A3 | Unknown parameter keys                      | ignore / refuse                                            | **Refuse**, naming the accepted keys                                                                                 | Paul   |
| D-A4 | Writes over MCP and `serve` without roots   | allow / refuse                                             | **Refuse** `write-user-file` until `--automation-write-root` is given: Stage 2 makes AssetCraft the first suite app that renames and writes sidecars next to the user's originals | Paul |
