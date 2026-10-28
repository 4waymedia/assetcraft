//! Which mounts are network shares (Folders panel → System → Network; docs/design/folders-panel.md
//! F1). Pure parsers over the OS's mount tables, so every platform's rules are tested everywhere;
//! [`network_mounts`] runs the right one. Treats the text as hostile: unknown lines are skipped.
//!
//! | OS      | Source                         | A network mount is                                        |
//! |---------|--------------------------------|-----------------------------------------------------------|
//! | macOS   | `/sbin/mount` output           | type `smbfs`, `afpfs`, `nfs`, `webdav`, `cifs`, `ftp`                          |
//! | Linux   | `/proc/mounts`                 | type `cifs`, `smb3`, `nfs`, `nfs4`, `fuse.sshfs`, `davfs`                      |
//! | Windows | `net use` output               | a drive letter mapped to a `\\server\share`               |

use std::path::PathBuf;

/// File-system types that are network shares.
pub const NETWORK_TYPES: [&str; 12] =
    ["smbfs", "afpfs", "nfs", "nfs4", "webdav", "cifs", "smb3", "smb2", "fuse.sshfs", "sshfs", "davfs", "ftp"];

fn is_network(fstype: &str) -> bool {
    let t = fstype.trim().to_ascii_lowercase();
    NETWORK_TYPES.contains(&t.as_str())
}

/// macOS `mount` output: `//paul@nas/Share on /Volumes/Share (smbfs, nodev, nosuid, mounted by paul)`.
pub fn macos_network(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once(" on ")?;
            let (path, opts) = rest.rsplit_once(" (")?;
            let fstype = opts.split([',', ')']).next()?;
            (is_network(fstype) && path.starts_with('/')).then(|| PathBuf::from(path))
        })
        .collect()
}

/// Linux `/proc/mounts`: `server:/export /mnt/nas nfs4 rw,relatime 0 0` (spaces in paths are `\040`).
pub fn linux_network(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let (_src, path, fstype) = (f.next()?, f.next()?, f.next()?);
            (is_network(fstype) && path.starts_with('/')).then(|| PathBuf::from(unescape_octal(path)))
        })
        .collect()
}

/// `\040` → space (the kernel escapes space, tab, newline and backslash in `/proc/mounts`).
fn unescape_octal(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 4], 8)
        {
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Windows `net use` output: the drive letters (`Z:`) mapped to `\\server\share`. The status and
/// network columns are localized, so only the `X:` + `\\` pattern is trusted.
pub fn windows_mapped(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let at = words.iter().position(|w| {
                let b = w.as_bytes();
                b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
            })?;
            words.get(at + 1).filter(|w| w.starts_with("\\\\"))?;
            Some(words[at].to_ascii_uppercase())
        })
        .collect()
}

/// Network mount points on this machine (empty when the table can't be read).
#[cfg(not(windows))]
pub fn network_mounts() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        std::process::Command::new("/sbin/mount")
            .output()
            .map(|o| macos_network(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default()
    } else {
        std::fs::read_to_string("/proc/mounts").map(|t| linux_network(&t)).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_mount_output() {
        let text = "/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)\n\
                    devfs on /dev (devfs, local, nobrowse)\n\
                    //paul@nas._smb._tcp.local/Media on /Volumes/Media (smbfs, nodev, nosuid, mounted by paul)\n\
                    nas:/export/art on /Volumes/Art Share (nfs, asynchronous)\n\
                    /dev/disk5s1 on /Volumes/USB Stick (msdos, local, nodev, nosuid, noowners)\n\
                    garbage line\n";
        assert_eq!(macos_network(text), [PathBuf::from("/Volumes/Media"), PathBuf::from("/Volumes/Art Share")]);
    }

    #[test]
    fn linux_proc_mounts() {
        let text = "/dev/nvme0n1p2 / ext4 rw 0 0\n\
                    //nas/media /mnt/media cifs rw,vers=3.0 0 0\n\
                    nas:/art /home/paul/art\\040share nfs4 rw 0 0\n\
                    paul@box:/ /home/paul/box fuse.sshfs rw 0 0\n\
                    tmpfs /tmp tmpfs rw 0 0\n\
                    short\n";
        assert_eq!(
            linux_network(text),
            [PathBuf::from("/mnt/media"), PathBuf::from("/home/paul/art share"), PathBuf::from("/home/paul/box")]
        );
    }

    #[test]
    fn windows_net_use() {
        let text = "New connections will be remembered.\r\n\r\n\
                    Status       Local     Remote                    Network\r\n\
                    -------------------------------------------------------------------------------\r\n\
                    OK           Z:        \\\\nas\\media                Microsoft Windows Network\r\n\
                    Nicht verfügbar  y:    \\\\server\\art               Microsoft Windows Network\r\n\
                    OK                     \\\\server\\ipc$              Microsoft Windows Network\r\n\
                    The command completed successfully.\r\n";
        assert_eq!(windows_mapped(text), ["Z:", "Y:"]);
    }
}
