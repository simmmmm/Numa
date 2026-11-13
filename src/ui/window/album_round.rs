use super::home::{account, curl, Running};
use super::*;
use numa::io::home::album_sync::{Call, Round};
use numa::io::home::sync;

thread_local! {

    static GOING: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn after_a_round(state: &App) {
    if GOING.with(|going| going.replace(true)) {
        return;
    }
    let state = state.clone();
    glib::spawn_future_local(async move {
        match go(&state).await {
            Ok((came, went)) if came + went > 0 => {
                log::info!("Numa Home: {came} albums changed here, {went} went up");
                refresh_places(&state);
                if state.stack.visible_child_name().as_deref() == Some("album") {
                    album_page::refill(&state);
                }
            }
            Ok(_) => {}
            Err(err) => log::warn!("Numa Home, albums: {err}"),
        }
        GOING.with(|going| going.set(false));
    });
}

async fn go(state: &App) -> Result<(usize, usize), String> {
    let account = account(state);
    if !account.signed_in() {
        return Ok((0, 0));
    }
    let running: Running = Rc::default();
    let ask = |calls: Vec<Call>| {
        let (account, running) = (account.clone(), running.clone());
        async move {
            let mut answers = Vec::with_capacity(calls.len());
            for call in &calls {
                answers.push(sync::status_and_body(&curl(sync::album_call(&account.address, call), account.header(), &running).await));
            }
            answers
        }
    };
    let (mut round, first) = Round::begin(&state.catalog)?;
    let listed = ask(vec![first]).await;
    let docs = round.listed(&state.catalog, listed[0].0, &listed[0].1)?;
    let fetched = ask(docs).await;
    let ups = round.fetched(&state.catalog, &fetched)?;
    let answered = ask(ups).await;
    let done = round.answered(&state.catalog, &answered)?;
    Ok((done.came, done.went))
}
