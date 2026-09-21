use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use numa::render::display::Display;

thread_local! {

    static SCREEN: RefCell<(Option<Rc<Display>>, String)> = RefCell::new((None, "Not looked for yet".to_string()));
}

pub fn texture(mut image: image::RgbImage) -> gdk::Texture {
    if let Some(display) = SCREEN.with(|screen| screen.borrow().0.clone()) {
        display.apply(&mut image);
    }
    unconverted(image)
}

pub fn unconverted(image: image::RgbImage) -> gdk::Texture {
    let (width, height) = (image.width() as i32, image.height() as i32);
    gdk::MemoryTexture::new(width, height, gdk::MemoryFormat::R8g8b8, &glib::Bytes::from_owned(image.into_raw()), width as usize * 3)
        .upcast()
}

pub fn described() -> String {
    SCREEN.with(|screen| screen.borrow().1.clone())
}

pub fn watch(changed: impl Fn() + 'static) {
    look();
    let changed = Rc::new(changed);
    let again = move || {
        look();
        changed();
    };
    let again = Rc::new(again);
    for (bus, sender, interface, member) in [
        (gio::BusType::System, "org.freedesktop.ColorManager", "org.freedesktop.ColorManager", "DeviceChanged"),
        (gio::BusType::Session, "org.gnome.Mutter.DisplayConfig", "org.gnome.Mutter.DisplayConfig", "MonitorsChanged"),
    ] {
        let Ok(connection) = gio::bus_get_sync(bus, gio::Cancellable::NONE) else { continue };
        let again = again.clone();

        std::mem::forget(connection.subscribe_to_signal(
            Some(sender),
            Some(interface),
            Some(member),
            None,
            None,
            gio::DBusSignalFlags::NONE,
            move |_| again(),
        ));
    }
}

fn look() {
    let (display, said) = match find() {
        Found::Profile { title, filename } => match std::fs::read(&filename).map_err(|err| err.to_string()).and_then(|bytes| Display::from_icc(&bytes)) {
            Ok(Some(display)) => (Some(Rc::new(display)), format!("{title} (automatic)")),
            Ok(None) => (None, format!("{title} — sRGB, nothing to convert")),
            Err(err) => (None, format!("{title} — not used: {err}")),
        },
        Found::Compositor(mode) => (None, format!("Colour managed by GNOME ({mode} mode)")),
        Found::Nothing(why) => (None, why),
    };
    log::info!("display: {said}");
    SCREEN.with(|screen| *screen.borrow_mut() = (display, said));
}

enum Found {
    Profile { title: String, filename: String },
    Compositor(&'static str),
    Nothing(String),
}

const TIMEOUT: i32 = 1000;

fn find() -> Found {
    let Ok(system) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) else {
        return Found::Nothing("sRGB — no system bus to ask".to_string());
    };
    let call = |path: &str, interface: &str, method: &str, args: Option<&glib::Variant>| {
        system
            .call_sync(Some("org.freedesktop.ColorManager"), path, interface, method, args, None, gio::DBusCallFlags::NONE, TIMEOUT, gio::Cancellable::NONE)
            .ok()
    };
    let property = |path: &str, interface: &str, name: &str| {
        call(path, "org.freedesktop.DBus.Properties", "Get", Some(&(interface, name).to_variant()))
            .map(|reply| reply.child_value(0).as_variant().unwrap_or(reply))
    };

    let Some(devices) = call("/org/freedesktop/ColorManager", "org.freedesktop.ColorManager", "GetDevicesByKind", Some(&("display",).to_variant())) else {
        return Found::Nothing("sRGB — colord is not running".to_string());
    };

    let mut chosen: Option<(String, String)> = None;
    for device in devices.child_value(0).iter() {
        let Some(path) = device.str().map(str::to_string) else { continue };
        let Some(profiles) = property(&path, "org.freedesktop.ColorManager.Device", "Profiles") else { continue };
        let Some(profile) = profiles.iter().next().and_then(|first| first.str().map(str::to_string)) else { continue };
        let metadata = property(&path, "org.freedesktop.ColorManager.Device", "Metadata");
        let entry = |key: &str| metadata.as_ref().and_then(|dict| {
            dict.iter().find(|pair| pair.child_value(0).str() == Some(key)).and_then(|pair| pair.child_value(1).str().map(str::to_string))
        });
        let primary = entry("OutputPriority").as_deref() == Some("primary");
        if chosen.is_none() || primary {
            chosen = Some((profile, entry("XRANDR_name").unwrap_or_default()));
        }
        if primary {
            break;
        }
    }
    let Some((profile, connector)) = chosen else {
        return Found::Nothing("sRGB — colord has no profile for this screen".to_string());
    };
    if let Some(mode) = mutter_mode(&connector) {
        return Found::Compositor(mode);
    }
    let text = |name: &str| property(&profile, "org.freedesktop.ColorManager.Profile", name).and_then(|value| value.str().map(str::to_string));
    match (text("Title"), text("Filename")) {
        (title, Some(filename)) => Found::Profile { title: title.unwrap_or_else(|| "Display profile".to_string()), filename },
        _ => Found::Nothing("sRGB — the screen's profile has no file".to_string()),
    }
}

fn mutter_mode(connector: &str) -> Option<&'static str> {
    let session = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
    let state = session
        .call_sync(
            Some("org.gnome.Mutter.DisplayConfig"),
            "/org/gnome/Mutter/DisplayConfig",
            "org.gnome.Mutter.DisplayConfig",
            "GetCurrentState",
            None,
            None,
            gio::DBusCallFlags::NONE,
            TIMEOUT,
            gio::Cancellable::NONE,
        )
        .ok()?;

    let monitor = state.child_value(1).iter().find(|monitor| monitor.child_value(0).child_value(0).str() == Some(connector))?;
    let mode = monitor
        .child_value(2)
        .iter()
        .find(|pair| pair.child_value(0).str() == Some("color-mode"))
        .and_then(|pair| pair.child_value(1).as_variant())
        .and_then(|value| value.get::<u32>())?;
    match mode {
        0 => None,
        1 => Some("HDR"),
        2 => Some("sdr-native"),
        _ => Some("another"),
    }
}
