use super::*;

pub(super) fn appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|path| path.is_file())
}

pub(super) fn sandboxed() -> bool {
    std::env::var_os("FLATPAK_ID").is_some()
}

pub(super) fn menu_entry_files() -> Option<(PathBuf, PathBuf)> {

    let data = numa::core::paths::data_dir().parent()?.to_path_buf();
    Some((
        data.join("applications/com.tijmen.Numa.desktop"),
        data.join("icons/hicolor/256x256/apps/com.tijmen.Numa.png"),
    ))
}

pub(super) fn menu_entry_for(path: &std::path::Path, icon: &std::path::Path) -> String {

    let quoted: String = path
        .to_string_lossy()
        .chars()
        .flat_map(|c| match c {
            '"' | '`' | '$' | '\\' => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    include_str!("../../../data/com.tijmen.Numa.desktop")
        .lines()
        .map(|line| match line {
            "Exec=numa %F" => format!("TryExec={}\nExec=\"{quoted}\" %F", path.display()),

            "Icon=com.tijmen.Numa" => format!("Icon={}", icon.display()),
            line => line.to_string(),
        })
        .chain(std::iter::once("X-Numa-AppImage=true".to_string()))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

pub(super) fn menu_entry_target() -> Option<PathBuf> {
    let (entry, _) = menu_entry_files()?;
    let text = std::fs::read_to_string(entry).ok()?;
    if !text.contains("X-Numa-AppImage=true") {
        return None;
    }
    text.lines().find_map(|line| line.strip_prefix("TryExec=")).map(PathBuf::from)
}

pub(super) fn write_menu_entry(path: &std::path::Path) -> Result<(), String> {
    let (entry, icon) = menu_entry_files().ok_or("no data folder")?;
    for file in [&entry, &icon] {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        }
    }
    std::fs::write(&icon, include_bytes!("../../../data/icons/hicolor/256x256/apps/com.tijmen.Numa.png"))
        .map_err(|err| err.to_string())?;
    std::fs::write(&entry, menu_entry_for(path, &icon)).map_err(|err| err.to_string())
}

pub(super) fn opened_types() -> Vec<String> {
    include_str!("../../../data/com.tijmen.Numa.desktop")
        .lines()
        .find_map(|line| line.strip_prefix("MimeType="))
        .map(|types| types.split(';').filter(|t| !t.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

pub(super) fn desktop_entry() -> Option<gio::AppInfo> {
    gio::AppInfo::all().into_iter().find(|info| {
        info.id().is_some_and(|id| id == "com.tijmen.Numa.desktop")
    })
}

pub(super) fn opens_photographs() -> bool {
    let Some(ours) = desktop_entry() else { return false };
    gio::AppInfo::default_for_type("image/x-adobe-dng", false)
        .is_some_and(|other| other.id() == ours.id())
}

pub(super) fn set_opens_photographs(on: bool) -> Result<(), String> {
    let ours = desktop_entry().ok_or("Numa has no entry in the applications menu yet")?;
    for kind in opened_types() {
        if !on {
            gio::AppInfo::reset_type_associations(&kind);
            continue;
        }

        if let Err(err) = ours.set_as_default_for_type(&kind) {
            log::warn!("could not set the default for {kind}: {err}");
        }
    }
    Ok(())
}

pub(super) fn remove_menu_entry() {
    if menu_entry_target().is_none() {
        return;
    }
    if let Some((entry, icon)) = menu_entry_files() {
        let _ = std::fs::remove_file(entry);
        let _ = std::fs::remove_file(&icon);

        if let Some(theme) = icon.ancestors().nth(3) {
            let _ = std::fs::File::open(theme).and_then(|dir| dir.set_modified(std::time::SystemTime::now()));
        }
    }
}

fn remove_dead_menu_entry() {
    if menu_entry_target().is_some_and(|path| !path.exists()) {
        remove_menu_entry();
    }
}

pub(super) fn integrate_appimage(state: &App) {
    let Some(path) = appimage() else {
        remove_dead_menu_entry();
        return;
    };
    if menu_entry_target().is_some() {

        let current = menu_entry_files().and_then(|(entry, icon)| {
            Some((std::fs::read_to_string(entry).ok()?, menu_entry_for(&path, &icon)))
        });
        if current.is_none_or(|(have, want)| have != want) {
            if let Err(err) = write_menu_entry(&path) {
                log::warn!("could not update the menu entry: {err}");
            }
        }
        return;
    }
    if state.catalog.recall::<bool>(OFFERED_MENU_ENTRY).unwrap_or(false) {
        return;
    }
    state.catalog.remember(OFFERED_MENU_ENTRY, &true);

    let toast = adw::Toast::new("Add Numa to the applications menu?");
    toast.set_button_label(Some("Add"));
    toast.set_timeout(0);
    toast.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |_| match write_menu_entry(&path) {
            Ok(()) => state.toast("Numa is in the applications menu"),
            Err(err) => state.toast(&format!("Could not add the menu entry: {err}")),
        }
    ));
    state.toasts.add_toast(toast);
}
