use super::*;

const KEEP_AFTER_MS: u64 = 900;

pub(super) fn caption_group(state: &App, id: i64) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Caption");

    let text = gtk::TextView::new();
    text.set_wrap_mode(gtk::WrapMode::WordChar);
    text.set_accepts_tab(false);
    text.set_top_margin(10);
    text.set_bottom_margin(10);
    text.set_left_margin(12);
    text.set_right_margin(12);
    text.set_tooltip_text(Some("Write what you remember of this photo"));
    text.update_property(&[gtk::accessible::Property::Label("Caption")]);
    let buffer = text.buffer();
    buffer.set_text(&state.catalog.caption(id).ok().flatten().unwrap_or_default());

    let scroll = gtk::ScrolledWindow::builder()
        .child(&text)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(220)
        .build();
    scroll.add_css_class("card");

    let hint = gtk::Label::new(Some("Write what you remember"));
    hint.add_css_class("dim-label");
    hint.set_halign(gtk::Align::Start);
    hint.set_valign(gtk::Align::Start);
    hint.set_margin_top(text.top_margin());
    hint.set_margin_start(text.left_margin());
    hint.set_can_target(false);
    hint.set_visible(buffer.char_count() == 0);
    let field = gtk::Overlay::new();
    field.set_child(Some(&scroll));
    field.add_overlay(&hint);
    group.add(&field);

    let kept = Rc::new(RefCell::new(numa::io::catalog::tidy_caption(&words_of(&buffer))));
    let keep = {
        let (state, buffer, kept) = (state.clone(), buffer.clone(), kept.clone());
        move || {
            let words = numa::io::catalog::tidy_caption(&words_of(&buffer));
            if *kept.borrow() == words {
                return false;
            }
            match state.catalog.set_caption(id, &words) {
                Ok(()) => drop(kept.replace(words)),
                Err(err) => state.toast(&format!("The caption was not kept: {err}")),
            }
            true
        }
    };

    let waiting: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    buffer.connect_changed(glib::clone!(
        #[strong] waiting,
        #[strong] keep,
        #[weak] hint,
        move |buffer| {
            hint.set_visible(buffer.char_count() == 0);

            let limit = numa::io::catalog::CAPTION_LIMIT as i32;
            if buffer.char_count() > limit {
                let (mut from, mut to) = (buffer.iter_at_offset(limit), buffer.end_iter());
                buffer.delete(&mut from, &mut to);
                return;
            }
            if let Some(timer) = waiting.take() {
                timer.remove();
            }
            let (again, keep) = (waiting.clone(), keep.clone());
            let timer = glib::timeout_add_local_once(std::time::Duration::from_millis(KEEP_AFTER_MS), move || {
                again.take();
                keep();
            });
            waiting.replace(Some(timer));
        }
    ));

    let leave = {
        let (state, waiting) = (state.clone(), waiting.clone());
        move || {
            if let Some(timer) = waiting.take() {
                timer.remove();
            }
            if keep() {

                let state = state.clone();
                glib::idle_add_local_once(move || home_sync::edited(&state, numa::io::catalog::library_of(id)));
            }
        }
    };
    let focus = gtk::EventControllerFocus::new();
    focus.connect_leave(glib::clone!(
        #[strong] leave,
        move |_| leave()
    ));
    text.add_controller(focus);
    text.connect_unrealize(glib::clone!(
        #[strong] leave,
        move |_| leave()
    ));

    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[weak] text,
        #[upgrade_or] glib::Propagation::Proceed,
        move |_, key, _, _| {
            if key != gtk::gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            if let Some(root) = text.root() {
                root.set_focus(gtk::Widget::NONE);
            }
            glib::Propagation::Stop
        }
    ));
    text.add_controller(keys);
    group
}

fn words_of(buffer: &gtk::TextBuffer) -> String {
    buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string()
}
