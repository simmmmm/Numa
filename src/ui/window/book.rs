use super::*;
use numa::io::book::{self, By, Chapter, Shot};

const BOOK_PRINTER: &str = "book-printer";

pub(super) fn install(state: &App, window: &adw::ApplicationWindow) {
    let action = gio::SimpleAction::new("book", None);
    action.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| book_dialog(&state, &window)
    ));
    window.add_action(&action);
}

fn shots(state: &App) -> Vec<(Shot, PathBuf)> {
    let selected = selected_ids(state);
    let cards = state.grid.cards.borrow();
    let chosen: Vec<&Photo> = match selected.is_empty() {
        true => cards.values().filter(|photo| photo.flag == Flag::Picked).collect(),
        false => selected.iter().filter_map(|id| cards.get(id)).collect(),
    };
    let ids: Vec<i64> = chosen.iter().map(|photo| photo.id).collect();
    let mut albums = state.catalog.albums_of(&ids).unwrap_or_else(|err| {
        log::warn!("book: no albums: {err}");
        Default::default()
    });
    chosen
        .into_iter()
        .map(|photo| {
            let shot = Shot { id: photo.id, taken: photo.taken.unwrap_or(photo.mtime), albums: albums.remove(&photo.id).unwrap_or_default() };
            (shot, photo.path.clone())
        })
        .collect()
}

fn book_dialog(state: &App, window: &adw::ApplicationWindow) {
    let Some(library) = state.libraries.current.borrow().clone() else {
        state.toast("No library selected — pick one to make a book from");
        return;
    };
    let shots = shots(state);
    if shots.is_empty() {
        state.toast("Pick the photographs for the book first, or select them");
        return;
    }
    let paths: Rc<HashMap<i64, PathBuf>> = Rc::new(shots.iter().map(|(shot, path)| (shot.id, path.clone())).collect());
    let shots: Rc<Vec<Shot>> = Rc::new(shots.into_iter().map(|(shot, _)| shot).collect());

    let dialog = adw::Dialog::new();
    dialog.set_title("For a Book");
    dialog.set_content_width(600);
    dialog.set_content_height(680);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    let page = adw::PreferencesPage::new();
    page.set_description(
        "The photographs in the order they happened, a numbered folder per chapter, sized for the printer. \
         Open the folder in CEWE, Albelli or Popsa and the book is half laid out. Numa never moves or changes the originals.",
    );
    view.set_content(Some(&page));

    let chapters_group = adw::PreferencesGroup::new();
    chapters_group.set_title("Chapters");
    let by_album = shots.iter().any(|shot| !shot.albums.is_empty());
    let by = adw::ComboRow::new();
    by.set_title("Chapters By");
    by.set_model(Some(&gtk::StringList::new(&["Day", "Album"])));
    by.set_visible(by_album);
    chapters_group.add(&by);
    page.add(&chapters_group);

    let printer = printer_group(state);
    page.add(&printer.group);

    let destination = Rc::new(RefCell::new(library.export_dir()));
    let place = adw::PreferencesGroup::new();
    place.add(&folder_row(&destination));
    page.add(&place);

    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("pill");
    let make = primary_button("Make the Folders");
    make.add_css_class("pill");
    let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bottom.set_halign(gtk::Align::Center);
    bottom.set_margin_top(12);
    bottom.set_margin_bottom(12);
    bottom.append(&cancel);
    bottom.append(&make);
    view.add_bottom_bar(&bottom);
    dialog.set_child(Some(&view));
    dialog.set_default_widget(Some(&make));

    let rows: Rc<RefCell<Vec<(Chapter, adw::SwitchRow)>>> = Rc::new(RefCell::new(Vec::new()));
    let number = glib::clone!(
        #[strong] rows,
        #[weak] make,
        move || {
            let mut next = 0;
            for (chapter, row) in rows.borrow().iter() {
                let on = row.is_active();
                next += usize::from(on);
                row.set_title(&match on {
                    true => format!("{next:02} · {}", chapter.name),
                    false => chapter.name.clone(),
                });
            }
            make.set_sensitive(next > 0);
        }
    );
    let fill = glib::clone!(
        #[strong] rows,
        #[strong] shots,
        #[strong] number,
        #[weak] chapters_group,
        #[weak] by,
        move || {
            for (_, row) in rows.borrow_mut().drain(..) {
                chapters_group.remove(&row);
            }
            let chosen = if by.selected() == 1 { By::Album } else { By::Day };
            for chapter in book::chapters(&shots, chosen) {
                let row = adw::SwitchRow::new();
                row.set_active(true);
                row.set_subtitle(&count(chapter.ids.len()));
                row.connect_active_notify(glib::clone!(#[strong] number, move |_| number()));
                chapters_group.add(&row);
                rows.borrow_mut().push((chapter, row));
            }
            number();
        }
    );

    by.set_selected(u32::from(by_album));
    fill();
    by.connect_selected_notify(move |_| fill());

    cancel.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            dialog.close();
        }
    ));
    make.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        move |_| {
            let chapters: Vec<Chapter> = rows.borrow().iter().filter(|(_, row)| row.is_active()).map(|(chapter, _)| chapter.clone()).collect();
            let into = free_folder(&destination.borrow(), &format!("{} Book", library.label()));
            let settings = printer.settings(&state, &into);
            dialog.close();
            make_folders(&state, &chapters, &paths, settings, into);
        }
    ));
    dialog.present(Some(window));
}

