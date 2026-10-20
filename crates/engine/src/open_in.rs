//! "Open in…" the sibling Crafting Apps: which app opens which files, where its program is
//! installed, and how it takes file paths on the command line.
//!
//! The catalog ([`SIBLING_APPS`]) comes from each sibling's own sources (see the per-app notes).
//! Every app's desktop binary takes files as plain positional arguments after its options and
//! opens each one; paths handed over here are always absolute, so none can be mistaken for an
//! option. FilmCraft and EffectCraft also understand `--`, so they get one first.
//!
//! Discovery ([`SystemLocator`]): the `ASSETCRAFT_<APP>_EXE` environment variable, then `PATH`,
//! then the folder the sibling's Windows installer uses (`%ProgramFiles%\<App>\<app>.exe`, all
//! per-machine MSIs; `%LOCALAPPDATA%\Programs\<App>\` is checked too for per-user copies). The
//! session caches the result and looks again on `nav.refresh` (F5) and `nav.computer`.
//! Launching goes through [`crate::Launcher::spawn`]: no shell, one argv item per path.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// How an app takes file paths on its command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgStyle {
    /// `app <file> <file> …`
    Positional,
    /// `app -- <file> <file> …` (the app treats everything after `--` as a file).
    AfterDoubleDash,
}

/// One sibling app AssetCraft can hand files to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiblingApp {
    /// Stable id, used in `file.openIn {app}` and the environment variable name.
    pub id: &'static str,
    /// Display name.
    pub label: &'static str,
    /// Program name without `.exe`.
    pub binary: &'static str,
    /// Folder name under `Program Files` (the WiX `INSTALLFOLDER` name).
    pub install_dir: &'static str,
    /// Lower-case extensions (no dot) the app opens from its command line.
    pub extensions: &'static [&'static str],
    /// `true` if the app also accepts folders (LightCraft imports them).
    pub folders: bool,
    /// How paths are passed.
    pub args: ArgStyle,
}

impl SiblingApp {
    /// The environment variable that overrides discovery, e.g. `ASSETCRAFT_PHOTOCRAFT_EXE`.
    pub fn env_var(&self) -> String {
        format!("ASSETCRAFT_{}_EXE", self.id.to_ascii_uppercase())
    }

    /// `true` if this app opens `path` by its extension (case-insensitive).
    pub fn opens_extension(&self, path: &Path) -> bool {
        path.extension()
            .and_then(OsStr::to_str)
            .map(str::to_ascii_lowercase)
            .is_some_and(|e| self.extensions.contains(&e.as_str()))
    }

    /// The argv (after the program) that opens `paths`.
    pub fn argv(&self, paths: &[PathBuf]) -> Vec<OsString> {
        let mut out = Vec::with_capacity(paths.len() + 1);
        if self.args == ArgStyle::AfterDoubleDash {
            out.push(OsString::from("--"));
        }
        out.extend(paths.iter().map(|p| p.as_os_str().to_owned()));
        out
    }
}

/// PhotoCraft: `photocraft-engine` `file_cmds.rs` `OPENABLE`, plus AVIF from its installer's
/// `SupportedTypes` (`packaging/windows/photocraft.wxs`).
const PHOTOCRAFT_EXTS: &[&str] = &[
    "pdn", "ora", "psd", "psb", "pcraft", "png", "jpg", "jpeg", "tif", "tiff", "webp", "gif", "bmp", "tga", "exr",
    "hdr", "qoi", "ico", "pnm", "ppm", "pgm", "fits", "fit", "fts", "heic", "heif", "hif", "dng", "cr2", "nef", "nrw",
    "arw", "pef", "svg", "svgz", "avif",
];

/// FilmCraft: `filmcraft-media` `STILL_EXTENSIONS`, `AUDIO_EXTENSIONS`, `VIDEO_EXTENSIONS`, plus
/// its `.fcproj` projects (`apps/filmcraft/src/args.rs`).
const FILMCRAFT_EXTS: &[&str] = &[
    "fcproj", "png", "jpg", "jpeg", "gif", "webp", "tif", "tiff", "bmp", "tga", "dpx", "exr", "dng", "wav", "wave",
    "bwf", "aif", "aiff", "mp3", "mp2", "m4a", "aac", "flac", "ogg", "opus", "mp4", "m4v", "mov", "mkv", "webm", "avi",
    "mxf", "mts", "m2ts", "m2t", "ts", "mpg", "mpeg", "vob", "mod", "tod", "m2v", "m1v", "mpv", "3gp", "y4m", "apv",
];

