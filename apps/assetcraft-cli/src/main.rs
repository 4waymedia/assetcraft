//! AssetCraft headless command line.
//!
//! Every subcommand goes through an engine [`Session`] and its commands, so the CLI and the UI
//! can't disagree: `list` is `view.*` + `nav.open` + `app.state`, `probe` is `item.probe`,
//! `exec` runs any command by id. Only `thumb` calls `assetcraft_thumbs::render` directly (the
//! engine has no thumbnail command in Stage 0) and is the only code path that writes a file.
//!
//! Exit codes: 0 success, 1 the operation failed (I/O, unreadable, write refused), 2 usage error
//! (unknown subcommand, flag or command id, bad parameters). Results go to stdout, diagnostics
//! to stderr.

mod args;
mod text;

use args::{Parsed, Spec};
use assetcraft_engine::{CmdError, Prefs, RecordingLauncher, Session, commands};
use assetcraft_fs::RealFs;
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use text::{Align, human_size, iso_utc, table};

const HELP: &str = "\
assetcraft-cli: AssetCraft from the command line

USAGE:
    assetcraft-cli <COMMAND> [ARGS] [OPTIONS]

COMMANDS:
    list <dir>                 List a folder the way the navigator shows it
    probe <file>               File type, dimensions, size, times, header status
    thumb <file> --out <png>   Render a thumbnail to a PNG file
    commands                   List every engine command
    exec <id> [<json>]         Run one engine command headlessly; prints its JSON result
    help [<COMMAND>]           Help for one command

OPTIONS:
    -h, --help       Print help (also: <COMMAND> --help)
    -V, --version    Print version

For agents and scripts: `assetcraft-cli commands --json` lists every command id with its
parameters, and `assetcraft-cli exec <id> '<json>' [--dir <folder>]` runs one, so anything the
UI can do is scriptable. Exit codes: 0 ok, 1 failed, 2 usage error.
";

const LIST_HELP: &str = "\
assetcraft-cli list: list a folder the way the navigator shows it

USAGE:
    assetcraft-cli list <dir> [--json] [--sort name|date|size|type] [--desc] [--hidden]

OPTIONS:
    --json           JSON array of {name, path, kind, size, modified, created}, identical
                     to the `items` of app.state (times: unix seconds or null)
    --sort <key>     name (natural order, default), date, size or type; folders come first
    --desc           Descending order
    --hidden         Include hidden files
    -h, --help       Print help
";

const PROBE_HELP: &str = "\
assetcraft-cli probe: file type, dimensions, size, times and header status (engine item.probe)

USAGE:
    assetcraft-cli probe <file> [--json]

An unrecognized or damaged file is still a successful probe (exit 0, status and `unreadable`
say so). Exit 1 only when the file can't be read at all.

OPTIONS:
    --json           Print the item.probe JSON
    -h, --help       Print help
";

const THUMB_HELP: &str = "\
assetcraft-cli thumb: render a thumbnail to a PNG file

USAGE:
    assetcraft-cli thumb <file> --out <file.png> [--size <px>] [--force] [--heic]

OPTIONS:
    --out <png>      Where to write the PNG (must end in .png; never the input file;
                     written atomically: temp file, then rename)
    --size <px>      Longest side, 16..1024 (default 256; out-of-range values are clamped)
    --force          Overwrite --out if it exists (otherwise an existing file is never touched)
    --heic           Decode HEIF/HEIC (HEVC; off by default; needs a build with --features heic)
    -h, --help       Print help

Unsupported, unreadable or too-large files print a message on stderr and exit 1.
";

const COMMANDS_HELP: &str = "\
assetcraft-cli commands: list every engine command

USAGE:
    assetcraft-cli commands [--json]

OPTIONS:
    --json           Print the app.commands JSON (id, label, menu, shortcut, params)
    -h, --help       Print help
";

const EXEC_HELP: &str = "\
assetcraft-cli exec: run one engine command headlessly and print its JSON result

USAGE:
    assetcraft-cli exec <command-id> [<json-params>] [--dir <folder>]

