use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::manager::NotesManager;
use crate::ui::AmzWindow;

mod imp {
    use super::*;
    use std::cell::OnceCell;

    #[derive(Default)]
    pub struct AmzApplication {
        pub window: OnceCell<AmzWindow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AmzApplication {
        const NAME: &'static str = "AmzApplication";
        type Type = super::AmzApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for AmzApplication {}

    impl ApplicationImpl for AmzApplication {
        fn activate(&self) {
            if let Some(win) = self.window.get() {
                win.present();
                return;
            }

            let app = self.obj();

            // Read server URL from GSettings (falls back to default if schema not installed yet).
            let server_url = gio::SettingsSchemaSource::default()
                .and_then(|src| src.lookup("com.amadeuz.Notes", true))
                .map(|_| {
                    let s = gio::Settings::with_path("com.amadeuz.Notes", "/com/amadeuz/Notes/");
                    s.string("server-url").to_string()
                })
                .filter(|url| !url.is_empty())
                .unwrap_or_else(|| "http://localhost:8080".into());

            // Load token from keyring synchronously via tokio block_in_place.
            let saved_token = crate::TOKIO_HANDLE
                .get()
                .expect("tokio not ready")
                .block_on(async { crate::backend::keyring::load_token().await.ok().flatten() });

            let is_authenticated = saved_token.is_some();

            // Create manager + event channel.
            let (mgr, rx) = NotesManager::new(server_url, saved_token);
            mgr.borrow_mut().is_authenticated = is_authenticated;

            // Attach event receiver to GLib main loop.
            glib::MainContext::default().spawn_local(glib::clone!(
                #[weak]
                mgr,
                async move {
                    while let Ok(event) = rx.recv().await {
                        NotesManager::handle_event(mgr.clone(), event);
                    }
                }
            ));

            // Create window and wire it to the manager.
            let win = AmzWindow::new(&*app);
            win.set_manager(mgr.clone());
            self.window.set(win.clone()).unwrap();
            win.present();

            // If already authenticated, kick off a full sync.
            if is_authenticated {
                NotesManager::full_sync(mgr);
            }
        }
    }

    impl GtkApplicationImpl for AmzApplication {}
    impl AdwApplicationImpl for AmzApplication {}
}

glib::wrapper! {
    pub struct AmzApplication(ObjectSubclass<imp::AmzApplication>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl AmzApplication {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", *crate::config::APP_ID)
            .property("flags", gio::ApplicationFlags::default())
            .build()
    }

    pub fn run() -> glib::ExitCode {
        let app = Self::new();
        app.run()
    }
}