/// LightCraft: `lightcraft-engine` `import.rs` `EXTENSIONS` (files and folders are imported).
const LIGHTCRAFT_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "dng", "cr2", "cr3", "nef", "nrw", "arw", "raf", "orf", "rw2", "rwl",
    "raw", "pef", "srw", "psd", "jxl", "gif", "bmp", "heic", "heif", "avif", "iiq", "crw", "mrw", "x3f", "kdc", "mos",
    "erf", "3fr", "fff",
];

/// The sibling apps, in menu order. Sources (storytold repositories): `apps/<app>/src/main.rs`
/// for the command line, `packaging/windows/<app>.wxs` for the program name and install folder.
pub const SIBLING_APPS: &[SiblingApp] = &[
    SiblingApp {
        id: "photocraft",
        label: "PhotoCraft",
        binary: "photocraft",
        install_dir: "PhotoCraft",
        extensions: PHOTOCRAFT_EXTS,
        folders: false,
        args: ArgStyle::Positional,
    },
    SiblingApp {
        id: "lightcraft",
        label: "LightCraft",
        binary: "lightcraft",
        install_dir: "LightCraft",
        extensions: LIGHTCRAFT_EXTS,
        folders: true,
        args: ArgStyle::Positional,
    },
    SiblingApp {
        id: "filmcraft",
        label: "FilmCraft",
        binary: "filmcraft",
        install_dir: "FilmCraft",
        extensions: FILMCRAFT_EXTS,
        folders: false,
        args: ArgStyle::AfterDoubleDash,
    },
    // EffectCraft: projects only (`.ecproj`, and the `.ecprojx` XML copy it also opens as a
    // project). It imports stills and video too, but PhotoCraft and FilmCraft already serve those.
    // storytold/effectcraft 05955c0 (v0.10.0): `apps/effectcraft/src/main.rs:3` (usage, `--`),
    // `:50-55` (`is_project_file`); `packaging/windows/effectcraft.wxs:42` (App Paths, INSTALLFOLDER).
    SiblingApp {
        id: "effectcraft",
        label: "EffectCraft",
        binary: "effectcraft",
        install_dir: "EffectCraft",
        extensions: &["ecproj", "ecprojx"],
        folders: false,
        args: ArgStyle::AfterDoubleDash,
    },
    SiblingApp {
        id: "pdfcraft",
        label: "PdfCraft",
        binary: "pdfcraft",
        install_dir: "PdfCraft",
        extensions: &["pdf"],
        folders: false,
        args: ArgStyle::Positional,
    },
    SiblingApp {
        id: "designcraft",
        label: "DesignCraft",
        binary: "designcraft",
        install_dir: "DesignCraft",
        extensions: &["designcraft", "idml"],
        folders: false,
        args: ArgStyle::Positional,
    },
    SiblingApp {
        id: "wordcraft",
        label: "WordCraft",
        binary: "wordcraft",
        install_dir: "WordCraft",
        extensions: &["docx", "docm", "dotx", "dotm", "doc", "dot", "rtf", "odt", "md", "txt"],
        folders: false,
        args: ArgStyle::Positional,
    },
];

/// Look an app up by id.
pub fn find_app(id: &str) -> Option<&'static SiblingApp> {
    SIBLING_APPS.iter().find(|a| a.id == id)
}

/// Most files handed to an app at once (Windows command lines are capped at 32,767 chars).
pub const MAX_OPEN_IN_FILES: usize = 100;

/// Most characters of paths handed over in one launch.
pub const MAX_OPEN_IN_CHARS: usize = 30_000;

/// Finds the installed program of a sibling app. Behind a trait so tests never look at the
/// real machine.
pub trait AppLocator: Send + Sync {
    /// The program for `app`, if it is installed.
    fn locate(&self, app: &SiblingApp) -> Option<PathBuf>;
}

