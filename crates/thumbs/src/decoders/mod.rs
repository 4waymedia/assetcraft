//! The decoder registry: which code turns which [`Format`] into a [`Thumb`].
//!
//! [`crate::render`] probes a file, picks the first registered decoder whose
//! [`Decoder::formats`] lists the probed format, and wraps the call in the generic guards, so a
//! decoder never has to repeat them:
//! - the file size must be at most [`Decoder::max_file_bytes`] (default [`MAX_FILE_BYTES`]);
//! - header dimensions from the probe must be at most [`MAX_IMAGE_SIDE`];
//! - a panic inside [`Decoder::render`] is caught and becomes [`ThumbError::Decode`];
//! - the returned [`Thumb`] is checked (non-zero size, `width * height * 4` bytes) and shrunk to
//!   `max_side` if the decoder returned something larger.
//!
//! # Adding a decoder (Phase B: PSD, raw/HEIF, PDF, video, 3D)
//!
//! 1. Add `decoders/<name>.rs` with a unit struct implementing [`Decoder`]. Read the file only
//!    through the `services` argument ([`read_file`] reads it whole within the size limit);
//!    never call `std::fs` directly, so tests can run on `FakeFs`.
//! 2. Add `mod <name>;` below and one `&<name>::<Struct>,` line to [`REGISTRY`]. Earlier entries
//!    win when two decoders claim the same format.
//! 3. Bound your own allocations by [`MAX_DECODE_ALLOC`] (for the `image` crate use
//!    [`image_limits`]); return [`ThumbError::TooLarge`] when a file is over a limit and
//!    [`ThumbError::Decode`] for bad data. Never `unwrap`, `expect` or `panic`.
//! 4. You may return the full-size image; [`crate::render`] downsizes it. Set
//!    `source_width`/`source_height` to the original size.
//! 5. If the output of an existing decoder changes, bump [`crate::disk::RENDERER_VERSION`] so
//!    stale disk-cache files are not shown.
//! 6. Tests: put a small sample in `test_data/library` (with an `ATTRIBUTION.md` row) or build
//!    bytes in the test, render through [`crate::render`] on a `FakeFs` (`file_with`), and add
//!    one damaged-input case that must return an error, not panic. The registry test in
//!    `tests.rs` already checks that every claimed format has a working sample when one exists
//!    in `test_data/library`.

#[cfg(feature = "heic")]
mod heif;
mod model;
mod pdf;
mod psd;
mod raster;
mod raw;
pub(crate) mod video;

pub use video::FILMCRAFT_CLI_ENV;

use crate::{MAX_DECODE_ALLOC, MAX_FILE_BYTES, MAX_IMAGE_SIDE, Thumb, ThumbError};
use assetcraft_fs::Services;
use assetcraft_probe::{Format, Probe};
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

