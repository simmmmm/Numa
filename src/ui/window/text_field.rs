use super::*;
use numa::io::markdown::{self, Block, Run, TEXT_LIMIT};

const KEEP_AFTER_MS: u64 = 900;

const BULLET: &str = "• ";

pub(super) struct Kept {
    pub(super) keep: Box<Keep>,
    pub(super) left: Box<Left>,
}

type Keep = dyn Fn(&App, &str) -> Result<(), String>;
type Left = dyn Fn(&App, &str, bool);

#[derive(Clone)]
struct Tags {
    bold: gtk::TextTag,
    italic: gtk::TextTag,
}

pub(super) fn field(state: &App, text: &str, prompt: &str, name: &str, kept: Kept) -> gtk::Box {
    let view = gtk::TextView::new();
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_accepts_tab(false);
    view.update_property(&[gtk::accessible::Property::Label(name)]);
    let buffer = view.buffer();
    let tags = Tags { bold: gtk::TextTag::builder().weight(700).build(), italic: gtk::TextTag::builder().style(gtk::pango::Style::Italic).build() };
    buffer.tag_table().add(&tags.bold);
    buffer.tag_table().add(&tags.italic);
    fill(&buffer, &tags, text);
    let Parts { frame, hint, kept: kept_label, buttons } = build(&view, prompt, name);
    show(&frame, false, buffer.char_count() == 0);
    hint.set_visible(buffer.char_count() == 0);

    let mode: Rc<Cell<(bool, bool)>> = Rc::default();
    let inserting = Rc::new(Cell::new(false));
    let kept_text = Rc::new(RefCell::new(markdown::write(&blocks(&buffer, &tags))));
    let changed_since = Rc::new(Cell::new(false));
    let store: Rc<Keep> = Rc::from(kept.keep);
    let keep: Rc<dyn Fn()> = Rc::new(glib::clone!(
        #[strong] state,
        #[strong] buffer,
        #[strong] tags,
        #[strong] kept_text,
        #[strong] changed_since,
        #[strong] store,
        #[weak] kept_label,
        move || {
            let words = markdown::write(&blocks(&buffer, &tags));
            if *kept_text.borrow() == words {
                return;
            }
            match store(&state, &words) {
                Ok(()) => {
                    kept_text.replace(words);
                    changed_since.set(true);
                    kept_label.set_visible(true);
                }
                Err(err) => state.toast(&format!("The words were not kept: {err}")),
            }
        }
    ));
    let waiting: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let later: Rc<dyn Fn()> = Rc::new(glib::clone!(
        #[strong] waiting,
        #[strong] keep,
        move || {
            if let Some(timer) = waiting.take() {
                timer.remove();
            }
            let (again, keep) = (waiting.clone(), keep.clone());
            waiting.replace(Some(glib::timeout_add_local_once(std::time::Duration::from_millis(KEEP_AFTER_MS), move || {
                again.take();
                keep();
            })));
        }
    ));
    let now: Rc<dyn Fn()> = Rc::new(glib::clone!(
        #[strong] waiting,
        #[strong] keep,
        move || {
            if let Some(timer) = waiting.take() {
                timer.remove();
            }
            keep();
        }
    ));
    let show_mode: Rc<dyn Fn()> = Rc::new(glib::clone!(
        #[strong] buffer,
        #[strong] mode,
        #[strong] buttons,
        move || {
            let (bold, italic) = mode.get();
            for (button, on) in [(&buttons[0], bold), (&buttons[1], italic), (&buttons[2], in_list(&buffer))] {
                if on { button.add_css_class("in") } else { button.remove_css_class("in") }
            }
        }
    ));

    typing(&buffer, &tags, &mode, &inserting, &hint, &later, &show_mode);
    let toggle = {
        let (buffer, tags, mode, later, show_mode) = (buffer.clone(), tags.clone(), mode.clone(), later.clone(), show_mode.clone());
        Rc::new(move |which: u8| {
            match which {
                0 | 1 => restyle(&buffer, if which == 0 { &tags.bold } else { &tags.italic }, &mode, which == 0),
                _ => list_lines(&buffer),
            }
            show_mode();
            later();
        })
    };
    for (at, button) in buttons.iter().enumerate() {
        let toggle = toggle.clone();
        button.connect_clicked(move |_| toggle(at as u8));
    }
    view.add_controller(keys(&view, &buffer, toggle));
    pasting(&view, &tags, &inserting);
    watch_focus(state, &view, &frame, &buffer, &tags, kept.left, now, changed_since, show_mode);
    frame
}

