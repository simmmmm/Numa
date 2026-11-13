use super::home::{account, curl, Running};
use super::*;
use numa::io::home::album_sync::Call;
use numa::io::home::sync;

const ENDS: [(&str, u32); 4] = [("7 days", 7), ("30 days", 30), ("90 days", 90), ("A year", 365)];

pub(super) fn ask(state: &App, window: &adw::ApplicationWindow, album: &str) {
    let Some(home) = state.catalog.albums_at_home().ok().and_then(|paired| paired.into_iter().find(|(key, ..)| key == album).map(|(_, id, ..)| id)) else {
        state.toast(if account(state).signed_in() { "This trip is not at Numa Home yet. It goes with the next sync." } else { "A link to a trip is made at Numa Home. Sign in to Numa Home first." });
        album_round::after_a_round(state);
        return;
    };
    let group = adw::PreferencesGroup::new();
    let route = adw::SwitchRow::new();
    route.set_title("Shows the Route and the Places");
    route.set_subtitle("Off: no map, no route and no positions. A chapter keeps the name you gave it.");
    route.set_active(true);
    let follows = adw::SwitchRow::new();
    follows.set_title("Follows the Trip");
    follows.set_subtitle("New chapters and photographs show as you add them");
    follows.set_active(true);
    let ends = adw::ComboRow::new();
    ends.set_title("Ends");
    ends.set_model(Some(&gtk::StringList::new(&ENDS.map(|(label, _)| label))));
    ends.set_selected(1);
    group.add(&route);
    group.add(&follows);
    group.add(&ends);
    let dialog = adw::AlertDialog::new(Some("A Link to the Trip"), Some("Anyone with the link reads the trip: its route, its chapters and the photographs in them. Nothing else at Numa Home."));
    dialog.set_extra_child(Some(&group));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("make", "Copy Link");
    dialog.set_default_response(Some("make"));
    dialog.set_close_response("cancel");
    let (state, parent, window) = (state.clone(), window.clone(), window.clone());
    dialog.connect_response(None, move |_, response| {
        if response != "make" {
            return;
        }
        let days = ENDS[(ends.selected() as usize).min(ENDS.len() - 1)].1;
        let body = serde_json::json!({ "route": route.is_active(), "follows": follows.is_active(), "days": days }).to_string();
        let call = Call { method: "POST", path: format!("albums/{home}/links"), body: Some(body) };
        let (state, window) = (state.clone(), window.clone());
        glib::spawn_future_local(async move {
            let account = account(&state);
            let running: Running = Rc::default();
            let (status, said) = sync::status_and_body(&curl(sync::album_call(&account.address, &call), account.header(), &running).await);
            let said: serde_json::Value = serde_json::from_str(&said).unwrap_or_default();
            match (status, said["link"].as_str()) {
                (200, Some(link)) => {
                    let base = said["base"].as_str().map_or_else(|| numa::io::home::base(&account.address), str::to_string);
                    made(&state, &window, &format!("{base}{link}"));
                }
                (0, _) => state.toast("Numa Home did not answer."),
                _ => state.toast(said["error"].as_str().unwrap_or("Numa Home did not make the link.")),
            }
        });
    });
    dialog.present(Some(&parent));
}

fn made(state: &App, window: &adw::ApplicationWindow, link: &str) {
    window.clipboard().set_text(link);
    let field = gtk::Entry::new();
    field.set_text(link);
    field.set_editable(false);
    let dialog = adw::AlertDialog::new(Some("Link Made"), Some("Send it to whoever should read the trip. Numa Home shows it only now; End on the trip at Numa Home stops it."));
    dialog.set_extra_child(Some(&field));
    dialog.add_response("done", "Done");
    dialog.set_default_response(Some("done"));
    dialog.set_close_response("done");
    dialog.present(Some(window));
    state.toast("Link copied");
}
