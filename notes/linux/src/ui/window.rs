use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate, TemplateChild};

use crate::config;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/com/amadeuz/Notes/ui/window.ui")]
    pub struct AmzWindow {
        #[template_child]
        pub split_view: TemplateChild<adw::NavigationSplitView>,
        #[template_child]
        pub note_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub new_note_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub title_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub text_view: TemplateChild<gtk::TextView>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AmzWindow {
        const NAME: &'static str = "AmzWindow";
        type Type = super::AmzWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.install_action("win.show-about", None, |win, _, _| {
                win.show_about_dialog();
            });
            klass.install_action("win.show-preferences", None, |win, _, _| {
                win.show_preferences_dialog();
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AmzWindow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_callbacks();
        }
    }

    impl WidgetImpl for AmzWindow {}
    impl WindowImpl for AmzWindow {}
    impl ApplicationWindowImpl for AmzWindow {}
    impl AdwApplicationWindowImpl for AmzWindow {}
}

glib::wrapper! {
    pub struct AmzWindow(ObjectSubclass<imp::AmzWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap,
                   gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                   gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl AmzWindow {
    pub fn new<P: glib::prelude::IsA<adw::Application>>(app: &P) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    fn setup_callbacks(&self) {
        let imp = self.imp();

        imp.new_note_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| {
                // Placeholder: will create a new note
                log::debug!("New note button clicked");
            }
        ));
    }

    fn show_about_dialog(&self) {
        let dialog = adw::AboutDialog::builder()
            .application_name("Amadeuz Notes")
            .application_icon(*config::APP_ID)
            .version(*config::VERSION)
            .developer_name("The Amadeuz Project")
            .license_type(gtk::License::MitX11)
            .website("https://github.com/mozartdiniz/amadeuz")
            .build();
        dialog.present(Some(self));
    }

    fn show_preferences_dialog(&self) {
        // Placeholder: will show server URL settings
        log::debug!("Preferences requested");
    }
}
