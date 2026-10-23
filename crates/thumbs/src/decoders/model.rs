//! 3D models (GLB, glTF, OBJ): parsed by `assetcraft-mesh`, drawn by its CPU rasterizer from
//! the fixed 3/4 view, smooth-shaded with the base colour texture (or the material's colour,
//! or neutral grey) on transparent, then cropped to the drawn pixels. No GPU, so it works on
//! worker threads and headless CI.

use super::Decoder;
use crate::model::{RenderOptions, View, load_model};
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
        let model = load_model(services, probe)?;
        let side = max_side.clamp(1, crate::SIDE_RANGE.1);
        let image = assetcraft_mesh::render_with(
            &model.mesh,
            &model.textures,
            View::THREE_QUARTER,
            &RenderOptions::new(side, side),
        )?;
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
