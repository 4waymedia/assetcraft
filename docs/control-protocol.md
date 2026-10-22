# Control protocol

_2026-10-22 · Stage 1.5 (API) · spec v1: not built yet (steps C5, C5b in [cli.md](cli.md) §4)_

- **Scope:** the opt-in loopback channel that lets scripts, agents and PipelineCraft drive a
  *running* AssetCraft window, and the headless `assetcraft-cli serve` that speaks the same
  protocol. The command-line surface is [cli.md](cli.md); MCP is [mcp.md](mcp.md).
- **Project goal it serves:** the same wire format as the other Crafting Apps, so one client drives
  every app (plan Stage 1.5 exit: a PowerShell `TcpClient` runs `nav.open` and `app.state`).
- **Immediate area of improvement:** today nothing outside the window can reach a running
  AssetCraft. `assetcraft-cli exec` starts a fresh headless session per call.

Suite sources (2026-10-22): designcraft `docs/control-protocol.md` @ `5aeb4dd7`, effectcraft @
`2c08f675`, filmcraft @ `256151ae`, soundcraft @ `8198d6e`. The wire format below is the one all
four share. AssetCraft-only additions are marked **(AC)**.

## 1. Starting it

```text
assetcraft --control <port|0> [--control-token-file <file>]     the app, with its window
assetcraft-cli serve --control <port|0> [--dir <folder>] [--automation-read-root D] [--automation-write-root D]
ASSETCRAFT_CONTROL_PORT=<port>                                   same as --control; the flag wins
```

- **Opt-in:** off unless asked for, and loopback only (`127.0.0.1`). Only enable it while you use it.
- **Bad port:** a bad or busy port exits 1 before any window opens.
- **Port 0:** asks the OS for a free port. The real port is printed on stdout as `ASSETCRAFT_CONTROL_PORT=<n>` (the soundcraft convention) and logged.
- **Manifest (AC):** while the server runs, `%LOCALAPPDATA%\CraftApps\run\assetcraft-<pid>.json` holds `{app, app_version, pid, port, protocol: 1, token_required}`. It is written atomically and removed on exit, so PipelineCraft can find running apps (plan Stage 1.5).
- **Isolation for tests and agents:**
  - `ASSETCRAFT_CONFIG_DIR=<dir>` (an empty value means no prefs file);
  - `ASSETCRAFT_NO_PREFS=1`;
  - `ASSETCRAFT_CACHE_DIR` (already exists).
  - Your own prefs and favorites are never touched by an automated run.
- **Focus:** the window opens without taking focus. Only `ui.focus` brings it forward.

## 2. Wire format (suite, unchanged)

One JSON object per line in each direction, UTF-8, and replies come back in request order on a connection. Blank lines are skipped.

```text
→ {"id": 1, "method": "engine.execute", "params": {"command": "nav.open", "params": {"path": "F:\\Art"}}}
← {"id": 1, "ok": true, "result": {"location": "F:\\Art", …}}
→ {"id": 2, "method": "engine.execute", "params": {"command": "nope"}}
← {"id": 2, "ok": false, "error": "unknown command `nope`"}
```

- **Request:** `{"id"?: any, "method": string, "params"?: object}`. `params` defaults to `{}`, and `id` is echoed back unchanged.
- **Reply:** `{"id", "ok": true, "result"}` or `{"id", "ok": false, "error": "<message>"}`. Errors are plain strings.

Only requests are read (designcraft `docs/control-protocol.md:18-26`; the same text is in filmcraft and effectcraft). The connection gets one error reply ending "closing the connection" and is then closed when:
- a line isn't a JSON object, isn't UTF-8, has no `method`, or is longer than **4 MiB**;
- a line looks like HTTP, so a browser page can't smuggle a request.

Server limits:
- at most **16** connections;
- a request has **60 s** to finish;
- `ui.screenshot` fails after 10 s if no frame was presented.

## 3. Methods

