use super::*;
use numa::io::home::{self as api, sync::Pace, Account, Failure};
use std::ffi::OsString;

pub(super) const NUMA_HOME: &str = "Numa Home";
pub(super) const SEND_HOME: &str = "Send Home";

const ACCOUNT: &str = "numa-home";
const CHOICE: &str = "numa-home-export";

pub(super) fn account(state: &App) -> Account {
    state.catalog.recall(ACCOUNT).unwrap_or_default()
}

pub(super) fn forget_token(state: &App) {
    state.catalog.remember(ACCOUNT, &Account { token: String::new(), ..account(state) });
}

pub(super) type Running = Rc<RefCell<Option<gio::Subprocess>>>;

pub(super) async fn curl(argv: Vec<OsString>, input: String, running: &Running) -> String {
    let argv: Vec<&std::ffi::OsStr> = argv.iter().map(OsString::as_os_str).collect();
    let flags = gio::SubprocessFlags::STDIN_PIPE | gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE;
    let Ok(process) = gio::Subprocess::newv(&argv, flags) else {
        log::warn!("Send Home: curl is not installed");
        return String::new();
    };
    running.replace(Some(process.clone()));
    let out = process.communicate_utf8_future(Some(input.into())).await;
    running.replace(None);
    match out {
        Ok((out, err)) => {
            if let Some(err) = err.filter(|err| !err.is_empty()) {
                log::warn!("Send Home: {}", err.trim());
            }
            out.map(String::from).unwrap_or_default()
        }
        Err(err) => {
            log::warn!("Send Home: {err}");
            String::new()
        }
    }
}

pub(super) async fn sha256(path: &Path) -> Result<String, Failure> {
    let unread = || Failure::Said(format!("{} could not be read", path.display()));
    let process = gio::Subprocess::newv(&["sha256sum".as_ref(), path.as_os_str()], gio::SubprocessFlags::STDOUT_PIPE)
        .map_err(|_| Failure::Said("sha256sum is not installed".to_string()))?;
    let (out, _) = process.communicate_utf8_future(None).await.map_err(|_| unread())?;
    out.as_deref().and_then(|out| out.split_whitespace().next()).filter(|hex| hex.len() == 64).map(str::to_string).ok_or_else(unread)
}

pub(super) fn said(failure: Failure, account: &Account) -> String {
    match failure {
        Failure::SignIn(_) => "Sign in again in Preferences".to_string(),
        Failure::Unreachable => format!("{NUMA_HOME} at {} cannot be reached", api::host(&account.address)),
        Failure::Said(sentence) => sentence,
    }
}

pub(super) fn preferences_group(state: &App, dialog: &adw::PreferencesDialog) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title(NUMA_HOME);
    group.set_description(Some(&format!(
        "The household's own photo server. Signed in, Export can send photographs there with {SEND_HOME}, \
         and Import can keep a library there. \
         The address is the server's, as nas.local or 192.168.1.20."
    )));
    let address = adw::EntryRow::new();
    address.set_title("Address");
    let name = adw::EntryRow::new();
    name.set_title("Name");
    let password = adw::PasswordEntryRow::new();
    password.set_title("Password");
    let sign_in = gtk::Button::with_label("Sign In");
    sign_in.set_valign(gtk::Align::Center);
    let signed = adw::ActionRow::new();
    let sign_out = gtk::Button::with_label("Sign Out");
    sign_out.set_valign(gtk::Align::Center);
    signed.add_suffix(&sign_out);
    for row in [address.upcast_ref::<gtk::Widget>(), name.upcast_ref(), password.upcast_ref(), signed.upcast_ref()] {
        group.add(row);
    }
    group.set_header_suffix(Some(&sign_in));
    for row in pace_rows(state) {
        group.add(&row);
    }

    let show = Rc::new(glib::clone!(
        #[weak] address,
        #[weak] name,
        #[weak] password,
        #[weak] sign_in,
        #[weak] signed,
        move |account: &Account| {
            address.set_text(&account.address);
            name.set_text(&account.name);
            for row in [address.upcast_ref::<gtk::Widget>(), name.upcast_ref(), password.upcast_ref(), sign_in.upcast_ref()] {
                row.set_visible(!account.signed_in());
            }
            signed.set_visible(account.signed_in());
            signed.set_title(&format!("Signed in as {} on {}", account.name, api::host(&account.address)));
        }
    ));
    show(&account(state));

    password.connect_entry_activated(glib::clone!(
        #[weak] sign_in,
        move |_| sign_in.emit_clicked()
    ));
    sign_in.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] show,
        #[weak] dialog,
        #[weak] address,
        #[weak] name,
        #[weak] password,
        move |button| {
            let account = Account { address: address.text().trim().to_string(), name: name.text().trim().to_string(), token: String::new() };
            if account.address.is_empty() || account.name.is_empty() {
                dialog.add_toast(adw::Toast::new("Fill in the address and your name"));
                return;
            }
            let body = api::login_body(&account.name, &password.text(), &glib::host_name());
            button.set_sensitive(false);
            glib::spawn_future_local(glib::clone!(
                #[strong] state,
                #[strong] show,
                #[weak] dialog,
                #[weak] password,
                #[weak] button,
                async move {
                    let out = curl(api::login(&account.address), body, &Running::default()).await;
                    button.set_sensitive(true);
                    match api::signed_in(&out) {
                        Ok((token, name)) => {
                            let account = Account { name, token, ..account };
                            state.catalog.remember(ACCOUNT, &account);
                            password.set_text("");
                            show(&account);
                        }
                        Err(Failure::SignIn(sentence)) => dialog.add_toast(adw::Toast::new(&glib::markup_escape_text(&sentence))),
                        Err(failure) => dialog.add_toast(adw::Toast::new(&glib::markup_escape_text(&said(failure, &account)))),
                    }
                }
            ));
        }
    ));
    sign_out.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] show,
        move |_| {
            let account = account(&state);

            let (argv, header) = (api::logout(&account.address), account.header());
            glib::spawn_future_local(async move {
                let _ = api::taken(&curl(argv, header, &Running::default()).await);
            });
            let account = Account { token: String::new(), ..account };
            state.catalog.remember(ACCOUNT, &account);
            show(&account);
        }
    ));
    group
}

