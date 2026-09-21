use super::*;

thread_local! {

    static LENS: RefCell<Option<(gtk::Label, gtk::Box)>> = const { RefCell::new(None) };
}

pub(super) fn build_detail(
    state: &App,
    all: &[(&'static str, &gtk::Scale, Readout); SLIDER_COUNT],
    global_only: &dyn Fn(&gtk::Widget),
) -> gtk::Box {
    let detail = page_column();

    detail.add_css_class("quiet");

    let sharpening = section_header("Sharpening");
    detail.append(&sharpening);
    for (name, scale, readout) in &all[13..16] {
        let row = slider_row(state, name, scale, *readout);
        if *name == "Sharpening" {
            row.add_css_class("lead");
        } else {
            global_only(row.as_ref());
        }
        detail.append(&row);
    }

    let sharpen = ai_denoise::build(state, ai_denoise::Pass::Sharpen);
    global_only(sharpen.as_ref());
    detail.append(&sharpen);
    let noise = section_header("Noise");
    detail.append(&noise);

    for (name, scale, readout) in all[16..18].iter().chain(&all[19..20]) {
        let row = slider_row(state, name, scale, *readout);

        if *name == "Detail" {
            global_only(row.as_ref());
        }
        detail.append(&row);
    }
    let ai = ai_denoise::build(state, ai_denoise::Pass::Denoise);
    global_only(ai.as_ref());
    detail.append(&ai);

    let lens = section_header("Lens");
    detail.append(&lens);
    for (name, scale, readout) in &all[20..22] {
        detail.append(&slider_row(state, name, scale, *readout));
    }

    let note = gtk::Label::new(None);
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.set_margin_top(8);
    note.set_margin_bottom(4);
    note.add_css_class("profile-note");
    global_only(note.as_ref());
    detail.append(&note);

    let scope = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let manual = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for (name, scale, readout) in &all[37..39] {
        manual.append(&slider_row(state, name, scale, *readout));
    }
    scope.append(&manual);
    global_only(scope.as_ref());
    detail.append(&scope);
    LENS.set(Some((note, manual)));

    lens.set_tooltip_text(Some(
        "These act only where the fault is — a fringed edge, a moiré pattern — \
         and on a frame without one they correctly do nothing. Zoom to 100 % on \
         a hard edge to judge them.",
    ));
    sharpening.set_tooltip_text(Some(
        "Below 100 % these work on detail smaller than a screen pixel. Zoom in to judge them.",
    ));
    noise.set_tooltip_text(Some(
        "Below 100 % these work on detail smaller than a screen pixel. Zoom in to judge them.",
    ));
    detail
}

pub(super) fn write_lens(state: &App) {
    let Some((note, manual)) = LENS.with_borrow(|lens| lens.clone()) else { return };
    let open = state.open.borrow();
    let corrected = open.as_ref().is_some_and(|photo| photo.lens_corrected);
    let by_hand = open.as_ref().is_some_and(|photo| {
        let basic = photo.document.basic();
        basic.optics.lens_distortion != 0.0 || basic.optics.lens_vignetting != 0.0
    });
    let lens = open.as_ref().and_then(|photo| photo.summary.as_ref()).and_then(|s| s.lens.clone());

    note.set_text(&match (corrected, lens) {
        (true, Some(lens)) => format!("Corrected for {lens}"),
        (true, None) => "Corrected from the profile the camera recorded".to_string(),
        (false, _) => "No lens profile for this frame".to_string(),
    });
    manual.set_visible(!corrected || by_hand);
}
