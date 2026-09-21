use super::*;

pub(super) const BUSY_AFTER: std::time::Duration = std::time::Duration::from_millis(400);

#[derive(Clone, Default)]
pub(super) struct Cancel(pub(super) Rc<Cell<bool>>);

impl Cancel {
    pub(super) fn stop(&self) {
        self.0.set(true);
    }

    pub(super) fn stopped(&self) -> bool {
        self.0.get()
    }
}

pub(super) fn busy<T: Send + 'static>(
    state: &App,
    label: &str,
    work: impl FnOnce() -> T + Send + 'static,
) -> impl std::future::Future<Output = Result<T, Box<dyn std::any::Any + Send>>> {
    busy_until(state, label, BUSY_AFTER, work)
}

pub(super) fn progress_toast(state: &App, cancel: &Cancel) -> (adw::Toast, gtk::Label, gtk::ProgressBar) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let text = gtk::Label::new(None);

    text.add_css_class("numeric");
    let bar = gtk::ProgressBar::new();
    bar.set_hexpand(true);
    column.append(&text);
    column.append(&bar);

    let toast = adw::Toast::new("");
    toast.set_custom_title(Some(&column));
    toast.set_timeout(0);
    toast.set_button_label(Some("Stop"));
    toast.connect_button_clicked(glib::clone!(
        #[strong] cancel,
        move |_| cancel.stop()
    ));
    state.toasts.add_toast(toast.clone());
    (toast, text, bar)
}

pub(super) fn busy_until<T: Send + 'static>(
    state: &App,
    label: &str,
    after: std::time::Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> impl std::future::Future<Output = Result<T, Box<dyn std::any::Any + Send>>> {
    let state = state.clone();
    let label = label.to_string();
    async move {
        let done = Rc::new(Cell::new(false));
        let shown: Rc<RefCell<Option<adw::Toast>>> = Rc::new(RefCell::new(None));
        glib::timeout_add_local_once(
            after,
            glib::clone!(
                #[strong] done,
                #[strong] shown,
                #[strong] state,
                move || {
                    if done.get() {
                        return;
                    }
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    let spinner = gtk::Spinner::new();
                    spinner.set_spinning(true);
                    row.append(&spinner);
                    row.append(&gtk::Label::new(Some(&label)));
                    let toast = adw::Toast::new("");
                    toast.set_custom_title(Some(&row));
                    toast.set_timeout(0);
                    state.toasts.add_toast(toast.clone());
                    *shown.borrow_mut() = Some(toast);
                }
            ),
        );

        let out = gtk::gio::spawn_blocking(work).await;
        done.set(true);
        if let Some(toast) = shown.borrow_mut().take() {
            toast.dismiss();
        }
        out
    }
}

pub(super) fn busy_sync(state: &App, label: &str, work: impl FnOnce(&App) + 'static) {
    if state.busy.replace(true) {
        return;
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let spinner = gtk::Spinner::new();
    spinner.set_spinning(true);
    row.append(&spinner);
    row.append(&gtk::Label::new(Some(label)));
    let toast = adw::Toast::new("");
    toast.set_custom_title(Some(&row));
    toast.set_timeout(0);
    state.toasts.add_toast(toast.clone());

    let state = state.clone();
    let label = label.to_string();

    glib::idle_add_local_once(move || {
        timed(&label, || work(&state));
        toast.dismiss();
        state.busy.set(false);
    });
}

pub(super) fn timed<T>(label: &str, work: impl FnOnce() -> T) -> T {
    let started = std::time::Instant::now();
    let out = work();
    let took = started.elapsed();
    if took > BUSY_AFTER {
        log::warn!("{label} took {took:.0?} on the main thread");
    }
    out
}
