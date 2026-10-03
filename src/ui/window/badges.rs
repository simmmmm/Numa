use super::*;

pub(super) fn make_card() -> gtk::Widget {
    let picture = gtk::Picture::new();

    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Cover);
    let card = gtk::Overlay::new();
    card.add_css_class("grid-card");

    card.set_overflow(gtk::Overflow::Hidden);
    card.set_child(Some(&picture));

    let pill = |class: &str, halign: gtk::Align, valign: gtk::Align| {
        let label = gtk::Label::new(None);
        label.add_css_class("card-badge");
        label.add_css_class(class);
        label.set_halign(halign);
        label.set_valign(valign);
        label
    };
    let rating = pill("card-rating", gtk::Align::Start, gtk::Align::End);
    rating.add_css_class("card-foot");
    let burst = pill("card-burst", gtk::Align::End, gtk::Align::Start);

    let marks = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    marks.add_css_class("card-foot");
    marks.add_css_class("card-marks");
    marks.set_halign(gtk::Align::End);
    marks.set_valign(gtk::Align::End);
    let away = gtk::Image::from_icon_name("drive-removable-media-symbolic");
    away.set_pixel_size(11);
    away.add_css_class("card-badge");
    away.add_css_class("card-away");
    let edited = edited_mark();
    marks.append(&away);
    marks.append(&edited);

    let caption = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    caption.add_css_class("card-caption");
    caption.set_valign(gtk::Align::End);
    let name = gtk::Label::new(None);
    name.add_css_class("card-name");
    name.set_hexpand(true);
    name.set_xalign(1.0);

    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    let stars = gtk::Label::new(None);
    stars.add_css_class("card-stars");
    caption.append(&stars);
    caption.append(&name);

    for part in [rating.upcast_ref::<gtk::Widget>(), burst.upcast_ref(), marks.upcast_ref(), caption.upcast_ref()] {

        part.set_can_target(false);
        card.add_overlay(part);
    }
    card.upcast()
}

pub(super) fn edited_mark() -> gtk::Box {
    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.add_css_class("card-dot");
    let edited = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    edited.add_css_class("card-badge");
    edited.add_css_class("card-edited");
    edited.append(&dot);
    edited
}

struct CardParts {
    picture: gtk::Picture,
    rating: gtk::Label,
    burst: gtk::Label,
    away: gtk::Widget,
    edited: gtk::Widget,
    name: gtk::Label,
    stars: gtk::Label,
}

impl CardParts {
    fn of(card: &gtk::Widget) -> Option<Self> {
        Some(Self {
            picture: part(card, "picture")?,
            rating: part(card, "card-rating")?,
            burst: part(card, "card-burst")?,
            away: part(card, "card-away")?,
            edited: part(card, "card-edited")?,
            name: part(card, "card-name")?,
            stars: part(card, "card-stars")?,
        })
    }
}

