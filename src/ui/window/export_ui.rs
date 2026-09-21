use super::*;

pub(super) struct ExportJob {
    pub(super) source: Source,
    pub(super) document: Document,
}

pub(super) fn open_job(state: &App) -> Option<ExportJob> {
    state.open.borrow().as_ref().map(|photo| ExportJob {
        source: photo.source.clone(),
        document: photo.document.clone(),
    })
}

pub(super) fn export_current(state: &App, parent: &impl IsA<gtk::Widget>) {
    let Some(job) = open_job(state) else { return };
    export_dialog(state, parent, vec![job]);
}

pub(super) fn export_now(state: &App) {
    let Some(job) = open_job(state) else { return };
    let Some(directory) = export_directory(state) else { return };
    let settings = state.export.settings.borrow().clone();
    exporting::run_export(state, vec![job], settings, directory);
}

pub(super) fn export_directory(state: &App) -> Option<PathBuf> {
    if let Some(folder) = state.export.settings.borrow().folder.clone() {
        return Some(folder);
    }
    match state.libraries.current.borrow().clone() {
        Some(library) => Some(library.export_dir()),
        None => {
            state.toast("No library selected — pick one to export into");
            None
        }
    }
}

pub(super) fn export_buttons(state: &App, act: fn(&App)) -> (gtk::Box, gtk::Button) {
    let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    group.add_css_class("linked");

    let export = gtk::Button::with_label("Export");
    export.add_css_class("suggested-action");
    export.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| act(&state)
    ));
    group.append(&export);

    let more = gtk::Button::from_icon_name("pan-end-symbolic");
    more.add_css_class("suggested-action");
    more.set_tooltip_text(Some("Export settings…"));
    more.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| {

            if state.stack.visible_child_name().as_deref() == Some("editor") {
                export_current(&state, button);
            } else {
                export_selection(&state, button);
            }
        }
    ));
    group.append(&more);

    (group, export)
}

pub(super) fn refresh_export_button(state: &App) {
    let summary = state.export.settings.borrow().summary();
    for button in [&state.export.button, &state.export.library_export] {
        if let Some(button) = button.borrow().clone() {
            button.set_tooltip_text(Some(&format!(
                "Export — {summary}. The arrow asks for something else"
            )));
        }
    }
}

pub(super) fn export_selected_now(state: &App) {
    let jobs = selected_jobs(state);
    if jobs.is_empty() {
        state.toast("Select photos to export");
        return;
    }
    let Some(directory) = export_directory(state) else { return };
    let settings = state.export.settings.borrow().clone();
    exporting::run_export(state, jobs, settings, directory);
}

pub(super) fn export_selection(state: &App, parent: &impl IsA<gtk::Widget>) {
    let jobs = selected_jobs(state);
    if jobs.is_empty() {
        state.toast("Select photos to export");
        return;
    }
    export_dialog(state, parent, jobs);
}

pub(super) fn selected_jobs(state: &App) -> Vec<ExportJob> {
    let selected = selected_cards(state);
    let cards = state.grid.cards.borrow();
    selected
        .iter()
        .filter_map(|child| child.widget_name().parse::<i64>().ok())
        .filter_map(|id| cards.get(&id).map(|(photo, _)| (id, photo.path.clone())))
        .map(|(id, path)| {
            let document = state
                .catalog
                .load_edits(id)
                .ok()
                .flatten()
                .unwrap_or_else(|| Document::new(path.to_string_lossy().to_string()));
            ExportJob { source: Source::Photo { id, path }, document }
        })
        .collect()
}

