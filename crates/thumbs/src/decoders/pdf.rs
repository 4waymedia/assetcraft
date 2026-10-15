//! PDF first-page thumbnails through PdfCraft's `pdfcraft-render` (hayro + vello_cpu).
//!
//! PDF needs the whole file (the cross-reference table is at the end), so it is read through
//! `services` within [`MAX_PDF_BYTES`]. The page is rendered twice: once at a tiny scale to learn
//! its size in points (the renderer has no cheaper page-size query; `inspect` walks the whole
//! document structure), then at the scale that makes its longest side `max_side`. Both renders
//! are bounded by PdfCraft's own caps (`MAX_SIDE` 8192 px, `MAX_PIXELS` 64 MP, nesting and
//! allocation caps in its patched hayro) and contain renderer panics.

use super::{Decoder, read_file};
use crate::{Thumb, ThumbError};
use assetcraft_fs::Services;
use assetcraft_probe::{Format, Probe};
use pdfcraft_render::{PageRenderer, RenderConfig, RenderRequest, RenderedPage};
use std::sync::Arc;

/// First page of a PDF.
pub(super) struct Pdf;

/// Largest PDF read for a thumbnail: it is held in memory whole, so this stays at the default
/// [`crate::MAX_FILE_BYTES`] that the worker memory budget assumes.
pub(crate) const MAX_PDF_BYTES: u64 = crate::MAX_FILE_BYTES;

/// Scale of the sizing render (device pixels per point): a US Letter page is 62 × 80 px.
const PROBE_SCALE: f32 = 0.1;

const FORMATS: &[Format] = &[Format::Pdf];

impl Decoder for Pdf {
    fn name(&self) -> &'static str {
        "pdf"
    }

    fn formats(&self) -> &'static [Format] {
        FORMATS
    }

    fn max_file_bytes(&self) -> u64 {
        MAX_PDF_BYTES
    }

    fn render(&self, services: &dyn Services, probe: &Probe, max_side: u32) -> Result<Thumb, ThumbError> {
        let bytes = read_file(services, probe, self.max_file_bytes())?;
        render_first_page(bytes, max_side)
    }
}

/// Render page 1 of `bytes` with its longest side about `max_side` (at least `max_side`, so
/// the caller's downscale only ever shrinks).
pub(crate) fn render_first_page(bytes: Vec<u8>, max_side: u32) -> Result<Thumb, ThumbError> {
    let mut renderer = PageRenderer::new(Arc::new(bytes), RenderConfig::default());
    if renderer.page_count() == 0 {
        return Err(ThumbError::Decode("not a readable PDF (no pages)".into()));
    }
    let sizing = page(&mut renderer, PROBE_SCALE)?;
    // Device pixels are rounded up, so the page is more than (px - 1) / scale points long.
    let longest_pt = ((sizing.width.max(sizing.height) as f32 - 1.0) / PROBE_SCALE).max(1.0);
    let scale = max_side.max(1) as f32 / longest_pt;
    let full = page(&mut renderer, scale)?;
    let (width, height) = (full.width, full.height);
    // The source size is the page size in points (1/72 inch), as PDF viewers report it; the
    // renderer rounds device pixels up, so it is accurate to about one pixel's worth of points.
    let points = |px: u32| ((px as f32 / scale).round() as u32).max(1);
    // Premultiplied RGBA over an opaque white page, so every alpha is 255 and the bytes are
    // already straight RGBA.
    let rgba = full.rgba.to_vec();
    Ok(Thumb { width, height, source_width: points(width), source_height: points(height), rgba })
}

/// Page 1 at `scale`, as a preview render (images covering only a few pixels are skipped,
/// which is what PdfCraft's own page thumbnails use).
fn page(renderer: &mut PageRenderer, scale: f32) -> Result<RenderedPage, ThumbError> {
    let out = renderer.render(RenderRequest { page: 0, scale, preview: true, ..RenderRequest::default() });
    match out.error {
        Some(e) => Err(ThumbError::Decode(e)),
        None if out.width == 0 || out.height == 0 => Err(ThumbError::Decode("empty page".into())),
        None => Ok(out),
    }
}
