mod ui;

use adw::prelude::*;
use gtk::{gio, glib};

const APP_ID: &str = "com.tijmen.Numa";

fn main() -> glib::ExitCode {

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("numa=warn"),
    )
    .format_timestamp(None)
    .init();

    numa::io::adopt_former_name();

    #[cfg(debug_assertions)]
    glib::log_set_default_handler(|domain, level, message| {
        eprintln!("{}: {message}", domain.unwrap_or("glib"));
        if matches!(level, glib::LogLevel::Critical | glib::LogLevel::Error) {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| {
                eprintln!("{}", std::backtrace::Backtrace::force_capture());
            });
        }
    });

    let flags = match std::env::var_os("NUMA_OPEN") {
        Some(_) => gio::ApplicationFlags::NON_UNIQUE | gio::ApplicationFlags::HANDLES_OPEN,
        None => gio::ApplicationFlags::HANDLES_OPEN,
    };

    glib::set_prgname(Some(APP_ID));
    glib::set_application_name("Numa");
    let app = adw::Application::new(Some(APP_ID), flags);
    setup_actions(&app);

    app.connect_startup(|_| {

        unsafe {
            libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
        }
    });

    app.connect_activate(|app| {
        ui::window::build_window(app).present();
    });

    app.connect_open(|app, files, _| {
        let window = match app.active_window().and_downcast::<adw::ApplicationWindow>() {
            Some(window) => window,
            None => ui::window::build_window(app),
        };
        window.present();
        if let Some(path) = files.first().and_then(|file| file.path()) {
            gio::prelude::ActionGroupExt::activate_action(&window, "open-path", Some(&path.to_string_lossy().to_variant()));
        }
    });

    app.run()
}

fn setup_actions(app: &adw::Application) {

    let quit = gio::SimpleAction::new("quit", None);
    quit.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| {
            for window in app.windows() {
                window.close();
            }
        }
    ));
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<primary>q"]);

    let about = gio::SimpleAction::new("about", None);
    about.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| show_about(app.active_window().as_ref())
    ));
    app.add_action(&about);
}

fn show_about(parent: Option<&gtk::Window>) {
    ui::window::show_app_about(parent);
}
