use super::*;

pub(super) fn section_header(title: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(&title.to_uppercase()));
    label.set_xalign(0.0);
    label.add_css_class("section-header");
    label
}

pub(super) fn cardify(column: &gtk::Box) {

    let mut child = column.first_child();
    while let Some(this) = child {
        child = this.next_sibling();
        if let Some(inner) = holds_sections(&this) {
            if inner.first_child().is_some_and(|first| header_label(&first).is_none()) {
                let mut at = this.prev_sibling();
                let mut moving = inner.first_child();
                while let Some(widget) = moving {
                    moving = widget.next_sibling();
                    inner.remove(&widget);
                    column.insert_child_after(&widget, at.as_ref());
                    at = Some(widget);
                }

                column.remove(&this);
            }
        }
    }
    let mut children = Vec::new();
    let mut child = column.first_child();
    while let Some(this) = child {
        child = this.next_sibling();
        children.push(this);
    }
    let mut card: Option<gtk::Box> = None;
    for widget in children {
        if let Some(label) = header_label(&widget) {
            let line = header_line(column, &widget, &label);
            let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
            body.add_css_class("panel-card");
            column.insert_child_after(&body, Some(&line));
            card = Some(body);
            continue;
        }

        if let Some(inner) = holds_sections(&widget) {
            card = None;
            cardify(&inner);
            continue;
        }
        let Some(body) = &card else { continue };
        column.remove(&widget);
        body.append(&widget);
    }

    let mut child = column.first_child();
    while let Some(this) = child {
        child = this.next_sibling();
        let Ok(body) = this.clone().downcast::<gtk::Box>() else { continue };
        if !body.has_css_class("panel-card") {
            continue;
        }
        follow_rows(&body);
        if let Some(line) = body.prev_sibling().and_downcast::<gtk::Box>() {
            section_reset(&line, &body);

            body.bind_property("visible", &line, "visible").sync_create().build();
        }
    }
}

fn holds_sections(widget: &gtk::Widget) -> Option<gtk::Box> {
    let inner = widget.downcast_ref::<gtk::Box>()?;
    if inner.orientation() != gtk::Orientation::Vertical || inner.has_css_class("panel-card") {
        return None;
    }
    let mut child = inner.first_child();
    while let Some(this) = child {
        if header_label(&this).is_some() || holds_sections(&this).is_some() {
            return Some(inner.clone());
        }
        child = this.next_sibling();
    }
    None
}

fn header_label(widget: &gtk::Widget) -> Option<gtk::Label> {
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        return label.has_css_class("section-header").then(|| label.clone());
    }
    let line = widget.downcast_ref::<gtk::Box>()?;
    if line.orientation() != gtk::Orientation::Horizontal {
        return None;
    }
    let mut child = line.first_child();
    while let Some(this) = child {
        if let Some(label) = this.downcast_ref::<gtk::Label>() {
            if label.has_css_class("section-header") {
                return Some(label.clone());
            }
        }
        child = this.next_sibling();
    }
    None
}

fn header_line(column: &gtk::Box, widget: &gtk::Widget, label: &gtk::Label) -> gtk::Box {
    let line = match widget.clone().downcast::<gtk::Box>() {
        Ok(line) => line,
        Err(_) => {
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            column.insert_child_after(&line, Some(widget));
            column.remove(widget);
            line.append(label);

            label.bind_property("visible", &line, "visible").sync_create().build();
            line
        }
    };
    line.set_spacing(8);
    line.add_css_class("section-line");

    let mut taken = false;
    let mut child = line.first_child();
    while let Some(this) = child {
        taken |= this != *label.upcast_ref::<gtk::Widget>() && this.hexpands();
        child = this.next_sibling();
    }
    label.set_hexpand(!taken);
    label.set_valign(gtk::Align::Center);
    line
}

fn follow_rows(body: &gtk::Box) {
    let update = glib::clone!(
        #[weak] body,
        move || {
            let (mut any, mut controls) = (false, false);
            let mut child = body.first_child();
            while let Some(this) = child {

                if this.get_visible() {
                    any = true;
                    controls |= match this.downcast_ref::<gtk::ListBox>() {
                        Some(list) => list.first_child().is_some(),
                        None => !this.is::<gtk::Label>(),
                    };
                }
                child = this.next_sibling();
            }
            body.set_visible(any);
            match controls {
                true => body.remove_css_class("bare"),
                false => body.add_css_class("bare"),
            }
        }
    );
    let mut child = body.first_child();
    while let Some(this) = child {
        let update = update.clone();
        this.connect_visible_notify(move |_| update());
        child = this.next_sibling();
    }
    update();
}

