use std::sync::atomic::{AtomicBool, Ordering};

use gtk::gio;
use gtk::gio::prelude::*;
use gtk::glib;

static SAVER: AtomicBool = AtomicBool::new(false);
static BATTERY: AtomicBool = AtomicBool::new(false);

thread_local! {

    static KEPT: std::cell::RefCell<Vec<glib::Object>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn apply() {
    let forced = std::env::var("NUMA_FRUGAL").as_deref() == Ok("1");
    let on = forced || SAVER.load(Ordering::Relaxed) || BATTERY.load(Ordering::Relaxed);
    if numa::core::power::frugal() == on {
        return;
    }
    log::info!("power: {}", if on { "frugal — nothing decoded ahead, background work on a quarter of the cores" } else { "full speed" });
    numa::core::power::set_frugal(on);

    numa::io::raw::gpu_follows_power();

    if on {
        numa::infer::release_all();
        give_back();
    }
}

fn give_back() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    std::thread::spawn(|| unsafe {
        libc::malloc_trim(0);
    });
}

const IDLE: std::time::Duration = std::time::Duration::from_secs(180);
const IDLE_FRUGAL: std::time::Duration = std::time::Duration::from_secs(30);

fn sweep_idle_models() {
    glib::timeout_add_seconds_local(30, || {
        let idle = if numa::core::power::frugal() { IDLE_FRUGAL } else { IDLE };
        let released = numa::infer::release_idle(idle);
        if released > 0 {
            log::info!("{released} model(s) let go after {} s unused", idle.as_secs());
            give_back();
        }
        glib::ControlFlow::Continue
    });
}

pub fn watch() {
    let monitor = gio::PowerProfileMonitor::get_default();
    SAVER.store(monitor.is_power_saver_enabled(), Ordering::Relaxed);
    monitor.connect_power_saver_enabled_notify(|monitor| {
        SAVER.store(monitor.is_power_saver_enabled(), Ordering::Relaxed);
        apply();
    });
    KEPT.with(|kept| kept.borrow_mut().push(monitor.upcast()));

    let upower = gio::DBusProxy::for_bus_sync(
        gio::BusType::System,
        gio::DBusProxyFlags::DO_NOT_AUTO_START,
        None,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower",
        "org.freedesktop.UPower",
        gio::Cancellable::NONE,
    );
    if let Ok(proxy) = upower {
        let read = |proxy: &gio::DBusProxy| {
            let on = proxy.cached_property("OnBattery").and_then(|value| value.get::<bool>()).unwrap_or(false);
            BATTERY.store(on, Ordering::Relaxed);
        };
        read(&proxy);
        proxy.connect_g_properties_changed(move |proxy, _, _| {
            read(proxy);

            glib::MainContext::default().invoke(apply);
        });
        KEPT.with(|kept| kept.borrow_mut().push(proxy.upcast()));
    }
    apply();
    sweep_idle_models();
}