| Method                          | Params                                  | Result / notes                                                                                     | Step |
|---------------------------------|-----------------------------------------|----------------------------------------------------------------------------------------------------|------|
| `engine.execute`                | `{command, params?}`                    | The command's JSON, exactly as `assetcraft-cli exec` prints it. Never opens a dialog                 | C5   |
| `engine.commands`               | `{filter?}`                             | Same array as `assetcraft-cli commands --json` (suite shape, cli.md §3.3)                           | C5   |
| `app.quit`                      | `{}`                                    | Closes the app (prefs saved, as File → Exit)                                                       | C5   |
| `ui.menu.list`                  | `{}`                                    | The menu model (`menu::menus`): titles, labels, enabled, checked, shortcuts                        | C5b  |
| `ui.menu.invoke`                | `{path: ["Tools","Batch Rename…"]}`     | Like a click; may open a dialog                                                                    | C5b  |
| `ui.inspect`                    | `{}`                                    | UI state: open dialogs, panels, mode, focused widget                                               | C5b  |
| `ui.elements`                   | `{prefix?}`                             | Widgets with stable automation ids and their rects                                                 | C5b  |
| `ui.click` `ui.move` `ui.drag`  | `{id}` or `{x, y}` (+ `to` for drag)    | Synthetic pointer input                                                                            | C5b  |
| `ui.key` `ui.type`              | `{key, modifiers?}` / `{text}`          | Synthetic keys and text                                                                            | C5b  |
| `ui.screenshot`                 | `{path?}`                               | PNG of the window; base64 when `path` is left out. `path` is a new file only                        | C5b  |
| `ui.resize` `ui.focus`          | `{width, height}` / `{}`                |                                                                                                    | C5b  |

- **Same commands everywhere:** the UI, CLI, MCP and this channel all go through `Session::execute`, so `engine.commands` here equals `assetcraft-cli commands --json` (plan Stage 1.5 exit).
- **Remote doors:**
  - A command with `remoteAllowed: false` (`file.run` today) is refused here, whatever the auth **(AC)**.
  - `write-user-file` commands follow `--automation-write-root` in `serve`, as in cli.md §3.4. The app's own `--control` acts for the user at the keyboard and is not confined, which is decision D-A4 in cli.md.

## 4. Auth (decision D-A1)

The suite has no auth: designcraft says "only enable it while you use it". AssetCraft's plan text adds a token. Recommended rule (**Paul to confirm**):

- `--control` alone: no auth, so suite clients work unchanged.
- `--control-token-file F`: the token is read from `F` (or generated and written to `F`, never logged).
  - The first frame must be `{"method":"auth","params":{"token":"…"}}`.
  - Anything else, or more than 4 KiB before auth, gets one error and the connection closes.
  - The manifest says `token_required: true`.
  - PipelineCraft always launches this way.

## 5. Building blocks (reuse before build)

| Need                                    | From                                                                     | How                                         |
|-----------------------------------------|--------------------------------------------------------------------------|---------------------------------------------|
| Guarded NDJSON server with deadlines    | VectorCraft `apps/vectorcraft/src/control_server.rs`                     | Copied into `crates/craft-control-server` (L1); NOTICE line |
| Token handshake                         | PhotoCraft `crates/automation/src/security.rs`                           | Copied; NOTICE line                         |
| Manifest write (stage + rename)         | PdfCraft `apps/pdfcraft/src/main.rs`                                     | Same pattern                                |
| Server thread → UI thread               | every sibling: a channel drained once per frame                           | `Navigator::run`, so the menu, keys and remote all take one path |

## 6. Examples (PowerShell)

```powershell
Set-Location F:\Script-Projects\ArtCrafting\assetcraft
Start-Process .\target\release\assetcraft.exe -ArgumentList '--control','49152'
$c = [Net.Sockets.TcpClient]::new('127.0.0.1', 49152)
$s = $c.GetStream(); $w = [IO.StreamWriter]::new($s); $r = [IO.StreamReader]::new($s); $w.AutoFlush = $true
$w.WriteLine('{"id":1,"method":"engine.execute","params":{"command":"nav.open","params":{"path":"F:\\Script-Projects\\ArtCrafting\\assetcraft\\test_data\\library"}}}')
$r.ReadLine()
$w.WriteLine('{"id":2,"method":"engine.execute","params":{"command":"app.state"}}')
$r.ReadLine()
$c.Close()
```

## 7. Tests (step C5 exit)

- A fake client over a real socket covers a round trip, hostile frames (non-JSON, an HTTP line, 4 MiB + 1, a 17th connection), a deadline, and the token being refused or accepted.
- `engine.commands` equals `commands --json`.
- The manifest is written and then removed.
- `file.run` is refused remotely.
- `--control 0` prints the real port.
