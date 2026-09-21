use image::{DynamicImage, RgbImage};
use std::path::Path;

use numa_core::document::Document;

use crate::raw;

const RENDER_EDGE: u32 = 1024;

pub(super) fn render(path: &Path, max_edge: u32, edits: &str) -> Result<RgbImage, String> {
    let document: Document =
        serde_json::from_str(edits).map_err(|err| format!("{}: {err}", path.display()))?;

    let linear = raw::decode_linear_any(path)?;
    let proxy = linear.downscaled(max_edge.max(RENDER_EDGE)).unwrap_or(linear);

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
        profile: document
            .colour_profile
            .as_deref()
            .filter(|name| *name != numa_render::NO_COLOUR_PROFILE)
            .and_then(crate::dcp::by_name),
        denoised: None,
        sharpened: None,
    }
}