fn count(photographs: usize) -> String {
    match photographs {
        1 => "1 photograph".to_string(),
        n => format!("{} photographs", places::grouped(n as i64)),
    }
}

fn free_folder(parent: &Path, name: &str) -> PathBuf {
    (1..)
        .map(|number| match number {
            1 => parent.join(name),
            n => parent.join(format!("{name} {n}")),
        })
        .find(|path| !path.exists())
        .expect("a free name")
}

struct Printer {
    group: adw::PreferencesGroup,
    product: adw::ComboRow,
    custom: adw::SpinRow,
    proof: adw::ComboRow,
    profiles: Vec<numa::io::print::Profile>,
}

impl Printer {

    fn edge(&self) -> u32 {
        match book::PRODUCTS.get(self.product.selected() as usize) {
            Some(product) => book::long_edge(product.long_mm),
            None => book::long_edge(self.custom.value() as f32 * 10.0),
        }
    }

    fn settings(&self, state: &App, into: &Path) -> export::ExportSettings {
        state.catalog.remember(BOOK_PRINTER, &(self.product.selected(), self.custom.value()));
        let perceptual = state.editor_page.proof.choice.borrow().1;
        let proof = (self.proof.selected() as usize).checked_sub(1).and_then(|at| self.profiles.get(at));
        export::ExportSettings {
            format: export::Format::Jpeg,
            quality: 95,
            size: export::Size::LongEdge(self.edge()),
            sharpen: true,
            folder: Some(into.to_path_buf()),
            space: ColourSpace::Srgb,
            hdr: false,
            watermark: export::Watermark::default(),
            proof: proof.map(|profile| numa::io::print::Target { file: profile.file.clone(), perceptual }),
            ..state.export.settings.borrow().clone()
        }
    }
}

