use gtk::{glib, prelude::*};

use super::{
    build_editor_page, consider_update_check, downloads, integrate_appimage, offer_crash_report, open_requested,
    reload_libraries, report_panel, request_render, watch_cards, App,
};

pub(super) fn fill_the_window(state: &App, window: &adw::ApplicationWindow) {

    crate::ui::display::watch(glib::clone!(
        #[strong] state,
        move || request_render(&state)
    ));

    if !numa::core::power::frugal() {
        std::thread::spawn(numa::io::lensfun::warm);
    }
    reload_libraries(state);

    if !state.grid.welcome.get_visible() {
        state.grid.wall.grab_focus();
    }

    watch_cards(state);

    super::import_done::recheck_now_and_then(state);
    downloads::at_startup(state);
    integrate_appimage(state);

    consider_update_check(state, window);

    offer_crash_report(state, window);

    report_panel(state);
    open_requested(state);
}

pub(super) fn show_editor(state: &App) {
    ensure_editor_page(state);
    state.stack.set_visible_child_name("editor");
}

pub(super) fn ensure_editor_page(state: &App) {
    if state.stack.child_by_name("editor").is_none() {
        state.stack.add_named(&build_editor_page(state), Some("editor"));
    }
}
