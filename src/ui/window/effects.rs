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
        let row = slider_row(state, name, scale, *readout);

        if *name == "Clarity" {
            row.add_css_class("lead");
        }
        effects.append(&row);
    }

    for (title, range, frame_only) in [("Mist", 39..40, true), ("Vignette", 23..27, true), ("Grain", 27..30, false)] {
        let header = section_header(title);
        if frame_only {
            global_only(header.as_ref());
        }
        effects.append(&header);
        for (index, (name, scale, readout)) in all[range].iter().enumerate() {
            let row = slider_row(state, name, scale, *readout);

            if index == 0 {
                row.add_css_class("lead");
            }
            if frame_only {
                global_only(row.as_ref());
            }
            effects.append(&row);
        }
    }

    effects
}
