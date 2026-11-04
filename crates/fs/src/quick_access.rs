//! Windows Explorer's Quick Access pins (Folders panel → Favorites on Windows; F3).
//!
//! Explorer keeps them in `%APPDATA%\Microsoft\Windows\Recent\AutomaticDestinations\`
//! [`FILE_NAME`], a compound file (OLE "CFB", the format Microsoft documents as MS-CFB) whose
//! `DestList` stream lists every folder Explorer has opened: each entry carries the folder's
//! path (or `knownfolder:{GUID}` for Documents, Downloads, …) and a pin index, `-1` for the
//! "Frequent folders" Explorer shows under the pins and `0..` for the pins in their order. Only
//! pinned entries become sidebar items; the `.lnk` streams in the same file are never read.
//!
//! Hostile input: the file is size-capped, every sector index and offset is bounds-checked,
//! sector chains stop on a repeat, entry counts are capped, and bad data yields an empty list,
//! never a panic. Parsed on every platform so the reader is tested everywhere; only Windows
//! reads the real file ([`read`]).

use crate::{KnownFolder, SidebarItem};
use std::path::{Path, PathBuf};

/// Explorer's Quick Access jump list (the AppID is Explorer's own, fixed since Windows 7).
pub const FILE_NAME: &str = "f01b4d95cf55d32a.automaticDestinations-ms";
/// Largest file read (a busy machine's is a few hundred KiB).
pub const MAX_FILE_BYTES: u64 = 8 << 20;
/// Most `DestList` entries walked.
const MAX_ENTRIES: usize = 10_000;
/// Oldest `DestList` layout read (Windows 10 1607 and later; Windows 7's `1` had no trailing
/// padding and no Quick Access).
const MIN_VERSION: u32 = 3;
/// A `DestList` header and the fixed part of an entry, in bytes.
const HEADER_LEN: usize = 32;
const ENTRY_FIXED_LEN: usize = 130;

/// What a pin points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinTarget {
    /// An absolute Windows path (`C:\…` or `\\server\share\…`).
    Path(PathBuf),
    /// A known folder by GUID, resolved on the machine that reads it.
    Known(KnownFolder),
    /// Something else (a known folder we don't list, a shell namespace item): kept for
    /// diagnostics, never shown.
    Other(String),
}

/// One pinned entry, in Explorer's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// Explorer's position for it (0 = top).
    pub index: u32,
    /// Where it points.
    pub target: PinTarget,
}

/// The pins in a whole `.automaticDestinations-ms` file, top to bottom. Empty when the file
/// isn't a compound file, has no `DestList`, or the list can't be read.
pub fn parse_pins(file: &[u8]) -> Vec<Pin> {
    cfb::stream(file, "DestList").map(|s| parse_dest_list(&s)).unwrap_or_default()
}

/// The pins in a `DestList` stream, top to bottom.
pub fn parse_dest_list(stream: &[u8]) -> Vec<Pin> {
    let version = u32_at(stream, 0).unwrap_or(0);
    if version < MIN_VERSION {
        return Vec::new();
    }
    let mut pins = Vec::new();
    let mut at = HEADER_LEN;
    for _ in 0..MAX_ENTRIES {
        let Some(fixed) = stream.get(at..at + ENTRY_FIXED_LEN) else { break };
        let pin = i32::from_le_bytes([fixed[108], fixed[109], fixed[110], fixed[111]]);
        let chars = usize::from(u16::from_le_bytes([fixed[128], fixed[129]]));
        let Some(text) = stream.get(at + ENTRY_FIXED_LEN..at + ENTRY_FIXED_LEN + 2 * chars) else { break };
        if let Ok(index) = u32::try_from(pin) {
            pins.push(Pin { index, target: target(&utf16(text)) });
        }
        // Each entry ends with 4 bytes of padding from version 3 on.
        at += ENTRY_FIXED_LEN + 2 * chars + 4;
    }
    pins.sort_by_key(|p| p.index);
    pins
}

