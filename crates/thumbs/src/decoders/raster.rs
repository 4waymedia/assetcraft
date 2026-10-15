//! Common raster formats through the `image` crate (the Stage 0 decoder).

use super::{Decoder, from_image, image_limits, read_file};
use crate::{Thumb, ThumbError};
use assetcraft_fs::Services;
use assetcraft_probe::{Format, Probe};
use image::ImageReader;
use std::io::Cursor;

/// PNG, JPEG, GIF, BMP, WebP and TIFF.
pub(super) struct Raster;

const FORMATS: &[Format] = &[Format::Png, Format::Jpeg, Format::Gif, Format::Bmp, Format::WebP, Format::Tiff];

impl Decoder for Raster {
    fn name(&self) -> &'static str {
        "raster"
    }

    fn formats(&self) -> &'static [Format] {
        FORMATS
    }

    fn render(&self, services: &dyn Services, probe: &Probe, max_side: u32) -> Result<Thumb, ThumbError> {
        let format = image_format(probe.format).ok_or(ThumbError::Unsupported(probe.format))?;
        let bytes = read_file(services, probe, self.max_file_bytes())?;
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        reader.limits(image_limits());
        let image = reader.decode().map_err(|e| match e {
            image::ImageError::Limits(_) => ThumbError::TooLarge,
            other => ThumbError::Decode(other.to_string()),
        })?;
        Ok(from_image(image, max_side))
    }
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
