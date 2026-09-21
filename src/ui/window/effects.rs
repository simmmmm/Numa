use super::*;

pub(super) fn build_effects(
    state: &App,
    all: &[(&'static str, &gtk::Scale, Readout); SLIDER_COUNT],
    global_only: &dyn Fn(&gtk::Widget),
) -> gtk::Box {
    let effects = page_column();
    effects.add_css_class("quiet");

    let presence_header = section_header("Presence");
    effects.append(&presence_header);

    for (name, scale, readout) in all[11..13].iter().chain(&all[22..23]) {
        effects.append(&slider_row(state, name, scale, *readout));
    }

    for (title, range) in [("Vignette", 23..27), ("Grain", 27..30)] {
        let header = section_header(title);
        global_only(header.as_ref());
        effects.append(&header);
        for (name, scale, readout) in &all[range] {
            let row = slider_row(state, name, scale, *readout);
            global_only(row.as_ref());
            effects.append(&row);
        }
    }
    effects
}
