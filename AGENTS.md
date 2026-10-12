# AGENTS.md: rules for coding agents

AssetCraft is a clean-room, pure-Rust asset app (browse, structure, organize, plan, execute) and a
sibling of the storytold Crafting Apps. Read this file, then `STATUS.md` (current stage and next
action), then `docs/assetcraft-plan.md`. Sibling patterns with file references are in
`docs/crafting-apps-notes.md`.

## Non-negotiables

1. **Rust only.** egui/eframe on wgpu for the desktop, the same Rust compiled to WASM for the web.
   Never add Tauri, Electron, a webview or any JS UI framework.
2. **Layering is enforced.** `cargo xtask layers` checks `xtask/src/layers.rs` `TABLE`. A crate may
   depend only on workspace crates in *lower* layers, and UI crates (`egui*`, `eframe`, `winit`,
   `rfd`, `craft-ui`) only from L6. A new crate needs a row in `TABLE` first.
3. **Everything is a command.** Every user action is a command with a stable id and JSON params,
   dispatched through one `Session::execute(id, params)`. The UI, keys, CLI, control channel and
   MCP all use it. The UI only draws.
4. **Never crash.** Workspace lints forbid `unsafe` and deny `unwrap`, `expect` and `panic` outside
   tests. Treat every file, path and number read from disk as hostile. Return errors; don't abort.
5. **Read before write.** Nothing writes to the user's files before Stage 2. After that, every
   write is atomic, batch operations have a dry run, and undo works.
6. **Clean room.** No Adobe or Microsoft code or assets, no GPL/LGPL/AGPL dependencies. Every bundled
   asset gets a row in `ATTRIBUTION.md`. Code adapted from a sibling app gets a provenance line in
   `NOTICE` (repo, path, revision).
7. **AI is optional.** Providers are chosen in Preferences (Local, API, ArtCraft account), and every
   feature works with none. Secrets live in the OS keyring, never in files. Elo is out of scope.

## Layers

| Layer | Crates            | Role                                                        |
|-------|-------------------|-------------------------------------------------------------|
| L0    | `fs`              | File-system access behind a `Services` trait (real and fake) |
| L1    | `probe`           | Type sniffing, dimensions, file times                       |
| L2    | `thumbs`          | Decode, downscale, worker pool, caches                      |
| L3    | (planned) `boards`, `interchange` | Storyline model, OTIO and DesignCraft output |
| L4    | (planned) `bridge` | Sessions with the sibling apps                             |
| L5    | `engine`          | Session, command registry, navigation state                 |
| L6    | `ui-egui`, (planned) `mcp` | Drawing, docking, MCP server                       |
| Apps  | `assetcraft`, `assetcraft-cli`, `xtask` | Binaries and tooling (exempt)         |

## Before you hand work back

```sh
cargo xtask ci     # fmt --check, clippy -D warnings, tests, layers
```

- Test against `test_data/library`. Every file there must either load or fail gracefully.
- Update `STATUS.md`: checkboxes, findings (scope, goal served, fix, owner) and the next action.
- **Never run git write commands.** Paul commits and pushes himself, so hand back a PowerShell block.
- Markdown tables are space-padded so columns line up in plain text.
