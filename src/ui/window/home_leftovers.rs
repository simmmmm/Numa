use super::*;
use numa::io::home::same::{self, Held, Print};
use numa::io::home::sync::{self, Link};
use numa::io::home::Failure;

const OFFERED: &str = "old-numa-home-folders-offered";

pub(super) fn offer(state: &App) {
    if state.catalog.recall::<bool>(OFFERED).unwrap_or(false) || !home::account(state).signed_in() {
        return;
    }
    let root = home_sync::home_root();

    if home_sync::photos_root(state).as_ref() == Some(&root) {
        return;
    }
    let kept: Vec<PathBuf> = state.catalog.libraries().unwrap_or_default().into_iter().filter(|library| home_sync::is_synced(state, library.id)).map(|library| library.path).collect();
    let state = state.clone();
    glib::spawn_future_local(async move {
        glib::timeout_future_seconds(8).await;
        let looked = root.clone();
        let (folders, size) = gio::spawn_blocking(move || {
            let inside = std::fs::read_dir(&looked).into_iter().flatten().flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect();
            let folders = sync::old_folders(inside, &kept);
            let size: u64 = folders.iter().map(|folder| preferences::folder_size(folder, &[])).sum();
            (folders, size)
        })
        .await
        .unwrap_or_default();
        if size > 0 {
            ask(&state, &root, folders, size);
        }
    });
}

fn ask(state: &App, root: &Path, folders: Vec<PathBuf>, size: u64) {
    let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() else { return };
    let many = match folders.len() {
        1 => "1 folder".to_string(),
        count => format!("{count} folders"),
    };
    let body = format!(
        "{} holds {many} from an earlier Numa, {}. Your libraries are shown here without them.\n\nMove to Trash first checks that {} has every photograph in a folder, and a folder where it does not stays. It gives the space back when the trash is emptied, and can be undone until then. Nothing changes unless you choose it, and this is asked once.",
        root.display(),
        glib::format_size(size),
        home::NUMA_HOME,
    );
    let dialog = adw::AlertDialog::new(Some(&format!("{} on This Computer?", glib::format_size(size))), Some(&body));
    dialog.add_response("keep", "Keep Them");
    dialog.add_response("move", "Move to a Folder\u{2026}");
    dialog.add_response("trash", "Move to Trash");
    dialog.set_response_appearance("trash", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("keep"));
    dialog.set_close_response("keep");
    let state = state.clone();
    dialog.connect_response(None, move |_, response| {
        state.catalog.remember(OFFERED, &true);
        match response {
            "trash" => to_trash(&state, folders.clone()),
            "move" => {
                let (state, folders) = (state.clone(), folders.clone());
                home_sync::with_photos_root(&state.clone(), move |to| move_to(&state, folders, to, size));
            }
            _ => {}
        }
    });
    dialog.present(Some(&window));
}

async fn all_at_home(state: &App) -> Result<Vec<Print>, Failure> {
    let account = home::account(state);
    let listed = home::curl(sync::list_libraries(&account.address), account.header(), &home::Running::default()).await;
    let mut all = Vec::new();
    for library in sync::libraries(&listed)? {
        let photos = home::curl(sync::list_photos(&account.address, &library.id, 0), account.header(), &home::Running::default()).await;
        all.extend(sync::photos(&photos)?.into_iter().map(|photo| Print { path: photo.path, size: photo.size, held: Held::Home(photo.id) }));
    }
    Ok(all)
}

async fn why_it_stays(state: &App, folder: &Path, at_home: &[Print]) -> Option<String> {
    let looked = folder.to_path_buf();
    let Ok(here) = gio::spawn_blocking(move || same::prints_in(&looked)).await else {
        return Some("it could not be read".to_string());
    };
    let pairs = same::matched(&here, at_home);
    match here.len() - pairs.len() {
        0 => {}
        1 => return Some(format!("1 photograph is not at {}", home::NUMA_HOME)),
        missing => return Some(format!("{missing} photographs are not at {}", home::NUMA_HOME)),
    }
    if pairs.is_empty() || same_photos::same_bytes(state, &here, at_home, &pairs).await.unwrap_or(false) {
        return None;
    }
    Some(format!("its photographs are not the same as those at {}", home::NUMA_HOME))
}

fn to_trash(state: &App, folders: Vec<PathBuf>) {
    let state = state.clone();
    glib::spawn_future_local(async move {
        let libraries = state.catalog.libraries().unwrap_or_default();
        let mut said = false;
        let mut went_round = Vec::new();
        for folder in folders {
            let row = libraries.iter().find(|library| library.path == folder);
            if let Some(library) = row {
                if home_sync::is_linked(&state, library.id) && !home_sync::a_round_of(&state, library).await {
                    state.toast(&format!("{} stays: {} could not be reached for all of it", folder.display(), home::NUMA_HOME));
                    said = true;
                    continue;
                }
            }
            went_round.push((folder, row.map(|library| library.id)));
        }

        let Ok(at_home) = all_at_home(&state).await else {
            return state.toast(&format!("Nothing moved: {} could not be reached", home::NUMA_HOME));
        };
        let mut going = Vec::new();
        for (folder, row) in went_round {
            match why_it_stays(&state, &folder, &at_home).await {
                Some(why) => {
                    state.toast(&format!("{} stays: {why}", folder.display()));
                    said = true;
                }
                None => going.push((folder, row)),
            }
        }
        let hold = home_sync::hold_rounds().await;
        for (folder, row) in going {
            if let Some(id) = row {
                if let Err(err) = state.catalog.remove_library(id) {
                    state.toast(&err);
                    said = true;
                    continue;
                }
            }
            let trashed = folder.clone();
            if let Ok(Err(err)) = gio::spawn_blocking(move || gio::File::for_path(&trashed).trash(gio::Cancellable::NONE).map_err(|err| err.to_string())).await {
                state.toast(&format!("{}: {err}", folder.display()));
                said = true;
            }
        }
        drop(hold);
        if !said {
            state.toast("Moved to the trash");
        }
        reload_libraries(&state);
        home_sync::adopt(&state, true);
    });
}

fn move_to(state: &App, folders: Vec<PathBuf>, to: PathBuf, size: u64) {
    let state = state.clone();
    glib::spawn_future_local(async move {
        let hold = home_sync::hold_rounds().await;
        let libraries = state.catalog.libraries().unwrap_or_default();
        state.toast(&format!("Moving {}\u{2026}", glib::format_size(size)));
        for folder in folders {
            let Some(name) = folder.file_name() else { continue };
            let target = to.join(name);
            if target.exists() {
                state.toast(&format!("{} exists already", target.display()));
                continue;
            }
            let row = libraries.iter().find(|library| library.path == folder);

            if let Some((library, link)) = row.and_then(|library| Some((library, home_sync::linked(&state, library.id)?))) {
                home_sync::keep(&state, library.id, &Link { way: Some(sync::way(&link, true)), ..link });
            }
            let (from, into) = (folder.clone(), target.clone());
            let moved = gio::spawn_blocking(move || home_sync::move_folder(&from, &into)).await.unwrap_or_else(|_| Err("The move stopped".to_string()));
            if let Err(err) = moved {
                state.toast(&err);
                continue;
            }
            if let Some(library) = row {
                if let Err(err) = state.catalog.relocate_library(library.id, &target) {
                    state.toast(&err.to_string());
                }
                home_sync::forget_way(library.id);
            }
        }
        drop(hold);
        reload_libraries(&state);
        state.toast("Moved");
    });
}