pub(super) fn export_dialog(state: &App, parent: &impl IsA<gtk::Widget>, jobs: Vec<ExportJob>) {
    let Some(library) = state.libraries.current.borrow().clone() else {
        state.toast("No library selected — pick one to export into");
        return;
    };

    let settings = state.export.settings.borrow().clone();
    let destination = Rc::new(RefCell::new(settings.folder.clone().unwrap_or_else(|| library.export_dir())));
    let controls = Controls::new(state, &destination);
    controls.write(&settings);

    let jobs_holder = Rc::new(RefCell::new(jobs));
    let count = jobs_holder.borrow().len();

    let dialog = adw::Dialog::new();
    dialog.set_title(&match count {
        1 => "Export this photograph".to_string(),
        many => format!("Export {many} photographs"),
    });
    dialog.set_content_width(480);
    dialog.set_content_height(720);
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    let cancel = gtk::Button::with_label("Cancel");
    let export = gtk::Button::with_label("Export");
    export.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&export);
    let clamp = adw::Clamp::new();
    clamp.set_child(Some(&controls.page));
    clamp.set_margin_top(12);
    clamp.set_margin_bottom(24);
    clamp.set_margin_start(12);
    clamp.set_margin_end(12);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_child(Some(&clamp));
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&scroller));
    dialog.set_child(Some(&view));
    dialog.set_default_widget(Some(&export));

    cancel.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            dialog.close();
        }
    ));
    export.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] jobs_holder,
        #[weak] dialog,
        move |_| {
            let chosen = controls.read();
            *state.export.settings.borrow_mut() = chosen.clone();
            state.catalog.remember(EXPORT_SETTINGS, &chosen);
            refresh_export_button(&state);

            let jobs = std::mem::take(&mut *jobs_holder.borrow_mut());
            let directory = destination.borrow().clone();
            dialog.close();
            exporting::run_export(&state, jobs, chosen, directory);
        }
    ));

    dialog.present(Some(parent));
}

fn formats() -> Vec<(export::Format, &'static str)> {
    use export::Format::*;
    let mut formats = vec![(Jpeg, "JPEG"), (Png, "PNG"), (Tiff, "TIFF 16-bit"), (Avif, "AVIF")];
    if numa::io::jxl::is_available() {
        formats.push((Jxl, "JPEG XL"));
    }
    formats.push((Dng, "DNG"));
    formats
}

const EXPORT_PRESETS: &str = "export-presets";

#[derive(Clone)]
struct Controls {
    page: gtk::Box,
    formats: Vec<export::Format>,
    format: adw::ComboRow,
    space: adw::ComboRow,
    quality: adw::SpinRow,
    size: adw::ComboRow,
    sharpen: adw::SwitchRow,
    hdr: adw::SwitchRow,
    metadata: adw::SwitchRow,
    strip_location: adw::SwitchRow,
    keywords: adw::SwitchRow,
    creator: adw::EntryRow,
    copyright: adw::EntryRow,
    watermark: adw::ExpanderRow,
    watermark_text: adw::EntryRow,
    corner: adw::ComboRow,
    watermark_size: adw::SpinRow,
    template: adw::EntryRow,
    destination: Rc<RefCell<PathBuf>>,
}

