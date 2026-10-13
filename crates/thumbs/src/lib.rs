//! Thumbnail decode and downscale on a prioritized worker pool, with a memory cache.
//!
//! Layer 2. [`render`] turns one file into a small RGBA [`Thumb`]: it probes the file first
//! (contents decide the format, see `assetcraft-probe`), refuses formats Stage 0 can't decode,
//! applies decode limits so a hostile header can't allocate gigabytes, and catches decoder
//! panics so one bad file never takes the app down.
//!
//! [`ThumbPool`] runs [`render`] on worker threads. Requests are ordered by priority and then
//! newest-first, so the rows you just scrolled to win; [`ThumbPool::retain`] drops queued work
//! for rows that scrolled away (LightCraft's `preview/src/pool.rs` idea). Finished thumbnails go
//! into a byte-budgeted LRU [`cache::ThumbCache`].

pub mod cache;
mod pool;
mod queue;

pub use cache::ThumbCache;
pub use pool::{ThumbPool, ThumbResult};

use assetcraft_fs::{FsError, Services};
use assetcraft_probe::{Format, Status};
use image::{ImageReader, Limits};
use std::fmt;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// Files larger than this are not decoded for a thumbnail (Stage 0 reads the whole file).
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// Largest image side the decoder will accept.
pub const MAX_IMAGE_SIDE: u32 = 32_768;

/// Most memory one decode may allocate.
pub const MAX_DECODE_ALLOC: u64 = 1024 * 1024 * 1024;

/// Smallest and largest thumbnail sides callers may ask for.
pub const SIDE_RANGE: (u32, u32) = (16, 1024);

/// What to render: a file at a maximum side length.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThumbKey {
    /// The file.
    pub path: PathBuf,
    /// Longest side of the thumbnail in pixels (clamped to [`SIDE_RANGE`]).
    pub max_side: u32,
}

impl ThumbKey {
    /// A key for `path` at `max_side` pixels.
    pub fn new(path: impl Into<PathBuf>, max_side: u32) -> Self {
        Self { path: path.into(), max_side: max_side.clamp(SIDE_RANGE.0, SIDE_RANGE.1) }
    }
}

/// A decoded thumbnail: unpremultiplied RGBA8, row-major, `width * height * 4` bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Thumb {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Size of the source image.
    pub source_width: u32,
    /// Size of the source image.
    pub source_height: u32,
    /// Pixels.
    pub rgba: Vec<u8>,
}

impl Thumb {
    /// Memory this thumbnail occupies, for cache budgets.
    pub fn cost(&self) -> usize {
        self.rgba.len() + std::mem::size_of::<Self>()
    }
}

impl fmt::Debug for Thumb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Thumb({}x{} from {}x{})", self.width, self.height, self.source_width, self.source_height)
    }
}

/// Why a thumbnail could not be made. The UI turns these into badges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThumbError {
    /// Stage 0 has no decoder for this format (it gets a type icon instead).
    Unsupported(Format),
    /// The file's header is unrecognized or damaged (see `assetcraft-probe`).
    Unreadable(Status),
    /// The file or image is over [`MAX_FILE_BYTES`], [`MAX_IMAGE_SIDE`] or [`MAX_DECODE_ALLOC`].
    TooLarge,
    /// The file could not be read.
    Io(FsError),
    /// The decoder rejected the data (or panicked, which is caught).
    Decode(String),
}

impl fmt::Display for ThumbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(format) => write!(f, "no thumbnail for {} yet", format.label()),
            Self::Unreadable(Status::Unrecognized) => write!(f, "not a recognized file"),
            Self::Unreadable(_) => write!(f, "damaged header"),
            Self::TooLarge => write!(f, "too large to preview"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Decode(m) => write!(f, "can't decode: {m}"),
        }
    }
}

impl std::error::Error for ThumbError {}

/// Render one thumbnail. Blocking; call from a worker (see [`ThumbPool`]), not the UI thread.
pub fn render(services: &dyn Services, key: &ThumbKey) -> Result<Thumb, ThumbError> {
    let probe = assetcraft_probe::probe(services, &key.path).map_err(ThumbError::Io)?;
    if probe.status != Status::Ok {
        return Err(ThumbError::Unreadable(probe.status));
    }
    if !probe.format.thumbnail_in_stage0() {
        return Err(ThumbError::Unsupported(probe.format));
    }
    if probe.size > MAX_FILE_BYTES {
        return Err(ThumbError::TooLarge);
    }
    if let Some(d) = probe.dimensions
        && (d.width > MAX_IMAGE_SIDE || d.height > MAX_IMAGE_SIDE)
    {
        return Err(ThumbError::TooLarge);
    }
    let len = usize::try_from(probe.size).map_err(|_| ThumbError::TooLarge)?;
    let bytes = services.read_range(&key.path, 0, len).map_err(ThumbError::Io)?;
    let format = image_format(probe.format).ok_or(ThumbError::Unsupported(probe.format))?;
    let max_side = key.max_side;
    // Decoders are third-party code fed untrusted bytes: contain any panic.
    std::panic::catch_unwind(move || decode_and_shrink(&bytes, format, max_side))
        .unwrap_or_else(|_| Err(ThumbError::Decode("decoder panicked".into())))
}

fn decode_and_shrink(bytes: &[u8], format: image::ImageFormat, max_side: u32) -> Result<Thumb, ThumbError> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| match e {
        image::ImageError::Limits(_) => ThumbError::TooLarge,
        other => ThumbError::Decode(other.to_string()),
    })?;
    let (source_width, source_height) = (image.width(), image.height());
    let small = if source_width.max(source_height) > max_side { image.thumbnail(max_side, max_side) } else { image };
    let rgba = small.to_rgba8();
    Ok(Thumb { width: rgba.width(), height: rgba.height(), source_width, source_height, rgba: rgba.into_raw() })
}

fn image_format(format: Format) -> Option<image::ImageFormat> {
    Some(match format {
        Format::Png => image::ImageFormat::Png,
        Format::Jpeg => image::ImageFormat::Jpeg,
        Format::Gif => image::ImageFormat::Gif,
        Format::Bmp => image::ImageFormat::Bmp,
        Format::WebP => image::ImageFormat::WebP,
        Format::Tiff => image::ImageFormat::Tiff,
        _ => return None,
    })
}

/// Convenience for callers that only have a path.
pub fn render_path(services: &dyn Services, path: &Path, max_side: u32) -> Result<Thumb, ThumbError> {
    render(services, &ThumbKey::new(path, max_side))
}

#[cfg(test)]
mod tests;
