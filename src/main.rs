mod crash;
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

    if let Err(err) = numa::io::presets::seed_looks(&numa::io::presets::dir()) {
        log::warn!("the looks could not be put in the presets folder: {err}");
    }

    #[cfg(debug_assertions)]
    glib::log_set_default_handler(|domain, level, message| {

        if matches!(level, glib::LogLevel::Debug | glib::LogLevel::Info) && std::env::var_os("G_MESSAGES_DEBUG").is_none() {
            return;
        }
        eprintln!("{}: {message}", domain.unwrap_or("glib"));
        if matches!(level, glib::LogLevel::Critical | glib::LogLevel::Error) {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| {
                eprintln!("{}", std::backtrace::Backtrace::force_capture());
            });
        }
    });

    let args: Vec<_> = std::env::args_os().collect();
    let self_test = args.get(1).is_some_and(|arg| arg == "--self-test");
    if self_test {
        if args.len() < 4 {
            eprintln!("usage: numa --self-test <folder for the JPEGs> <raw>…");
            return glib::ExitCode::from(2);
        }
        ui::window::request_self_test(args[2].clone().into(), args[3..].iter().map(Into::into).collect());
    }
    let beside = self_test || ["NUMA_OPEN", "NUMA_COMPARE", "NUMA_IMPORT"].iter().any(|name| std::env::var_os(name).is_some());
    let flags = match beside {
        true => gio::ApplicationFlags::NON_UNIQUE | gio::ApplicationFlags::HANDLES_OPEN,
        false => gio::ApplicationFlags::HANDLES_OPEN,
    };

    glib::set_prgname(Some(APP_ID));
    glib::set_application_name("Numa");
    let app = adw::Application::new(Some(APP_ID), flags);
    setup_actions(&app);

    app.connect_startup(|_| {

        unsafe {
            libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
        }

        crash::install();
    });
    app.connect_shutdown(|_| crash::clean_exit());

    app.connect_activate(|app| {
        ui::window::build_window(app).present();
    });

    app.connect_open(|app, files, _| {
        let window = match app.active_window().and_downcast::<adw::ApplicationWindow>() {
            Some(window) => window,
            None => ui::window::build_window(app),
        };
        window.present();
        let Some(file) = files.first() else { return };

        let uri = file.uri();
        if uri.starts_with("numa:") {
            gio::prelude::ActionGroupExt::activate_action(&window, "open-link", Some(&uri.to_variant()));
        } else if let Some(path) = file.path() {
            gio::prelude::ActionGroupExt::activate_action(&window, "open-path", Some(&path.to_string_lossy().to_variant()));
        }
    });

    if !self_test {
        return app.run();
    }
    let code = app.run_with_args(&args[..1].iter().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>());
    if ui::window::self_test_failed() { glib::ExitCode::FAILURE } else { code }
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