struct Parts {
    frame: gtk::Box,
    hint: gtk::Label,
    kept: gtk::Label,
    buttons: Rc<[gtk::Button; 3]>,
}

fn build(view: &gtk::TextView, prompt: &str, name: &str) -> Parts {
    let hint = gtk::Label::new(Some(prompt));
    hint.add_css_class("text-field-hint");
    hint.set_halign(gtk::Align::Start);
    hint.set_valign(gtk::Align::Start);
    hint.set_can_target(false);

    let pencil = gtk::Image::from_icon_name("document-edit-symbolic");
    pencil.add_css_class("text-field-pencil");
    pencil.set_pixel_size(16);
    pencil.set_halign(gtk::Align::End);
    pencil.set_valign(gtk::Align::Start);
    pencil.set_can_target(false);
    let words = gtk::Overlay::new();
    words.set_child(Some(view));
    words.add_overlay(&hint);
    words.add_overlay(&pencil);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let icon = gtk::Image::from_icon_name("document-edit-symbolic");
    icon.set_pixel_size(16);
    row.append(&icon);
    row.append(&gtk::Label::new(Some(prompt)));
    let unfold = gtk::Button::new();
    unfold.add_css_class("flat");
    unfold.add_css_class("text-field-prompt");
    unfold.set_halign(gtk::Align::Start);
    unfold.set_child(Some(&row));
    unfold.update_property(&[gtk::accessible::Property::Label(name), gtk::accessible::Property::Description(prompt)]);

    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    bar.add_css_class("text-field-bar");
    let buttons = bar_buttons(&bar);
    let kept_label = gtk::Label::new(Some("Kept"));
    kept_label.add_css_class("text-field-kept");
    kept_label.set_visible(false);

    let room = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    room.set_hexpand(true);
    bar.append(&room);
    bar.append(&kept_label);
    let done = done_button(view);
    bar.append(&done);
    let foot = gtk::Box::new(gtk::Orientation::Vertical, 0);
    foot.add_css_class("text-field-foot");
    foot.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    foot.append(&bar);
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.add_css_class("text-field");
    frame.append(&unfold);
    frame.append(&words);
    frame.append(&foot);
    unfold.connect_clicked(glib::clone!(
        #[weak] words,
        #[weak] view,
        move |unfold| {
            unfold.set_visible(false);
            words.set_visible(true);
            view.grab_focus();
        }
    ));
    view.buffer().connect_changed(glib::clone!(
        #[weak] kept_label,
        move |_| kept_label.set_visible(false)
    ));
    Parts { frame, hint, kept: kept_label, buttons }
}

fn show(frame: &gtk::Box, writing: bool, empty: bool) {
    let Some(unfold) = frame.first_child() else { return };
    let Some(words) = unfold.next_sibling() else { return };
    unfold.set_visible(!writing && empty);
    words.set_visible(writing || !empty);
    if let Some(foot) = words.next_sibling() {
        foot.set_visible(writing);
    }
    if let Some(pencil) = words.last_child() {
        pencil.set_visible(!writing);
    }
    for (class, on) in [("writing", writing), ("written", !writing && !empty), ("empty", !writing && empty)] {
        if on { frame.add_css_class(class) } else { frame.remove_css_class(class) }
    }
}

pub(super) fn write(field: &gtk::Box) {
    let Some(unfold) = field.first_child().and_downcast::<gtk::Button>() else { return };
    if unfold.is_visible() {
        unfold.emit_clicked();
    } else if let Some(view) = unfold.next_sibling().and_downcast::<gtk::Overlay>().and_then(|words| words.child()) {
        view.grab_focus();
    }
}

pub(super) fn leave_on_press_outside(page: &impl IsA<gtk::Widget>) {
    let press = gtk::GestureClick::new();
    press.set_propagation_phase(gtk::PropagationPhase::Capture);
    press.connect_pressed(|press, _, x, y| {
        let Some(page) = press.widget() else { return };
        let Some(root) = page.root() else { return };
        let Some(writing) = root.focus().filter(|focus| focus.is::<gtk::TextView>()).and_then(|view| view.ancestor(gtk::Box::static_type())).filter(|frame| frame.has_css_class("text-field")) else {
            return;
        };
        if !page.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|at| at == writing || at.is_ancestor(&writing)) {
            root.set_focus(gtk::Widget::NONE);
        }
    });
    page.as_ref().add_controller(press);
}

