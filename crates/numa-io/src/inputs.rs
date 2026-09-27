use std::path::Path;

use numa_core::document::Document;
use numa_render::RenderInputs;

pub fn render_inputs(document: &Document) -> RenderInputs {
    RenderInputs {
        profile: document
            .colour_profile
            .as_deref()
            .filter(|name| *name != numa_render::NO_COLOUR_PROFILE)
            .and_then(crate::dcp::by_name),
        denoised: (document.ai_denoise > 0.0).then(|| crate::denoised::load(Path::new(&document.source.path))).flatten(),

        sharpened: (document.ai_sharpen > 0.0)
            .then(|| crate::denoised::load_sharpened(Path::new(&document.source.path), document.ai_denoise > 0.0))
            .flatten(),
    }
}
