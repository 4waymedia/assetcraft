//! File type sniffing by magic bytes, header-only image dimensions, and file facts.
//!
//! Layer 1. [`probe`] reads at most [`HEAD_BYTES`] from the start of a file through
//! [`assetcraft_fs::Services`] and never decodes pixels. The file's *contents* decide its
//! [`Format`]; the extension is only a claim, kept in [`Probe::claimed`], so a `.jpg` that isn't
//! a JPEG is reported as [`Status::Unrecognized`] instead of being trusted.
//!
//! Every parser works on untrusted bytes with bounds-checked reads; a damaged header gives
//! [`Status::Damaged`], never a panic.

mod dims;
mod format;

pub use format::{Category, Format};

use assetcraft_fs::{EntryKind, FsError, Services};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// How many bytes [`probe`] reads from the start of a file. Large enough for JPEG SOF markers
/// behind typical EXIF blocks and for TIFF first IFDs.
pub const HEAD_BYTES: usize = 256 * 1024;

/// How many bytes the second, targeted TIFF read takes (512 IFD entries).
const TIFF_IFD_BYTES: usize = 2 + 512 * 12 + 4;

/// JPEGs whose APPn segments (large XMP or ICC profiles) push the SOF marker past
/// [`HEAD_BYTES`] are followed with small targeted reads: at most this many …
const JPEG_MAX_HOPS: usize = 64;
/// … never starting past this offset …
const JPEG_MAX_OFFSET: u64 = 16 * 1024 * 1024;
/// … each this many bytes (a marker, its length and an SOF payload fit easily).
const JPEG_WINDOW: usize = 64;

/// Pixel dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dimensions {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Dimensions {
    /// Width divided by height, or `None` for a zero height.
    pub fn aspect(self) -> Option<f64> {
        (self.height != 0).then(|| f64::from(self.width) / f64::from(self.height))
    }
}

/// How well the file's header could be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Format recognized; dimensions read where the format has them.
    Ok,
    /// The magic bytes match no known format.
    Unrecognized,
    /// Format recognized, but the header is truncated or inconsistent.
    Damaged,
}

/// Everything known about a file without decoding it.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    /// Full path.
    pub path: PathBuf,
    /// File name.
    pub name: String,
    /// Format from the file's contents.
    pub format: Format,
    /// Format the extension claims, if the extension is a known one.
    pub claimed: Option<Format>,
    /// Pixel dimensions, when the format has them and the header holds them.
    pub dimensions: Option<Dimensions>,
    /// File size in bytes.
    pub size: u64,
    /// Last modification time.
    pub modified: Option<SystemTime>,
    /// Creation time, when the OS reports one.
    pub created: Option<SystemTime>,
    /// Header status.
    pub status: Status,
}

impl Probe {
    /// What the UI should treat this file as: the real format, else the claimed one.
    pub fn category(&self) -> Category {
        match (self.format, self.claimed) {
            (Format::Unknown, Some(claimed)) => claimed.category(),
            (format, _) => format.category(),
        }
    }

    /// `true` when the extension promises a format the contents don't deliver,
    /// e.g. `broken_not_an_image.jpg`. The UI shows an "unreadable" badge for these.
    pub fn is_unreadable(&self) -> bool {
        self.status != Status::Ok && self.claimed.is_some()
    }

    /// `true` when contents and extension disagree (both known, but different).
    pub fn extension_mismatch(&self) -> bool {
        matches!(self.claimed, Some(c) if self.format != Format::Unknown && !c.same_family(self.format))
    }
}

/// Probe one file through `services`. Fails only if the file can't be read at all.
pub fn probe(services: &dyn Services, path: &Path) -> Result<Probe, FsError> {
    let entry = services.entry(path)?;
    if entry.kind != EntryKind::File {
        return Err(FsError::Io { path: path.to_path_buf(), message: "not a regular file".into() });
    }
    let head = services.read_head(path, HEAD_BYTES)?;
    let claimed = entry.extension().and_then(|ext| Format::from_extension(&ext));
    let (format, mut dimensions, mut status) = probe_bytes(&head, entry.extension().as_deref());
    // Some TIFF writers put the first IFD at the end of the file: one bounded second read.
    if format == Format::Tiff
        && dimensions.is_none()
        && let Some((little, offset)) = dims::tiff_ifd_offset(&head)
        && let Ok(ifd) = services.read_range(path, offset, TIFF_IFD_BYTES)
        && let Some(found) = dims::tiff_ifd(&ifd, little).filter(|d| d.width > 0 && d.height > 0)
    {
        dimensions = Some(found);
        status = Status::Ok;
    }
    // Big metadata segments can push a JPEG's SOF past the head: keep hopping segments.
    if format == Format::Jpeg
        && dimensions.is_none()
        && (head.len() as u64) < entry.size
        && let dims::JpegScan::NeedMore(resume) = dims::jpeg_scan(&head, 2)
        && let Some(found) = jpeg_beyond_head(services, path, resume as u64)
    {
        dimensions = Some(found);
        status = Status::Ok;
    }
    Ok(Probe {
        path: entry.path,
        name: entry.name,
        format,
        claimed,
        dimensions,
        size: entry.size,
        modified: entry.modified,
        created: entry.created,
        status,
    })
}

/// Follow JPEG segments from `offset` with bounded reads (see [`JPEG_MAX_HOPS`]).
fn jpeg_beyond_head(services: &dyn Services, path: &Path, mut offset: u64) -> Option<Dimensions> {
    for _ in 0..JPEG_MAX_HOPS {
        if offset > JPEG_MAX_OFFSET {
            return None;
        }
        let window = services.read_range(path, offset, JPEG_WINDOW).ok()?;
        match dims::jpeg_scan(&window, 0) {
            dims::JpegScan::Found(d) if d.width > 0 && d.height > 0 => return Some(d),
            // No progress (end of file, or a run of fill bytes longer than the window): stop.
            dims::JpegScan::Found(_) | dims::JpegScan::Stop | dims::JpegScan::NeedMore(0) => return None,
            dims::JpegScan::NeedMore(n) => offset = offset.checked_add(n as u64)?,
        }
    }
    None
}

/// Sniff a format and read dimensions from the first bytes of a file.
/// `extension` (lower-case, no dot) only refines formats that share a container, such as
/// camera raw files that are TIFF inside.
pub fn probe_bytes(head: &[u8], extension: Option<&str>) -> (Format, Option<Dimensions>, Status) {
    let format = format::sniff(head, extension);
    if format == Format::Unknown {
        return (format, None, Status::Unrecognized);
    }
    if !format.has_header_dimensions() {
        return (format, None, Status::Ok);
    }
    match dims::read(format, head) {
        Some(d) if d.width > 0 && d.height > 0 => (format, Some(d), Status::Ok),
        _ => (format, None, Status::Damaged),
    }
}

#[cfg(test)]
mod tests;
