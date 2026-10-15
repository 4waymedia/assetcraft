//! Opening files in other apps. Behind a trait so tests never spawn processes.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Hands files to the operating system.
pub trait Launcher: Send + Sync {
    /// Open `path` in its default app.
    fn open(&self, path: &Path) -> Result<(), String>;
    /// Show `path` selected in the system file manager.
    fn reveal(&self, path: &Path) -> Result<(), String>;
    /// Start `program` with `args` (one argv item each, no shell) and don't wait for it.
    /// Used by `file.openIn`. The default refuses, for launchers that can't start programs.
    fn spawn(&self, program: &Path, args: &[OsString]) -> Result<(), String> {
        let _ = args;
        Err(format!("can't start {}", program.display()))
    }
}

/// Extensions of files that run code when "opened": Windows programs, scripts and shortcuts,
/// plus Unix shell scripts and macOS apps. `file.open` reveals these instead of running them;
/// `file.run` runs them. Lower-case, without the dot. `appref-ms` is ClickOnce.
pub const PROGRAM_EXTENSIONS: &[&str] = &[
    "exe",
    "bat",
    "cmd",
    "com",
    "msi",
    "ps1",
    "vbs",
    "js",
    "jse",
    "wsf",
    "wsh",
    "scr",
    "lnk",
    "url",
    "hta",
    "cpl",
    "msc",
    "reg",
    "appref-ms",
    "sh",
    "command",
    "app",
];

/// `true` if `path`'s extension is in [`PROGRAM_EXTENSIONS`] (case-insensitive). The Unix
/// execute bit is checked separately, through `Services::is_executable`.
pub fn has_program_extension(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| PROGRAM_EXTENSIONS.contains(&e.as_str()))
}

/// The real launcher: Explorer on Windows, `open` on macOS, `xdg-open` elsewhere.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemLauncher;

/// What to ask the file manager for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchAction {
    /// Open in the default app.
    Open,
    /// Show selected in the file manager.
    Reveal,
}

/// The raw command line `explorer.exe` gets for `action` on `path`.
///
/// Explorer parses its own command line and splits unquoted arguments at commas, so
/// `C:\Art\shot,final.png` must reach it as `"C:\Art\shot,final.png"` (open) or
/// `/select,"C:\Art\shot,final.png"` (reveal), passed verbatim with `raw_arg` rather than
/// through Rust's default quoting. Windows file names can't contain `"`, so a path that does is
/// refused rather than mangled. Pure and cfg-independent so it is tested on every platform.
pub fn explorer_command_line(action: LaunchAction, path: &str) -> Result<String, String> {
    if path.contains('"') {
        return Err(format!("not a valid Windows path (contains a quote): {path}"));
    }
    // `/` can't appear in a Windows file name, so it is always a separator; Explorer only
    // understands `\`. A trailing `\` (a drive root like `C:\`) would read as `\"`, an escaped
    // quote, so it is dropped; a root has nothing to select, so Reveal on a root opens it.
    let path = path.replace('/', "\\");
    let trimmed = path.trim_end_matches('\\');
    let is_root = trimmed.is_empty() || trimmed.ends_with(':');
    if is_root {
        return Ok(format!("\"{trimmed}\\.\""));
    }
    Ok(match action {
        LaunchAction::Open => format!("\"{trimmed}\""),
        LaunchAction::Reveal => format!("/select,\"{trimmed}\""),
    })
}

impl SystemLauncher {
    fn launch(action: LaunchAction, path: &Path) -> Result<(), String> {
        let verb = if action == LaunchAction::Open { "open" } else { "reveal" };
        let cmd = command(action, path)?;
        Self::start(cmd).map_err(|e| format!("could not {verb} {}: {e}", path.display()))
    }