A fresh session is created (opened at --dir first when given; otherwise it starts where the app
would: Pictures, else home). Params default to {}. file.open and file.reveal are recorded, never
launched. Use `--` before params that start with `-`.

EXAMPLES:
    assetcraft-cli exec app.state --dir test_data/library
    assetcraft-cli exec view.sort '{\"by\":\"size\",\"ascending\":false}' --dir .
    assetcraft-cli exec item.probe '{\"path\":\"shot.png\"}'

Exit codes: 0 ok, 1 the command failed, 2 unknown command id or bad params.

OPTIONS:
    --dir <folder>   Open this folder before running the command
    -h, --help       Print help
";

const LIST_SPEC: Spec = Spec { switches: &["--json", "--desc", "--hidden"], values: &["--sort"] };
const PROBE_SPEC: Spec = Spec { switches: &["--json"], values: &[] };
const THUMB_SPEC: Spec = Spec { switches: &["--force", "--heic"], values: &["--size", "--out"] };
const COMMANDS_SPEC: Spec = Spec { switches: &["--json"], values: &[] };
const EXEC_SPEC: Spec = Spec { switches: &[], values: &["--dir"] };

/// Default `thumb --size`.
const DEFAULT_THUMB_SIZE: u32 = 256;

/// Why a subcommand stopped.
#[derive(Debug)]
enum Failure {
    /// The operation failed (exit 1).
    Failed(String),
    /// Bad invocation (exit 2); the usage text to show.
    Usage(String, &'static str),
    /// Unknown engine command id or bad command params (exit 2).
    BadCommand(String),
}

type Outcome = Result<String, Failure>;

fn main() -> ExitCode {
    let mut argv = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.into_string() {
            Ok(s) => argv.push(s),
            Err(raw) => {
                eprintln!("assetcraft-cli: argument is not valid UTF-8: {}", raw.to_string_lossy());
                return ExitCode::from(2);
            }
        }
    }
    match run(&argv) {
        Ok(out) => {
            print_out(&out);
            ExitCode::SUCCESS
        }
        Err(Failure::Failed(msg)) => {
            eprintln!("assetcraft-cli: {msg}");
            ExitCode::from(1)
        }
        Err(Failure::Usage(msg, usage)) => {
            eprintln!("assetcraft-cli: {msg}\n\n{usage}");
            ExitCode::from(2)
        }
        Err(Failure::BadCommand(msg)) => {
            eprintln!("assetcraft-cli: {msg}\nSee `assetcraft-cli commands` for ids and parameters.");
            ExitCode::from(2)
        }
    }
}

/// Write to stdout, ignoring a closed pipe (`assetcraft-cli list . | head`).
fn print_out(s: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(s.as_bytes()).and_then(|()| out.flush());
}

fn run(argv: &[String]) -> Outcome {
    let Some((sub, rest)) = argv.split_first() else { return Ok(HELP.into()) };
    match sub.as_str() {
        "-h" | "--help" => Ok(HELP.into()),
        "-V" | "--version" => {
            Ok(format!("assetcraft-cli {} ({})\n", env!("CARGO_PKG_VERSION"), assetcraft_engine::CRATE))
        }
        "help" => match rest.first().map(String::as_str) {
            None => Ok(HELP.into()),
            Some(name) => sub_help(name).map(str::to_string).ok_or_else(|| unknown_subcommand(name)),
        },
        "list" => with(rest, &LIST_SPEC, LIST_HELP, list),
        "probe" => with(rest, &PROBE_SPEC, PROBE_HELP, probe),
        "thumb" => with(rest, &THUMB_SPEC, THUMB_HELP, thumb),
        "commands" => with(rest, &COMMANDS_SPEC, COMMANDS_HELP, list_commands),
        "exec" => with(rest, &EXEC_SPEC, EXEC_HELP, exec),
        other => Err(unknown_subcommand(other)),
    }
}

fn unknown_subcommand(name: &str) -> Failure {
    let what = if name.starts_with('-') { "flag" } else { "command" };
    Failure::Usage(format!("unknown {what} `{name}`"), HELP)
}

