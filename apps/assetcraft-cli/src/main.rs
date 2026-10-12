//! AssetCraft headless command line.
//!
//! Stage 0 step 1: `--version` and `--help` only. `list`, `probe`, `thumb` and `commands`
//! arrive in step 7 (`docs/assetcraft-plan.md`).

use std::process::ExitCode;

const HELP: &str = "\
assetcraft-cli: AssetCraft from the command line

USAGE:
    assetcraft-cli <COMMAND>

COMMANDS (Stage 0 step 7):
    list <dir> [--json]        List a folder the way the navigator shows it
    probe <file> [--json]      File type, dimensions and times
    thumb <file> --size <px> --out <png>
    commands                   List every engine command

OPTIONS:
    -h, --help       Print help
    -V, --version    Print version
";

fn main() -> ExitCode {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        None | Some("-h") | Some("--help") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some("-V") | Some("--version") => {
            println!("assetcraft-cli {} ({})", env!("CARGO_PKG_VERSION"), assetcraft_engine::CRATE);
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("assetcraft-cli: `{other}` is not available yet (Stage 0 step 7). Try --help.");
            ExitCode::from(2)
        }
    }
}
