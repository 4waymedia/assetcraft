//! Media duration from the container header, without decoding (SR2b: a video dropped on a
//! section's inset slot sets the section's length). MP4 / MOV only for now: the top-level boxes
//! are walked with small bounded reads (`mdat` is skipped by its size, never read), and the
//! `moov` box — wherever the writer put it — gives `mvhd`'s timescale and duration. Everything
//! is bounds-checked; a strange file gives `None`, never a panic.

use assetcraft_fs::Services;
use std::path::Path;

/// Most of a `moov` box read to find `mvhd` (it is normally its first child).
const MOOV_BYTES: usize = 1 << 20;
/// Most top-level boxes walked.
const MAX_BOXES: usize = 64;

/// The file's duration in seconds, when the container says.
pub fn media_duration(services: &dyn Services, path: &Path) -> Option<f64> {
    let size = services.entry(path).ok()?.size;
    let mut at = 0u64;
    for _ in 0..MAX_BOXES {
        if at + 8 > size {
            return None;
        }
        let head = services.read_range(path, at, 16).ok()?;
        if head.len() < 8 {
            return None;
        }
        let kind = &head[4..8];
        let (len, header) = match u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]])) {
            1 if head.len() >= 16 => {
                let big =
                    u64::from_be_bytes([head[8], head[9], head[10], head[11], head[12], head[13], head[14], head[15]]);
                (big, 16u64)
            }
            1 => return None,
            0 => (size.saturating_sub(at), 8u64),
            n => (n, 8u64),
        };
        if len < header || at.saturating_add(len) > size {
            return None;
        }
        if kind == b"moov" {
            let want = usize::try_from(len.min(MOOV_BYTES as u64)).ok()?;
            let moov = services.read_range(path, at, want).ok()?;
            return mvhd_seconds(&moov, header as usize);
        }
        at = at.saturating_add(len);
    }
    None
}

/// `mvhd` inside a `moov` box (`body` starts at the box's own header; `from` is its size).
fn mvhd_seconds(moov: &[u8], from: usize) -> Option<f64> {
    let mut at = from;
    for _ in 0..MAX_BOXES {
        let head = moov.get(at..at + 8)?;
        let len = usize::try_from(u32::from_be_bytes([head[0], head[1], head[2], head[3]])).ok()?;
        if len < 8 {
            return None;
        }
        if &head[4..8] == b"mvhd" {
            let body = moov.get(at + 8..(at + len).min(moov.len()))?;
            let version = *body.first()?;
            let be32 = |o: usize| body.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
            let be64 = |o: usize| {
                body.get(o..o + 8).map(|b| u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
            };
            // version/flags 4 · creation · modification · timescale · duration
            let (scale, duration) = match version {
                0 => (be32(12)?, u64::from(be32(16)?)),
                1 => (be32(20)?, be64(24)?),
                _ => return None,
            };
            if scale == 0 || duration == u64::MAX || duration == u64::from(u32::MAX) {
                return None;
            }
            return Some(duration as f64 / f64::from(scale)).filter(|s| s.is_finite() && *s > 0.0);
        }
        at = at.checked_add(len)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use assetcraft_fs::{FakeFs, RealFs};

    fn library(rel: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library").join(rel)
    }

    #[test]
    fn mp4_and_mov_durations_come_from_mvhd_and_other_files_give_none() {
        let fs = RealFs;
        for name in ["environments/snow_clearing_flythrough.mp4", "environments/snow_clearing_flythrough.mov"] {
            let d = media_duration(&fs, &library(name)).unwrap_or_else(|| panic!("{name}"));
            assert!((d - 2.0).abs() < 0.01, "{name}: {d}");
        }
        assert_eq!(media_duration(&fs, &library("environments/snow_clearing_plate.jpg")), None);
        assert_eq!(media_duration(&fs, &library("environments/frozen_lake_pan.avi")), None, "AVI: not yet");
        assert_eq!(media_duration(&fs, &library("edge-cases/broken_not_an_image.jpg")), None);
        assert_eq!(media_duration(&fs, &library("nope.mp4")), None);
    }

    #[test]
    fn hostile_boxes_are_refused() {
        let mut fs = FakeFs::new();
        // A box longer than the file, a zero-length box loop, a 64-bit size with no room.
        for (name, bytes) in [
            ("long.mp4", vec![0, 0, 0, 0x40, b'f', b't', b'y', b'p', 1, 2, 3, 4]),
            ("tiny.mp4", vec![0, 0, 0, 4, b'm', b'o', b'o', b'v']),
            ("big.mp4", vec![0, 0, 0, 1, b'm', b'd', b'a', b't', 0, 0]),
            ("empty.mp4", Vec::new()),
        ] {
            fs = fs.file_with(format!("/v/{name}"), bytes);
        }
        // A moov whose mvhd claims a zero timescale.
        let mut moov = vec![0, 0, 0, 8 + 8 + 100, b'm', b'o', b'o', b'v', 0, 0, 0, 108, b'm', b'v', b'h', b'd'];
        moov.extend_from_slice(&[0u8; 100]);
        fs = fs.file_with("/v/zero.mp4", moov);
        for name in ["long", "tiny", "big", "empty", "zero"] {
            assert_eq!(media_duration(&fs, Path::new(&format!("/v/{name}.mp4"))), None, "{name}");
        }
    }
}
