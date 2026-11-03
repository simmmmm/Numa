use std::path::Path as FilePath;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::grants::Level;
use crate::store::{Account, Store};
use crate::{Fail, Server};

const MOST_PEOPLE: usize = 100;

pub fn tables(store: &Store) -> Result<(), String> {
    store.with(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS album_owner (album TEXT PRIMARY KEY, owner TEXT NOT NULL COLLATE NOCASE);
             CREATE TABLE IF NOT EXISTS album_people (
                 album TEXT NOT NULL, account TEXT NOT NULL COLLATE NOCASE, may_add INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (album, account));",
        )
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shared {
    pub key: String,
    pub owner: Option<String>,

    pub people: Vec<(String, bool)>,
}

impl Shared {

    pub fn pool(&self) -> Vec<&str> {
        self.owner.iter().map(String::as_str).chain(self.people.iter().filter(|(_, add)| *add).map(|(name, _)| name.as_str())).collect()
    }
}

impl Store {
    pub fn album_owner(&self, key: &str) -> Option<String> {
        self.with(|conn| conn.query_row("SELECT owner FROM album_owner WHERE album = ?1", [key], |row| row.get(0)).optional()).ok().flatten()
    }

    pub fn claim_album(&self, key: &str, owner: &str) -> Result<(), String> {
        self.with(|conn| conn.execute("INSERT OR IGNORE INTO album_owner (album, owner) VALUES (?1, ?2)", params![key, owner])).map(drop)
    }

    pub fn album_people(&self, key: &str) -> Vec<(String, bool)> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT account, may_add FROM album_people WHERE album = ?1 ORDER BY account COLLATE NOCASE")?;
            let rows = statement.query_map([key], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn set_album_people(&self, key: &str, people: &[(String, bool)]) -> Result<(), String> {
        let mut conn = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let work = conn.transaction().and_then(|tx| {
            tx.execute("DELETE FROM album_people WHERE album = ?1", [key])?;
            for (account, add) in people {
                tx.execute("INSERT OR REPLACE INTO album_people (album, account, may_add) VALUES (?1, ?2, ?3)", params![key, account, add])?;
            }
            tx.commit()
        });
        work.map_err(|err| err.to_string())
    }

    pub fn forget_album(&self, key: &str) {
        let _ = self.with(|conn| {
            conn.execute("DELETE FROM album_owner WHERE album = ?1", [key])?;
            conn.execute("DELETE FROM album_people WHERE album = ?1", [key])
        });
    }

    pub fn shared_albums(&self, account: &str) -> Result<Vec<Shared>, String> {
        self.with(|conn| {
            let mut statement = conn.prepare(
                "SELECT DISTINCT p.album FROM album_people p LEFT JOIN album_owner o ON o.album = p.album
                 WHERE p.account = ?1 OR o.owner = ?1",
            )?;
            let keys = statement.query_map([account], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(keys)
        })
        .map(|keys| keys.into_iter().map(|key| Shared { owner: self.album_owner(&key), people: self.album_people(&key), key }).collect())
    }
}

pub fn may_add(store: &Store, me: &Account, key: &str) -> bool {
    me.admin
        || me.sight.household
        || store.album_owner(key).is_some_and(|owner| owner.eq_ignore_ascii_case(&me.name))
        || store.album_people(key).iter().any(|(account, add)| *add && account.eq_ignore_ascii_case(&me.name))
}

pub fn may_share(store: &Store, me: &Account, key: &str) -> bool {
    store.album_owner(key).is_some_and(|owner| owner.eq_ignore_ascii_case(&me.name))
}

fn may_manage_people(store: &Store, me: &Account, key: &str) -> bool {
    may_share(store, me, key) || store.album_owner(key).is_none() && me.sight.household
}

pub fn reachable(ids: &[i64], pool: &[crate::grants::Sight]) -> Vec<i64> {
    ids.iter().copied().filter(|id| pool.iter().any(|sight| sight.level(*id) >= Level::Edit)).collect()
}

async fn known(server: &Arc<Server>, key: &str) -> Result<(), Fail> {
    let key = key.to_string();
    match server.home.ask(move |catalog| catalog.albums().unwrap_or_default().iter().any(|(have, _)| *have == key)).await? {
        true => Ok(()),
        false => Err(Fail(StatusCode::NOT_FOUND, "There is no such album.".into())),
    }
}

fn nobody() -> Fail {
    Fail(StatusCode::NOT_FOUND, "There is no such album.".into())
}

async fn people(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>) -> Result<Json<Value>, Fail> {
    known(&server, &key).await?;
    if !may_manage_people(&server.store, &me, &key) {
        return Err(nobody());
    }
    let owner = server.store.album_owner(&key).unwrap_or_else(|| me.name.clone());
    let candidates: Vec<String> = match me.sight.household {
        true => server.store.accounts().map_err(Fail::internal)?.into_iter().map(|account| account.name).filter(|name| !name.eq_ignore_ascii_case(&owner)).collect(),
        false => Vec::new(),
    };
    let people: Vec<Value> = server.store.album_people(&key).into_iter().map(|(account, add)| json!({ "account": account, "level": if add { "add" } else { "view" } })).collect();
    Ok(Json(json!({ "owner": owner, "people": people, "candidates": candidates })))
}

#[derive(Deserialize)]
struct Sharing {

    people: std::collections::HashMap<String, String>,
}

async fn share(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>, Json(asked): Json<Sharing>) -> Result<Json<Value>, Fail> {
    known(&server, &key).await?;
    if !may_manage_people(&server.store, &me, &key) {
        return Err(nobody());
    }
    if asked.people.len() > MOST_PEOPLE {
        return Err(Fail::bad("An album is shared with at most 100 people."));
    }
    server.store.claim_album(&key, &me.name).map_err(Fail::internal)?;
    let owner = server.store.album_owner(&key).unwrap_or_else(|| me.name.clone());
    let mut people = Vec::new();
    for (name, level) in &asked.people {
        let add = match level.as_str() {
            "view" => false,
            "add" => true,
            _ => return Err(Fail::bad("A person may look or add.")),
        };
        let (name, _, _) = server.store.account(name).ok_or_else(|| Fail::bad("There is no such account."))?;
        if !name.eq_ignore_ascii_case(&owner) {
            people.push((name, add));
        }
    }
    server.store.set_album_people(&key, &people).map_err(Fail::internal)?;
    Ok(Json(json!({ "people": people.len() })))
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new().route("/api/v1/albums/{key}/people", get(people).put(share))
}

#[cfg(test)]
mod tests {
    use super::*;
    use numa_io::catalog::Library;
    use std::path::PathBuf;

    fn store() -> Store {
        let store = Store::open(FilePath::new(":memory:")).unwrap();
        tables(&store).unwrap();
        store
    }

    fn library(id: i64, path: &str) -> Library {
        Library { id, path: PathBuf::from(path), name: None }
    }

    fn photo(library: i64, row: i64) -> i64 {
        (library << 32) | row
    }

    #[test]
    fn an_album_keeps_its_owner_and_people() {
        let store = store();
        store.claim_album("1a", "Sam").unwrap();
        store.claim_album("1a", "Alex").unwrap();
        assert_eq!(store.album_owner("1a").as_deref(), Some("Sam"), "the first owner stays");
        store.set_album_people("1a", &[("Alex".into(), false), ("Kim".into(), true)]).unwrap();
        store.set_album_people("1a", &[("Kim".into(), true), ("Jo".into(), false)]).unwrap();
        assert_eq!(store.album_people("1a"), vec![("Jo".to_string(), false), ("Kim".to_string(), true)]);
        let seen = |name: &str| store.shared_albums(name).unwrap().into_iter().map(|album| album.key).collect::<Vec<_>>();
        assert_eq!((seen("Sam"), seen("kim"), seen("Alex")), (vec!["1a".to_string()], vec!["1a".to_string()], Vec::<String>::new()), "owner and people, not others");
        store.forget_album("1a");
        assert!(store.album_owner("1a").is_none() && store.album_people("1a").is_empty());
    }

    #[test]
    fn a_shared_album_shows_what_its_pool_may_edit() {
        let shelf = [library(1, "/photos/Holiday"), library(2, "/sent/Sam"), library(3, "/sent/Kim"), library(4, "/sent/Jo")];
        let ids = [photo(1, 1), photo(2, 1), photo(3, 1), photo(4, 1), photo(9, 1)];
        let sight = |name: &str, holiday: bool| {
            let grants = crate::grants::Grants { libraries: if holiday { [(1, Level::Edit)].into() } else { Default::default() }, ..Default::default() };
            crate::grants::resolve(false, &PathBuf::from("/sent").join(name), FilePath::new("/photos"), &shelf, &grants, &[])
        };
        let pool = [sight("Sam", true), sight("Kim", false)];
        assert_eq!(reachable(&ids, &pool), vec![photo(1, 1), photo(2, 1), photo(3, 1)], "Jo is no one of the pool; Holiday is Sam's to share");
        let pool = [sight("Sam", false), sight("Kim", false)];
        assert_eq!(reachable(&ids, &pool), vec![photo(2, 1), photo(3, 1)], "Sam may no longer edit Holiday: it leaves the album");
        assert!(reachable(&ids, &[]).is_empty(), "a pool of no one shows nothing");
    }

    fn account(name: &str, admin: bool, household: bool) -> Account {
        let sight = crate::grants::resolve(
            admin,
            FilePath::new("/sent/x"),
            FilePath::new("/photos"),
            &[],
            &crate::grants::Grants { access: if household { crate::grants::Level::Edit } else { crate::grants::Level::None }, ..Default::default() },
            &[],
        );
        Account { name: name.into(), admin, sight: Arc::new(sight), ..Account::default() }
    }

    #[test]
    fn only_the_right_people_add_and_share() {
        let store = store();
        store.claim_album("1a", "Sam").unwrap();
        store.set_album_people("1a", &[("Kim".into(), true), ("Jo".into(), false)]).unwrap();
        let (sam, kim, jo, alex, admin) = (account("Sam", false, false), account("Kim", false, false), account("Jo", false, false), account("Alex", false, true), account("Root", true, true));
        let adds: Vec<bool> = [&sam, &kim, &jo, &alex, &admin].iter().map(|who| may_add(&store, who, "1a")).collect();
        assert_eq!(adds, [true, true, false, true, true], "a viewer does not add; the household still does");
        let shares: Vec<bool> = [&sam, &kim, &jo, &alex, &admin].iter().map(|who| may_share(&store, who, "1a")).collect();
        assert_eq!(shares, [true, false, false, false, false], "only the owner shares; an admin does not");
    }
}
