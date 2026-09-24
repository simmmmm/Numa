use super::*;

pub(super) fn build_card(state: &App, photo: &Photo) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 5);

    let picture = gtk::Picture::new();

    picture.set_can_shrink(true);
    picture.set_vexpand(true);
    picture.set_content_fit(gtk::ContentFit::Cover);

    picture.connect_paintable_notify(glib::clone!(
        #[weak(rename_to = wall)] state.grid.wall,
        #[weak] card,
        move |picture| {
            let Some(paintable) = picture.paintable() else { return };
            let aspect = paintable.intrinsic_aspect_ratio();
            if aspect > 0.0 {
                wall.set_aspect(&card, aspect as f32);
            }
        }
    ));
    picture.add_css_class("thumbnail");

    picture.set_overflow(gtk::Overflow::Hidden);

    let card_thumb = LazyThumb {
        id: photo.id,
        path: photo.path.clone(),
        mtime: photo.mtime,
        edited: photo.edited,
        edge: grid_edge(state),
        asked: 0,
        fitted: 0,
        picture: picture.clone(),

        widget: picture.clone().upcast(),
        wanted: false,
    };

    let name = gtk::Label::new(photo.path.file_name().and_then(|name| name.to_str()));
    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    name.set_max_width_chars(20);
    name.add_css_class("photo-name");

    let titled = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    titled.set_halign(gtk::Align::Center);
    if photo.edited {
        let mark = gtk::Image::from_icon_name("document-edit-symbolic");
        mark.set_pixel_size(11);
        mark.add_css_class("edited-mark");
        mark.set_tooltip_text(Some("Has adjustments"));
        titled.append(&mark);
    }
    titled.append(&name);

    let badge = gtk::Label::new(Some(&badge_text(photo.rating, photo.flag)));
    badge.add_css_class("photo-badge");
    style_badge(&badge, photo.rating, photo.flag);

    let scale = state.libraries.scale.get();
    let note = cull_note(photo, &scale);
    if !note.is_empty() {
        let hint = gtk::Label::new(Some(&note));
        hint.add_css_class("cull-note");
        hint.set_tooltip_text(Some(&cull_detail(photo, &scale)));
        hint.set_ellipsize(gtk::pango::EllipsizeMode::End);
        card.append(&picture);
        card.append(&hint);
    } else {
        card.append(&picture);
    }
    card.append(&titled);
    card.append(&badge);

    badge.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let child: gtk::Widget = card.clone().upcast();
    child.set_tooltip_text(photo.path.to_str());

    child.set_widget_name(&photo.id.to_string());

    state.grid.lazy.borrow_mut().push(LazyThumb { widget: child.clone(), ..card_thumb });

    state.grid.cards.borrow_mut().insert(photo.id, (photo.clone(), badge));
    child
}

pub(super) fn style_badge(badge: &gtk::Label, rating: u8, flag: Flag) {
    badge.remove_css_class("rated");
    badge.remove_css_class("rejected");

    if flag == Flag::Rejected {
        badge.add_css_class("rejected");
    } else if rating > 0 || flag == Flag::Picked {
        badge.add_css_class("rated");
    }
}

pub(super) fn badge_text(rating: u8, flag: Flag) -> String {
    let stars: String = (1..=5).map(|n| if n <= rating { '★' } else { '☆' }).collect();
    match flag {
        Flag::Picked => format!("{stars}  ⚑"),
        Flag::Rejected => format!("{stars}  ✕"),
        Flag::None => stars,
    }
}

pub(super) fn strip_badge_text(rating: u8, flag: Flag) -> String {
    let stars = match rating {
        0 => String::new(),
        n => format!("{n}\u{2605}"),
    };
    match flag {
        Flag::Picked => format!("{stars} \u{2691}").trim().to_string(),
        Flag::Rejected => format!("{stars} \u{2715}").trim().to_string(),
        Flag::None => stars,
    }
}
