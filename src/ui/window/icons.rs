use super::*;

pub(super) fn drawn_icon(draw: impl Fn(&gtk::cairo::Context, f64) + 'static) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(19);
    area.set_content_height(19);
    area.set_halign(gtk::Align::Center);
    area.set_valign(gtk::Align::Center);
    area.set_draw_func(move |area, context, width, height| {
        let colour = area.color();
        context.set_source_rgba(
            colour.red() as f64,
            colour.green() as f64,
            colour.blue() as f64,
            colour.alpha() as f64,
        );
        let scale = (width.min(height) as f64 / 16.0).max(0.1);
        let _ = context.save();
        context.translate(
            (width as f64 - 16.0 * scale) / 2.0,
            (height as f64 - 16.0 * scale) / 2.0,
        );
        draw(context, scale);
        let _ = context.restore();
    });
    area
}

pub(super) fn rail_icon(name: &str) -> gtk::DrawingArea {
    let name = name.to_string();
    drawn_icon(move |context, scale| {

        let u = 16.0 / 24.0 * scale;
        let at = |x: f64, y: f64| (x * u, y * u);
        let line = |context: &gtk::cairo::Context, points: &[(f64, f64)]| {
            for (index, (x, y)) in points.iter().enumerate() {
                let (x, y) = at(*x, *y);
                match index {
                    0 => context.move_to(x, y),
                    _ => context.line_to(x, y),
                }
            }
        };
        let circle = |context: &gtk::cairo::Context, cx: f64, cy: f64, r: f64| {
            let (cx, cy) = at(cx, cy);
            context.new_sub_path();
            context.arc(cx, cy, r * u, 0.0, std::f64::consts::PI * 2.0);
        };
        let curve = |context: &gtk::cairo::Context, c1: (f64, f64), c2: (f64, f64), to: (f64, f64)| {
            let (c1, c2, to) = (at(c1.0, c1.1), at(c2.0, c2.1), at(to.0, to.1));
            context.curve_to(c1.0, c1.1, c2.0, c2.1, to.0, to.1);
        };

        context.set_line_width(1.7 * u);
        context.set_line_cap(gtk::cairo::LineCap::Round);
        context.set_line_join(gtk::cairo::LineJoin::Round);

        match name.as_str() {
            "presets" => {
                line(context, &[
                    (12.0, 3.5), (14.6, 8.9), (20.5, 9.7), (16.2, 13.8), (17.3, 19.7),
                    (12.0, 16.9), (6.7, 19.7), (7.8, 13.8), (3.5, 9.7), (9.4, 8.9),
                ]);
                context.close_path();
                let _ = context.stroke();
            }
            "light" => {
                circle(context, 12.0, 12.0, 4.0);
                for ray in [
                    ((12.0, 2.0), (12.0, 5.0)), ((12.0, 19.0), (12.0, 22.0)),
                    ((2.0, 12.0), (5.0, 12.0)), ((19.0, 12.0), (22.0, 12.0)),
                    ((4.9, 4.9), (7.0, 7.0)), ((17.0, 17.0), (19.1, 19.1)),
                    ((4.9, 19.1), (7.0, 17.0)), ((17.0, 7.0), (19.1, 4.9)),
                ] {
                    line(context, &[ray.0, ray.1]);
                }
                let _ = context.stroke();
            }

            "colour" => {
                let (x, y) = at(12.0, 3.0);
                context.move_to(x, y);
                curve(context, (12.0, 3.0), (18.0, 9.5), (18.0, 14.0));
                let (cx, cy) = at(12.0, 14.0);
                context.arc(cx, cy, 6.0 * u, 0.0, std::f64::consts::PI);
                curve(context, (6.0, 9.5), (12.0, 3.0), (12.0, 3.0));
                context.close_path();
                let _ = context.stroke();
            }
            "effects" => {
                line(context, &[
                    (12.0, 4.0), (13.8, 9.2), (19.0, 11.0), (13.8, 12.8),
                    (12.0, 18.0), (10.2, 12.8), (5.0, 11.0), (10.2, 9.2),
                ]);
                context.close_path();
                line(context, &[
                    (5.0, 4.0), (5.7, 6.0), (7.7, 6.7), (5.7, 7.4),
                    (5.0, 9.5), (4.3, 7.4), (2.3, 6.7), (4.3, 6.0),
                ]);
                context.close_path();
                let _ = context.stroke();
            }
            "grade" => {
                circle(context, 12.0, 12.0, 8.0);
                let _ = context.stroke();
                let (cx, cy) = at(12.0, 12.0);
                context.new_sub_path();
                context.arc(
                    cx, cy, 8.0 * u,
                    -std::f64::consts::FRAC_PI_2,
                    std::f64::consts::FRAC_PI_2,
                );
                context.close_path();
                let _ = context.fill();
            }
            "detail" => {
                circle(context, 11.0, 11.0, 6.5);
                line(context, &[(16.0, 16.0), (21.0, 21.0)]);
                let _ = context.stroke();
            }
            "masks" | "mask" => {
                context.set_dash(&[3.0 * u, 3.0 * u], 0.0);
                circle(context, 12.0, 12.0, 8.0);
                let _ = context.stroke();
                context.set_dash(&[], 0.0);
                circle(context, 12.0, 12.0, 3.5);
                let _ = context.fill();
            }
            "retouch" => {
                circle(context, 12.0, 12.0, 8.5);
                let _ = context.stroke();

                for eye in [(9.0, 10.0), (15.0, 10.0)] {
                    circle(context, eye.0, eye.1, 0.85);
                    let _ = context.fill();
                }
                let (x, y) = at(9.0, 14.5);
                context.move_to(x, y);
                curve(context, (9.0, 14.5), (10.0, 16.0), (12.0, 16.0));
                curve(context, (14.0, 16.0), (15.0, 14.5), (15.0, 14.5));
                let _ = context.stroke();
            }

            _ => {
                line(context, &[(7.0, 3.0), (7.0, 17.0), (21.0, 17.0)]);
                line(context, &[(3.0, 7.0), (17.0, 7.0), (17.0, 21.0)]);
                let _ = context.stroke();
            }
        }
    })
}