fn sub_help(name: &str) -> Option<&'static str> {
    Some(match name {
        "list" => LIST_HELP,
        "probe" => PROBE_HELP,
        "thumb" => THUMB_HELP,
        "commands" => COMMANDS_HELP,
        "exec" => EXEC_HELP,
        _ => return None,
    })
}

/// Parse, handle `--help`, then run `f` with usage errors tagged with `help`.
fn with(rest: &[String], spec: &Spec, help: &'static str, f: fn(&Parsed, &'static str) -> Outcome) -> Outcome {
    let parsed = args::parse(rest, spec).map_err(|m| Failure::Usage(m, help))?;
    if parsed.help {
        return Ok(help.into());
    }
    f(&parsed, help)
}

/// Exactly `n` positionals (or between `min` and `max`).
fn positionals<'a>(
    p: &'a Parsed,
    min: usize,
    max: usize,
    what: &str,
    help: &'static str,
) -> Result<&'a [String], Failure> {
    let n = p.positionals.len();
    if n < min {
        return Err(Failure::Usage(format!("missing {what}"), help));
    }
    if n > max {
        return Err(Failure::Usage(format!("unexpected argument `{}`", p.positionals[max]), help));
    }
    Ok(&p.positionals)
}

/// Absolute form of a user-given path (no symlink resolution), as the UI would hold it.
fn absolute(path: &str) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path))
}

/// A headless session: real disk, a launcher that never launches, starting at `start`.
fn session(start: Option<&Path>) -> Session {
    let prefs = Prefs { last_folder: start.map(Path::to_path_buf), ..Prefs::default() };
    Session::new(Arc::new(RealFs::new()), Arc::new(RecordingLauncher::default()), prefs)
}

/// Run a command; unknown id / bad params are usage errors, the rest are failures.
fn execute(s: &mut Session, id: &str, params: &Value) -> Outcome {
    match s.execute(id, params) {
        Ok(v) => Ok(pretty(&v)),
        Err(e @ (CmdError::Unknown(_) | CmdError::BadParams(_))) => Err(Failure::BadCommand(format!("{id}: {e}"))),
        Err(e) => Err(Failure::Failed(format!("{id}: {e}"))),
    }
}

fn pretty(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_else(|_| "null".into());
    s.push('\n');
    s
}

fn path_str(p: &Path) -> Result<&str, Failure> {
    p.to_str().ok_or_else(|| Failure::Failed(format!("path is not valid UTF-8: {}", p.display())))
}

// ---- list ----------------------------------------------------------------------------------

fn list(p: &Parsed, help: &'static str) -> Outcome {
    let dir = absolute(&positionals(p, 1, 1, "<dir>", help)?[0]);
    let mut s = session(Some(&dir));
    let ascending = !p.has("--desc");
    let by = p.value("--sort").unwrap_or("name");
    if !matches!(by, "name" | "date" | "size" | "type") {
        return Err(Failure::Usage(format!("unknown sort key `{by}` (name, date, size or type)"), help));
    }
    execute(&mut s, "view.sort", &json!({ "by": by, "ascending": ascending }))?;
    if p.has("--hidden") {
        execute(&mut s, "view.hidden", &json!({ "show": true }))?;
    }
    execute(&mut s, "nav.open", &json!({ "path": path_str(&dir)? }))?;
    let state = s.execute("app.state", &json!({})).map_err(|e| Failure::Failed(e.to_string()))?;
    if state.get("truncated").and_then(Value::as_bool) == Some(true) {
        eprintln!("assetcraft-cli: listing truncated at {} items", assetcraft_engine::MAX_ITEMS);
    }
    if let Some(n) = state.get("skipped").and_then(Value::as_u64).filter(|n| *n > 0) {
        eprintln!("assetcraft-cli: {n} entries could not be read and were skipped");
    }
    let items = state.get("items").cloned().unwrap_or(Value::Array(Vec::new()));
    if p.has("--json") {
        return Ok(pretty(&items));
    }
    Ok(list_table(items.as_array().map(Vec::as_slice).unwrap_or(&[])))
}

