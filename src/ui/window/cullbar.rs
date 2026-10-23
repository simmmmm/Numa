use super::*;

pub(super) fn build(state: &App) -> gtk::ActionBar {
    let bar = gtk::ActionBar::new();
    bar.set_revealed(false);

    let clear = gtk::Button::from_icon_name("edit-clear-all-symbolic");
    clear.add_css_class("flat");
    clear.set_tooltip_text(Some("Clear (Esc)"));
    clear.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| state.grid.wall.unpick()
    ));
    let count = gtk::Label::new(None);
    count.add_css_class("heading");
    count.add_css_class("numeric");
    count.set_margin_start(4);
    count.set_margin_end(10);
    bar.pack_start(&clear);
    bar.pack_start(&count);

    let everything = gtk::Button::with_label("Select All");
    everything.set_tooltip_text(Some("Select All (Ctrl+A)"));
    everything.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| state.grid.wall.select_all()
    ));
    let chip = chip_row();
    chip.set_valign(gtk::Align::Center);
    chip.set_margin_end(4);
    chip.append(&everything);
    bar.pack_start(&chip);

    let mut acts: Vec<gtk::Widget> = Vec::new();

    let rate = rate_button("win.photo-rate");
    back_to_grid(state, &rate);
    bar.pack_start(&rate);
    acts.push(rate.upcast());
    for (label, icon, tip, flag) in [
        ("Pick", "emoji-flags-symbolic", "Pick (P), again to clear", Flag::Picked),
        ("Reject", "window-close-symbolic", "Reject (X), again to clear", Flag::Rejected),
    ] {
        let button = gtk::Button::new();
        button.set_child(Some(&labelled(label, icon)));
        button.add_css_class("flat");
        button.set_tooltip_text(Some(tip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| apply_to_selection(&state, toggled(&state, &selected_ids(&state), Action::Flag(flag)))
        ));
        bar.pack_start(&button);
        acts.push(button.upcast());
    }
    let line = gtk::Separator::new(gtk::Orientation::Vertical);
    line.set_margin_start(6);
    line.set_margin_end(6);
    bar.pack_start(&line);
    acts.push(line.upcast());

    let paste = gtk::Button::with_label("Paste Settings");
    paste.add_css_class("flat");
    paste.set_action_name(Some("win.photo-paste"));

    paste.set_tooltip_text(Some(
        "Paste the settings copied with Copy Settings — a photograph's right-click menu, or Ctrl+C in the editor — onto the selection (Ctrl+V)",
    ));
    bar.pack_start(&paste);
    let album = album_button(state);
    back_to_grid(state, &album);
    bar.pack_start(&album);

    acts.push(paste.upcast());

    let merge = gtk::Button::with_label("Merge…");
    merge.add_css_class("flat");
    merge.set_tooltip_text(Some("Merge the selected exposures to HDR"));
    merge.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| merge_selection(&state, button)
    ));
    bar.pack_start(&merge);

    let export = gtk::Button::new();
    export.set_tooltip_text(Some("Export the selection, choosing how"));
    export.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| export_selection(&state, button)
    ));
    bar.pack_end(&export);
    let more = more_button(state);
    bar.pack_end(&more);
    acts.extend([merge.clone().upcast(), export.clone().upcast(), more.upcast()]);

    let into = primary_button("Add to");
    into.set_visible(false);
    into.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| into_new_album(&state)
    ));
    bar.pack_end(&into);

    let update = Rc::new(glib::clone!(
        #[weak] bar,
        #[weak] count,
        #[weak] merge,
        #[weak] export,
        #[weak] album,
        #[weak] into,
        #[strong] state,
        move || {
            let chosen = state.grid.wall.selected().len();
            let number = places::grouped(chosen as i64);

            if !state.grid.wall.choosing() {
                state.libraries.new_album.replace(None);
            }
            let making = state.libraries.new_album.borrow().clone();
            count.set_text(&match (chosen, &making) {
                (0, Some(name)) => format!("Choose photographs for {name}"),
                (0, None) => "Choose photographs".to_string(),
                _ => format!("{number} chosen"),
            });
            for act in &acts {
                act.set_visible(chosen > 0);
            }
            album.set_visible(chosen > 0 && making.is_none());
            into.set_visible(chosen > 0 && making.is_some());
            if let Some(name) = &making {
                into.set_label(&format!("Add to {name}"));
            }
            merge.set_sensitive(chosen >= 2);
            export.set_label(&format!("Export {number}…"));

            let open = chosen > 0 || state.grid.wall.choosing();
            bar.set_revealed(open && !state.loupe.reveal.reveals_child());
        }
    ));
    state.grid.wall.connect_selection_changed(glib::clone!(
        #[strong] update,
        move |_| update()
    ));
    state.loupe.reveal.connect_reveal_child_notify(move |_| update());
    keep_focus(bar.upcast_ref());
    bar
}

fn more_button(state: &App) -> gtk::MenuButton {
    let more_menu = gio::Menu::new();
    more_menu.append(Some("Tonight…"), Some("win.tonight"));
    more_menu.append(Some("For a Book…"), Some("win.book"));

    more_menu.append(Some("Copy File Names"), Some("win.copy-names"));
    let more = gtk::MenuButton::new();
    more.set_icon_name("view-more-symbolic");
    more.set_has_frame(false);
    more.set_menu_model(Some(&more_menu));
    more.set_tooltip_text(Some("More"));
    back_to_grid(state, &more);
    more
}

pub(super) fn back_to_grid(state: &App, button: &gtk::MenuButton) {
    button.connect_active_notify(glib::clone!(
        #[strong] state,
        move |button| {
            if !button.is_active() && state.stack.visible_child_name().as_deref() == Some("library") {

                let wall = state.grid.wall.clone();
                glib::idle_add_local_once(move || {
                    wall.grab_focus();
                });
            }
        }
    ));
}

pub(super) fn keep_focus(root: &gtk::Widget) {
    if root.is::<gtk::Button>() {
        root.set_focus_on_click(false);
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        keep_focus(&widget);
        child = widget.next_sibling();
    }
}

fn labelled(label: &str, icon: &str) -> adw::ButtonContent {
    let content = adw::ButtonContent::new();
    content.set_label(label);
    content.set_icon_name(icon);
    content
}

pub(super) fn rate_button(action: &str) -> gtk::MenuButton {
    let menu = gio::Menu::new();
    for stars in 0..=5i32 {
        let label = match stars {
            0 => "No Rating".to_string(),
            1 => "1 Star".to_string(),
            n => format!("{n} Stars"),
        };
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some(action), Some(&stars.to_variant()));
        menu.append_item(&item);
    }
    let rate = gtk::MenuButton::new();
    rate.set_child(Some(&labelled("Rate", "non-starred-symbolic")));
    rate.set_always_show_arrow(true);
    rate.set_has_frame(false);
    rate.set_menu_model(Some(&menu));
    rate.set_tooltip_text(Some("Rate the selection (0–5)"));
    rate
}

fn album_button(state: &App) -> gtk::MenuButton {
    let album = gtk::MenuButton::new();
    album.set_label("Add to Album");
    album.set_has_frame(false);
    album.set_tooltip_text(Some("Add the selection to an album"));
    album.set_create_popup_func(glib::clone!(
        #[strong] state,
        move |button| {
            let adding = state.libraries.albums_menu.item_link(0, gio::MENU_LINK_SUBMENU);
            button.set_menu_model(adding.as_ref());
        }
    ));
    album
}