/// Where to look for `app`, in order, given an environment (`env`) and the platform. Pure, so
/// the Windows rules are tested everywhere.
pub fn candidates(app: &SiblingApp, env: &dyn Fn(&str) -> Option<OsString>, windows: bool) -> Vec<PathBuf> {
    let var = |name: &str| env(name).filter(|v| !v.is_empty());
    let exe = if windows { format!("{}.exe", app.binary) } else { app.binary.to_string() };
    let mut out = Vec::new();
    // Relative overrides and PATH entries (empty, `.`) are skipped: they would resolve against
    // whatever folder AssetCraft was started from.
    if let Some(path) = var(&app.env_var()).map(PathBuf::from).filter(|p| absolute_for(p, windows)) {
        out.push(path);
    }
    if let Some(path) = var("PATH") {
        out.extend(std::env::split_paths(&path).filter(|d| absolute_for(d, windows)).map(|d| d.join(&exe)));
    }
    if windows {
        // The siblings' MSIs are per-machine into ProgramFiles6432Folder; ProgramW6432 is the
        // 64-bit folder even when a 32-bit process reads it.
        for base in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(dir) = var(base) {
                out.push(PathBuf::from(dir).join(app.install_dir).join(&exe));
            }
        }
        if let Some(dir) = var("LOCALAPPDATA") {
            out.push(PathBuf::from(dir).join("Programs").join(app.install_dir).join(&exe));
        }
    }
    let mut seen = Vec::<PathBuf>::new();
    out.retain(|p| {
        let new = !seen.iter().any(|s| crate::same_path(s, p));
        if new {
            seen.push(p.clone());
        }
        new
    });
    out
}

/// `true` if `p` is a full path on the given platform: `X:\…`, `X:/…` or `\\server\…` on
/// Windows, a rooted path elsewhere. Textual, so the Windows rules are tested everywhere.
fn absolute_for(p: &Path, windows: bool) -> bool {
    if !windows {
        return p.has_root();
    }
    let text = p.to_string_lossy();
    let b = text.as_bytes();
    let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/');
    drive || text.starts_with("\\\\")
}

/// The real locator: [`candidates`] over this process's environment, first regular file wins.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemLocator;

impl AppLocator for SystemLocator {
    fn locate(&self, app: &SiblingApp) -> Option<PathBuf> {
        let env = |name: &str| std::env::var_os(name);
        candidates(app, &env, cfg!(windows)).into_iter().find(|p| p.is_file())
    }
}

/// A locator with a fixed answer, for tests and headless use.
#[derive(Debug, Default, Clone)]
pub struct FakeLocator {
    /// `(app id, program)` pairs that count as installed.
    pub installed: Vec<(&'static str, PathBuf)>,
}

impl AppLocator for FakeLocator {
    fn locate(&self, app: &SiblingApp) -> Option<PathBuf> {
        self.installed.iter().find(|(id, _)| *id == app.id).map(|(_, p)| p.clone())
    }
}

/// One entry of the Open In menu for a selection (see [`crate::Session::open_in_targets`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenInTarget {
    /// The app.
    pub app: &'static SiblingApp,
    /// Its program, when installed.
    pub program: Option<PathBuf>,
}