impl Controls {
    fn new(state: &App, destination: &Rc<RefCell<PathBuf>>) -> Self {
        let formats = formats();
        let names: Vec<&str> = formats.iter().map(|(_, name)| *name).collect();
        let format = combo("Format", &names);

        let space = combo("Colour space", &ColourSpace::ALL.iter().map(|space| space.name()).collect::<Vec<_>>());
        let quality = adw::SpinRow::with_range(1.0, 100.0, 1.0);
        quality.set_title("Quality");
        let mut sizes = vec!["Full size".to_string()];
        sizes.extend(EDGES.iter().map(|edge| format!("{edge} px on the long edge")));

        if numa::io::upscale::is_installed() {
            sizes.push("Twice the size (Super Resolution)".to_string());
        }
        let size = combo("Size", &sizes.iter().map(String::as_str).collect::<Vec<_>>());

        let sharpen = switch("Sharpen after resizing", "Puts back the edge the reduction took off");
        let hdr = switch("HDR", "Brighter highlights on screens that can show them");

        let metadata = switch("Keep the camera's data", "Camera, lens, exposure, date");
        let strip_location = switch("Remove location", "Leave the camera's GPS position out");
        let keywords = switch("Stars and keywords", "The rating, and the people and albums it is in");
        let creator = entry("Creator");
        let copyright = entry("Copyright");
        let details = adw::ExpanderRow::new();
        details.set_title("Metadata");
        for row in [metadata.upcast_ref::<gtk::Widget>(), strip_location.upcast_ref(), keywords.upcast_ref(), creator.upcast_ref(), copyright.upcast_ref()] {
            details.add_row(row);
        }

        let watermark = adw::ExpanderRow::new();
        watermark.set_title("Watermark");
        watermark.set_show_enable_switch(true);
        let watermark_text = entry("Text");
        let corner = combo("Position", &export::Corner::ALL.map(export::Corner::name));
        let watermark_size = adw::SpinRow::with_range(5.0, 80.0, 1.0);
        watermark_size.set_title("Size");
        watermark_size.set_subtitle("Thousandths of the long edge");
        for row in [watermark_text.upcast_ref::<gtk::Widget>(), corner.upcast_ref(), watermark_size.upcast_ref()] {
            watermark.add_row(row);
        }

        let template = entry("File name");
        template.set_tooltip_text(Some("{stem} is the original's name, {index} a number that keeps it new"));

        let presets = adw::PreferencesGroup::new();
        let file = adw::PreferencesGroup::new();
        for row in [format.upcast_ref::<gtk::Widget>(), space.upcast_ref(), quality.upcast_ref(), size.upcast_ref(), sharpen.upcast_ref(), hdr.upcast_ref()] {
            file.add(row);
        }
        let extra = adw::PreferencesGroup::new();
        extra.add(&details);
        extra.add(&watermark);
        let place = adw::PreferencesGroup::new();
        place.add(&template);
        place.add(&folder_row(destination));

        let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
        for group in [&presets, &file, &extra, &place] {
            page.append(group);
        }
        let controls = Self {
            page,
            formats: formats.iter().map(|(format, _)| *format).collect(),
            format,
            space,
            quality,
            size,
            sharpen,
            hdr,
            metadata,
            strip_location,
            keywords,
            creator,
            copyright,
            watermark,
            watermark_text,
            corner,
            watermark_size,
            template,
            destination: destination.clone(),
        };
        presets.add(&preset_row(state, &controls));
        controls.connect_sensitivity();
        controls
    }

    fn read(&self) -> export::ExportSettings {
        let format = self.formats.get(self.format.selected() as usize).copied().unwrap_or(export::Format::Jpeg);
        export::ExportSettings {
            template: match self.template.text().trim() {
                "" => export::ExportSettings::default().template,
                text => text.to_string(),
            },
            format,
            quality: self.quality.value() as u8,
            size: match self.size.selected() as usize {
                0 => export::Size::Full,
                at if at <= EDGES.len() => export::Size::LongEdge(EDGES[at - 1]),
                _ => export::Size::Double,
            },
            sharpen: self.sharpen.is_active(),
            metadata: self.metadata.is_active(),
            folder: Some(self.destination.borrow().clone()),
            space: ColourSpace::ALL.get(self.space.selected() as usize).copied().unwrap_or_default(),
            hdr: self.hdr.is_active(),
            creator: self.creator.text().to_string(),
            copyright: self.copyright.text().to_string(),
            keywords: self.keywords.is_active(),
            strip_location: self.strip_location.is_active(),
            watermark: export::Watermark {
                on: self.watermark.enables_expansion(),
                text: self.watermark_text.text().to_string(),
                corner: export::Corner::ALL.get(self.corner.selected() as usize).copied().unwrap_or(export::Corner::BottomRight),
                size: self.watermark_size.value() as u16,
            },
        }
    }

