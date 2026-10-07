use super::*;
use rayon::prelude::*;
use std::collections::HashMap;

pub(super) type Marks = Rc<RefCell<HashMap<usize, (u8, Flag)>>>;

const STRIP_EDGE: u32 = 108;

const LOOK_EDGE: u32 = 2048;

pub(super) fn new_frames(card: &Card) -> Vec<usize> {
    (0..card.found.len()).filter(|at| card.already[*at].is_none()).collect()
}

pub(super) fn left_on_card(marks: &Marks) -> usize {
    marks.borrow().values().filter(|(_, flag)| *flag == Flag::Rejected).count()
}

pub(super) fn look_group(dialog: &adw::Dialog, card: &Card, marks: &Marks, changed: Rc<dyn Fn()>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Look First");
    let left = gtk::Label::new(None);
    left.add_css_class("dim-label");
    group.set_header_suffix(Some(&left));

    let frames: Rc<Vec<(usize, PathBuf)>> =
        Rc::new(new_frames(card).into_iter().map(|at| (at, card.found[at].path.clone())).collect());
    let strip = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    strip.add_css_class("card-strip");
    let tiles: Rc<Vec<(gtk::Overlay, gtk::Picture, gtk::Label)>> = Rc::new(frames.iter().map(|_| tile()).collect());
    for (overlay, _, _) in tiles.iter() {
        strip.append(overlay);
    }
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    scroller.set_child(Some(&strip));

    let refresh: Rc<dyn Fn()> = Rc::new(glib::clone!(
        #[strong] marks,
        #[strong] frames,
        #[strong] tiles,
        #[weak] left,
        move || {
            let marks = marks.borrow();
            for ((at, _), (overlay, _, badge)) in frames.iter().zip(tiles.iter()) {
                let (rating, flag) = marks.get(at).copied().unwrap_or((0, Flag::None));
                let (text, class) = card_badge(rating, flag, None, false);
                badge.set_text(&text);
                badge.set_visible(!text.is_empty());
                match class {
                    Some(class) => badge.add_css_class(class),
                    None => badge.remove_css_class("rejected"),
                }
                match flag == Flag::Rejected {
                    true => overlay.add_css_class("rejected"),
                    false => overlay.remove_css_class("rejected"),
                }
            }
            let rejected = marks.values().filter(|(_, flag)| *flag == Flag::Rejected).count();
            left.set_text(&match rejected {
                0 => String::new(),
                count => format!("{} left on the card", places::grouped(count as i64)),
            });
            drop(marks);
            changed();
        }
    ));

    let look = gtk::Button::with_label("Look at the Card…");
    look.set_valign(gtk::Align::Center);
    look.set_tooltip_text(Some("← → to step · X leaves a frame on the card · P picks · 0–5 rate"));
    let open: Rc<dyn Fn(usize)> = Rc::new(glib::clone!(
        #[strong] frames,
        #[strong] marks,
        #[strong] refresh,
        #[weak] dialog,
        move |from: usize| viewer(&dialog, frames.clone(), from, marks.clone(), refresh.clone())
    ));
    look.connect_clicked(glib::clone!(
        #[strong] open,
        move |_| open(0)
    ));
    for (index, (overlay, _, _)) in tiles.iter().enumerate() {
        let click = gtk::GestureClick::new();
        click.connect_released(glib::clone!(
            #[strong] open,
            move |_, _, _, _| open(index)
        ));
        overlay.add_controller(click);
    }

    let caption = gtk::Label::new(Some("The embedded previews, straight off the card."));
    caption.add_css_class("dim-label");
    caption.add_css_class("caption");
    caption.set_xalign(0.0);
    caption.set_hexpand(true);
    caption.set_wrap(true);
    let foot = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    foot.append(&caption);
    foot.append(&look);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    column.set_margin_top(12);
    column.set_margin_bottom(12);
    column.set_margin_start(12);
    column.set_margin_end(12);
    column.append(&scroller);
    column.append(&foot);
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_child(Some(&column));
    group.add(&row);
    look.set_sensitive(!frames.is_empty());
    refresh();
    fill_strip(frames, tiles);
    group
}

fn tile() -> (gtk::Overlay, gtk::Picture, gtk::Label) {
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_size_request(54, 36);
    let overlay = gtk::Overlay::new();
    overlay.set_overflow(gtk::Overflow::Hidden);
    overlay.set_child(Some(&picture));
    let badge = gtk::Label::new(None);
    badge.add_css_class("strip-badge");
    badge.set_halign(gtk::Align::Start);
    badge.set_valign(gtk::Align::End);
    badge.set_visible(false);
    overlay.add_overlay(&badge);
    (overlay, picture, badge)
}

fn fill_strip(frames: Rc<Vec<(usize, PathBuf)>>, tiles: Rc<Vec<(gtk::Overlay, gtk::Picture, gtk::Label)>>) {
    glib::spawn_future_local(async move {
        let paths: Vec<PathBuf> = frames.iter().map(|(_, path)| path.clone()).collect();
        for (chunk_at, chunk) in paths.chunks(12).enumerate() {
            if tiles.first().is_none_or(|(overlay, _, _)| overlay.root().is_none()) {
                return;
            }
            let chunk = chunk.to_vec();
            let loaded: Vec<Option<image::RgbImage>> = gio::spawn_blocking(move || {
                chunk.par_iter().map(|path| raw::load_thumbnail(path, STRIP_EDGE).ok()).collect()
            })
            .await
            .unwrap_or_default();
            for (offset, image) in loaded.into_iter().enumerate() {
                if let (Some(image), Some((_, picture, _))) = (image, tiles.get(chunk_at * 12 + offset)) {
                    picture.set_paintable(Some(&texture_from(&image)));
                }
            }
        }
    });
}