fn printer_group(state: &App) -> Rc<Printer> {
    let group = adw::PreferencesGroup::new();
    group.set_title("Printer");
    let product = adw::ComboRow::new();
    product.set_title("Photo Book");
    let names: Vec<&str> = book::PRODUCTS.iter().map(|product| product.name).chain(["Custom Size"]).collect();
    product.set_model(Some(&gtk::StringList::new(&names)));
    let custom = adw::SpinRow::with_range(10.0, 100.0, 0.5);
    custom.set_title("Long Edge");
    custom.set_subtitle("Centimetres");
    custom.set_digits(1);
    let size = adw::ActionRow::new();
    size.set_title("Size");
    size.add_css_class("property");
    let (proof, profiles) = proof_row();
    for row in [product.upcast_ref::<gtk::Widget>(), custom.upcast_ref(), size.upcast_ref(), proof.upcast_ref()] {
        group.add(row);
    }
    let (chosen, edge) = state.catalog.recall::<(u32, f64)>(BOOK_PRINTER).unwrap_or((0, 30.0));
    product.set_selected(chosen.min(book::PRODUCTS.len() as u32));
    custom.set_value(edge);
    let printer = Rc::new(Printer { group, product, custom, proof, profiles });
    let refresh = glib::clone!(
        #[strong] printer,
        #[weak] size,
        move || {
            printer.custom.set_visible(printer.product.selected() as usize >= book::PRODUCTS.len());
            let proofed = printer.proof.selected() > 0;
            size.set_subtitle(&format!(
                "{} px on the long edge, 300 ppi · {}",
                places::grouped(printer.edge() as i64),
                if proofed { "converted to the lab's profile" } else { "sRGB, as most book printers take" }
            ));
            printer.proof.set_subtitle(match (proofed, printer.profiles.is_empty()) {
                (true, _) => "Tagged with it, for a printer that asks for its own profile",
                (false, true) => "A lab's own profile: add one in the editor, under Proof for Print",
                (false, false) => "Most book printers take sRGB",
            });
        }
    );
    refresh();
    printer.product.connect_selected_notify(glib::clone!(#[strong] refresh, move |_| refresh()));
    printer.proof.connect_selected_notify(glib::clone!(#[strong] refresh, move |_| refresh()));
    printer.custom.connect_value_notify(move |_| refresh());
    printer
}

fn make_folders(state: &App, chapters: &[Chapter], paths: &HashMap<i64, PathBuf>, settings: export::ExportSettings, into: PathBuf) {
    let mut jobs = Vec::new();
    for (at, chapter) in chapters.iter().enumerate() {
        let folder = into.join(book::folder(at + 1, &chapter.name));
        for (number, id) in chapter.ids.iter().enumerate() {
            let Some(path) = paths.get(id) else { continue };
            let document = state
                .catalog
                .load_edits(*id)
                .ok()
                .flatten()
                .unwrap_or_else(|| Document::new(path.to_string_lossy().to_string()));
            jobs.push(ExportJob {
                source: Source::Photo { id: *id, path: path.clone() },
                document,
                to: Some(folder.join(book::file(number + 1, path))),
                subfolder: None,
            });
        }
    }
    let shown = into.clone();
    exporting::run_export_then(
        state,
        jobs,
        settings,
        into,
        move |state: &App, written: usize, failures: &[String], last: &str| {
            if written == 0 {
                exporting::exported(state, written, failures, last);
                return;
            }
            let failed = failures.len();

            let folders = std::fs::read_dir(&shown).map_or(0, |entries| entries.flatten().filter(|entry| entry.path().is_dir()).count());
            let failed = match failed {
                0 => String::new(),
                n => format!(" · {n} failed"),
            };
            let toast = adw::Toast::new(&glib::markup_escape_text(&format!(
                "{} {} · {}{failed}",
                folders,
                if folders == 1 { "folder" } else { "folders" },
                count(written)
            )));
            toast.set_button_label(Some("Open Folder"));
            let window = state.canvas.root().and_downcast::<gtk::Window>();
            toast.connect_button_clicked(move |_| {
                gtk::FileLauncher::new(Some(&gio::File::for_path(&shown))).launch(window.as_ref(), gio::Cancellable::NONE, |result| {
                    if let Err(err) = result {
                        log::warn!("could not open the book's folder: {err}");
                    }
                });
            });
            state.toasts.add_toast(toast);
        },
    );
}
