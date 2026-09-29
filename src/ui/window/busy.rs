use super::*;

pub(super) const BUSY_AFTER: std::time::Duration = std::time::Duration::from_millis(400);

pub(super) const LINGER: std::time::Duration = std::time::Duration::from_millis(200);

pub(super) fn spinner(label: &str) -> gtk::Spinner {
    let spinner = gtk::Spinner::new();
    spinner.update_property(&[gtk::accessible::Property::Label(label)]);
    spinner.connect_map(|spinner| spinner.start());
    spinner.connect_unmap(|spinner| spinner.stop());
    spinner
}

#[derive(Clone)]
pub(super) struct Waiting {
    pub(super) spinner: gtk::Spinner,

    region: gtk::Widget,
    holds: Rc<Cell<u32>>,
    shown: Rc<Cell<Option<std::time::Instant>>>,

    turn: Rc<Cell<u64>>,
}

pub(super) struct Hold(Waiting);

impl Drop for Hold {
    fn drop(&mut self) {
        self.0.let_go();
    }
}

impl Waiting {
    pub(super) fn new(label: &str, region: &impl IsA<gtk::Widget>) -> Self {
        let spinner = spinner(label);
        spinner.set_visible(false);
        Self {
            spinner,
            region: region.clone().upcast(),
            holds: Rc::default(),
            shown: Rc::default(),
            turn: Rc::default(),
        }
    }

    pub(super) fn hold(&self) -> Hold {
        let holds = self.holds.get();
        self.holds.set(holds + 1);
        if holds == 0 {
            let turn = self.next_turn();

            if self.shown.get().is_none() {
                let this = self.clone();
                glib::timeout_add_local_once(BUSY_AFTER, move || {
                    if this.turn.get() == turn {
                        this.show(true);
                    }
                });
            }
        }
        Hold(self.clone())
    }

    fn let_go(&self) {
        let holds = self.holds.get().saturating_sub(1);
        self.holds.set(holds);
        if holds > 0 {
            return;
        }
        let turn = self.next_turn();
        let Some(shown) = self.shown.get() else { return };
        let left = LINGER.saturating_sub(shown.elapsed());
        if left.is_zero() {
            self.show(false);
            return;
        }
        let this = self.clone();
        glib::timeout_add_local_once(left, move || {
            if this.turn.get() == turn {
                this.show(false);
            }
        });
    }

    fn next_turn(&self) -> u64 {
        let turn = self.turn.get().wrapping_add(1);
        self.turn.set(turn);
        turn
    }

    fn show(&self, on: bool) {
        self.shown.set(on.then(std::time::Instant::now));
        self.spinner.set_visible(on);
        self.region.update_state(&[gtk::accessible::State::Busy(on)]);
    }
}

pub(super) fn busy_in<T: Send + 'static>(
    waiting: &Waiting,
    work: impl FnOnce() -> T + Send + 'static,
) -> impl std::future::Future<Output = Result<T, Box<dyn std::any::Any + Send>>> {
    let hold = waiting.hold();
    async move {
        let out = gtk::gio::spawn_blocking(work).await;
        drop(hold);
        out
    }
}

pub(super) fn dismiss(toast: adw::Toast, up: std::time::Instant) {
    let left = LINGER.saturating_sub(up.elapsed());
    if left.is_zero() {
        toast.dismiss();
        return;
    }
    glib::timeout_add_local_once(left, move || toast.dismiss());
}

fn loader_toast(state: &App, label: &str) -> adw::Toast {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&spinner(label));
    row.append(&gtk::Label::new(Some(label)));
    let toast = adw::Toast::new("");
    toast.set_custom_title(Some(&row));
    toast.set_timeout(0);
    state.toasts.add_toast(toast.clone());
    toast
}

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
        let shown: Rc<RefCell<Option<(adw::Toast, std::time::Instant)>>> = Rc::new(RefCell::new(None));
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
                    *shown.borrow_mut() = Some((loader_toast(&state, &label), std::time::Instant::now()));
                }
            ),
        );

        let out = gtk::gio::spawn_blocking(work).await;
        done.set(true);
        if let Some((toast, up)) = shown.borrow_mut().take() {
            dismiss(toast, up);
        }
        out
    }
}

pub(super) fn busy_sync(state: &App, label: &str, work: impl FnOnce(&App) + 'static) {
    if state.busy.replace(true) {
        return;
    }
    let toast = loader_toast(state, label);
    let up = std::time::Instant::now();

    let state = state.clone();
    let label = label.to_string();

    glib::idle_add_local_once(move || {
        timed(&label, || work(&state));
        dismiss(toast, up);
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
