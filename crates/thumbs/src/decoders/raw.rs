//! Camera raw through LightCraft's `lightcraft-raw`: the largest embedded JPEG preview.
//!
//! Every raw container LightCraft reads (DNG, CR2, CR3, NEF/NRW, ARW, RAF, ORF, RW2, PEF, SRW and
//! more) carries a camera-rendered JPEG; [`lightcraft_raw::embedded_preview`] finds the largest
//! one and the `image` crate decodes it under [`image_limits`]. The preview is turned upright with
//! the JPEG's own EXIF orientation, else the raw file's (read by `lightcraft-meta`). Developing
//! the sensor data is not done here: LightCraft's colour pipeline lives above its raw crate, so a
//! raw without a JPEG preview (rare; some DNG exports, JPEG XL previews) is a decode error.

use super::{Decoder, from_image, image_limits, read_file};
use crate::{Thumb, ThumbError};
use assetcraft_fs::Services;
use assetcraft_probe::{Format, Probe};
use image::metadata::Orientation;
use image::{ImageDecoder, ImageReader};
use std::io::Cursor;

/// Camera raw files with an embedded JPEG preview.
pub(super) struct Raw;

impl Decoder for Raw {
    fn name(&self) -> &'static str {
        "raw"
    }

    fn formats(&self) -> &'static [Format] {
        &[Format::CameraRaw]
    }

    fn render(&self, services: &dyn Services, probe: &Probe, max_side: u32) -> Result<Thumb, ThumbError> {
        let bytes = read_file(services, probe, self.max_file_bytes())?;
        let jpeg = lightcraft_raw::embedded_preview(&bytes)
            .ok_or_else(|| ThumbError::Decode("no embedded JPEG preview in this raw file".into()))?;
        if !jpeg.starts_with(&[0xFF, 0xD8]) {
            return Err(ThumbError::Decode("the embedded preview is not a JPEG".into()));
        }
        let mut reader = ImageReader::with_format(Cursor::new(jpeg), image::ImageFormat::Jpeg);
        reader.limits(image_limits());
        let mut decoder = reader.into_decoder().map_err(image_error)?;
        let jpeg_orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        let mut image = image::DynamicImage::from_decoder(decoder).map_err(image_error)?;
        let orientation =
            if jpeg_orientation == Orientation::NoTransforms { raw_orientation(&bytes) } else { jpeg_orientation };
        drop(bytes);
        image.apply_orientation(orientation);
        Ok(from_image(image, max_side))
    }
}

/// The raw file's own orientation (TIFF/EXIF tag), `NoTransforms` when it has none.
fn raw_orientation(bytes: &[u8]) -> Orientation {
    let exif = lightcraft_meta::extract(bytes).orientation.map_or(1, |o| o.to_exif());
    u8::try_from(exif).ok().and_then(Orientation::from_exif).unwrap_or(Orientation::NoTransforms)
}

fn image_error(e: image::ImageError) -> ThumbError {
    match e {
        image::ImageError::Limits(_) => ThumbError::TooLarge,
        other => ThumbError::Decode(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use crate::{ThumbError, render_path};
    use assetcraft_fs::{FakeFs, RealFs};
    use std::path::{Path, PathBuf};

    fn sample_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/environments/clearing_raw_orient6.dng")
    }

    #[test]
    fn dng_preview_renders_upright() {
        // The 320 × 200 preview of a file tagged Orientation 6 (rotate 90° clockwise).
        let t = render_path(&RealFs, &sample_path(), 256).unwrap();
        assert_eq!((t.source_width, t.source_height), (200, 320));
        assert_eq!((t.width, t.height), (160, 256));
    }

    #[test]
    fn raw_without_a_usable_preview_is_a_decode_error() {
        let dng = std::fs::read(sample_path()).unwrap();
        // Wipe the preview JPEG's SOI marker wherever it sits.
        let soi = dng.windows(3).position(|w| w == [0xFF, 0xD8, 0xFF]).unwrap();
        let mut no_preview = dng.clone();
        no_preview[soi..soi + 2].copy_from_slice(&[0, 0]);
        let fs = FakeFs::new().file_with("/a.dng", no_preview);
        assert!(matches!(render_path(&fs, Path::new("/a.dng"), 64), Err(ThumbError::Decode(_))));
    }

    #[test]
    fn every_prefix_fails_gracefully() {
        let dng = std::fs::read(sample_path()).unwrap();
        for len in (0..dng.len()).step_by(211) {
            let fs = FakeFs::new().file_with("/p.dng", dng[..len].to_vec());
            let _ = render_path(&fs, Path::new("/p.dng"), 64); // error or image, never a panic
        }
        // Raw magic the probe knows, with nothing behind it.
        for magic in [&b"FUJIFILMCCD-RAW 0201"[..], b"IIRO\x08\0\0\0", b"IIU\0\x08\0\0\0"] {
            let fs = FakeFs::new().file_with("/m.raw", magic.to_vec());
            assert!(render_path(&fs, Path::new("/m.raw"), 64).is_err());
        }
    }
}
