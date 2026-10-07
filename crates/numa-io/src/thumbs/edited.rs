use image::{DynamicImage, RgbImage};
use std::path::Path;

use numa_core::document::Document;

use crate::raw;

const RENDER_EDGE: u32 = 1024;

pub(super) fn render(path: &Path, max_edge: u32, edits: &str) -> Result<RgbImage, String> {
    let mut document: Document =
        serde_json::from_str(edits).map_err(|err| format!("{}: {err}", path.display()))?;

    if let Err(why) = crate::camera_look::fill(&mut document) {
        log::info!("{}: As Shot left out: {why}", path.display());
    }

    let (proxy, _) = raw::proxy_from_mosaic(path, max_edge.max(RENDER_EDGE))?;

    let document = numa_render::with_masks_resolved(&document, &proxy);

    let inputs = inputs(&document);
    let rendered = DynamicImage::ImageRgb8(numa_render::develop(&document, proxy, &inputs));

    Ok(match rendered.width().max(rendered.height()) > max_edge {
        true => rendered.thumbnail(max_edge, max_edge),
        false => rendered,
    }
    .into_rgb8())
}

fn inputs(document: &Document) -> numa_render::RenderInputs {
    numa_render::RenderInputs {
        profile: crate::inputs::chosen_profile(document),
        denoised: None,
        sharpened: None,
    }
}