fn done_button(view: &gtk::TextView) -> gtk::Button {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.append(&gtk::Image::from_icon_name("object-select-symbolic"));
    row.append(&gtk::Label::new(Some("Done")));
    let done = gtk::Button::new();
    done.set_child(Some(&row));
    done.add_css_class("flat");
    done.add_css_class("text-field-done");
    done.set_focus_on_click(false);
    done.set_can_focus(false);
    done.connect_clicked(glib::clone!(
        #[weak] view,
        move |_| {
            if let Some(root) = view.root() {
                root.set_focus(gtk::Widget::NONE);
            }
        }
    ));
    done
}

fn bar_buttons(bar: &gtk::Box) -> Rc<[gtk::Button; 3]> {
    let make = |glyph: &str, tip: &str| {

        let button = match glyph {
            "B" | "I" => {
                let letter = gtk::Label::new(None);
                letter.set_markup(&if glyph == "B" { "<b>B</b>".to_string() } else { "<i>I</i>".to_string() });
                let button = gtk::Button::new();
                button.set_child(Some(&letter));
                button
            }
            icon => gtk::Button::from_icon_name(icon),
        };
        button.set_focus_on_click(false);
        button.set_can_focus(false);
        button.set_tooltip_text(Some(tip));
        bar.append(&button);
        button
    };
    Rc::new([make("B", "Bold (Ctrl+B)"), make("I", "Italic (Ctrl+I)"), make("view-list-bullet-symbolic", "List")])
}

fn typing(buffer: &gtk::TextBuffer, tags: &Tags, mode: &Rc<Cell<(bool, bool)>>, inserting: &Rc<Cell<bool>>, hint: &gtk::Label, later: &Rc<dyn Fn()>, show_mode: &Rc<dyn Fn()>) {
    buffer.connect_insert_text(glib::clone!(
        #[strong] tags,
        #[strong] mode,
        #[strong] inserting,
        move |buffer, at, text| {
            if inserting.get() {
                return;
            }

            buffer.stop_signal_emission_by_name("insert-text");
            inserting.set(true);
            let from = at.offset();
            buffer.insert(at, text);
            let to = from + text.chars().count() as i32;
            let (start, end) = (buffer.iter_at_offset(from), buffer.iter_at_offset(to));
            let (bold, italic) = mode.get();
            for (tag, on) in [(&tags.bold, bold), (&tags.italic, italic)] {
                if on { buffer.apply_tag(tag, &start, &end) } else { buffer.remove_tag(tag, &start, &end) }
            }
            *at = buffer.iter_at_offset(to);
            inserting.set(false);
        }
    ));

    buffer.connect_cursor_position_notify(glib::clone!(
        #[strong] tags,
        #[strong] mode,
        #[strong] inserting,
        #[strong] show_mode,
        move |buffer| {
            if inserting.get() {
                return;
            }
            let mut before = buffer.iter_at_offset(buffer.cursor_position());
            let styled = before.backward_char() && !before.ends_line();
            mode.set((styled && before.has_tag(&tags.bold), styled && before.has_tag(&tags.italic)));
            show_mode();
        }
    ));
    buffer.connect_changed(glib::clone!(
        #[weak] hint,
        #[strong] later,
        move |buffer| {
            hint.set_visible(buffer.char_count() == 0);

            if buffer.char_count() > TEXT_LIMIT as i32 {
                let (mut from, mut to) = (buffer.iter_at_offset(TEXT_LIMIT as i32), buffer.end_iter());
                buffer.delete(&mut from, &mut to);
                return;
            }
            later();
        }
    ));
}

fn keys(view: &gtk::TextView, buffer: &gtk::TextBuffer, toggle: Rc<dyn Fn(u8)>) -> gtk::EventControllerKey {
    let keys = gtk::EventControllerKey::new();

    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[weak] view,
        #[weak] buffer,
        #[upgrade_or] glib::Propagation::Proceed,
        move |_, key, _, modifiers| {
            use gtk::gdk::{Key, ModifierType};
            let ctrl = modifiers.contains(ModifierType::CONTROL_MASK);
            let plain = !modifiers.intersects(ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK | ModifierType::ALT_MASK);
            let before = line_before_cursor(&buffer);
            match key {
                Key::b | Key::B if ctrl => toggle(0),
                Key::i | Key::I if ctrl => toggle(1),
                Key::space if plain && before == "-" => {
                    let mut at = buffer.iter_at_offset(buffer.cursor_position());
                    let mut start = at;
                    start.set_line_offset(0);
                    buffer.delete(&mut start, &mut at);
                    buffer.insert_at_cursor(BULLET);
                }
                Key::Return | Key::KP_Enter if plain && in_list(&buffer) => {
                    buffer.delete_selection(true, true);
                    if line_before_cursor(&buffer) == BULLET && line_after_cursor(&buffer).is_empty() {

                        let mut at = buffer.iter_at_offset(buffer.cursor_position());
                        let mut start = at;
                        start.set_line_offset(0);
                        buffer.delete(&mut start, &mut at);
                    } else {
                        buffer.insert_at_cursor(&format!("\n{BULLET}"));
                    }
                }
                Key::Escape => {
                    if let Some(root) = view.root() {
                        root.set_focus(gtk::Widget::NONE);
                    }
                }
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        }
    ));
    keys
}