fn viewer(parent: &adw::Dialog, frames: Rc<Vec<(usize, PathBuf)>>, from: usize, marks: Marks, changed: Rc<dyn Fn()>) {
    let Some(window) = parent.root().and_downcast::<gtk::Window>() else { return };
    let dialog = adw::Dialog::new();
    dialog.set_title("The Card");
    dialog.set_content_width(1200);
    dialog.set_content_height(860);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_can_shrink(true);
    picture.set_vexpand(true);
    picture.add_css_class("loupe");
    view.set_content(Some(&picture));
    let caption = gtk::Label::new(None);
    caption.add_css_class("numeric");
    caption.set_margin_top(8);
    caption.set_margin_bottom(8);
    view.add_bottom_bar(&caption);
    dialog.set_child(Some(&view));

    let at = Rc::new(Cell::new(from.min(frames.len().saturating_sub(1))));
    let turn = Rc::new(Cell::new(0u64));
    let cache: Rc<RefCell<HashMap<usize, gtk::gdk::Texture>>> = Rc::default();
    let show: Rc<dyn Fn()> = Rc::new(glib::clone!(
        #[strong] frames,
        #[strong] marks,
        #[strong] at,
        #[strong] turn,
        #[strong] cache,
        #[weak] picture,
        #[weak] caption,
        move || {
            let index = at.get();
            let Some((frame, path)) = frames.get(index) else { return };
            let (rating, flag) = marks.borrow().get(frame).copied().unwrap_or((0, Flag::None));
            let mut said = vec![
                path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                format!("{} of {}", places::grouped(index as i64 + 1), places::grouped(frames.len() as i64)),
            ];
            if rating > 0 {
                said.push("★".repeat(rating as usize));
            }
            match flag {
                Flag::Picked => said.push("Picked".into()),
                Flag::Rejected => said.push("Left on the card".into()),
                Flag::None => {}
            }
            caption.set_text(&said.join(" \u{b7} "));
            picture.set_opacity(if flag == Flag::Rejected { 0.35 } else { 1.0 });

            let wanted: Vec<usize> = [index, index + 1].into_iter().filter(|i| *i < frames.len()).collect();
            if let Some(texture) = cache.borrow().get(&index) {
                picture.set_paintable(Some(texture));
            }
            turn.set(turn.get() + 1);
            let (turn, mine, cache, frames, picture) = (turn.clone(), turn.get(), cache.clone(), frames.clone(), picture.clone());
            glib::spawn_future_local(async move {
                for want in wanted {
                    if cache.borrow().contains_key(&want) {
                        continue;
                    }
                    let path = frames[want].1.clone();
                    let image = gio::spawn_blocking(move || raw::load_thumbnail(&path, LOOK_EDGE).ok()).await.ok().flatten();
                    if turn.get() != mine {
                        return;
                    }
                    if let Some(image) = image {
                        let texture = texture_from(&image);
                        if want == index {
                            picture.set_paintable(Some(&texture));
                        }
                        let mut cache = cache.borrow_mut();
                        cache.retain(|kept, _| kept.abs_diff(index) <= 1);
                        cache.insert(want, texture);
                    }
                }
            });
        }
    ));

    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[strong] frames,
        #[strong] marks,
        #[strong] at,
        #[strong] show,
        #[strong] changed,
        move |_, key, _, _| {
            let index = at.get();
            let Some(frame) = frames.get(index).map(|(frame, _)| *frame) else { return glib::Propagation::Proceed };
            let mark = |change: &dyn Fn(&mut (u8, Flag))| {
                change(marks.borrow_mut().entry(frame).or_insert((0, Flag::None)));
                changed();
                show();
            };
            let flip = |flag: Flag| move |mark: &mut (u8, Flag)| mark.1 = if mark.1 == flag { Flag::None } else { flag };
            let stars = |key: gtk::gdk::Key| key.to_unicode().and_then(|c| c.to_digit(10)).filter(|digit| *digit <= 5);
            match key {
                gtk::gdk::Key::Left | gtk::gdk::Key::Page_Up if index > 0 => {
                    at.set(index - 1);
                    show();
                }
                gtk::gdk::Key::Right | gtk::gdk::Key::Page_Down | gtk::gdk::Key::space if index + 1 < frames.len() => {
                    at.set(index + 1);
                    show();
                }
                gtk::gdk::Key::x | gtk::gdk::Key::X => mark(&flip(Flag::Rejected)),
                gtk::gdk::Key::p | gtk::gdk::Key::P => mark(&flip(Flag::Picked)),
                gtk::gdk::Key::u | gtk::gdk::Key::U => mark(&|mark| mark.1 = Flag::None),
                key if stars(key).is_some() => {
                    let value = stars(key).unwrap_or(0) as u8;
                    mark(&move |mark| mark.0 = value);
                }
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        }
    ));
    dialog.add_controller(keys);
    show();
    dialog.present(Some(&window));
}