/// Sidebar items for `pins`: paths as they are, known folders through `known` (the machine's
/// own locations, OneDrive redirects included), anything else dropped. Names stay `None`: the
/// panel shows the folder name, as Explorer does.
pub fn items(pins: &[Pin], known: &dyn Fn(KnownFolder) -> Option<PathBuf>) -> Vec<SidebarItem> {
    pins.iter()
        .filter_map(|pin| match &pin.target {
            PinTarget::Path(p) => Some(p.clone()),
            PinTarget::Known(k) => known(*k),
            PinTarget::Other(_) => None,
        })
        .map(|path| SidebarItem { path, name: None })
        .take(crate::sidebar::MAX_ITEMS)
        .collect()
}

/// Explorer's Quick Access pins for the user whose `%APPDATA%` is `appdata`, as sidebar
/// items. Empty when the file is missing, too large, or unreadable.
pub fn read(appdata: &Path, known: &dyn Fn(KnownFolder) -> Option<PathBuf>) -> Vec<SidebarItem> {
    let file = file_path(appdata);
    let ok = std::fs::metadata(&file).is_ok_and(|m| m.is_file() && m.len() <= MAX_FILE_BYTES);
    match ok.then(|| std::fs::read(&file).ok()).flatten() {
        Some(bytes) => items(&parse_pins(&bytes), known),
        None => Vec::new(),
    }
}

/// Where Quick Access reading stops on this machine, step by step (`assetcraft-cli sidebar
/// --why` on Windows).
pub fn report(appdata: Option<&Path>) -> Vec<String> {
    let Some(appdata) = appdata else { return vec!["%APPDATA% is not set".into()] };
    let file = file_path(appdata);
    let mut out = vec![format!("file: {}", file.display())];
    match std::fs::metadata(&file) {
        Err(e) => out.push(format!("missing: {e} ({:?})", e.kind())),
        Ok(m) if m.len() > MAX_FILE_BYTES => out.push(format!("too large ({} bytes)", m.len())),
        Ok(m) => match std::fs::read(&file) {
            Err(e) => out.push(format!("{} bytes, but reading failed: {e} ({:?})", m.len(), e.kind())),
            Ok(bytes) => {
                out.push(format!("{} bytes read", bytes.len()));
                out.extend(diagnose(&bytes));
            }
        },
    }
    out
}

/// What the parser sees in `file`, one line per step.
pub fn diagnose(file: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(stream) = cfb::stream(file, "DestList") else {
        out.push("not a compound file, or no DestList stream in it".into());
        return out;
    };
    let version = u32_at(&stream, 0).unwrap_or(0);
    let entries = u32_at(&stream, 4).unwrap_or(0);
    let pinned = u32_at(&stream, 8).unwrap_or(0);
    out.push(format!("DestList: {} bytes, version {version}, {entries} entries, {pinned} pinned", stream.len()));
    if version < MIN_VERSION {
        out.push(format!("version {version} is older than {MIN_VERSION}: not read"));
    }
    for pin in parse_dest_list(&stream) {
        out.push(match pin.target {
            PinTarget::Path(p) => format!("pin {}: {}", pin.index, p.display()),
            PinTarget::Known(k) => format!("pin {}: known folder {k:?}", pin.index),
            PinTarget::Other(s) => format!("pin {}: not a folder we list: {s}", pin.index),
        });
    }
    out
}

fn file_path(appdata: &Path) -> PathBuf {
    appdata.join("Microsoft").join("Windows").join("Recent").join("AutomaticDestinations").join(FILE_NAME)
}

/// Known-folder GUIDs Explorer writes as `knownfolder:{…}` (the `FOLDERID_*` constants).
const KNOWN_FOLDERS: [(&str, KnownFolder); 7] = [
    ("5E6C858F-0E22-4760-9AFE-EA3317B67173", KnownFolder::Home),
    ("B4BFCC3A-DB2C-424C-B029-7FE99A87C641", KnownFolder::Desktop),
    ("FDD39AD0-238F-46AF-ADB4-6C85480369C7", KnownFolder::Documents),
    ("374DE290-123F-4565-9164-39C4925E467B", KnownFolder::Downloads),
    ("33E28130-4E1E-4676-835A-98395C3BC3BB", KnownFolder::Pictures),
    ("18989B1D-99B5-455B-841C-AB7C74E4DDFC", KnownFolder::Videos),
    ("4BD8D571-6D19-48D3-BE97-422220080E43", KnownFolder::Music),
];