fn pace_rows(state: &App) -> [adw::PreferencesRow; 2] {
    let pace = home_sync::pace(state);
    let keep = adw::ComboRow::new();
    keep.set_title("Keep in Step");
    keep.set_model(Some(&gtk::StringList::new(&["At Night", "While Open"])));
    keep.set_selected(if pace.night { 0 } else { 1 });
    let said = |keep: &adw::ComboRow| {
        keep.set_subtitle(match keep.selected() {
            0 => "01:00 to 06:00, with Numa open: Linux has no background service",
            _ => "Every few minutes, with Numa open",
        });
    };
    said(&keep);
    keep.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |keep| {
            said(keep);
            home_sync::set_pace(&state, Pace { night: keep.selected() == 0, ..home_sync::pace(&state) });
        }
    ));
    let unmetered = adw::SwitchRow::new();
    unmetered.set_title("Only on Unmetered Networks");
    unmetered.set_subtitle("Not over a phone's hotspot or another connection the system calls metered");
    unmetered.set_active(pace.unmetered);
    unmetered.connect_active_notify(glib::clone!(
        #[strong] state,
        move |row| home_sync::set_pace(&state, Pace { unmetered: row.is_active(), ..home_sync::pace(&state) })
    ));
    [keep.upcast(), unmetered.upcast()]
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum How {

    Jpeg,

    Originals,
}

#[derive(Clone)]
pub(super) struct Destination {
    pub(super) group: adw::PreferencesGroup,
    to: adw::ComboRow,
    how: adw::ComboRow,
}

impl Destination {
    pub(super) fn new(state: &App) -> Self {
        let combo = |title: &str, names: &[&str]| {
            let row = adw::ComboRow::new();
            row.set_title(title);
            row.set_model(Some(&gtk::StringList::new(names)));
            row
        };
        let to = combo("Destination", &["Folder", NUMA_HOME]);
        let how = combo("Send", &["As JPEG", "Original and Edits"]);
        let (at, way): (u32, u32) = state.catalog.recall(CHOICE).unwrap_or_default();
        how.set_selected(way);
        let group = adw::PreferencesGroup::new();
        group.add(&to);
        group.add(&how);
        if account(state).signed_in() {
            to.set_selected(at);
        } else {
            to.set_sensitive(false);
            to.set_subtitle(&format!("Sign in to {NUMA_HOME}, the household's own photo server, to send photographs there"));
            let sign_in = gtk::Button::with_label("Sign In…");
            sign_in.set_valign(gtk::Align::Center);
            sign_in.connect_clicked(|button| {
                let window = button.root().and_downcast::<gtk::Window>();
                if let Some(dialog) = button.ancestor(adw::Dialog::static_type()).and_downcast::<adw::Dialog>() {
                    dialog.close();
                }
                if let Some(window) = window {
                    let _ = window.activate_action("win.preferences", None);
                }
            });
            group.set_header_suffix(Some(&sign_in));
        }
        Self { group, to, how }
    }

    pub(super) fn chosen(&self) -> Option<How> {
        match (self.to.selected(), self.how.selected()) {
            (0, _) => None,
            (_, 0) => Some(How::Jpeg),
            _ => Some(How::Originals),
        }
    }

    pub(super) fn remember(&self, state: &App) {

        if !self.to.is_sensitive() {
            return;
        }
        state.catalog.remember(CHOICE, &(self.to.selected(), self.how.selected()));
    }

    pub(super) fn connect_changed(&self, changed: impl Fn(Option<How>) + 'static) {
        let changed = Rc::new(move |this: &Self| {
            let how = this.chosen();
            this.how.set_visible(this.to.selected() == 1);
            this.how.set_subtitle(match how {
                Some(How::Originals) => "The file as the camera wrote it, then its edits; of a RAW+JPEG, the raw",
                _ => "The export set up below, as a JPEG",
            });
            changed(how);
        });
        changed(self);
        for row in [&self.to, &self.how] {
            row.connect_selected_notify(glib::clone!(
                #[strong(rename_to = this)] self,
                #[strong] changed,
                move |_| changed(&this)
            ));
        }
    }
}

