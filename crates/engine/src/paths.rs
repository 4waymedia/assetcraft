//! Path helpers for what users type or paste, and for comparing paths the way the OS does.

use std::path::Path;

/// Tidy a path the user typed or pasted, before it is made absolute:
/// - surrounding whitespace is trimmed, and one pair of matching surrounding quotes is removed
///   (Explorer's "Copy as path" yields `"C:\Art\shot.png"`);
/// - with `windows`, a bare drive `X:` becomes the drive root `X:\` (on Windows `X:` alone
///   means "the current folder on drive X", which is never what a path field means).
///
/// Pure and platform-independent (the caller passes `cfg!(windows)`), so both behaviours are
/// tested everywhere.
pub fn clean_user_path(raw: &str, windows: bool) -> String {
    let mut text = raw.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = text.strip_prefix(quote).and_then(|t| t.strip_suffix(quote)) {
            text = inner.trim();
            break;
        }
    }
    let bytes = text.as_bytes();
    if windows && bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return format!("{text}\\");
    }
    text.to_string()
}

/// `true` when `a` and `b` name the same path. On Windows the comparison ignores case (NTFS
/// and Explorer do); elsewhere it is exact.
pub fn same_path(a: &Path, b: &Path) -> bool {
    paths_equal(a, b, cfg!(windows))
}

/// The pure part of [`same_path`].
pub(crate) fn paths_equal(a: &Path, b: &Path, ignore_case: bool) -> bool {
    if a == b {
        return true;
    }
    ignore_case && a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_whitespace_are_trimmed() {
        assert_eq!(clean_user_path("  \"C:\\Art\\shot,final.png\" ", true), "C:\\Art\\shot,final.png");
        assert_eq!(clean_user_path("'/home/paul/Pictures'", false), "/home/paul/Pictures");
        assert_eq!(clean_user_path("\" /x \"", false), "/x");
        assert_eq!(clean_user_path("\"unbalanced", false), "\"unbalanced", "only a matching pair is removed");
        assert_eq!(clean_user_path("   ", false), "");
    }

    #[test]
    fn bare_drive_means_the_drive_root_on_windows_only() {
        assert_eq!(clean_user_path("C:", true), "C:\\");
        assert_eq!(clean_user_path(" \"d:\" ", true), "d:\\");
        assert_eq!(clean_user_path("C:\\Art", true), "C:\\Art");
        assert_eq!(clean_user_path("C:", false), "C:", "a plain relative name elsewhere");
        assert_eq!(clean_user_path("1:", true), "1:");
    }

    #[test]
    fn same_path_ignores_case_only_where_the_os_does() {
        let (a, b) = (Path::new("C:\\Art\\Reapers"), Path::new("c:\\art\\REAPERS"));
        assert!(paths_equal(a, b, true));
        assert!(!paths_equal(a, b, false));
        assert!(paths_equal(a, a, false));
        assert!(!paths_equal(Path::new("C:\\Art"), Path::new("C:\\Arts"), true));
    }
}
