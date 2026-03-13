mod app;
mod backend;
mod config;
mod manager;
mod model;
mod ui;

use app::AmzApplication;
use gtk::{gio, glib};

/// Global tokio runtime handle — set once at startup, read by manager + sync_worker.
pub static TOKIO_HANDLE: std::sync::OnceLock<tokio::runtime::Handle> =
    std::sync::OnceLock::new();

/// Convenience: spawn a future on the tokio runtime from any thread.
pub fn spawn<F>(fut: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    TOKIO_HANDLE
        .get()
        .expect("tokio runtime not initialised")
        .spawn(fut);
}

fn main() -> glib::ExitCode {
    pretty_env_logger::init();

    // Start tokio multi-thread runtime in the background.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    TOKIO_HANDLE
        .set(rt.handle().clone())
        .expect("tokio handle already set");

    // Load GResource bundle from installed data directory.
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
                 Hint: run `meson install -C build` first, or use `meson devenv`."
            );
            return glib::ExitCode::FAILURE;
        }
    }

    glib::set_application_name("Amadeuz Notes");

    // Run GTK (blocking).
    let exit = AmzApplication::run();

    // Keep tokio runtime alive until GTK exits.
    drop(rt);
    exit
}
