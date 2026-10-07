use super::*;
use numa::io::print;
use numa::render::proof::Proof;

const PROOF_CHOICE: &str = "proof-choice";

pub(super) fn proofing(state: &App) -> Option<(Rc<Proof>, bool)> {
    let proof = &state.editor_page.proof;
    proof.view.borrow().clone().map(|view| (view, proof.out_of_range.is_active()))
}

pub(super) fn build(state: &App) -> gtk::Box {
    let proof = &state.editor_page.proof;
    if let Some(choice) = state.catalog.recall::<(Option<String>, bool)>(PROOF_CHOICE) {
        *proof.choice.borrow_mut() = choice;
    }
    let part = proof.part.clone();
    part.set_visible(false);

    part.insert_action_group("editor", Some(&state.editor_page.actions));

    let chooser = &proof.chooser;
    chooser.set_has_frame(false);
    chooser.set_always_show_arrow(true);
    chooser.set_tooltip_text(Some("The printer or lab's profile, and how colours it cannot print are brought in"));
    chooser.set_menu_model(Some(&proof.menu));
    fill_menu(state);
    part.append(chooser);

    proof.note.add_css_class("proof-note");
    part.append(&proof.note);

    let out_of_range = &proof.out_of_range;
    out_of_range.set_label("Out of Range");
    out_of_range.set_tooltip_text(Some("Hatch what this printer cannot reach"));
    out_of_range.connect_toggled(glib::clone!(
        #[strong] state,
        move |_| request_render(&state)
    ));
    part.append(out_of_range);

    let stop = gtk::Button::from_icon_name("window-close-symbolic");
    stop.set_tooltip_text(Some("Stop Proofing"));
    stop.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let proof = &state.editor_page.proof;
            proof.view.replace(None);
            proof.part.set_visible(false);

            proof.out_of_range.set_active(false);
            request_render(&state);
        }
    ));
    part.append(&stop);
    install_actions(state);
    part
}

fn install_actions(state: &App) {
    let actions = &state.editor_page.actions;
    let start = gio::SimpleAction::new("proof", None);
    start.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let proof = &state.editor_page.proof;
            if proof.view.borrow().is_none() && !rebuild(&state) {
                return;
            }
            proof.part.set_visible(true);

            let chooser = proof.chooser.clone();
            glib::idle_add_local_once(move || chooser.popup());
        }
    ));
    actions.add_action(&start);

    let (file, perceptual) = state.editor_page.proof.choice.borrow().clone();
    let profile = gio::SimpleAction::new_stateful("proof-profile", Some(glib::VariantTy::STRING), &file.unwrap_or_default().to_variant());
    profile.connect_change_state(glib::clone!(
        #[strong] state,
        move |action, value| {
            let Some(file) = value.and_then(|value| value.get::<String>()) else { return };
            action.set_state(&file.to_variant());
            state.editor_page.proof.choice.borrow_mut().0 = (!file.is_empty()).then_some(file);
            chosen(&state);
        }
    ));
    actions.add_action(&profile);

    let intent = gio::SimpleAction::new_stateful("proof-intent", Some(glib::VariantTy::STRING), &intent_name(perceptual).to_variant());
    intent.connect_change_state(glib::clone!(
        #[strong] state,
        move |action, value| {
            let Some(name) = value.and_then(|value| value.get::<String>()) else { return };
            action.set_state(&name.to_variant());
            state.editor_page.proof.choice.borrow_mut().1 = name == "perceptual";
            chosen(&state);
        }
    ));
    actions.add_action(&intent);

    let add = gio::SimpleAction::new("proof-add", None);
    add.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| add_profile(&state)
    ));
    actions.add_action(&add);
}

fn intent_name(perceptual: bool) -> &'static str {
    if perceptual { "perceptual" } else { "relative" }
}

fn chosen(state: &App) {
    state.catalog.remember(PROOF_CHOICE, &*state.editor_page.proof.choice.borrow());
    if rebuild(state) {
        state.editor_page.proof.part.set_visible(true);
    }
}

