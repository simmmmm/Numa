use gtk::glib;

use super::{
    build_editor_page, consider_update_check, downloads, integrate_appimage, open_requested,
    reload_libraries, report_panel, request_render, watch_cards, App,
};

pub(super) fn fill_the_window(state: &App, window: &adw::ApplicationWindow) {

    crate::ui::display::watch(glib::clone!(
        #[strong] state,
        move || request_render(&state)
    ));
    reload_libraries(state);

    watch_cards(state);
    downloads::at_startup(state, window);
    integrate_appimage(state);

    glib::timeout_add_seconds_local_once(
        30,
        glib::clone!(
            #[strong] state,
            #[weak] window,
            move || consider_update_check(&state, &window)
        ),
    );

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