fn list_table(items: &[Value]) -> String {
    let mut folders = 0;
    let rows: Vec<Vec<String>> = items
        .iter()
        .map(|item| {
            let kind = item.get("kind").and_then(Value::as_str).unwrap_or("");
            let marker = match kind {
                "Dir" => {
                    folders += 1;
                    "DIR"
                }
                "Symlink" => "LINK",
                "Other" => "OTHER",
                _ => "",
            };
            let size = match (kind, item.get("size").and_then(Value::as_u64)) {
                ("File", Some(n)) => human_size(n),
                _ => "-".into(),
            };
            let modified = item.get("modified").and_then(Value::as_u64).map_or_else(|| "-".into(), iso_utc);
            let name = item.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            vec![marker.to_string(), size, modified, name]
        })
        .collect();
    let mut out = table(
        &["KIND", "SIZE", "MODIFIED (UTC)", "NAME"],
        &[Align::Left, Align::Right, Align::Left, Align::Left],
        &rows,
    );
    out.push_str(&format!("{} items ({folders} folders, {} files)\n", items.len(), items.len() - folders));
    out
}

// ---- probe ---------------------------------------------------------------------------------

fn probe(p: &Parsed, help: &'static str) -> Outcome {
    let file = absolute(&positionals(p, 1, 1, "<file>", help)?[0]);
    let mut s = session(file.parent());
    let result = s
        .execute("item.probe", &json!({ "path": path_str(&file)? }))
        .map_err(|e| Failure::Failed(format!("can't read {}: {e}", file.display())))?;
    if p.has("--json") {
        return Ok(pretty(&result));
    }
    let field = |k: &str| result.get(k).cloned().unwrap_or(Value::Null);
    let text = |k: &str| match field(k) {
        Value::String(s) => s,
        Value::Null => "-".into(),
        other => other.to_string(),
    };
    let dims = match (field("width").as_u64(), field("height").as_u64()) {
        (Some(w), Some(h)) => format!("{w} x {h}"),
        _ => "-".into(),
    };
    let size = field("size").as_u64().map_or_else(|| "-".into(), |n| format!("{} ({n} bytes)", human_size(n)));
    let modified = field("modified").as_u64().map_or_else(|| "-".into(), iso_utc);
    let created = field("created").as_u64().map_or_else(|| "-".into(), iso_utc);
    let rows = [
        ("path", text("path")),
        ("format", text("format")),
        ("category", text("category")),
        ("dimensions", dims),
        ("size", size),
        ("modified", modified),
        ("created", created),
        ("status", text("status")),
        ("unreadable", text("unreadable")),
        ("extension_mismatch", text("extension_mismatch")),
    ];
    Ok(rows.iter().map(|(k, v)| format!("{:<19} {v}\n", format!("{k}:"))).collect())
}

// ---- thumb ---------------------------------------------------------------------------------