fn rebuild(state: &App) -> bool {
    let proof = &state.editor_page.proof;
    let (file, perceptual) = proof.choice.borrow().clone();
    let made = match &file {
        Some(file) => print::bytes(file).and_then(|bytes| {
            let name = numa::render::proof::describe(&bytes)?.name;
            Ok((Proof::new(&bytes, perceptual)?, name))
        }),
        None => Proof::srgb().map(|view| (view, "sRGB".to_string())),
    };
    let (view, name) = match made {
        Ok(made) => made,
        Err(err) => {
            state.toast(&format!("Could not proof with that profile: {err}"));
            if file.is_none() {
                return false;
            }
            proof.choice.borrow_mut().0 = None;
            if let Some(action) = state.editor_page.actions.lookup_action("proof-profile").and_downcast::<gio::SimpleAction>() {
                action.set_state(&"".to_variant());
            }
            return rebuild(state);
        }
    };
    proof.chooser.set_label(&format!("Proof · {name}"));
    proof.note.set_text("paper white shown");
    proof.note.set_visible(view.paper);
    proof.view.replace(Some(Rc::new(view)));
    request_render(state);
    true
}

fn fill_menu(state: &App) {
    let menu = &state.editor_page.proof.menu;
    menu.remove_all();
    let profiles = gio::Menu::new();
    let item = gio::MenuItem::new(Some("sRGB · Most Labs"), None);
    item.set_action_and_target_value(Some("editor.proof-profile"), Some(&"".to_variant()));
    profiles.append_item(&item);
    for profile in print::profiles() {
        let item = gio::MenuItem::new(Some(&profile.name), None);
        item.set_action_and_target_value(Some("editor.proof-profile"), Some(&profile.file.to_variant()));
        profiles.append_item(&item);
    }
    menu.append_section(Some("Print With"), &profiles);
    let add = gio::Menu::new();
    add.append(Some("Add a Profile…"), Some("editor.proof-add"));
    menu.append_section(None, &add);
    let intents = gio::Menu::new();
    for (label, name) in [("Perceptual", "perceptual"), ("Relative Colorimetric", "relative")] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("editor.proof-intent"), Some(&name.to_variant()));
        intents.append_item(&item);
    }
    menu.append_section(Some("Rendering Intent"), &intents);
}

fn add_profile(state: &App) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("ICC Profiles"));
    for pattern in ["*.icc", "*.icm", "*.ICC", "*.ICM"] {
        filter.add_pattern(pattern);
    }
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Add a Profile");
    dialog.set_filters(Some(&filters));
    let window = state.canvas.root().and_downcast::<gtk::Window>();
    let state = state.clone();
    dialog.open(window.as_ref(), gio::Cancellable::NONE, move |chosen| {
        let Some(path) = chosen.ok().and_then(|file| file.path()) else { return };
        match print::add(&path) {
            Ok(profile) => {
                fill_menu(&state);
                if let Some(action) = state.editor_page.actions.lookup_action("proof-profile") {
                    action.change_state(&profile.file.to_variant());
                }
                state.toast(&format!("Added {} — Numa keeps its own copy", profile.name));
            }
            Err(err) => state.toast(&format!("Could not add that profile: {err}")),
        }
    });
}

#[derive(Clone)]
pub(super) struct State {
    pub(super) view: Rc<RefCell<Option<Rc<Proof>>>>,

    pub(super) choice: Rc<RefCell<(Option<String>, bool)>>,
    pub(super) part: gtk::Box,
    pub(super) chooser: gtk::MenuButton,
    pub(super) note: gtk::Label,
    pub(super) out_of_range: gtk::ToggleButton,
    pub(super) menu: gio::Menu,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            view: Rc::new(RefCell::new(None)),
            choice: Rc::new(RefCell::new((None, true))),
            part: gtk::Box::new(gtk::Orientation::Horizontal, 4),
            chooser: gtk::MenuButton::new(),
            note: gtk::Label::new(None),
            out_of_range: gtk::ToggleButton::new(),
            menu: gio::Menu::new(),
        }
    }
}