#[allow(clippy::too_many_arguments)]
fn watch_focus(state: &App, view: &gtk::TextView, frame: &gtk::Box, buffer: &gtk::TextBuffer, tags: &Tags, left: Box<Left>, now: Rc<dyn Fn()>, changed_since: Rc<Cell<bool>>, show_mode: Rc<dyn Fn()>) {
    let focus = gtk::EventControllerFocus::new();
    focus.connect_enter(glib::clone!(
        #[weak] frame,
        #[weak] buffer,
        #[strong] changed_since,
        move |_| {
            changed_since.set(false);
            show(&frame, true, buffer.char_count() == 0);
            show_mode();
        }
    ));
    focus.connect_leave(glib::clone!(
        #[strong] state,
        #[weak] frame,
        #[weak] view,
        #[strong] buffer,
        #[strong] tags,
        #[strong] now,
        move |_| {
            now();
            let words = markdown::write(&blocks(&buffer, &tags));

            if words.is_empty() && buffer.char_count() > 0 {
                buffer.set_text("");
            }
            show(&frame, false, words.is_empty());

            if view.is_mapped() {
                left(&state, &words, changed_since.get());
            }
        }
    ));
    view.add_controller(focus);
    let again = now.clone();
    view.connect_unmap(move |_| again());
    view.connect_unrealize(move |_| now());
}

fn fill(buffer: &gtk::TextBuffer, tags: &Tags, text: &str) {
    buffer.set_text("");
    put(buffer, tags, &mut buffer.end_iter(), text);
}

fn put(buffer: &gtk::TextBuffer, tags: &Tags, at: &mut gtk::TextIter, text: &str) {
    let mut last: Option<bool> = None;
    for block in markdown::parse(text) {
        for (n, line) in block.lines.iter().enumerate() {
            if let Some(list) = last {
                buffer.insert(at, if n == 0 && list == block.list { "\n\n" } else { "\n" });
            }
            last = Some(block.list);
            if block.list {
                buffer.insert(at, BULLET);
            }
            for run in line {
                let styled: Vec<&gtk::TextTag> = [(run.bold, &tags.bold), (run.italic, &tags.italic)].into_iter().filter(|(on, _)| *on).map(|(_, tag)| tag).collect();
                buffer.insert_with_tags(at, &run.text, &styled);
            }
        }
    }
}

fn pasting(view: &gtk::TextView, tags: &Tags, inserting: &Rc<Cell<bool>>) {
    view.connect_paste_clipboard(glib::clone!(
        #[strong] tags,
        #[strong] inserting,
        move |view| {
            view.stop_signal_emission_by_name("paste-clipboard");
            let buffer = view.buffer();
            let (tags, inserting) = (tags.clone(), inserting.clone());
            view.clipboard().read_text_async(None::<&gio::Cancellable>, move |text| {
                let Ok(Some(text)) = text else { return };
                buffer.delete_selection(true, true);

                inserting.set(true);
                put(&buffer, &tags, &mut buffer.iter_at_offset(buffer.cursor_position()), &markdown::clean(&text));
                inserting.set(false);
            });
        }
    ));
}

fn blocks(buffer: &gtk::TextBuffer, tags: &Tags) -> Vec<Block> {
    let lines = (0..buffer.line_count())
        .filter_map(|line| buffer.iter_at_line(line))
        .map(|start| {
            let mut end = start;
            if !end.ends_line() {
                end.forward_to_line_end();
            }
            runs(&start, &end, tags)
        })
        .collect();
    lines_to_blocks(lines)
}

