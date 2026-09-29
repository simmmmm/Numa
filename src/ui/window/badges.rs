use super::*;

pub(super) fn make_card() -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 5);

    let picture = gtk::Picture::new();

    picture.set_can_shrink(true);
    picture.set_vexpand(true);
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.add_css_class("thumbnail");

    picture.set_overflow(gtk::Overflow::Hidden);

    let hint = gtk::Label::new(None);
    hint.add_css_class("cull-note");
    hint.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let titled = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    titled.set_halign(gtk::Align::Center);
    let mark = gtk::Image::from_icon_name("document-edit-symbolic");
    mark.set_pixel_size(11);
    mark.add_css_class("edited-mark");
    mark.set_tooltip_text(Some("Has adjustments"));
    let name = gtk::Label::new(None);
    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    name.set_max_width_chars(20);
    name.add_css_class("photo-name");

    let away = gtk::Image::from_icon_name("drive-removable-media-symbolic");
    away.set_pixel_size(11);
    away.add_css_class("dim-label");
    away.set_tooltip_text(Some("Source file not available — its drive is not connected"));
    titled.append(&mark);
    titled.append(&name);
    titled.append(&away);

    let badge = gtk::Label::new(None);
    badge.add_css_class("photo-badge");

    badge.set_ellipsize(gtk::pango::EllipsizeMode::End);

    card.append(&picture);
    card.append(&hint);
    card.append(&titled);
    card.append(&badge);
    card.upcast()
}

struct CardParts {
    picture: gtk::Picture,
    hint: gtk::Label,
    mark: gtk::Widget,
    name: gtk::Label,
    away: gtk::Widget,
    badge: gtk::Label,
}

impl CardParts {
    fn of(card: &gtk::Widget) -> Option<Self> {
        let picture = card.first_child()?;
        let hint = picture.next_sibling()?;
        let titled = hint.next_sibling()?;
        let badge = titled.next_sibling()?;
        let mark = titled.first_child()?;
        let name = mark.next_sibling()?;
        let away = name.next_sibling()?;
        Some(Self {
            picture: picture.downcast().ok()?,
            hint: hint.downcast().ok()?,
            mark,
            name: name.downcast().ok()?,
            away,
            badge: badge.downcast().ok()?,
        })
    }
}

pub(super) fn bind_card(state: &App, card: &gtk::Widget, index: usize) {
    let Some(parts) = CardParts::of(card) else { return };
    let lazy = state.grid.lazy.borrow();
    let Some(thumb) = lazy.get(index) else { return };
    let cards = state.grid.cards.borrow();
    let Some(photo) = cards.get(&thumb.id) else { return };

    parts.picture.set_paintable(thumb.texture.as_ref());
    parts.name.set_text(&photo.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default());
    parts.mark.set_visible(photo.edited);
    parts.away.set_visible(state.catalog.is_offline(numa::io::catalog::library_of(photo.id)));

    let scale = state.libraries.scale.get();
    let note = cull_note(photo, &scale);
    parts.hint.set_visible(!note.is_empty());
    if !note.is_empty() {
        parts.hint.set_text(&note);
        parts.hint.set_tooltip_text(Some(&cull_detail(photo, &scale)));
    }

    parts.badge.set_text(&badge_text(photo.rating, photo.flag));
    style_badge(&parts.badge, photo.rating, photo.flag);

    card.set_tooltip_text(photo.path.to_str());

    card.set_widget_name(&photo.id.to_string());
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
