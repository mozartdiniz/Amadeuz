// On Windows, suppress the console window for a GUI-only experience.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

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

/// On Windows, `windows_subsystem = "windows"` detaches stdout/stderr.
/// Re-attach to the parent console (if any) so that running from a terminal
/// still shows log output.
#[cfg(target_os = "windows")]
fn windows_hacks() {
    let _ = win32console::console::WinConsole::free_console();
    // ATTACH_PARENT_PROCESS = 0xFFFFFFFF
    let _ = win32console::console::WinConsole::attach_console(0xFFFFFFFF);
}

fn main() -> glib::ExitCode {
    #[cfg(target_os = "windows")]
    windows_hacks();

    pretty_env_logger::init();

    // Start tokio multi-thread runtime in the background.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    TOKIO_HANDLE
        .set(rt.handle().clone())
        .expect("tokio handle already set");

    // Load GResources.
    // Windows: embedded at compile time by build.rs via glib_build_tools.
    // Linux:   installed by Meson, loaded at runtime from the data directory.
    #[cfg(target_os = "windows")]
    gtk::gio::resources_register_include!("amadeuz-notes.gresource")
        .expect("Failed to register GResources");

    #[cfg(not(target_os = "windows"))]
    {
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
    }

    glib::set_application_name("Amadeuz Notes");

    // Run GTK (blocking).
    let exit = AmzApplication::run();

    // Keep tokio runtime alive until GTK exits.
    drop(rt);
    exit
}