pub(super) fn build_rail(state: &App) -> gtk::Box {

    let tabs = state.panel.tab_strip.clone();
    tabs.set_orientation(gtk::Orientation::Vertical);
    tabs.add_css_class("rail");
    tabs.set_spacing(4);
    tabs.set_valign(gtk::Align::Start);

    let mut first: Option<gtk::ToggleButton> = None;
    for (name, label, tooltip, _) in PANEL_TABS {
        let tab = gtk::ToggleButton::new();
        tab.set_tooltip_text(Some(tooltip));
        tab.add_css_class("rail-tab");

        tab.set_halign(gtk::Align::Center);
        tab.set_size_request(54, 54);

        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("rail-dot");
        dot.set_halign(gtk::Align::End);
        dot.set_valign(gtk::Align::Start);
        dot.set_margin_top(7);
        dot.set_margin_end(8);
        dot.set_visible(false);
        RAIL_DOTS.with(|dots| dots.borrow_mut().push((name, dot.clone())));

        let stack = gtk::Box::new(gtk::Orientation::Vertical, 1);
        stack.set_halign(gtk::Align::Center);
        stack.set_valign(gtk::Align::Center);
        let picture = rail_icon(name);
        let caption = gtk::Label::new(Some(label));
        caption.add_css_class("rail-label");
        caption.set_ellipsize(gtk::pango::EllipsizeMode::End);
        caption.set_max_width_chars(8);
        stack.append(&picture);
        stack.append(&caption);
        let over = gtk::Overlay::new();
        over.set_child(Some(&stack));
        over.add_overlay(&dot);
        tab.set_child(Some(&over));

        match &first {
            None => first = Some(tab.clone()),
            Some(first) => tab.set_group(Some(first)),
        }

        tab.set_active(name == "light");
        tab.connect_toggled(glib::clone!(
            #[strong] state,
            move |tab| {
                if tab.is_active() && !state.applying.get() {
                    show_panel_tab(&state, name);
                }
            }
        ));
        state.panel.tabs.borrow_mut().push((name, tab.clone()));
        tabs.append(&tab);
    }

    tabs
}
