use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate, TemplateChild};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/com/amadeuz/Notes/ui/auth.ui")]
    pub struct AmzAuthView {
        // Login
        #[template_child] pub login_email:    TemplateChild<adw::EntryRow>,
        #[template_child] pub login_password: TemplateChild<adw::PasswordEntryRow>,
        #[template_child] pub login_button:   TemplateChild<gtk::Button>,

        // Register
        #[template_child] pub register_email:    TemplateChild<adw::EntryRow>,
        #[template_child] pub register_password: TemplateChild<adw::PasswordEntryRow>,
        #[template_child] pub register_button:   TemplateChild<gtk::Button>,

        // Recover
        #[template_child] pub recover_email:    TemplateChild<adw::EntryRow>,
        #[template_child] pub recover_code:     TemplateChild<adw::EntryRow>,
        #[template_child] pub recover_password: TemplateChild<adw::PasswordEntryRow>,
        #[template_child] pub recover_button:   TemplateChild<gtk::Button>,

        // Shared
        #[template_child] pub auth_stack:     TemplateChild<gtk::Stack>,
        #[template_child] pub error_label:    TemplateChild<gtk::Label>,
        #[template_child] pub settings_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AmzAuthView {
        const NAME: &'static str = "AmzAuthView";
        type Type = super::AmzAuthView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AmzAuthView {}
    impl WidgetImpl for AmzAuthView {}
    impl BinImpl for AmzAuthView {}
}

glib::wrapper! {
    pub struct AmzAuthView(ObjectSubclass<imp::AmzAuthView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AmzAuthView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn show_error(&self, msg: &str) {
        let imp = self.imp();
        imp.error_label.set_text(msg);
        imp.error_label.set_visible(true);
    }

    pub fn clear_error(&self) {
        let imp = self.imp();
        imp.error_label.set_visible(false);
        imp.error_label.set_text("");
    }

    pub fn set_sensitive_all(&self, sensitive: bool) {
        let imp = self.imp();
        imp.login_button.set_sensitive(sensitive);
        imp.register_button.set_sensitive(sensitive);
        imp.recover_button.set_sensitive(sensitive);
    }
}

impl Default for AmzAuthView {
    fn default() -> Self {
        Self::new()
    }
}
