use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use numa_io::whats_new;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::store::Store;
use crate::{Fail, Server};

const KEY: &str = "whats-new:";

const ADOPTED: &str = "whats-new-adopted";

fn key(name: &str) -> String {
    format!("{KEY}{name}")
}

pub fn adopt(store: &Store) -> Result<(), String> {
    if store.setting(ADOPTED).is_some() {
        return Ok(());
    }
    for account in store.accounts()? {
        if store.setting(&key(&account.name)).is_none() {
            store.set_setting(&key(&account.name), whats_new::EARLIER)?;
        }
    }
    store.set_setting(ADOPTED, "1")
}

async fn now(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Value>, Fail> {
    let version = env!("CARGO_PKG_VERSION");
    let start = whats_new::at_start(server.store.setting(&key(&me.name)).as_deref(), version);
    if let Some(store) = &start.store {
        server.store.set_setting(&key(&me.name), store).map_err(Fail::internal)?;
    }
    let lines = whats_new::lines_for(version).unwrap_or_default();
    let more = format!("{}#{version}", whats_new::READ_MORE);
    Ok(Json(json!({ "version": version, "lines": lines, "show": start.show.is_some(), "more": more })))
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new().route("/api/v1/whats-new", get(now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grants::Level;
    use crate::share::tests::http::{account, ask, fixture};

    #[test]
    fn once_for_each_account_and_not_at_its_first_sign_in() {
        let version = env!("CARGO_PKG_VERSION");
        let has_lines = whats_new::lines_for(version).is_some();
        let (new, earlier) = (account("Whats New One", Level::View), account("Whats New Two", Level::View));
        let shown = |who: &str| ask("GET", "/api/v1/whats-new", Some(who), "", "").json()["show"].clone();
        assert_eq!(shown(&new), json!(false), "a first sign-in");
        assert_eq!(shown(&new), json!(false));
        fixture().server.store.set_setting(&key("Whats New Two"), "0.0.1").unwrap();
        let answer = ask("GET", "/api/v1/whats-new", Some(&earlier), "", "").json();
        assert_eq!((answer["version"].clone(), answer["show"].clone()), (json!(version), json!(has_lines)));
        assert_eq!(answer["lines"].as_array().unwrap().len(), whats_new::lines_for(version).map_or(0, |lines| lines.len()));
        assert_eq!(shown(&earlier), json!(false), "once");
        assert_eq!(ask("GET", "/api/v1/whats-new", None, "", "").status, 401, "not for a link's guest");
    }

    #[test]
    fn the_accounts_from_before_have_seen_nothing_yet() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        store.add_account("Alex", "no password", true, true).unwrap();
        adopt(&store).unwrap();
        assert_eq!(store.setting(&key("Alex")).as_deref(), Some(whats_new::EARLIER));
        store.add_account("Sam", "no password", false, false).unwrap();
        adopt(&store).unwrap();
        assert_eq!(store.setting(&key("Sam")), None, "made after: a first sign-in");
    }
}
