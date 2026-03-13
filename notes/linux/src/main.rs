mod app;
mod backend;
mod config;
mod model;
mod ui;

use app::AmzApplication;
use gtk::{gio, glib};

fn main() -> glib::ExitCode {
    pretty_env_logger::init();

    // Load the GResource bundle compiled by Meson from the install data directory.
    let res_path = format!(
        "{}/{}/{}.gresource",
        *config::DATADIR,
        *config::PKGNAME,
        *config::APP_ID,
    );
    match gio::Resource::load(&res_path) {
        Ok(res) => gio::resources_register(&res),
        Err(err) => {
            eprintln!(
                "Could not load GResource bundle at {res_path}: {err}\n\
                 Hint: install first with `meson install -C build` (prefix=~/.local),\n\
                 or run via `meson devenv -C build ./amadeuz-notes`."
            );
            return glib::ExitCode::FAILURE;
        }
    }

    glib::set_application_name("Amadeuz Notes");

    AmzApplication::run()
}