fn thumb(p: &Parsed, help: &'static str) -> Outcome {
    let file = PathBuf::from(&positionals(p, 1, 1, "<file>", help)?[0]);
    let out = PathBuf::from(p.value("--out").ok_or_else(|| Failure::Usage("missing --out <file.png>".into(), help))?);
    let size = match p.value("--size") {
        None => DEFAULT_THUMB_SIZE,
        Some(v) => v.parse::<u32>().map_err(|_| Failure::Usage(format!("--size wants a number, got `{v}`"), help))?,
    };
    let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    let key = assetcraft_thumbs::ThumbKey::new(&file, size).with_modified(modified);
    if key.max_side != size {
        eprintln!("assetcraft-cli: --size {size} clamped to {}", key.max_side);
    }
    let is_png = out.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"));
    if !is_png {
        return Err(Failure::Usage(
            format!("--out must end in .png (the thumbnail is a PNG), e.g. {}", out.with_extension("png").display()),
            help,
        ));
    }
    // Never let --force overwrite the source image with its own thumbnail.
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(&file), std::fs::canonicalize(&out))
        && a == b
    {
        return Err(Failure::Failed(format!("--out {} is the input file itself", out.display())));
    }
    let force = p.has("--force");
    if !force && std::fs::symlink_metadata(&out).is_ok() {
        return Err(Failure::Failed(format!("{} already exists (use --force to overwrite)", out.display())));
    }
    if out.is_dir() {
        return Err(Failure::Failed(format!("{} is a folder", out.display())));
    }
    let heic = p.has("--heic");
    if heic && !assetcraft_thumbs::HEIC_BUILT {
        return Err(Failure::Failed(assetcraft_engine::NO_HEIC_BUILD.into()));
    }
    let options = assetcraft_thumbs::DecodeOptions { heic };
    let t = assetcraft_thumbs::render_with(&RealFs::new(), &key, &options)
        .map_err(|e| Failure::Failed(format!("{}: {e}", file.display())))?;
    let mut png = Vec::new();
    {
        use image::ImageEncoder;
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&t.rgba, t.width, t.height, image::ExtendedColorType::Rgba8)
            .map_err(|e| Failure::Failed(format!("can't encode PNG: {e}")))?;
    }
    write_atomic(&out, &png, force).map_err(|e| Failure::Failed(format!("can't write {}: {e}", out.display())))?;
    Ok(format!(
        "{}: {}x{} from {}x{} -> {}\n",
        file.display(),
        t.width,
        t.height,
        t.source_width,
        t.source_height,
        out.display()
    ))
}

/// Write `bytes` to a temp file next to `out`, then rename it over `out`. Without `force`, an
/// `out` that appeared meanwhile is left alone.
fn write_atomic(out: &Path, bytes: &[u8], force: bool) -> std::io::Result<()> {
    let dir = match out.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = out.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "thumb".into());
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        if force {
            return std::fs::rename(&tmp, out);
        }
        // Without --force, publish with a hard link: it fails if `out` exists, atomically, so a
        // file that appears while we write is never replaced. File systems without hard links
        // (FAT, some network shares) fall back to check-then-rename.
        match std::fs::hard_link(&tmp, out) {
            Ok(()) => std::fs::remove_file(&tmp),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "file appeared while writing"))
            }
            Err(_) => {
                if std::fs::symlink_metadata(out).is_ok() {
                    return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "file appeared while writing"));
                }
                std::fs::rename(&tmp, out)
            }
        }
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

// ---- commands / exec -----------------------------------------------------------------------

fn list_commands(p: &Parsed, help: &'static str) -> Outcome {
    positionals(p, 0, 0, "", help)?;
    if p.has("--json") {
        let mut s = session(None);
        return execute(&mut s, "app.commands", &json!({}));
    }
    let rows: Vec<Vec<String>> = commands()
        .iter()
        .map(|c| vec![c.id.into(), c.menu.into(), c.shortcut.unwrap_or("-").into(), c.params.into()])
        .collect();
    let mut out = table(&["ID", "MENU", "SHORTCUT", "PARAMS"], &[Align::Left; 4], &rows);
    out.push_str(&format!("{} commands. Run one with: assetcraft-cli exec <id> '<json-params>'\n", rows.len()));
    Ok(out)
}

fn exec(p: &Parsed, help: &'static str) -> Outcome {
    let pos = positionals(p, 1, 2, "<command-id>", help)?;
    let id = pos[0].as_str();
    if assetcraft_engine::find(id).is_none() {
        return Err(Failure::BadCommand(format!("unknown command `{id}`")));
    }
    let params: Value = match pos.get(1) {
        None => json!({}),
        Some(text) => {
            serde_json::from_str(text).map_err(|e| Failure::Usage(format!("params are not JSON: {e}"), help))?
        }
    };
    let dir = p.value("--dir").map(absolute);
    let mut s = session(dir.as_deref());
    if let Some(dir) = &dir {
        s.execute("nav.open", &json!({ "path": path_str(dir)? }))
            .map_err(|e| Failure::Failed(format!("can't open {}: {e}", dir.display())))?;
    }
    execute(&mut s, id, &params)
}
