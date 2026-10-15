//! HEIF / HEIC through LightCraft's `lightcraft-heif` (heic-rs: pure-Rust HEVC, panic-guarded).
//!
//! The container's declared size is checked before any picture is decoded. When the file carries
//! its own thumbnail image at least `max_side` on its long edge with the primary image's aspect
//! (iPhones store one of 320 × 240), that is decoded instead of the full photo. Rotation, mirror
//! and crop from the container are applied. Built only with the `heic` feature (off by default), and used only while
//! [`crate::DecodeOptions::heic`] is on.
//! AVIF (AV1-coded HEIF) is not covered: no pure-Rust AV1 decoder is in the sibling apps yet.

use super::{Decoder, read_file};
use crate::{MAX_DECODE_ALLOC, MAX_IMAGE_SIDE, Thumb, ThumbError};
use assetcraft_fs::Services;
use assetcraft_probe::{Format, Probe};
use lightcraft_heif::{Decoded, Error, Options};

/// HEIF and HEIC stills.
pub(super) struct Heif;

/// Bytes one decoded pixel can hold at once: 16-bit RGBA output plus the coded YCbCr planes.
const BYTES_PER_PIXEL: u64 = 16;

impl Decoder for Heif {
    fn name(&self) -> &'static str {
        "heif"
    }

    fn formats(&self) -> &'static [Format] {
        &[Format::Heif]
    }

    fn render(&self, services: &dyn Services, probe: &Probe, max_side: u32) -> Result<Thumb, ThumbError> {
        let bytes = read_file(services, probe, self.max_file_bytes())?;
        let info = lightcraft_heif::probe(&bytes).map_err(heif_error)?;
        if info.width > MAX_IMAGE_SIDE || info.height > MAX_IMAGE_SIDE {
            return Err(ThumbError::TooLarge);
        }
        let options = Options { max_pixels: MAX_DECODE_ALLOC / BYTES_PER_PIXEL, apply_transforms: true };
        if u64::from(info.width) * u64::from(info.height) > options.max_pixels {
            return Err(ThumbError::TooLarge);
        }
        let aspect = |w: u32, h: u32| f64::from(w) / f64::from(h.max(1));
        let thumbnail = if max_side < info.width.max(info.height) {
            // Only the same picture: same orientation and aspect within 1 %.
            lightcraft_heif::decode_thumbnail(&bytes, max_side, &options)
                .ok()
                .flatten()
                .filter(|t| (aspect(t.width, t.height) / aspect(info.width, info.height) - 1.0).abs() < 0.01)
        } else {
            None
        };
        let decoded = match thumbnail {
            Some(t) => t,
            None => lightcraft_heif::decode(&bytes, &options).map_err(heif_error)?,
        };
        to_thumb(decoded, info.width, info.height)
    }
}

/// 16-bit RGB(A) to RGBA8.
fn to_thumb(decoded: Decoded, source_width: u32, source_height: u32) -> Result<Thumb, ThumbError> {
    let channels = if decoded.has_alpha { 4 } else { 3 };
    let pixels = (decoded.width as usize).checked_mul(decoded.height as usize);
    if pixels.and_then(|n| n.checked_mul(channels)) != Some(decoded.data.len()) {
        return Err(ThumbError::Decode("HEIF decoder returned a malformed image".into()));
    }
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(pixels.unwrap_or(0) * 4).map_err(|_| ThumbError::TooLarge)?;
    let to8 = |v: u16| (v >> 8) as u8;
    if decoded.has_alpha {
        rgba.extend(decoded.data.iter().map(|&v| to8(v)));
    } else {
        for px in decoded.data.as_chunks::<3>().0 {
            rgba.extend([to8(px[0]), to8(px[1]), to8(px[2]), 255]);
        }
    }
    Ok(Thumb { width: decoded.width, height: decoded.height, source_width, source_height, rgba })
}

fn heif_error(e: Error) -> ThumbError {
    match e {
        Error::Limit(_) => ThumbError::TooLarge,
        other => ThumbError::Decode(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use crate::{DecodeOptions, ThumbError, ThumbKey, render_with};
    use assetcraft_fs::{FakeFs, RealFs, Services};
    use assetcraft_probe::Format;
    use std::path::{Path, PathBuf};

    const ON: DecodeOptions = DecodeOptions { heic: true };

    fn render_path(services: &dyn Services, path: &Path, side: u32) -> Result<crate::Thumb, ThumbError> {
        render_with(services, &ThumbKey::new(path, side), &ON)
    }

    #[test]
    fn heic_is_unsupported_until_the_switch_is_on() {
        let off = crate::render_path(&RealFs, &sample_path(), 128);
        assert_eq!(off.err(), Some(ThumbError::Unsupported(Format::Heif)));
        assert!(render_path(&RealFs, &sample_path(), 128).is_ok());
    }

    fn sample_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/environments/lantern_dusk.heic")
    }

    #[test]
    fn heic_renders() {
        let t = render_path(&RealFs, &sample_path(), 128).unwrap();
        assert_eq!((t.width, t.height, t.source_width, t.source_height), (128, 80, 256, 160));
        // Top-left is the dark-blue sky (10, 20, 40) after lossy HEVC: blue clearly dominates red.
        let (r, b) = (t.rgba[0], t.rgba[2]);
        assert!(b > r + 10 && t.rgba[3] == 255, "{:?}", &t.rgba[..4]);
    }

    #[test]
    fn every_prefix_and_bit_flip_fails_gracefully() {
        let heic = std::fs::read(sample_path()).unwrap();
        for len in (0..heic.len()).step_by(53) {
            let fs = FakeFs::new().file_with("/p.heic", heic[..len].to_vec());
            assert!(render_path(&fs, Path::new("/p.heic"), 64).is_err(), "{len}");
        }
        for at in (12..heic.len()).step_by(37) {
            let mut flipped = heic.clone();
            flipped[at] ^= 0x5A;
            let fs = FakeFs::new().file_with("/f.heic", flipped);
            let _ = render_path(&fs, Path::new("/f.heic"), 64); // error or image, never a panic
        }
    }
}
