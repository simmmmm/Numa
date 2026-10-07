use super::import_done::Kept;
use super::*;
use numa::io::offload::Report;
use numa::io::stand_in::{self, StandIn};

pub(super) fn group(stand_ins: &[StandIn], on: &Rc<Cell<bool>>) -> adw::PreferencesGroup {
    let mut models: HashMap<&str, usize> = HashMap::new();
    for stand_in in stand_ins.iter().filter(|stand_in| !stand_in.model.is_empty()) {
        *models.entry(stand_in.model.as_str()).or_default() += 1;
    }
    let from = match models.into_iter().max_by_key(|(_, count)| *count) {
        Some((model, _)) => format!("the {model} app"),
        None => "the camera's app".to_string(),
    };
    let group = adw::PreferencesGroup::new();
    group.set_title("Already Here as JPEGs");
    let row = adw::SwitchRow::new();
    set_row_title(&row, &format!("{} from {from}", places::grouped(stand_ins.len() as i64)));
    row.set_subtitle("The raws take their place \u{b7} stars, flags, albums, names and crops stay");
    row.set_active(on.get());
    row.connect_active_notify(glib::clone!(
        #[strong] on,
        move |row| on.set(row.is_active())
    ));
    group.add(&row);
    group
}

pub(super) fn take_places(state: &App, stand_ins: &HashMap<PathBuf, PathBuf>, report: &Report, touched: &[Library]) -> Kept {
    let mut kept = Kept::default();
    if stand_ins.is_empty() {
        return kept;
    }
    let libraries: Vec<Library> = state.libraries.all.borrow().iter().chain(touched).cloned().collect();
    let (mut ids, mut read): (HashMap<PathBuf, i64>, Vec<i64>) = Default::default();

    let mut id_of = |path: &Path| -> Option<i64> {
        if let Some(id) = ids.get(path) {
            return Some(*id);
        }
        let library = libraries.iter().filter(|library| path.starts_with(&library.path)).max_by_key(|library| library.path.components().count())?;
        if read.contains(&library.id) {
            return None;
        }
        read.push(library.id);
        ids.extend(state.catalog.photos(library.id, &Filter::default()).unwrap_or_default().into_iter().map(|photo| (photo.path, photo.id)));
        ids.get(path).copied()
    };
    for landed in &report.landed {
        let Some(jpeg) = stand_ins.get(&landed.card) else { continue };
        let (Some(jpeg_id), Some(raw_id)) = (id_of(jpeg), id_of(&landed.to)) else { continue };
        let edit = state.catalog.load_edits(jpeg_id).ok().flatten();
        let (carried, behind) = edit.as_ref().map_or((None, false), |edit| stand_in::carry(edit, &landed.to));
        match state.catalog.take_place(jpeg_id, raw_id, carried.as_ref()) {
            Ok(()) => {
                kept.replaced += 1;
                kept.shapes += usize::from(carried.is_some());
                kept.behind += usize::from(behind);
            }
            Err(err) => log::warn!("{}: the raw could not take its place: {err}", jpeg.display()),
        }
    }
    log::info!("import: {} raws took the place of their JPEGs", kept.replaced);
    kept
}

pub(super) fn said(kept: &Kept) -> Vec<String> {
    if kept.replaced == 0 {
        return Vec::new();
    }
    let mut lines = vec![match kept.replaced {
        1 => "1 raw took the place of its JPEG, with its stars, flag, albums and names; the JPEG stays beside it as its pair.".to_string(),
        n => format!(
            "{} raws took the place of their JPEGs, with their stars, flags, albums and names; the JPEGs stay beside them as their pairs.",
            places::grouped(n as i64)
        ),
    }];
    match (kept.shapes > 0, kept.behind > 0) {
        (true, true) => lines.push("Crops and turns came along; colour and tone set on a JPEG stayed with it.".into()),
        (true, false) => lines.push("Their crops and turns came along.".into()),
        (false, true) => lines.push("Colour and tone set on a JPEG stayed with it: on a raw they mean something else.".into()),
        (false, false) => {}
    }
    lines
}