pub(super) fn scratch() -> PathBuf {
    let root = numa::core::paths::cache_dir().join("send-home");
    let day = std::time::Duration::from_secs(24 * 3600);
    for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        let old = entry.metadata().and_then(|meta| meta.modified()).is_ok_and(|at| at.elapsed().is_ok_and(|age| age > day));
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_nanos());
    let folder = root.join(format!("{}-{at}", std::process::id()));
    if let Err(err) = std::fs::create_dir_all(&folder) {
        log::warn!("Send Home: {}: {err}", folder.display());
    }
    folder
}

pub(super) fn send(state: &App, jobs: Vec<ExportJob>, how: How, settings: export::ExportSettings) {
    let folder = scratch();
    match how {
        How::Jpeg => {
            let settings = export::ExportSettings { format: export::Format::Jpeg, folder: Some(folder.clone()), ..settings };

            exporting::run_export_then(state, jobs, settings, folder.clone(), move |state, _, failures, _| {
                let mut files: Vec<PathBuf> = std::fs::read_dir(&folder).into_iter().flatten().flatten().map(|entry| entry.path()).collect();
                files.sort();
                upload(state, files.into_iter().map(|file| (file, None)).collect(), failures.to_vec(), folder);
            });
        }
        How::Originals => {
            let mut failures = Vec::new();
            let photos: Vec<(i64, PathBuf)> = jobs
                .into_iter()
                .filter_map(|job| match job.source {
                    Source::Photo { id, path } => Some((id, path)),
                    Source::Bracket { .. } => {
                        failures.push("An HDR merge has no one original: send it As JPEG".to_string());
                        None
                    }
                })
                .collect();
            let paths: Vec<PathBuf> = photos.iter().map(|(_, path)| path.clone()).collect();

            let files = api::originals(&paths)
                .into_iter()
                .map(|at| (photos[at].1.clone(), state.catalog.edits_json(photos[at].0).ok().flatten()))
                .collect();
            upload(state, files, failures, folder);
        }
    }
}

fn upload(state: &App, files: Vec<(PathBuf, Option<String>)>, mut failures: Vec<String>, folder: PathBuf) {
    let account = account(state);
    let total = files.len();
    let cancel = Cancel::default();
    let running = Running::default();
    let (toast, text, bar) = progress_toast(state, &cancel);

    toast.connect_button_clicked(glib::clone!(
        #[strong] running,
        move |_| {
            if let Some(process) = running.borrow().as_ref() {
                process.force_exit();
            }
        }
    ));
    let state = state.clone();
    glib::spawn_future_local(async move {
        let (mut sent, mut already) = (0, 0);
        for (at, (file, edits)) in files.iter().enumerate() {
            if cancel.stopped() {
                break;
            }
            text.set_text(&format!("Sending {} of {total} to {NUMA_HOME}", at + 1));
            bar.set_fraction(at as f64 / total as f64);
            match one(&account, file, edits.as_deref(), &folder, &running).await {
                Ok(duplicate) => {
                    sent += 1;
                    already += duplicate as usize;
                }
                Err(_) if cancel.stopped() => break,
                Err(failure) => {

                    let last = !matches!(failure, Failure::Said(_));
                    if let Failure::SignIn(_) = failure {
                        forget_token(&state);
                    }
                    failures.push(said(failure, &account));
                    if last {
                        break;
                    }
                }
            }
        }
        toast.dismiss();
        let _ = std::fs::remove_dir_all(&folder);
        for failure in &failures {
            log::warn!("Send Home: {failure}");
        }
        state.toast(&summary(sent, already, total, cancel.stopped(), &failures));
    });
}

async fn one(account: &Account, file: &Path, edits: Option<&str>, folder: &Path, running: &Running) -> Result<bool, Failure> {
    let sha256 = sha256(file).await?;
    let sent = api::sent(&curl(api::send(&account.address, file, &sha256), account.header(), running).await)?;
    if let Some(edits) = edits {
        let stack = folder.join("edits.json");
        std::fs::write(&stack, edits).map_err(|err| Failure::Said(err.to_string()))?;
        api::taken(&curl(api::edits(&account.address, &sent.id, &stack), account.header(), running).await)?;
    }
    Ok(sent.duplicate)
}

fn summary(sent: usize, already: usize, total: usize, stopped: bool, failures: &[String]) -> String {
    let photographs = |n: usize| if n == 1 { "1 photograph".to_string() } else { format!("{n} photographs") };
    let there = match already {
        0 => String::new(),
        1 => ", one was there already".to_string(),
        n => format!(", {n} were there already"),
    };
    match failures {
        _ if stopped => format!("Stopped after {sent} of {total}; those are on {NUMA_HOME}"),
        [] => format!("Sent {} to {NUMA_HOME}{there}", photographs(sent)),
        [only] if sent == 0 => only.clone(),
        [first, ..] => format!("Sent {sent}, {} failed: {first}", failures.len()),
    }
}