fn part<W: IsA<gtk::Widget>>(root: &gtk::Widget, class: &str) -> Option<W> {
    let mut child = root.first_child();
    while let Some(widget) = child {
        if widget.has_css_class(class) || widget.css_name() == class {
            if let Ok(found) = widget.clone().downcast::<W>() {
                return Some(found);
            }
        }
        if let Some(found) = part(&widget, class) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

pub(super) fn bind_card(state: &App, card: &gtk::Widget, index: usize) {
    let Some(parts) = CardParts::of(card) else { return };
    let lazy = state.grid.lazy.borrow();
    let Some(thumb) = lazy.get(index) else { return };
    let cards = state.grid.cards.borrow();
    let Some(photo) = cards.get(&thumb.id) else { return };

    parts.picture.set_paintable(thumb.texture.as_ref());
    parts.name.set_text(&photo.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default());
    parts.stars.set_markup(&hover_stars(photo.rating));
    parts.edited.set_visible(photo.edited);
    parts.away.set_visible(state.catalog.is_offline(numa::io::catalog::library_of(photo.id)));
    match photo.flag == Flag::Rejected {
        true => card.add_css_class("rejected"),
        false => card.remove_css_class("rejected"),
    }

    let scale = state.libraries.scale.get();
    let note = cull_note(photo, &scale);
    let (text, look) = card_badge(photo.rating, photo.flag, photo.suggested, note.contains("soft"));
    parts.rating.set_visible(!text.is_empty());
    parts.rating.set_text(&text);
    for class in ["rejected", "suggested"] {
        match look == Some(class) {
            true => parts.rating.add_css_class(class),
            false => parts.rating.remove_css_class(class),
        }
    }

    let burst = photo
        .burst
        .filter(|_| photo.best_of_burst)
        .and_then(|burst| state.grid.bursts.borrow().get(&(numa::io::catalog::library_of(photo.id), burst)).copied());
    parts.burst.set_visible(burst.is_some());
    if let Some(size) = burst {
        parts.burst.set_text(&format!("×{size}"));
    }

    card.set_tooltip_markup(Some(&card_tooltip(photo, &scale)));

    card.set_widget_name(&photo.id.to_string());
}

pub(super) fn card_badge(rating: u8, flag: Flag, suggested: Option<f32>, soft: bool) -> (String, Option<&'static str>) {
    match (flag, rating, suggested) {
        (Flag::Rejected, _, _) => ("✕".to_string(), Some("rejected")),
        (Flag::Picked, 0, _) => ("⚑".to_string(), None),
        (Flag::Picked, stars, _) => (format!("★ {stars}  ⚑"), None),
        (Flag::None, stars @ 1.., _) => (format!("★ {stars}"), None),
        (Flag::None, 0, Some(suggested)) => {
            let soft = if soft { "  soft" } else { "" };
            (format!("☆ {}{soft}", suggested.round() as u8), Some("suggested"))
        }
        (Flag::None, 0, None) => (String::new(), None),
    }
}

fn hover_stars(rating: u8) -> String {
    let set = "★".repeat(rating.min(5) as usize);
    let unset = "☆".repeat(5 - rating.min(5) as usize);
    format!("{set}<span alpha=\"60%\">{unset}</span>")
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
    card_badge(rating, flag, None, false).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_says_only_what_is_set() {
        assert_eq!(card_badge(0, Flag::None, None, false), (String::new(), None));
        assert_eq!(card_badge(3, Flag::None, Some(1.0), true).0, "★ 3");
        assert_eq!(card_badge(4, Flag::Picked, None, false).0, "★ 4  ⚑");
        assert_eq!(card_badge(5, Flag::Rejected, None, false), ("✕".to_string(), Some("rejected")));
        assert_eq!(card_badge(0, Flag::None, Some(2.6), true), ("☆ 3  soft".to_string(), Some("suggested")));
    }
}

fn card_tooltip(photo: &Photo, scale: &numa_cull::Scale) -> String {
    use numa::io::notes::{cull_detail_lines, DetailLine};
    let name = photo.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let mut text = format!("<b>{}</b>", glib::markup_escape_text(&name));
    let Some(lines) = cull_detail_lines(photo, scale) else { return text };
    let mut noticed: Vec<String> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    for line in &lines {
        match *line {
            DetailLine::Sharpness { sharpness, soft_below } => {
                noticed.push(if sharpness < soft_below { "Soft" } else { "Sharp" }.to_string())
            }
            DetailLine::Blown { blown, raw, holds, a_lot } if raw.unwrap_or(blown) > a_lot || blown > a_lot => {
                noticed.push(if holds { "Blown in the JPEG, the raw holds it" } else { "Blown highlights" }.to_string())
            }
            DetailLine::DeepShadow { share } if share >= 20.0 => noticed.push("Deep shadows".to_string()),
            DetailLine::Spread { contrast, blank } if contrast < blank => noticed.push("Almost empty".to_string()),
            DetailLine::EyesClosed => noticed.push("Eyes closed?".to_string()),
            DetailLine::Faces { count, .. } => noticed.push(format!("{count} {}", if count == 1 { "face" } else { "faces" })),
            DetailLine::Shutter { seconds, focal35, stops, named_past } => {
                taken.push(match seconds >= 1.0 {
                    true => format!("{seconds:.1} s"),
                    false => format!("1/{:.0} s", 1.0 / seconds),
                });
                taken.push(format!("{focal35:.0} mm"));
                if stops > named_past {
                    noticed.push("Slow for the lens".to_string());
                }
            }
            DetailLine::Suggested { stars } => taken.push(format!("suggested ★ {stars:.1}")),
            _ => {}
        }
    }
    for part in [noticed, taken] {
        if !part.is_empty() {
            text.push('\n');
            text.push_str(&glib::markup_escape_text(&part.join(" · ")));
        }
    }
    text
}