    fn write(&self, settings: &export::ExportSettings) {
        self.format.set_selected(self.formats.iter().position(|format| *format == settings.format).unwrap_or(0) as u32);
        self.space.set_selected(ColourSpace::ALL.iter().position(|one| *one == settings.space).unwrap_or(0) as u32);
        self.quality.set_value(settings.quality as f64);
        self.size.set_selected(match settings.size {
            export::Size::Full => 0,
            export::Size::LongEdge(edge) => EDGES.iter().position(|e| *e == edge).map_or(0, |at| at as u32 + 1),

            export::Size::Double => (numa::io::upscale::is_installed() as u32) * (EDGES.len() as u32 + 1),
        });
        self.sharpen.set_active(settings.sharpen);
        self.hdr.set_active(settings.hdr);
        self.metadata.set_active(settings.metadata);
        self.strip_location.set_active(settings.strip_location);
        self.keywords.set_active(settings.keywords);
        self.creator.set_text(&settings.creator);
        self.copyright.set_text(&settings.copyright);
        self.watermark.set_enable_expansion(settings.watermark.on);
        self.watermark_text.set_text(&settings.watermark.text);
        self.corner.set_selected(export::Corner::ALL.iter().position(|one| *one == settings.watermark.corner).unwrap_or(0) as u32);
        self.watermark_size.set_value(settings.watermark.size as f64);
        self.template.set_text(&settings.template);
        self.refresh();
    }

    fn refresh(&self) {
        let format = self.formats.get(self.format.selected() as usize).copied().unwrap_or(export::Format::Jpeg);
        let dng = format == export::Format::Dng;
        self.quality.set_sensitive(format.is_lossy());
        self.quality.set_subtitle(match format {
            export::Format::Jxl => "100 is lossless",
            _ => "92 is where a copy stops being distinguishable",
        });
        self.hdr.set_sensitive(format == export::Format::Jpeg);
        self.metadata.set_sensitive(format.carries_metadata());
        for row in [self.space.upcast_ref::<gtk::Widget>(), self.size.upcast_ref(), self.watermark.upcast_ref()] {
            row.set_sensitive(!dng);
        }
        let reduced = (1..=EDGES.len() as u32).contains(&self.size.selected());
        self.sharpen.set_sensitive(!dng && reduced);
        self.size.set_subtitle(match self.size.selected() as usize > EDGES.len() {
            true => "Minutes a photograph, on the graphics card or off it",
            false => "",
        });
        let chosen = ColourSpace::ALL.get(self.space.selected() as usize).copied().unwrap_or_default();
        self.space.set_subtitle(&match (format, chosen) {
            (export::Format::Dng, _) => "The raw data, with a preview in sRGB".to_string(),
            (export::Format::Avif, ColourSpace::AdobeRgb | ColourSpace::ProPhoto) => {
                "Written as Display P3: AVIF has no code for this one".to_string()
            }
            _ => chosen.note().to_string(),
        });
    }

    fn connect_sensitivity(&self) {
        for row in [&self.format, &self.space, &self.size] {
            row.connect_selected_notify(glib::clone!(
                #[strong(rename_to = controls)] self,
                move |_| controls.refresh()
            ));
        }
    }
}

fn combo(title: &str, names: &[&str]) -> adw::ComboRow {
    let row = adw::ComboRow::new();
    row.set_title(title);
    row.set_model(Some(&gtk::StringList::new(names)));
    row
}

fn switch(title: &str, subtitle: &str) -> adw::SwitchRow {
    let row = adw::SwitchRow::new();
    row.set_title(title);
    row.set_subtitle(subtitle);
    row
}

fn entry(title: &str) -> adw::EntryRow {
    let row = adw::EntryRow::new();
    row.set_title(title);
    row
}

