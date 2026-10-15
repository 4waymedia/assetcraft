//! 3D models (GLB, glTF, OBJ): parsed by `assetcraft-mesh`, drawn by its CPU rasterizer from
//! the fixed 3/4 view, neutral grey (or the material's base colour) on transparent, then
//! cropped to the drawn pixels. No GPU, so it works on worker threads and headless CI.

use super::Decoder;
use crate::model::{RenderOptions, View, load};
use crate::{Thumb, ThumbError};
use assetcraft_fs::Services;
use assetcraft_probe::{Format, Probe};

/// GLB, glTF and OBJ.
pub(super) struct Model;

const FORMATS: &[Format] = &[Format::Glb, Format::Gltf, Format::Obj];

impl Decoder for Model {
    fn name(&self) -> &'static str {
        "model"
    }

    fn formats(&self) -> &'static [Format] {
        FORMATS
    }

    fn render(&self, services: &dyn Services, probe: &Probe, max_side: u32) -> Result<Thumb, ThumbError> {
        let mesh = load(services, probe)?;
        let side = max_side.clamp(1, crate::SIDE_RANGE.1);
        let image = assetcraft_mesh::render(&mesh, View::THREE_QUARTER, &RenderOptions::new(side, side))?;
        let image = image.crop_to_content(0.04);
        Ok(Thumb {
            width: image.width,
            height: image.height,
            source_width: image.width,
            source_height: image.height,
            rgba: image.rgba,
        })
    }
}
