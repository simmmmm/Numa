use super::*;

pub(super) const UPDATE_CHECK: &str = "check-for-updates";
pub(super) const UPDATE_CHECKED_AT: &str = "updates-checked-at";

pub(super) fn consider_update_check(state: &App, window: &adw::ApplicationWindow) {
    if state.libraries.current.borrow().is_none() {
        return;
    }
    match state.catalog.setting(UPDATE_CHECK).as_deref() {
        Some("yes") => check_for_update(state, window),
        Some(_) => {}
        None => {
            let alert = adw::AlertDialog::new(
                Some("Look for new versions?"),
                Some(
                    "The AppImage does not update itself. Numa can look at its releases page \
                     on GitHub once a day and say when there is a new version. Nothing about you \
                     or your photographs is sent. This can be changed in Preferences.",
                ),
            );
            alert.add_response("no", "No");
            alert.add_response("yes", "Check Daily");
            alert.set_response_appearance("yes", adw::ResponseAppearance::Suggested);
            let (state, parent) = (state.clone(), window.clone());
            alert.connect_response(None, move |_, response| {
                let _ = state.catalog.set_setting(UPDATE_CHECK, response);
                if response == "yes" {
                    check_for_update(&state, &parent);
                }
            });
            alert.present(Some(window));
        }
    }
}

pub(super) fn check_for_update(state: &App, window: &adw::ApplicationWindow) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let last: u64 = state.catalog.setting(UPDATE_CHECKED_AT).and_then(|at| at.parse().ok()).unwrap_or(0);
    if now.saturating_sub(last) < 24 * 3600 {
        return;
    }
    let _ = state.catalog.set_setting(UPDATE_CHECKED_AT, &now.to_string());
    let argv: [&std::ffi::OsStr; 5] =
        ["curl".as_ref(), "--silent".as_ref(), "--location".as_ref(), "--max-time".as_ref(), "15".as_ref()];
    let argv: Vec<&std::ffi::OsStr> = argv.into_iter().chain([numa::io::update::RELEASES.as_ref()]).collect();
    let Ok(process) = gio::Subprocess::newv(&argv, gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE)
    else {
        return;
    };
    let (state, window) = (state.clone(), window.clone());
    glib::spawn_future_local(async move {
        let Ok((Some(json), _)) = process.communicate_utf8_future(None).await else { return };
        let Some((version, page)) = numa::io::update::newer_release(&json, env!("CARGO_PKG_VERSION")) else { return };
        let toast = adw::Toast::new(&format!("Numa {version} is available"));
        toast.set_button_label(Some("What's New"));
        toast.set_timeout(0);
        toast.connect_button_clicked(move |_| {
            gtk::UriLauncher::new(&page).launch(Some(&window), gio::Cancellable::NONE, |_| {});
        });
        state.toasts.add_toast(toast);
    });
}

pub(super) fn first_time(state: &App, key: &str) -> bool {
    state.catalog.setting(key).is_none()
}

pub(super) fn mark_seen(state: &App, key: &str) {
    if let Err(err) = state.catalog.set_setting(key, "1") {
        log::warn!("could not remember {key}: {err}");
    }
}

pub(super) fn key_hint(state: &App, key: &'static str, text: &str) -> adw::Banner {
    let banner = adw::Banner::new(text);
    banner.set_button_label(Some("Got it"));
    banner.set_revealed(first_time(state, key));
    banner.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |banner| {
            banner.set_revealed(false);
            mark_seen(&state, key);
        }
    ));
    banner
}
