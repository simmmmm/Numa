use super::*;

pub(super) fn report_panel(state: &App) {

    if let Ok(folder) = std::env::var("NUMA_IMPORT") {
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(2500),
            glib::clone!(
                #[strong] state,
                move || {
                    if let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() {
                        import_dialog(&state, &window, Some(PathBuf::from(folder)));
                    }
                }
            ),
        );
    }

    if let Ok(count) = std::env::var("NUMA_COMPARE") {
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(3000),
            glib::clone!(
                #[strong] state,
                move || {
                    let chosen = state.grid.lazy.borrow().iter().take(count.parse().unwrap_or(2)).map(|thumb| (thumb.id, thumb.path.clone(), thumb.mtime)).collect();
                    compare_these(&state, chosen);
                }
            ),
        );
    }

    if let Ok(wanted) = std::env::var("NUMA_TAB") {
        glib::timeout_add_local(
            std::time::Duration::from_millis(2500),
            glib::clone!(
                #[strong] state,
                move || {

                    super::startup::ensure_editor_page(&state);

                    if let Ok(index) = std::env::var("NUMA_MASK") {
                        if let Ok(index) = index.parse::<usize>() {
                            select_mask(&state, Some(index));
                        }
                    }

                    match std::env::var("NUMA_REFERENCE").as_deref() {
                        Ok("camera") => state.reference.camera_button.set_active(true),
                        Ok("frame") => state.reference.button.set_active(true),
                        _ => (),
                    }

                    if let Ok(steps) = std::env::var("NUMA_STEP") {
                        for _ in 0..steps.parse::<usize>().unwrap_or(0) {
                            step_photo(&state, true);
                        }
                    }

                    if std::env::var("NUMA_DRAFT").is_ok() {
                        state.render.drafting.set(true);
                        render_current(&state);
                    }
                    if let Ok(times) = std::env::var("NUMA_ZOOM") {
                        let (x, y) = (state.canvas.width() as f64 / 2.0, state.canvas.height() as f64 / 2.0);
                        for _ in 0..times.parse::<usize>().unwrap_or(0) {
                            toggle_one_to_one(&state, x, y);
                        }
                    }
                    if let Ok(name) = std::env::var("NUMA_PRESET") {
                        preview_preset(&state, &name);
                    }
                    if let Some((name, _, _, _)) = PANEL_TABS.iter().find(|tab| tab.0 == wanted) {
                        show_panel_tab(&state, name);
                        if let Some((_, tab)) = state.panel.tabs.borrow().iter().find(|(at, _)| at == name) {
                            tab.set_active(true);
                        }
                    }
                    for (name, _, _, _) in PANEL_TABS {
                        let Some(page) = state.panel.stack.child_by_name(name) else { continue };

                        let column = page
                            .downcast_ref::<gtk::ScrolledWindow>()
                            .and_then(|scroller| scroller.child())
                            .and_then(|viewport| viewport.downcast::<gtk::Viewport>().ok())
                            .and_then(|viewport| viewport.child())
                            .unwrap_or(page);
                        let (_, natural, _, _) = column.measure(gtk::Orientation::Vertical, PANEL_WIDTH - RAIL_WIDTH);
                        println!("PAGE {name} {natural}");
                    }
                    let wrong = check_rows();
                    println!("ROWS {} checked, {} wrong", ROWS.with(|r| r.borrow().len()), wrong.len());
                    for row in &wrong {
                        println!("  ROW {row}");
                    }

                    glib::timeout_add_local_once(
                        std::time::Duration::from_millis(250),
                        glib::clone!(
                            #[strong] state,
                            move || report_room(&state)
                        ),
                    );

                    if std::env::var("NUMA_AUDIT").is_ok() {
                        run_audit(&state);
                    }
                    glib::ControlFlow::Break
                }
            ),
        );
    }
}

fn report_room(state: &App) {
    println!(
        "ROOM {} (the stack's own height, which is what a page has to fit in)",
        state.panel.stack.height()
    );

    for (what, widget) in [
        ("panel", state.panel.stack.clone().upcast::<gtk::Widget>()),
                        ("rail", state.panel.tab_strip.clone().upcast()),
                        ("editor", state.stack.clone().upcast()),
                        ("filmstrip", state.filmstrip.scroller.clone().upcast()),
                        ("canvas", state.canvas.clone().upcast()),

                        (
                            "bar",
                            state
                                .stack
                                .child_by_name("editor")
                                .and_then(|page| page.first_child())
                                .unwrap_or_else(|| state.canvas.clone().upcast()),
                        ),
                    ] {
        let (minimum, natural, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
        println!("WIDE {what} {minimum} min, {natural} natural");
    }
}

pub(super) fn open_requested(state: &App) {
    let open_at = std::env::var("NUMA_OPEN").ok().and_then(|at| match at.split_once(':') {
        Some((library, photo)) => Some((library.parse::<i64>().ok()? << 32) | photo.parse::<i64>().ok()?),
        None => at.parse::<i64>().ok(),
    });
    let Some(id) = open_at else { return };
    glib::timeout_add_local(
        std::time::Duration::from_millis(100),
        glib::clone!(
            #[strong] state,
            move || {
                if !state.grid.cards.borrow().contains_key(&id) {
                    return glib::ControlFlow::Continue;
                }
                open_photo(&state, id);
                glib::ControlFlow::Break
            }
        ),
    );
}