    /// Start `cmd` and reap it on a detached thread.
    fn start(mut cmd: std::process::Command) -> std::io::Result<()> {
        let mut child = cmd.spawn()?;
        // Reap the child when it exits so short-lived helpers (`open`, `xdg-open`) don't linger
        // as zombies; the thread only waits, and the app never waits on it.
        let _ = std::thread::Builder::new().name("assetcraft-launch-reaper".into()).spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

/// The command that starts a sibling app: the program itself (never `cmd.exe` or a shell),
/// each argument a separate argv item, no inherited standard streams, detached from our console
/// (Windows) or process group (Unix) so closing AssetCraft or pressing Ctrl+C doesn't end it.
fn app_command(program: &Path, args: &[OsString]) -> std::process::Command {
    let mut c = std::process::Command::new(program);
    c.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(dir) = program.parent().filter(|d| !d.as_os_str().is_empty()) {
        c.current_dir(dir);
    }
    detach(&mut c);
    c
}

#[cfg(windows)]
fn detach(c: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    c.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(unix)]
fn detach(c: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    c.process_group(0);
}

#[cfg(not(any(windows, unix)))]
fn detach(_c: &mut std::process::Command) {}

#[cfg(windows)]
fn command(action: LaunchAction, path: &Path) -> Result<std::process::Command, String> {
    use std::os::windows::process::CommandExt;
    let text = path.to_str().ok_or_else(|| format!("path is not valid Unicode: {}", path.display()))?;
    let mut c = std::process::Command::new("explorer.exe");
    c.raw_arg(explorer_command_line(action, text)?);
    Ok(c)
}

#[cfg(not(windows))]
fn command(action: LaunchAction, path: &Path) -> Result<std::process::Command, String> {
    let mut c;
    if cfg!(target_os = "macos") {
        c = std::process::Command::new("open");
        if action == LaunchAction::Reveal {
            c.arg("-R");
        }
        c.arg(path);
    } else {
        c = std::process::Command::new("xdg-open");
        c.arg(if action == LaunchAction::Reveal { path.parent().unwrap_or(path) } else { path });
    }
    Ok(c)
}

impl Launcher for SystemLauncher {
    fn open(&self, path: &Path) -> Result<(), String> {
        Self::launch(LaunchAction::Open, path)
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        Self::launch(LaunchAction::Reveal, path)
    }

    fn spawn(&self, program: &Path, args: &[OsString]) -> Result<(), String> {
        Self::start(app_command(program, args)).map_err(|e| format!("could not start {}: {e}", program.display()))
    }
}

/// Records calls instead of launching anything. For tests and headless use.
#[derive(Debug, Default)]
pub struct RecordingLauncher {
    /// `("open" | "reveal", path)` in call order.
    pub calls: Mutex<Vec<(&'static str, PathBuf)>>,
    /// `(program, args)` of each [`Launcher::spawn`], in call order.
    pub spawned: Mutex<Vec<(PathBuf, Vec<OsString>)>>,
}

impl Launcher for RecordingLauncher {
    fn open(&self, path: &Path) -> Result<(), String> {
        self.calls.lock().map_err(|e| e.to_string())?.push(("open", path.to_path_buf()));
        Ok(())
    }
    fn reveal(&self, path: &Path) -> Result<(), String> {
        self.calls.lock().map_err(|e| e.to_string())?.push(("reveal", path.to_path_buf()));
        Ok(())
    }
    fn spawn(&self, program: &Path, args: &[OsString]) -> Result<(), String> {
        self.spawned.lock().map_err(|e| e.to_string())?.push((program.to_path_buf(), args.to_vec()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_commands_run_the_program_directly_with_one_arg_per_path() {
        let args = [OsString::from("--"), OsString::from("/art/a b,c.png")];
        let c = app_command(Path::new("/opt/craft/filmcraft"), &args);
        assert_eq!(c.get_program(), "/opt/craft/filmcraft");
        assert_eq!(c.get_args().collect::<Vec<_>>(), ["--", "/art/a b,c.png"]);
        assert_eq!(c.get_current_dir(), Some(Path::new("/opt/craft")));
    }

    #[test]
    fn program_extensions_are_case_insensitive() {
        for name in ["setup.EXE", "run.bat", "x.Lnk", "tool.appref-ms", "deploy.sh", "Foo.app", "a.b.ps1"] {
            assert!(has_program_extension(Path::new(name)), "{name}");
        }
        for name in ["shot.png", "exe", "notes.txt", "archive.exe.bak", ".bashrc"] {
            assert!(!has_program_extension(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn explorer_paths_are_quoted_so_commas_do_not_split_them() {
        let open = explorer_command_line(LaunchAction::Open, "C:\\Art\\shot,final.png");
        assert_eq!(open.as_deref(), Ok("\"C:\\Art\\shot,final.png\""));
        let reveal = explorer_command_line(LaunchAction::Reveal, "C:\\Art\\shot,final.png");
        assert_eq!(reveal.as_deref(), Ok("/select,\"C:\\Art\\shot,final.png\""));
        let spaced = explorer_command_line(LaunchAction::Reveal, "D:\\My Art\\Réaper ünïcode, v2.png");
        assert_eq!(spaced.as_deref(), Ok("/select,\"D:\\My Art\\Réaper ünïcode, v2.png\""));
        let unc = explorer_command_line(LaunchAction::Open, "\\\\nas\\share\\a,b");
        assert_eq!(unc.as_deref(), Ok("\"\\\\nas\\share\\a,b\""));
        assert!(explorer_command_line(LaunchAction::Open, "C:\\bad\"name").is_err());
    }

    #[test]
    fn explorer_paths_use_backslashes_and_roots_never_end_in_a_quote_escape() {
        let mixed = explorer_command_line(LaunchAction::Reveal, "C:/Art/Reapers\\shot.png");
        assert_eq!(mixed.as_deref(), Ok("/select,\"C:\\Art\\Reapers\\shot.png\""));
        for root in ["C:\\", "C:/", "C:"] {
            for action in [LaunchAction::Open, LaunchAction::Reveal] {
                assert_eq!(explorer_command_line(action, root).as_deref(), Ok("\"C:\\.\""), "{root}");
            }
        }
        let folder = explorer_command_line(LaunchAction::Open, "C:\\Art\\");
        assert_eq!(folder.as_deref(), Ok("\"C:\\Art\""));
    }
}