fn section_reset(line: &gtk::Box, body: &gtk::Box) {
    fn scales(widget: &gtk::Widget, found: &mut Vec<gtk::Scale>) {
        if let Some(scale) = widget.downcast_ref::<gtk::Scale>() {
            found.push(scale.clone());
        }
        let mut child = widget.first_child();
        while let Some(this) = child {
            scales(&this, found);
            child = this.next_sibling();
        }
    }
    let mut found = Vec::new();
    scales(body.upcast_ref(), &mut found);
    found.retain(|scale| {
        neutral_of(scale).is_some() && RESETS.with(|resets| resets.borrow().contains_key(&(scale.as_ptr() as usize)))
    });
    if found.is_empty() {
        return;
    }
    let title = header_label(line.upcast_ref()).map(|label| label.text().to_string()).unwrap_or_default();
    let words = title.split(' ').map(|word| {
        let mut chars = word.chars();
        chars.next().map(|first| first.to_string() + &chars.as_str().to_lowercase()).unwrap_or_default()
    });
    let tip = format!("Reset {}", words.collect::<Vec<_>>().join(" "));
    let button = gtk::Button::from_icon_name("edit-undo-symbolic");
    button.add_css_class("flat");
    button.add_css_class("panel-accessory");
    button.set_tooltip_text(Some(&tip));
    button.update_property(&[gtk::accessible::Property::Label(&tip)]);
    button.set_valign(gtk::Align::Center);
    let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    slot.add_css_class("reset-slot");
    slot.set_size_request(24, 24);
    slot.append(&button);
    line.append(&slot);
    let moved = Rc::new(found);
    let update = glib::clone!(
        #[strong] moved,
        #[weak] button,
        move || {
            let off = moved.iter().any(|scale| match neutral_of(scale) {
                Some(rest) => (scale.value() - rest).abs() > 1e-4,
                None => false,
            });

            button.set_visible(off);
        }
    );
    for scale in moved.iter() {
        let update = update.clone();
        scale.connect_value_changed(move |_| update());
    }
    update();
    button.connect_clicked(glib::clone!(
        #[strong] moved,
        move |_| {
            for scale in moved.iter() {
                let reset = RESETS.with(|resets| resets.borrow().get(&(scale.as_ptr() as usize)).cloned());
                if let Some(reset) = reset {
                    reset();
                }
            }
        }
    ));
}

pub(super) fn named_row(name: &str, end: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

    row.add_css_class("named-row");
    let title = gtk::Label::new(Some(name));
    title.set_xalign(0.0);
    title.set_hexpand(!end.hexpands());
    title.add_css_class("slider-name");
    end.set_valign(gtk::Align::Center);
    row.append(&title);
    row.append(end);
    row
}

pub(super) fn primary_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    primary(&button);
    button
}

pub(super) fn primary(button: &impl IsA<gtk::Widget>) {
    button.add_css_class("numa-primary");
    button.add_css_class("opaque");
}

pub(super) fn dialog_header(dialog: &adw::Dialog, primary: &impl IsA<gtk::Widget>) -> adw::HeaderBar {
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            dialog.close();
        }
    ));
    header.pack_start(&cancel);
    header.pack_end(primary);
    header
}

pub(super) fn whole_on_hover(label: &gtk::Label) {
    label.set_has_tooltip(true);
    label.connect_query_tooltip(|label, _, _, _, tip| {
        let cut = label.layout().is_ellipsized();
        if cut {
            tip.set_text(Some(&label.text()));
        }
        cut
    });
}

pub(super) fn keep_whole(label: &gtk::Label, most: i32) {
    label.set_size_request(-1, -1);
    let (_, whole, _, _) = label.measure(gtk::Orientation::Horizontal, -1);
    label.set_size_request(whole.min(most), -1);
}

pub(super) const LIBRARY_NAME_LEAST: i32 = 72;

pub(super) fn pin_foot(scroller: &gtk::ScrolledWindow, foot: &gtk::Box) {
    let rule = glib::clone!(
        #[weak] foot,
        move |adjustment: &gtk::Adjustment| {
            match adjustment.value() + adjustment.page_size() < adjustment.upper() - 1.0 {
                true => foot.add_css_class("ruled"),
                false => foot.remove_css_class("ruled"),
            }
        }
    );
    let adjustment = scroller.vadjustment();
    adjustment.connect_value_changed(rule.clone());
    adjustment.connect_changed(rule);
}

pub(super) fn chip_row() -> gtk::Box {

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.add_css_class("chip-row");
    row
}

thread_local! {

    static GLIDES: RefCell<HashMap<usize, adw::TimedAnimation>> = RefCell::new(HashMap::new());
}

pub(super) fn glide(widget: &impl IsA<gtk::Widget>, adjustment: &gtk::Adjustment, to: f64) {
    let key = adjustment.as_ptr() as usize;
    let target = adw::PropertyAnimationTarget::new(adjustment, "value");
    let animation = adw::TimedAnimation::new(widget, adjustment.value(), to, 200, target);
    animation.set_easing(adw::Easing::EaseOutCubic);
    animation.connect_done(move |_| {
        GLIDES.with(|glides| glides.borrow_mut().remove(&key));
    });
    if let Some(earlier) = GLIDES.with(|glides| glides.borrow_mut().insert(key, animation.clone())) {
        earlier.pause();
    }
    animation.play();
}
