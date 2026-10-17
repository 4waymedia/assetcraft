//! Thumbnail decode and downscale on a prioritized worker pool, with memory and disk caches.
//!
//! Layer 2. [`render`] turns one file into a small RGBA [`Thumb`]: it probes the file first
//! (contents decide the format, see `assetcraft-probe`), picks a decoder from the
//! [`decoders`] registry (no decoder: [`ThumbError::Unsupported`]), applies decode limits so a
//! hostile header can't allocate gigabytes, and catches decoder panics so one bad file never
//! takes the app down. [`disk::DiskCache`] keeps rendered thumbnails across runs, and
//! [`ProbePool`] runs file probes off the UI thread.
//!
//! [`ThumbPool`] runs [`render`] on worker threads. Requests are ordered by priority and then
//! newest-first, so the rows you just scrolled to win; [`ThumbPool::retain`] drops queued work
//! for rows that scrolled away (LightCraft's `preview/src/pool.rs` idea). Finished thumbnails go
//! into a byte-budgeted LRU [`cache::ThumbCache`].

pub mod cache;
pub mod decoders;
pub mod disk;
/// Cache keys (moved to the cache crate in C1; same keys).
pub use assetcraft_cache::hash;
pub mod model;
mod pool;
mod probe_pool;
mod queue;

pub use assetcraft_cache::SourceId;
pub use cache::ThumbCache;
pub use decoders::{Decoder, supports};
pub use disk::{DEFAULT_DISK_BUDGET, DiskCache};
pub use pool::{MAX_FAILED, ThumbPool, ThumbResult, default_workers};
pub use probe_pool::{DEFAULT_PROBE_WORKERS, ProbePool, ProbeResult};

use assetcraft_fs::{FsError, Services};
use assetcraft_probe::{Format, Status};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

/// Default [`Decoder::max_file_bytes`]: larger files are not decoded (decoders that read the
/// whole file would hold it in memory).
pub const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;

/// Largest image side the decoder will accept.
pub const MAX_IMAGE_SIDE: u32 = 32_768;

/// Most memory one decode may allocate.
pub const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;

/// Most worker threads [`ThumbPool::with_default_workers`] starts.
///
/// Worst case per worker: the whole file in memory ([`MAX_FILE_BYTES`], 128 MiB) plus the
/// decoder's allocations ([`MAX_DECODE_ALLOC`], 512 MiB) = 640 MiB. Four workers bound the
/// pool at 2.5 GiB even if every one of them is handed a pathological file at once (see
/// [`WORST_CASE_BYTES`]); typical thumbnails use a few MiB each. A decoder that raises
/// [`Decoder::max_file_bytes`] must not read the whole file.
pub const MAX_DEFAULT_WORKERS: usize = 4;

/// Peak memory the default pool can reach while decoding: `workers × (file + decode)`.
pub const WORST_CASE_BYTES: u64 = MAX_DEFAULT_WORKERS as u64 * (MAX_FILE_BYTES + MAX_DECODE_ALLOC);

/// Smallest and largest thumbnail sides callers may ask for.
pub const SIDE_RANGE: (u32, u32) = (16, 1024);

/// What to render: a file, as last modified at `modified`, at a maximum side length.
///
/// The modification time is part of the key, so an edited file is a cache miss and re-renders
/// instead of showing the stale thumbnail.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThumbKey {
    /// The file.
    pub path: PathBuf,
    /// Longest side of the thumbnail in pixels (clamped to [`SIDE_RANGE`]).
    pub max_side: u32,
    /// The file's modification time when the key was made (`None` if the OS gave none).
    pub modified: Option<SystemTime>,
}

impl ThumbKey {
    /// A key for `path` at `max_side` pixels, with no modification time.
    pub fn new(path: impl Into<PathBuf>, max_side: u32) -> Self {
        Self { path: path.into(), max_side: max_side.clamp(SIDE_RANGE.0, SIDE_RANGE.1), modified: None }
    }

    /// The same key for the file as modified at `modified` (from its directory entry).
    pub fn with_modified(mut self, modified: Option<SystemTime>) -> Self {
        self.modified = modified;
        self
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
    /// No registered decoder for this format (it gets a type icon instead).
    Unsupported(Format),
    /// The decoder needs an external tool that isn't installed, e.g. `filmcraft-cli` for video
    /// poster frames. Shown like [`ThumbError::Unsupported`]: a type icon, not an error.
    ToolMissing(&'static str),
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
            Self::ToolMissing(tool) => write!(f, "needs {tool} (not installed)"),
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
///
/// Probes the file, picks a decoder from the [`decoders`] registry and applies the generic
/// guards around it (size limits, caught panics, output check and downscale; see
/// [`decoders`]). Does not touch the disk cache; [`ThumbPool`] does that.
pub fn render(services: &dyn Services, key: &ThumbKey) -> Result<Thumb, ThumbError> {
    render_with(services, key, &DecodeOptions::default())
}

/// `true` when this build includes the HEIF/HEIC decoder (cargo feature `heic`).
pub const HEIC_BUILT: bool = cfg!(feature = "heic");

/// Runtime choices about which formats are decoded. Everything defaults to off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DecodeOptions {
    /// Decode HEIF/HEIC (HEVC). Only has an effect when [`HEIC_BUILT`]; off by default for
    /// patent reasons, turned on by the user (`formats.heic`).
    pub heic: bool,
}

impl DecodeOptions {
    /// `true` if `format` may be decoded under these options. A disallowed format renders as
    /// [`ThumbError::Unsupported`] (the type icon) and is never served from the disk cache.
    pub fn allows(&self, format: Format) -> bool {
        format != Format::Heif || (self.heic && HEIC_BUILT)
    }
}

/// [`render`] under `options`.
pub fn render_with(services: &dyn Services, key: &ThumbKey, options: &DecodeOptions) -> Result<Thumb, ThumbError> {
    let probe = assetcraft_probe::probe(services, &key.path).map_err(ThumbError::Io)?;
    if probe.status == Status::Unrecognized {
        return Err(ThumbError::Unreadable(probe.status));
    }
    let decoder = decoders::decoder_for(probe.format).filter(|_| options.allows(probe.format));
    let Some(decoder) = decoder else {
        // A damaged header in a format we can't decode anyway: report the damage.
        return Err(if probe.status == Status::Ok {
            ThumbError::Unsupported(probe.format)
        } else {
            ThumbError::Unreadable(probe.status)
        });
    };
    // A `Damaged` header in a decodable format is still handed to the decoder: the probe only
    // reads a bounded head, the decoder reads the whole file, and the limits keep a lying
    // header from allocating more than MAX_DECODE_ALLOC.
    if probe.size > decoder.max_file_bytes() {
        return Err(ThumbError::TooLarge);
    }
    if let Some(d) = probe.dimensions
        && (d.width > MAX_IMAGE_SIDE || d.height > MAX_IMAGE_SIDE)
    {
        return Err(ThumbError::TooLarge);
    }
    let max_side = key.max_side;
    // Decoders are third-party code fed untrusted bytes: contain any panic.
    let thumb = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decoder.render(services, &probe, max_side)))
        .unwrap_or_else(|_| Err(ThumbError::Decode(format!("{} decoder panicked", decoder.name()))))?;
    decoders::finish(thumb, max_side)
}

/// Convenience for callers that only have a path.
pub fn render_path(services: &dyn Services, path: &Path, max_side: u32) -> Result<Thumb, ThumbError> {
    render(services, &ThumbKey::new(path, max_side))
}

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod pdf_video_tests;
#[cfg(test)]
mod stage1_tests;
#[cfg(test)]
mod tests;