fn preset_row(state: &App, controls: &Controls) -> adw::ComboRow {
    let presets = Rc::new(RefCell::new(state.catalog.recall::<Vec<(String, export::ExportSettings)>>(EXPORT_PRESETS).unwrap_or_default()));
    let row = adw::ComboRow::new();
    row.set_title("Preset");
    let fill = glib::clone!(
        #[weak] row,
        #[strong] presets,
        move || {
            let mut names = vec!["As last time".to_string()];
            names.extend(presets.borrow().iter().map(|(name, _)| name.clone()));
            row.set_model(Some(&gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>())));
        }
    );
    fill();
    let last = state.export.settings.borrow().clone();
    row.connect_selected_notify(glib::clone!(
        #[strong] controls,
        #[strong] presets,
        move |row| match row.selected() {
            0 => controls.write(&last),
            at => {
                if let Some((_, settings)) = presets.borrow().get(at as usize - 1) {
                    controls.write(settings);
                }
            }
        }
    ));

    let save = gtk::Button::from_icon_name("document-save-symbolic");
    save.set_tooltip_text(Some("Save these settings as a preset"));
    let forget = gtk::Button::from_icon_name("user-trash-symbolic");
    forget.set_tooltip_text(Some("Forget this preset"));
    for button in [&save, &forget] {
        button.add_css_class("flat");
        button.set_valign(gtk::Align::Center);
        row.add_suffix(button);
    }
    save.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] controls,
        #[strong] presets,
        #[strong] fill,
        #[weak] row,
        move |button| {
            let name_row = entry("Name");
            let ask = adw::AlertDialog::new(Some("Save as preset"), None);
            let group = adw::PreferencesGroup::new();
            group.add(&name_row);
            ask.set_extra_child(Some(&group));
            ask.add_response("cancel", "Cancel");
            ask.add_response("save", "Save");
            ask.set_response_appearance("save", adw::ResponseAppearance::Suggested);
            ask.set_default_response(Some("save"));
            ask.connect_response(
                Some("save"),
                glib::clone!(
                    #[strong] state,
                    #[strong] controls,
                    #[strong] presets,
                    #[strong] fill,
                    #[weak] row,
                    move |_, _| {
                        let name = name_row.text().trim().to_string();
                        if name.is_empty() {
                            return;
                        }
                        let settings = export::ExportSettings { folder: None, ..controls.read() };
                        let mut list = presets.borrow_mut();
                        list.retain(|(have, _)| *have != name);
                        list.push((name.clone(), settings));
                        list.sort_by_key(|(name, _)| name.to_lowercase());
                        state.catalog.remember(EXPORT_PRESETS, &*list);
                        let at = list.iter().position(|(have, _)| *have == name).unwrap_or(0);
                        drop(list);
                        fill();
                        row.set_selected(at as u32 + 1);
                    }
                ),
            );
            ask.present(Some(button));
        }
    ));
    forget.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] presets,
        #[weak] row,
        move |_| {
            let at = row.selected() as usize;
            if at == 0 {
                return;
            }
            presets.borrow_mut().remove(at - 1);
            state.catalog.remember(EXPORT_PRESETS, &*presets.borrow());
            fill();
            row.set_selected(0);
        }
    ));
    row
}

const EDGES: [u32; 5] = [4096, 2560, 2048, 1600, 1080];

fn folder_row(destination: &Rc<RefCell<PathBuf>>) -> adw::ActionRow {
    let folder = adw::ActionRow::new();
    folder.set_title("Folder");
    set_row_subtitle(&folder, &destination.borrow().display().to_string());
    folder.set_subtitle_lines(2);
    let choose = gtk::Button::from_icon_name("folder-open-symbolic");
    choose.set_valign(gtk::Align::Center);
    choose.add_css_class("flat");
    choose.set_tooltip_text(Some("Choose another folder"));
    choose.connect_clicked(glib::clone!(
        #[weak] folder,
        #[strong] destination,
        move |button| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Export into");
            dialog.set_initial_folder(Some(&gtk::gio::File::for_path(
                destination.borrow().as_path(),
            )));
            let window = button.root().and_downcast::<gtk::Window>();
            dialog.select_folder(
                window.as_ref(),
                gtk::gio::Cancellable::NONE,
                glib::clone!(
                    #[weak] folder,
                    #[strong] destination,
                    move |chosen| {

                        let Some(path) = chosen.ok().and_then(|file| file.path()) else { return };
                        set_row_subtitle(&folder, &path.display().to_string());
                        *destination.borrow_mut() = path;
                    }
                ),
            );
        }
    ));
    folder.add_suffix(&choose);
    folder.set_activatable_widget(Some(&choose));
    folder
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) settings: Rc<RefCell<export::ExportSettings>>,

    pub(super) button: Rc<RefCell<Option<gtk::Button>>>,
    pub(super) library_export: Rc<RefCell<Option<gtk::Button>>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            settings: Rc::new(RefCell::new(export::ExportSettings::default())),
            button: Rc::new(RefCell::new(None)),
            library_export: Rc::new(RefCell::new(None)),
        }
    }
}