fn lines_to_blocks(lines: Vec<Vec<Run>>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut open: Option<Block> = None;
    for mut runs in lines {
        let list = runs.first().is_some_and(|run| run.text.starts_with(BULLET));
        if let Some(first) = runs.first_mut().filter(|_| list) {
            first.text.drain(..BULLET.len());
        }
        runs.retain(|run| !run.text.is_empty());
        let empty = runs.iter().all(|run| run.text.trim().is_empty());
        if empty && !list {
            blocks.extend(open.take());
            continue;
        }
        if open.as_ref().is_some_and(|block| block.list != list) {
            blocks.extend(open.take());
        }
        let block = open.get_or_insert_with(|| Block { list, lines: Vec::new() });
        if !empty {
            block.lines.push(runs);
        }
    }
    blocks.extend(open.take());
    blocks
}

fn runs(from: &gtk::TextIter, to: &gtk::TextIter, tags: &Tags) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut at = *from;
    while at.offset() < to.offset() {
        let mut next = at;
        next.forward_to_tag_toggle(None::<&gtk::TextTag>);
        if next.offset() <= at.offset() || next.offset() > to.offset() {
            next = *to;
        }
        let text = at.text(&next).to_string();
        let (bold, italic) = (at.has_tag(&tags.bold), at.has_tag(&tags.italic));
        match runs.last_mut() {
            Some(last) if last.bold == bold && last.italic == italic => last.text.push_str(&text),
            _ => runs.push(Run { text, bold, italic }),
        }
        at = next;
    }
    runs
}

fn restyle(buffer: &gtk::TextBuffer, tag: &gtk::TextTag, mode: &Cell<(bool, bool)>, bold: bool) {
    let (was_bold, was_italic) = mode.get();
    let on = match buffer.selection_bounds() {
        Some((start, end)) => {
            let mut all = true;
            let mut at = start;
            while at.offset() < end.offset() && all {
                all = at.has_tag(tag);
                at.forward_char();
            }
            if all { buffer.remove_tag(tag, &start, &end) } else { buffer.apply_tag(tag, &start, &end) }
            !all
        }
        None => !(if bold { was_bold } else { was_italic }),
    };
    mode.set(if bold { (on, was_italic) } else { (was_bold, on) });
}

fn list_lines(buffer: &gtk::TextBuffer) {
    let (start, end) = buffer.selection_bounds().unwrap_or_else(|| {
        let at = buffer.iter_at_offset(buffer.cursor_position());
        (at, at)
    });
    let removing = in_list(buffer);
    for line in (start.line()..=end.line()).rev() {
        let Some(mut at) = buffer.iter_at_line(line) else { continue };
        let mut after = at;
        after.forward_chars(BULLET.chars().count() as i32);
        let item = at.text(&after) == BULLET;
        if removing && item {
            buffer.delete(&mut at, &mut after);
        } else if !removing && !item {
            buffer.insert(&mut at, BULLET);
        }
    }
}

fn in_list(buffer: &gtk::TextBuffer) -> bool {
    let mut start = buffer.iter_at_offset(buffer.cursor_position());
    start.set_line_offset(0);
    let mut after = start;
    after.forward_chars(BULLET.chars().count() as i32);
    start.text(&after) == BULLET
}

fn line_before_cursor(buffer: &gtk::TextBuffer) -> String {
    let at = buffer.iter_at_offset(buffer.cursor_position());
    let mut start = at;
    start.set_line_offset(0);
    start.text(&at).to_string()
}

fn line_after_cursor(buffer: &gtk::TextBuffer) -> String {
    let at = buffer.iter_at_offset(buffer.cursor_position());
    let mut end = at;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    at.text(&end).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, bold: bool, italic: bool) -> Run {
        Run { text: text.into(), bold, italic }
    }

    #[test]
    fn the_field_is_written_as_the_subset() {
        let lines = vec![
            vec![run("Rain all morning, then ", false, false), run("the best light", true, false), run(" of the trip.", false, false)],
            vec![run("Over the river.", false, false)],
            vec![],
            vec![run("• the market hall", false, false)],
            vec![run("• ", false, false), run("smoked fish", false, true), run(" on the steps", false, false)],
            vec![run("• ", false, false)],
            vec![run("After.", false, false)],
        ];
        let written = markdown::write(&lines_to_blocks(lines));
        assert_eq!(written, "Rain all morning, then **the best light** of the trip.\nOver the river.\n\n- the market hall\n- *smoked fish* on the steps\n\nAfter.");
        assert_eq!(markdown::clean(&written), written);
        assert!(lines_to_blocks(vec![vec![], vec![run("  ", true, false)]]).is_empty(), "nothing written is no text");
    }
}
