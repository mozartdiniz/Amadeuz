use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::config;
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
            let app = self.obj();

            // Reuse existing window if already created.
            if let Some(win) = self.window.get() {
                win.present();
                return;
            }

            let win = AmzWindow::new(&*app);
            self.window.set(win.clone()).unwrap();
            win.present();
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
            .property("application-id", *config::APP_ID)
            .property("flags", gio::ApplicationFlags::default())
            .build()
    }

    pub fn run() -> glib::ExitCode {
        let app = Self::new();
        app.run()
    }
}