impl OpenInTarget {
    /// `true` if the app was found.
    pub fn installed(&self) -> bool {
        self.program.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |name| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| OsString::from(v))
    }

    #[test]
    fn catalog_ids_are_unique_and_lower_case() {
        for (i, a) in SIBLING_APPS.iter().enumerate() {
            assert!(SIBLING_APPS[i + 1..].iter().all(|b| b.id != a.id), "{}", a.id);
            assert_eq!(a.id, a.id.to_ascii_lowercase());
            assert!(a.extensions.iter().all(|e| *e == e.to_ascii_lowercase() && !e.starts_with('.')), "{}", a.id);
        }
        assert_eq!(find_app("photocraft").map(SiblingApp::env_var).as_deref(), Some("ASSETCRAFT_PHOTOCRAFT_EXE"));
        assert!(find_app("nope").is_none());
    }

    #[test]
    fn extensions_match_case_insensitively() {
        let pc = find_app("photocraft").map(|a| a.opens_extension(Path::new("/a/B.PSD")));
        assert_eq!(pc, Some(true));
        let pdf = find_app("pdfcraft")
            .map(|a| (a.opens_extension(Path::new("x.pdf")), a.opens_extension(Path::new("x.png"))));
        assert_eq!(pdf, Some((true, false)));
        assert_eq!(find_app("wordcraft").map(|a| a.opens_extension(Path::new("README"))), Some(false));
    }

    #[test]
    fn argv_is_one_item_per_path_and_filmcraft_gets_a_double_dash() {
        let paths = vec![PathBuf::from("/art/shot,final.png"), PathBuf::from("/art/a b \"c\".png")];
        let pc = find_app("photocraft").map(|a| a.argv(&paths));
        assert_eq!(pc, Some(vec![OsString::from("/art/shot,final.png"), OsString::from("/art/a b \"c\".png")]));
        let fc = find_app("filmcraft").map(|a| a.argv(&paths[..1]));
        assert_eq!(fc, Some(vec![OsString::from("--"), OsString::from("/art/shot,final.png")]));
    }

    #[test]
    fn effectcraft_opens_its_projects_only_and_gets_a_double_dash() {
        let ec = find_app("effectcraft");
        assert_eq!(ec.map(SiblingApp::env_var).as_deref(), Some("ASSETCRAFT_EFFECTCRAFT_EXE"));
        let opens = |p: &str| ec.is_some_and(|a| a.opens_extension(Path::new(p)));
        assert!(opens("/art/Title.ecproj") && opens("/art/Title copy.ECPROJX"));
        assert!(!opens("/art/plate.png") && !opens("/art/shot.mp4") && !opens("/art/Title.ecproj.bak"));
        assert_eq!(ec.map(|a| a.folders), Some(false));
        let argv = ec.map(|a| a.argv(&[PathBuf::from("/art/Title.ecproj")]));
        assert_eq!(argv, Some(vec![OsString::from("--"), OsString::from("/art/Title.ecproj")]));
    }

    #[test]
    fn windows_candidates_follow_override_path_then_install_folders() {
        let env = env_of(&[
            ("ASSETCRAFT_PHOTOCRAFT_EXE", "D:\\Tools\\pc.exe"),
            ("ProgramFiles", "C:\\Program Files"),
            ("ProgramW6432", "C:\\Program Files"),
            ("LOCALAPPDATA", "C:\\Users\\paul\\AppData\\Local"),
        ]);
        let Some(app) = find_app("photocraft") else { return };
        let got = candidates(app, &env, true);
        let want = [
            PathBuf::from("D:\\Tools\\pc.exe"),
            PathBuf::from("C:\\Program Files").join("PhotoCraft").join("photocraft.exe"),
            PathBuf::from("C:\\Users\\paul\\AppData\\Local").join("Programs").join("PhotoCraft").join("photocraft.exe"),
        ];
        assert_eq!(got, want, "duplicate Program Files entries are dropped");
    }

    #[test]
    fn non_windows_candidates_use_path_only_and_skip_relative_entries() {
        let (a, b) = (std::env::temp_dir().join("a"), std::env::temp_dir().join("b"));
        let path = std::env::join_paths([a.clone(), PathBuf::from("relative"), b.clone()]).unwrap_or_default();
        let env = move |name: &str| match name {
            "PATH" => Some(path.clone()),
            "ProgramFiles" => Some(OsString::from("C:\\x")),
            _ => None,
        };
        let Some(app) = find_app("lightcraft") else { return };
        assert_eq!(candidates(app, &env, false), [a.join("lightcraft"), b.join("lightcraft")]);
        let empty = |name: &str| (name == "ASSETCRAFT_LIGHTCRAFT_EXE").then(OsString::new);
        assert!(candidates(app, &empty, false).is_empty(), "an empty override is ignored");
        let relative = |name: &str| (name == "ASSETCRAFT_LIGHTCRAFT_EXE").then(|| OsString::from("bin/lightcraft"));
        assert!(candidates(app, &relative, false).is_empty(), "a relative override is ignored");
        assert!(candidates(app, &relative, true).is_empty());
    }

    #[test]
    fn windows_full_paths_are_drive_or_unc_paths() {
        for ok in ["C:\\Tools\\pc.exe", "d:/x/pc.exe", "\\\\nas\\apps\\pc.exe"] {
            assert!(absolute_for(Path::new(ok), true), "{ok}");
        }
        for bad in ["pc.exe", ".\\pc.exe", "C:pc.exe", "\\Tools\\pc.exe", ""] {
            assert!(!absolute_for(Path::new(bad), true), "{bad}");
        }
    }
}