thread_local! {
    /// The calling pool's stop flag, set once per worker thread ([`set_thread_cancel`]).
    static CANCEL: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// Give this thread a stop flag: decoders that wait on something slow (an external tool)
/// check [`cancelled`] and give up once it is set. Threads without one are never cancelled.
pub(crate) fn set_thread_cancel(flag: Option<Arc<AtomicBool>>) {
    CANCEL.with(|c| *c.borrow_mut() = flag);
}

/// `true` once this thread's pool is shutting down.
pub(crate) fn cancelled() -> bool {
    CANCEL.with(|c| c.borrow().as_ref().is_some_and(|f| f.load(Ordering::Acquire)))
}

/// Turns files of some formats into thumbnails. Implementations are stateless unit structs
/// (use a `OnceLock` inside if one needs lazily found state, such as a helper binary's path).
pub trait Decoder: Send + Sync {
    /// Short name for diagnostics, e.g. `"raster"`.
    fn name(&self) -> &'static str;

    /// Formats this decoder handles (as detected from contents by `assetcraft-probe`).
    fn formats(&self) -> &'static [Format];

    /// Largest file this decoder accepts. Override for formats that only read a small part
    /// of a big file (video poster frames, embedded raw previews).
    fn max_file_bytes(&self) -> u64 {
        MAX_FILE_BYTES
    }

    /// Render `probe.path` with its longest side at most `max_side` (larger output is shrunk
    /// by the caller). Blocking; runs on a worker thread.
    fn render(&self, services: &dyn Services, probe: &Probe, max_side: u32) -> Result<Thumb, ThumbError>;
}

/// Every decoder, in priority order. One line per decoder.
static REGISTRY: &[&dyn Decoder] = &[
    &raster::Raster, // PNG, JPEG, GIF, BMP, WebP, TIFF via the `image` crate
    &psd::Psd,       // PSD/PSB merged composite via `photocraft-psd`
    &raw::Raw,       // camera raw embedded JPEG preview via `lightcraft-raw`
    #[cfg(feature = "heic")]
    &heif::Heif, // HEIF/HEIC via `lightcraft-heif` (feature `heic`, plus the runtime switch)
    &pdf::Pdf,       // PDF page 1 via PdfCraft's `pdfcraft-render`
    &video::Video,   // MP4, MOV, MKV/WebM, AVI poster frames via the `filmcraft-cli` process
    &model::Model,   // GLB, glTF, OBJ via assetcraft-mesh (CPU rasterizer)
];

/// The registered decoders.
pub fn registry() -> &'static [&'static dyn Decoder] {
    REGISTRY
}

/// The decoder for `format`, if any is registered.
pub fn decoder_for(format: Format) -> Option<&'static dyn Decoder> {
    REGISTRY.iter().copied().find(|d| d.formats().contains(&format))
}

/// `true` when some registered decoder makes thumbnails for `format`.
pub fn supports(format: Format) -> bool {
    decoder_for(format).is_some()
}

/// Read the whole probed file, refusing files over `max_bytes`.
pub fn read_file(services: &dyn Services, probe: &Probe, max_bytes: u64) -> Result<Vec<u8>, ThumbError> {
    if probe.size > max_bytes {
        return Err(ThumbError::TooLarge);
    }
    let len = usize::try_from(probe.size).map_err(|_| ThumbError::TooLarge)?;
    services.read_range(&probe.path, 0, len).map_err(ThumbError::Io)
}

/// Decode limits for the `image` crate: [`MAX_IMAGE_SIDE`] per side, [`MAX_DECODE_ALLOC`] total.
pub fn image_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

/// A thumbnail from a decoded `image` crate image, shrunk to `max_side`.
pub fn from_image(image: image::DynamicImage, max_side: u32) -> Thumb {
    let (source_width, source_height) = (image.width(), image.height());
    let small = if source_width.max(source_height) > max_side { image.thumbnail(max_side, max_side) } else { image };
    let rgba = small.to_rgba8();
    Thumb { width: rgba.width(), height: rgba.height(), source_width, source_height, rgba: rgba.into_raw() }
}

/// Check a decoder's output and shrink it to `max_side` if needed.
pub(crate) fn finish(thumb: Thumb, max_side: u32) -> Result<Thumb, ThumbError> {
    let expected = (thumb.width as usize).checked_mul(thumb.height as usize).and_then(|n| n.checked_mul(4));
    if thumb.width == 0 || thumb.height == 0 || expected != Some(thumb.rgba.len()) {
        return Err(ThumbError::Decode("decoder returned a malformed image".into()));
    }
    if thumb.width.max(thumb.height) <= max_side {
        return Ok(thumb);
    }
    let Thumb { width, height, source_width, source_height, rgba } = thumb;
    let image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| ThumbError::Decode("decoder returned a malformed image".into()))?;
    let small = image::DynamicImage::ImageRgba8(image).thumbnail(max_side, max_side).to_rgba8();
    Ok(Thumb { width: small.width(), height: small.height(), source_width, source_height, rgba: small.into_raw() })
}