fn target(text: &str) -> PinTarget {
    if let Some(guid) = text.strip_prefix("knownfolder:{").and_then(|s| s.strip_suffix('}')) {
        return match KNOWN_FOLDERS.iter().find(|(g, _)| g.eq_ignore_ascii_case(guid)) {
            Some((_, k)) => PinTarget::Known(*k),
            None => PinTarget::Other(text.to_owned()),
        };
    }
    if is_windows_path(text) { PinTarget::Path(PathBuf::from(text)) } else { PinTarget::Other(text.to_owned()) }
}

/// `X:\…` or `\\server\share…`, with no control characters.
fn is_windows_path(s: &str) -> bool {
    if s.chars().any(char::is_control) {
        return false;
    }
    let b = s.as_bytes();
    let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\';
    let unc = b.len() >= 5 && b.starts_with(br"\\") && b[2] != b'\\';
    drive || unc
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    String::from_utf16_lossy(&units).trim_end_matches('\0').to_owned()
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// A bounded reader for the compound file format (MS-CFB): just enough to find one stream by
/// name. Versions 3 (512-byte sectors) and 4 (4096), the mini stream for streams under the
/// cut-off, DIFAT in the header and in DIFAT sectors.
mod cfb {
    pub(super) const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
    const HEADER_LEN: usize = 512;
    const HEADER_DIFAT_ENTRIES: usize = 109;
    const DIR_ENTRY_LEN: usize = 128;
    const MINI_SECTOR_LEN: usize = 64;
    const STREAM_ENTRY: u8 = 2;
    /// Most FAT / DIFAT sectors followed (a 1 GiB file; ours are kilobytes).
    const MAX_FAT_SECTORS: usize = 16_384;

    struct File<'a> {
        bytes: &'a [u8],
        sector_len: usize,
        fat: Vec<u32>,
    }

    impl File<'_> {
        /// Sector `n`'s bytes (sector 0 follows the header).
        fn sector(&self, n: u32) -> Option<&[u8]> {
            let start = (n as usize).checked_add(1)?.checked_mul(self.sector_len)?;
            self.bytes.get(start..start.checked_add(self.sector_len)?)
        }

        /// The sectors of the chain starting at `start`, stopping at the end marker, an index
        /// outside the FAT, or a repeat (a cycle in hostile data).
        fn chain(&self, start: u32) -> Vec<u32> {
            let mut seen = vec![false; self.fat.len()];
            let mut out = Vec::new();
            let mut n = start;
            while let Some(next) = self.fat.get(n as usize) {
                if seen[n as usize] {
                    break;
                }
                seen[n as usize] = true;
                out.push(n);
                n = *next;
            }
            // A chain that ended on anything but END_OF_CHAIN (free, bad, out of range) is
            // still returned: the stream's recorded size truncates it.
            out
        }

        fn read_chain(&self, start: u32) -> Vec<u8> {
            let mut out = Vec::new();
            for n in self.chain(start) {
                if let Some(s) = self.sector(n) {
                    out.extend_from_slice(s);
                }
            }
            out
        }
    }

    fn u16_at(b: &[u8], at: usize) -> Option<u16> {
        b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
    }

    fn u32_at(b: &[u8], at: usize) -> Option<u32> {
        super::u32_at(b, at)
    }

    fn u32s(b: &[u8]) -> impl Iterator<Item = u32> + '_ {
        b.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c))
    }

    fn open(bytes: &[u8]) -> Option<File<'_>> {
        if bytes.len() < HEADER_LEN || bytes[..8] != SIGNATURE {
            return None;
        }
        let sector_len = match u16_at(bytes, 30)? {
            9 => 512,
            12 => 4096,
            _ => return None,
        };
        if u16_at(bytes, 32)? != 6 {
            return None;
        }
        let fat_sectors = usize::try_from(u32_at(bytes, 44)?).ok()?.min(MAX_FAT_SECTORS);
        let mut difat: Vec<u32> = u32s(&bytes[76..76 + 4 * HEADER_DIFAT_ENTRIES]).collect();
        let mut difat_sector = u32_at(bytes, 68)?;
        let mut file = File { bytes, sector_len, fat: Vec::new() };
        // DIFAT sectors: all but the last u32 are FAT sector numbers; the last is the next DIFAT.
        for _ in 0..MAX_FAT_SECTORS {
            if difat.len() >= fat_sectors || difat_sector == END_OF_CHAIN {
                break;
            }
            let s = file.sector(difat_sector)?;
            let mut entries: Vec<u32> = u32s(s).collect();
            difat_sector = entries.pop()?;
            difat.extend(entries);
        }
        let total_sectors = bytes.len() / sector_len;
        let mut fat = Vec::new();
        for n in difat.into_iter().take(fat_sectors) {
            fat.extend(u32s(file.sector(n)?));
            if fat.len() > total_sectors + sector_len / 4 {
                break;
            }
        }
        file.fat = fat;
        Some(file)
    }

    /// The bytes of the stream named `name` (UTF-16 in the file, compared exactly).
    pub(super) fn stream(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
        let file = open(bytes)?;
        let dir = file.read_chain(u32_at(bytes, 48)?);
        let root = dir.get(..DIR_ENTRY_LEN)?;
        let mini_cutoff = u32_at(bytes, 56)? as usize;
        let wanted: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        for entry in dir.as_chunks::<DIR_ENTRY_LEN>().0 {
            let name_len = usize::from(u16_at(entry, 64)?);
            let is_stream = entry[66] == STREAM_ENTRY;
            if !is_stream || !(2..=64).contains(&name_len) || entry[..name_len - 2] != wanted[..] {
                continue;
            }
            let start = u32_at(entry, 116)?;
            // Version 3 files only use the low 32 bits of the size.
            let size = u32_at(entry, 120)? as usize;
            let mut data = if size < mini_cutoff {
                let mini_stream = {
                    let whole = file.read_chain(u32_at(root, 116)?);
                    let len = (u32_at(root, 120)? as usize).min(whole.len());
                    whole.into_iter().take(len).collect::<Vec<u8>>()
                };
                let mini_fat: Vec<u32> = u32s(&file.read_chain(u32_at(bytes, 60)?)).collect();
                let mut seen = vec![false; mini_fat.len()];
                let mut out = Vec::new();
                let mut n = start;
                while let Some(next) = mini_fat.get(n as usize) {
                    if seen[n as usize] || out.len() >= size {
                        break;
                    }
                    seen[n as usize] = true;
                    let at = (n as usize) * MINI_SECTOR_LEN;
                    out.extend_from_slice(mini_stream.get(at..at + MINI_SECTOR_LEN)?);
                    n = *next;
                }
                out
            } else {
                file.read_chain(start)
            };
            data.truncate(size);
            return Some(data);
        }
        None
    }
}

