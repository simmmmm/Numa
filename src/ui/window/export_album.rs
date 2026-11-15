use super::*;
use numa::io::album_export::{long_edge, make_folders, Folders, BOOK_SIZES};

const FOLDERS: [(Folders, &str); 4] = [
    (Folders::ByPlaceAndDay, "By Place and Day"),
    (Folders::ByChapterAndDay, "By Chapter and Day"),
    (Folders::ByDay, "By Day"),
    (Folders::OneFolder, "One Folder"),
];

pub(super) struct Group {
    pub(super) group: adw::PreferencesGroup,
    key: String,

    pub(super) name: String,

    pub(super) count: usize,
    folders: adw::ComboRow,
    texts: adw::SwitchRow,
    size: adw::ComboRow,
    custom: adw::SpinRow,
}

pub(super) fn ask(state: &App, window: &adw::ApplicationWindow, album: &str) {
    let kind = state.catalog.album_kind(album).unwrap_or(numa::io::catalog::AlbumKind::Album);
    let chapters = state.catalog.chapters(album).map(|chapters| chapters.len()).unwrap_or(0);
    let first = Folders::first_for(kind, chapters);
    let count = match state.catalog.album_export_plan(album, first, false) {
        Ok(plan) => plan.files.len(),
        Err(err) => return state.toast(&err),
    };
    if count == 0 {
        return state.toast("This album has no photographs to export");
    }
    let name = state.catalog.albums().ok().and_then(|albums| albums.into_iter().find(|(key, _)| key == album)).map(|(_, name)| name).unwrap_or_default();
    export_dialog_for(state, window, Vec::new(), Some(group(album, &name, count, first)));
}

fn group(key: &str, name: &str, count: usize, first: Folders) -> Group {
    let group = adw::PreferencesGroup::new();
    group.set_title("Album");
    let folders = adw::ComboRow::new();
    folders.set_title("Folders");
    folders.set_model(Some(&gtk::StringList::new(&FOLDERS.map(|(_, name)| name))));
    folders.set_selected(FOLDERS.iter().position(|(folders, _)| *folders == first).unwrap_or(0) as u32);
    let texts = adw::SwitchRow::new();
    texts.set_title("Put the texts beside the photographs");
    texts.set_active(true);

    let mut sizes = vec!["Full size".to_string()];
    sizes.extend(EDGES.iter().map(|edge| format!("{edge} px on the long edge")));
    sizes.extend(BOOK_SIZES.iter().map(|book| format!("Photo Book {}", book.name)));
    sizes.push("Custom Size".to_string());
    let size = adw::ComboRow::new();
    size.set_title("Size");
    size.set_model(Some(&gtk::StringList::new(&sizes.iter().map(String::as_str).collect::<Vec<_>>())));
    let custom = adw::SpinRow::with_range(5.0, 100.0, 0.5);
    custom.set_title("Long edge in centimetres");
    custom.set_digits(1);
    custom.set_value(28.0);
    for row in [folders.upcast_ref::<gtk::Widget>(), texts.upcast_ref(), size.upcast_ref(), custom.upcast_ref()] {
        group.add(row);
    }
    let shown = Group { group, key: key.to_string(), name: name.to_string(), count, folders, texts, size, custom };
    shown.refresh();
    let (size, custom) = (shown.size.clone(), shown.custom.clone());
    let refresh = move || refresh_rows(&size, &custom);
    let again = refresh.clone();
    shown.size.connect_selected_notify(move |_| again());
    shown.custom.connect_value_notify(move |_| refresh());
    shown
}

fn book_mm(size: &adw::ComboRow, custom: &adw::SpinRow) -> Option<f32> {
    let at = (size.selected() as usize).checked_sub(EDGES.len() + 1)?;
    match BOOK_SIZES.get(at) {
        Some(book) => Some(book.long_mm),
        None => Some(custom.value() as f32 * 10.0),
    }
}

fn refresh_rows(size: &adw::ComboRow, custom: &adw::SpinRow) {
    let mm = book_mm(size, custom);
    size.set_subtitle(&mm.map(|mm| format!("{} px", places::grouped(long_edge(mm) as i64))).unwrap_or_default());
    custom.set_visible(size.selected() as usize == EDGES.len() + 1 + BOOK_SIZES.len());
}

impl Group {
    fn refresh(&self) {
        refresh_rows(&self.size, &self.custom);
    }

    pub(super) fn follow(&self, hidden: &adw::ComboRow) {
        let hidden = hidden.clone();
        let reduced = move |size: &adw::ComboRow| hidden.set_selected(u32::from(size.selected() > 0));
        reduced(&self.size);
        self.size.connect_selected_notify(reduced);
    }

    pub(super) fn button(&self) -> String {
        match self.count {
            1 => "Export 1 Photo".to_string(),
            n => format!("Export {} Photos", places::grouped(n as i64)),
        }
    }

    pub(super) fn size(&self) -> export::Size {
        match (self.size.selected() as usize, book_mm(&self.size, &self.custom)) {
            (_, Some(mm)) => export::Size::LongEdge(long_edge(mm)),
            (0, None) => export::Size::Full,
            (at, None) => export::Size::LongEdge(EDGES[(at - 1).min(EDGES.len() - 1)]),
        }
    }

    pub(super) fn run(&self, state: &App, settings: export::ExportSettings, into: PathBuf) {
        let folders = FOLDERS.get(self.folders.selected() as usize).map_or(Folders::ByDay, |(folders, _)| *folders);
        let plan = match state.catalog.album_export_plan(&self.key, folders, self.texts.is_active()) {
            Ok(plan) => plan,
            Err(err) => return state.toast(&err),
        };
        let root = match make_folders(&plan, &into) {
            Ok(root) => root,
            Err(err) => return state.toast(&format!("Export failed: {err}")),
        };
        let mut jobs = photo_jobs(state, plan.files.iter().map(|file| (file.id, file.source.clone())));
        for (job, file) in jobs.iter_mut().zip(&plan.files) {
            job.to = Some(root.join(&file.folder).join(&file.name));
        }
        let name = root.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
        exporting::run_export_then(state, jobs, settings, root.clone(), move |state, written, failures, _| {
            let mut said = match written {
                1 => format!("1 photograph exported to {name}"),
                n => format!("{} photographs exported to {name}", places::grouped(n as i64)),
            };
            if !failures.is_empty() {
                said.push_str(&format!(", {} failed", failures.len()));
            }
            let toast = adw::Toast::new(&glib::markup_escape_text(&said));
            toast.set_button_label(Some("Show in Files"));
            let window = state.stack.root().and_downcast::<gtk::Window>();
            toast.connect_button_clicked(move |_| {
                gtk::FileLauncher::new(Some(&gio::File::for_path(&root))).launch(window.as_ref(), None::<&gio::Cancellable>, |result| {
                    if let Err(err) = result {
                        log::warn!("could not open the album's folder: {err}");
                    }
                });
            });
            state.toasts.add_toast(toast);
        });
    }
}
