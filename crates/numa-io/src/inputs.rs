use std::path::Path;

use numa_core::document::Document;
use numa_render::RenderInputs;

pub fn render_inputs(document: &Document) -> RenderInputs {
    RenderInputs {
        profile: chosen_profile(document),
        denoised: (document.ai_denoise > 0.0).then(|| crate::denoised::load(Path::new(&document.source.path))).flatten(),

        sharpened: (document.ai_sharpen > 0.0)
            .then(|| crate::denoised::load_sharpened(Path::new(&document.source.path), document.ai_denoise > 0.0))
            .flatten(),
    }
}

pub fn chosen_profile(document: &Document) -> Option<std::sync::Arc<numa_core::profile::DngProfile>> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static FITS: OnceLock<Mutex<HashMap<(String, String), bool>>> = OnceLock::new();
    let name = document.colour_profile.as_deref().filter(|name| *name != numa_render::NO_COLOUR_PROFILE)?;
    let fits = *FITS
        .get_or_init(Default::default)
        .lock()
        .ok()?
        .entry((document.source.path.clone(), name.to_string()))
        .or_insert_with(|| match crate::raw::summary(Path::new(&document.source.path)) {
            Some(camera) if !camera.model.is_empty() => {
                crate::dcp::names_for_camera(&camera.make, &camera.model).iter().any(|(own, _)| own == name)
            }
            _ => true,
        });
    fits.then(|| crate::dcp::by_name(name)).flatten()
}