#[cfg(test)]
pub(crate) mod build {
    //! Test builders: a `DestList` stream and a minimal compound file around it.

    /// A `DestList` (version 4) with one entry per `(pin, text)`.
    pub(crate) fn dest_list(entries: &[(i32, &str)]) -> Vec<u8> {
        let mut out = vec![0u8; super::HEADER_LEN];
        out[..4].copy_from_slice(&4u32.to_le_bytes());
        out[4..8].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        out[8..12].copy_from_slice(&(entries.iter().filter(|(p, _)| *p >= 0).count() as u32).to_le_bytes());
        for (i, (pin, text)) in entries.iter().enumerate() {
            let mut fixed = vec![0u8; super::ENTRY_FIXED_LEN];
            fixed[88..92].copy_from_slice(&(i as u32 + 1).to_le_bytes());
            fixed[108..112].copy_from_slice(&pin.to_le_bytes());
            let units: Vec<u16> = text.encode_utf16().collect();
            fixed[128..130].copy_from_slice(&(units.len() as u16).to_le_bytes());
            out.extend(fixed);
            out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
            out.extend([0u8; 4]);
        }
        out
    }

    /// A version-3 compound file (512-byte sectors) holding one stream, `name`, in the mini
    /// stream when it is under 4096 bytes and in regular sectors otherwise. Layout: header,
    /// sector 0 FAT, sector 1 directory, sector 2 mini FAT, then the data (mini stream or the
    /// stream itself).
    pub(crate) fn compound_file(name: &str, data: &[u8]) -> Vec<u8> {
        const S: usize = 512;
        const EOC: u32 = 0xFFFF_FFFE;
        const FREE: u32 = 0xFFFF_FFFF;
        let mini = data.len() < 4096;
        let data_sectors = data.len().div_ceil(S).max(1);
        let total = 3 + data_sectors;
        let mut fat: Vec<u32> = vec![FREE; S / 4];
        fat[0] = 0xFFFF_FFFD; // FAT sector marker
        fat[1] = EOC; // directory
        fat[2] = EOC; // mini FAT
        for i in 0..data_sectors {
            fat[3 + i] = if i + 1 == data_sectors { EOC } else { 3 + i as u32 + 1 };
        }
        let mut file = vec![0u8; S * (total + 1)];
        // Header.
        file[..8].copy_from_slice(&super::cfb::SIGNATURE);
        file[26..28].copy_from_slice(&3u16.to_le_bytes()); // major version
        file[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes()); // byte order
        file[30..32].copy_from_slice(&9u16.to_le_bytes()); // sector shift
        file[32..34].copy_from_slice(&6u16.to_le_bytes()); // mini sector shift
        file[44..48].copy_from_slice(&1u32.to_le_bytes()); // FAT sectors
        file[48..52].copy_from_slice(&1u32.to_le_bytes()); // directory start
        file[56..60].copy_from_slice(&4096u32.to_le_bytes()); // mini stream cut-off
        file[60..64].copy_from_slice(&2u32.to_le_bytes()); // mini FAT start
        file[64..68].copy_from_slice(&1u32.to_le_bytes()); // mini FAT sectors
        file[68..72].copy_from_slice(&EOC.to_le_bytes()); // DIFAT start
        for i in 0..109 {
            let v = if i == 0 { 0u32 } else { FREE };
            file[76 + 4 * i..80 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
        // FAT (sector 0).
        for (i, v) in fat.iter().enumerate() {
            file[S + 4 * i..S + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
        // Directory (sector 1): root entry, then the stream.
        let dir = 2 * S;
        let mut entry = |slot: usize, name: &str, kind: u8, start: u32, size: u32| {
            let at = dir + slot * 128;
            let units: Vec<u16> = name.encode_utf16().chain([0]).collect();
            for (i, u) in units.iter().enumerate() {
                file[at + 2 * i..at + 2 * i + 2].copy_from_slice(&u.to_le_bytes());
            }
            file[at + 64..at + 66].copy_from_slice(&((units.len() * 2) as u16).to_le_bytes());
            file[at + 66] = kind;
            file[at + 116..at + 120].copy_from_slice(&start.to_le_bytes());
            file[at + 120..at + 124].copy_from_slice(&size.to_le_bytes());
        };
        if mini {
            // The root entry owns the mini stream (sectors 3..); the stream starts at mini sector 0.
            entry(0, "Root Entry", 5, 3, (data.len().div_ceil(64) * 64) as u32);
            entry(1, name, 2, 0, data.len() as u32);
        } else {
            entry(0, "Root Entry", 5, EOC, 0);
            entry(1, name, 2, 3, data.len() as u32);
        }
        // Mini FAT (sector 2): a chain over the mini sectors.
        if mini {
            let n = data.len().div_ceil(64);
            for i in 0..n {
                let v = if i + 1 == n { EOC } else { i as u32 + 1 };
                file[3 * S + 4 * i..3 * S + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        // Data from sector 3.
        file[4 * S..4 * S + data.len()].copy_from_slice(data);
        file
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCS: &str = "knownfolder:{FDD39AD0-238F-46AF-ADB4-6C85480369C7}";

    fn resolve(k: KnownFolder) -> Option<PathBuf> {
        (k == KnownFolder::Documents).then(|| PathBuf::from(r"C:\Users\paul\OneDrive\Documents"))
    }

    #[test]
    fn pins_come_out_in_explorers_order_and_frequent_folders_stay_out() {
        let list = build::dest_list(&[
            (-1, r"C:\Users\paul\Downloads\images"),
            (2, r"F:\Script-Projects"),
            (0, DOCS),
            (-1, r"F:\Trellis"),
            (1, r"\\nas\media\Photos"),
            (3, "knownfolder:{754AC886-DF64-4CBA-86B5-F7FBF4FBCEF5}"), // 3D Objects: not listed
            (4, "::{20D04FE0-3AEA-1069-A2D8-08002B30309D}"),           // This PC: not a path
        ]);
        let pins = parse_dest_list(&list);
        assert_eq!(pins.iter().map(|p| p.index).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
        assert_eq!(pins[0].target, PinTarget::Known(KnownFolder::Documents));
        assert_eq!(pins[1].target, PinTarget::Path(PathBuf::from(r"\\nas\media\Photos")));
        assert!(matches!(pins[3].target, PinTarget::Other(_)) && matches!(pins[4].target, PinTarget::Other(_)));
        let items = items(&pins, &resolve);
        let paths: Vec<&Path> = items.iter().map(|i| i.path.as_path()).collect();
        assert_eq!(
            paths,
            [
                Path::new(r"C:\Users\paul\OneDrive\Documents"),
                Path::new(r"\\nas\media\Photos"),
                Path::new(r"F:\Script-Projects")
            ]
        );
        assert!(items.iter().all(|i| i.name.is_none()), "Explorer shows folder names");
    }

    #[test]
    fn reads_the_stream_from_a_compound_file_in_both_layouts() {
        let small = build::dest_list(&[(0, r"D:\small")]);
        assert!(small.len() < 4096);
        let file = build::compound_file("DestList", &small);
        assert_eq!(parse_pins(&file), [Pin { index: 0, target: PinTarget::Path(PathBuf::from(r"D:\small")) }]);

        let many: Vec<(i32, String)> =
            (0..40).map(|i| (i, format!(r"D:\folder-{i:02}\with a long enough name"))).collect();
        let many: Vec<(i32, &str)> = many.iter().map(|(p, s)| (*p, s.as_str())).collect();
        let big = build::dest_list(&many);
        assert!(big.len() >= 4096, "{}", big.len());
        let file = build::compound_file("DestList", &big);
        let pins = parse_pins(&file);
        assert_eq!(pins.len(), 40);
        assert_eq!(pins[39].target, PinTarget::Path(PathBuf::from(r"D:\folder-39\with a long enough name")));

        assert!(parse_pins(&build::compound_file("Other", &small)).is_empty(), "no DestList stream");
    }

    #[test]
    fn hostile_files_give_nothing_never_a_panic() {
        let good = build::compound_file("DestList", &build::dest_list(&[(0, r"D:\ok")]));
        assert!(parse_pins(&[]).is_empty());
        assert!(parse_pins(b"not a compound file at all").is_empty());
        assert!(parse_pins(&good[..600]).is_empty(), "truncated after the FAT");
        for cut in [0, 100, 511, 512, 1000, 1537, 2049] {
            let _ = parse_pins(&good[..cut.min(good.len())]);
        }
        let mut cycle = good.clone();
        cycle[512 + 4 * 3..512 + 4 * 4].copy_from_slice(&3u32.to_le_bytes()); // data sector points at itself
        let _ = parse_pins(&cycle);
        let mut huge = good.clone();
        huge[44..48].copy_from_slice(&u32::MAX.to_le_bytes()); // claims 4 billion FAT sectors
        assert!(parse_pins(&huge).is_empty());
        let mut list = build::dest_list(&[(0, r"D:\ok")]);
        list[128 + 32..130 + 32].copy_from_slice(&u16::MAX.to_le_bytes()); // string longer than the stream
        assert!(parse_dest_list(&list).is_empty());
        let mut old = build::dest_list(&[(0, r"D:\ok")]);
        old[..4].copy_from_slice(&1u32.to_le_bytes());
        assert!(parse_dest_list(&old).is_empty(), "Windows 7 layout is not read");
        let odd = build::dest_list(&[(0, "D:\\with\u{7}bell"), (1, "relative\\path"), (2, "C:no-slash")]);
        assert!(items(&parse_dest_list(&odd), &resolve).is_empty(), "only absolute Windows paths");
    }

    #[test]
    fn diagnose_names_each_pin() {
        let file =
            build::compound_file("DestList", &build::dest_list(&[(-1, r"D:\recent"), (0, r"D:\pinned"), (1, DOCS)]));
        let lines = diagnose(&file);
        assert!(
            lines[0].starts_with("DestList: ") && lines[0].ends_with("version 4, 3 entries, 2 pinned"),
            "{lines:?}"
        );
        assert_eq!(&lines[1..], [r"pin 0: D:\pinned", "pin 1: known folder Documents"]);
        assert_eq!(diagnose(b"junk"), ["not a compound file, or no DestList stream in it"]);
    }
}
