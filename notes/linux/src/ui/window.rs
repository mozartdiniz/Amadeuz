use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate, TemplateChild};

use crate::config;
use crate::manager::{ManagerRef, NotesManager};
use crate::model::{AmzFolder, AmzNote};
use crate::ui::{AmzAuthView, AmzNoteRow};

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/com/amadeuz/Notes/ui/window.ui")]
    pub struct AmzWindow {
        // Stack
        #[template_child] pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child] pub auth_view:  TemplateChild<AmzAuthView>,

        // Main layout
        #[template_child] pub outer_split: TemplateChild<adw::NavigationSplitView>,
        #[template_child] pub inner_split: TemplateChild<adw::NavigationSplitView>,

        // Folders
        #[template_child] pub all_notes_list:    TemplateChild<gtk::ListBox>,
        #[template_child] pub folder_list:       TemplateChild<gtk::ListBox>,
        #[template_child] pub new_folder_button: TemplateChild<gtk::Button>,

        // Notes list
        #[template_child] pub note_list:       TemplateChild<gtk::ListBox>,
        #[template_child] pub new_note_button: TemplateChild<gtk::Button>,
        #[template_child] pub search_button:   TemplateChild<gtk::ToggleButton>,
        #[template_child] pub search_bar:      TemplateChild<gtk::SearchBar>,
        #[template_child] pub search_entry:    TemplateChild<gtk::SearchEntry>,

        // Editor
        #[template_child] pub title_entry:         TemplateChild<gtk::Entry>,
        #[template_child] pub text_view:           TemplateChild<gtk::TextView>,
        #[template_child] pub delete_note_button:  TemplateChild<gtk::Button>,

        // Status bar
        #[template_child] pub status_icon:  TemplateChild<gtk::Image>,
        #[template_child] pub status_label: TemplateChild<gtk::Label>,

        // Manager
        pub manager: RefCell<Option<ManagerRef>>,
        pub loading_note: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AmzWindow {
        const NAME: &'static str = "AmzWindow";
        type Type = super::AmzWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            AmzAuthView::ensure_type();
            AmzNoteRow::ensure_type();
            klass.bind_template();

            klass.install_action("win.show-about", None, |win, _, _| win.show_about());
            klass.install_action("win.show-preferences", None, |win, _, _| win.show_preferences());
            klass.install_action("win.sign-out", None, |win, _, _| win.sign_out());
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
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl AmzWindow {
    pub fn new(app: &impl IsA<adw::Application>) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    // ── Manager binding ───────────────────────────────────────────────────────

    pub fn set_manager(&self, mgr: ManagerRef) {
        let imp = self.imp();

        {
            let win = self.clone();
            mgr.borrow_mut().on_auth_error = Some(Box::new(move |msg| win.on_auth_error(msg)));
        }
        {
            let win = self.clone();
            mgr.borrow_mut().on_recovery_code =
                Some(Box::new(move |code| win.show_recovery_code(code)));
        }
        {
            let win = self.clone();
            mgr.borrow_mut().on_state_changed = Some(Box::new(move || win.refresh_ui()));
        }

        // Bind folder store to folder_list.
        {
            let folder_store = mgr.borrow().folder_store.clone();
            let win_weak = self.downgrade();
            imp.folder_list.bind_model(Some(&folder_store), move |obj| {
                let folder = obj.downcast_ref::<AmzFolder>().unwrap();
                let row = build_folder_row(folder.name());
                if let Some(win) = win_weak.upgrade() {
                    let fid = folder.id();
                    row.connect_activate(glib::clone!(
                        #[weak]
                        win,
                        move |_| win.on_folder_selected(Some(fid.clone()))
                    ));
                }
                row.upcast()
            });
        }

        // Bind note store to note_list.
        {
            let note_store = mgr.borrow().note_store.clone();
            let win_weak = self.downgrade();
            imp.note_list.bind_model(Some(&note_store), move |obj| {
                let note = obj.downcast_ref::<AmzNote>().unwrap();
                let row = AmzNoteRow::new();
                row.bind_note(note);
                let list_row = gtk::ListBoxRow::new();
                list_row.set_child(Some(&row));
                if let Some(win) = win_weak.upgrade() {
                    let nid = note.id();
                    list_row.connect_activate(glib::clone!(
                        #[weak]
                        win,
                        move |_| win.on_note_selected(&nid)
                    ));
                }
                list_row.upcast()
            });
        }

        imp.manager.replace(Some(mgr));
        self.refresh_ui();
    }

    // ── State refresh ─────────────────────────────────────────────────────────

    fn refresh_ui(&self) {
        let imp = self.imp();
        let Some(mgr) = imp.manager.borrow().clone() else { return };
        let m = mgr.borrow();

        if m.is_authenticated {
            imp.main_stack.set_visible_child_name("main");
            if m.is_connected {
                imp.status_icon.set_icon_name(Some("network-transmit-receive-symbolic"));
                imp.status_label.set_text("Synced");
            } else {
                imp.status_icon.set_icon_name(Some("network-offline-symbolic"));
                imp.status_label.set_text("Offline");
            }
        } else {
            imp.main_stack.set_visible_child_name("auth");
        }
    }

    // ── Auth callbacks ────────────────────────────────────────────────────────

    fn on_auth_error(&self, msg: String) {
        let imp = self.imp();
        imp.auth_view.show_error(&msg);
        imp.auth_view.set_sensitive_all(true);
    }

    fn show_recovery_code(&self, code: String) {
        let label = gtk::Label::new(Some(&code));
        label.add_css_class("monospace");
        label.add_css_class("title-2");
        label.set_selectable(true);

        let dialog = adw::AlertDialog::new(
            Some("Save Your Recovery Code"),
            Some("This code lets you recover your account. Store it somewhere safe — it won't be shown again."),
        );
        dialog.set_extra_child(Some(&label));
        dialog.add_response("ok", "I've Saved It");
        dialog.set_default_response(Some("ok"));
        dialog.present(Some(self));
    }

    fn setup_auth_callbacks(&self) {
        let imp = self.imp();

        imp.auth_view.imp().login_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.do_login()
        ));
        imp.auth_view.imp().login_email.connect_entry_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.do_login()
        ));
        imp.auth_view.imp().login_password.connect_entry_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.do_login()
        ));
        imp.auth_view.imp().register_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.do_register()
        ));
        imp.auth_view.imp().recover_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.do_recover()
        ));
        imp.auth_view.imp().settings_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.show_preferences()
        ));
    }

    fn do_login(&self) {
        let imp = self.imp();
        let email = imp.auth_view.imp().login_email.text().to_string();
        let password = imp.auth_view.imp().login_password.text().to_string();
        if email.is_empty() || password.is_empty() {
            imp.auth_view.show_error("Please enter your email and password.");
            return;
        }
        imp.auth_view.clear_error();
        imp.auth_view.set_sensitive_all(false);
        if let Some(mgr) = imp.manager.borrow().clone() {
            mgr.borrow().login(email, password);
        }
    }

    fn do_register(&self) {
        let imp = self.imp();
        let email = imp.auth_view.imp().register_email.text().to_string();
        let password = imp.auth_view.imp().register_password.text().to_string();
        if email.is_empty() || password.is_empty() {
            imp.auth_view.show_error("Please enter an email and password.");
            return;
        }
        imp.auth_view.clear_error();
        imp.auth_view.set_sensitive_all(false);
        if let Some(mgr) = imp.manager.borrow().clone() {
            mgr.borrow().register(email, password);
        }
    }

    fn do_recover(&self) {
        let imp = self.imp();
        let email = imp.auth_view.imp().recover_email.text().to_string();
        let code = imp.auth_view.imp().recover_code.text().to_string();
        let password = imp.auth_view.imp().recover_password.text().to_string();
        if email.is_empty() || code.is_empty() || password.is_empty() {
            imp.auth_view.show_error("Please fill in all fields.");
            return;
        }
        imp.auth_view.clear_error();
        imp.auth_view.set_sensitive_all(false);
        if let Some(mgr) = imp.manager.borrow().clone() {
            mgr.borrow().recover(email, code, password);
        }
    }

    // ── Folder / note selection ───────────────────────────────────────────────

    fn on_folder_selected(&self, folder_id: Option<String>) {
        let imp = self.imp();
        if folder_id.is_some() {
            imp.all_notes_list.unselect_all();
        }
        if let Some(mgr) = imp.manager.borrow().clone() {
            let mut m = mgr.borrow_mut();
            m.selected_folder_id = folder_id;
            m.selected_note_id = None;
            m.disconnect_note_ws();
        }
        self.clear_editor();
    }

    fn on_all_notes_selected(&self) {
        let imp = self.imp();
        imp.folder_list.unselect_all();
        if let Some(mgr) = imp.manager.borrow().clone() {
            let mut m = mgr.borrow_mut();
            m.selected_folder_id = Some("__all__".into());
            m.selected_note_id = None;
            m.disconnect_note_ws();
        }
        self.clear_editor();
    }

    fn on_note_selected(&self, note_id: &str) {
        let imp = self.imp();
        let Some(mgr) = imp.manager.borrow().clone() else { return };

        // Cancel any pending debounce.
        if let Some(src) = mgr.borrow_mut().debounce_source.take() {
            src.remove();
        }

        let note = {
            let m = mgr.borrow();
            (0..m.note_store.n_items()).find_map(|i| {
                m.note_store
                    .item(i)
                    .and_downcast::<AmzNote>()
                    .filter(|n| n.id() == note_id)
            })
        };
        let Some(note) = note else { return };

        mgr.borrow_mut().selected_note_id = Some(note_id.to_owned());

        *imp.loading_note.borrow_mut() = true;
        imp.title_entry.set_text(&note.title());
        imp.text_view.buffer().set_text(&note.content());
        *imp.loading_note.borrow_mut() = false;

        mgr.borrow_mut().connect_note_ws(note_id);
        imp.inner_split.set_show_content(true);
    }

    fn on_editor_changed(&self) {
        let imp = self.imp();
        if *imp.loading_note.borrow() {
            return;
        }
        let Some(mgr) = imp.manager.borrow().clone() else { return };
        let note_id = mgr.borrow().selected_note_id.clone();
        let Some(note_id) = note_id else { return };

        let title = imp.title_entry.text().to_string();
        let buf = imp.text_view.buffer();
        let content = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        NotesManager::schedule_save(mgr, note_id, title, content);
    }

    fn clear_editor(&self) {
        let imp = self.imp();
        *imp.loading_note.borrow_mut() = true;
        imp.title_entry.set_text("");
        imp.text_view.buffer().set_text("");
        *imp.loading_note.borrow_mut() = false;
    }

    // ── Actions ───────────────────────────────────────────────────────────────

    fn prompt_new_folder(&self) {
        let entry = adw::EntryRow::new();
        entry.set_title("Folder name");
        let dialog = adw::AlertDialog::new(Some("New Folder"), None);
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("create", "Create");
        dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("create"));
        let win = self.clone();
        dialog.connect_response(Some("create"), move |d, _| {
            let name = d.extra_child()
                .and_downcast::<adw::EntryRow>()
                .map(|e| e.text().to_string())
                .unwrap_or_default();
            if !name.is_empty() {
                if let Some(mgr) = win.imp().manager.borrow().clone() {
                    mgr.borrow_mut().create_folder(name);
                }
            }
        });
        dialog.present(Some(self));
    }

    fn sign_out(&self) {
        let dialog = adw::AlertDialog::new(
            Some("Sign Out?"),
            Some("Your local notes will be cleared."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("signout", "Sign Out");
        dialog.set_response_appearance("signout", adw::ResponseAppearance::Destructive);
        let win = self.clone();
        dialog.connect_response(Some("signout"), move |_, _| {
            if let Some(mgr) = win.imp().manager.borrow().clone() {
                mgr.borrow_mut().logout();
            }
            win.clear_editor();
        });
        dialog.present(Some(self));
    }

    fn show_about(&self) {
        let dialog = adw::AboutDialog::builder()
            .application_name("Amadeuz Notes")
            .application_icon(*config::APP_ID)
            .version(*config::VERSION)
            .developer_name("The Amadeuz Project")
            .license_type(gtk::License::MitX11)
            .build();
        dialog.present(Some(self));
    }

    fn show_preferences(&self) {
        let imp = self.imp();
        let current_url = imp.manager.borrow()
            .as_ref()
            .map(|m| m.borrow().api.base_url.clone())
            .unwrap_or_else(|| "http://localhost:8080".into());

        let entry = adw::EntryRow::new();
        entry.set_title("Server URL");
        entry.set_text(&current_url);

        let dialog = adw::AlertDialog::new(Some("Server Settings"), None);
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("save", "Save");
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));

        let win = self.clone();
        dialog.connect_response(Some("save"), move |d, _| {
            let url = d.extra_child()
                .and_downcast::<adw::EntryRow>()
                .map(|e| e.text().to_string())
                .unwrap_or_default();
            let url = normalize_url(&url);
            if let Some(mgr) = win.imp().manager.borrow().clone() {
                mgr.borrow_mut().api.base_url = url.clone();
            }
            if gio::SettingsSchemaSource::default()
                .and_then(|src| src.lookup("com.amadeuz.Notes", true))
                .is_some()
            {
                let settings = gio::Settings::with_path("com.amadeuz.Notes", "/com/amadeuz/Notes/");
                settings.set_string("server-url", &url).ok();
            }
        });
        dialog.present(Some(self));
    }

    // ── setup_callbacks ───────────────────────────────────────────────────────

    fn setup_callbacks(&self) {
        let imp = self.imp();
        self.setup_auth_callbacks();

        imp.all_notes_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _| win.on_all_notes_selected()
        ));

        imp.new_note_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| {
                let mgr = win.imp().manager.borrow().clone();
                if let Some(mgr) = mgr {
                    let folder_id = mgr.borrow().selected_folder_id.clone()
                        .filter(|id| id != "__all__");
                    if let Some(note_id) = mgr.borrow_mut().create_note(folder_id) {
                        win.on_note_selected(&note_id);
                    }
                }
            }
        ));

        imp.delete_note_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| {
                let note_id = win.imp().manager.borrow().as_ref()
                    .and_then(|m| m.borrow().selected_note_id.clone());
                if let Some(id) = note_id {
                    if let Some(mgr) = win.imp().manager.borrow().clone() {
                        mgr.borrow_mut().delete_note(&id);
                    }
                    win.clear_editor();
                }
            }
        ));

        imp.new_folder_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.prompt_new_folder()
        ));

        imp.title_entry.connect_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.on_editor_changed()
        ));

        imp.text_view.buffer().connect_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.on_editor_changed()
        ));

        imp.search_button.connect_toggled(glib::clone!(
            #[weak]
            imp,
            move |btn| imp.search_bar.set_search_mode(btn.is_active())
        ));
        imp.search_entry.connect_search_changed(glib::clone!(
            #[weak]
            imp,
            move |entry| {
                let text = entry.text().to_lowercase();
                imp.note_list.set_filter_func(move |row| {
                    if text.is_empty() {
                        return true;
                    }
                    // Simple search through NoteRow's visible labels.
                    if let Some(child) = row.child().and_downcast::<AmzNoteRow>() {
                        let title = child.imp().title_label.text().to_lowercase();
                        let preview = child.imp().preview_label.text().to_lowercase();
                        return title.contains(&text) || preview.contains(&text);
                    }
                    true
                });
            }
        ));
    }
}

fn build_folder_row(name: String) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let label = gtk::Label::new(Some(&name));
    label.set_halign(gtk::Align::Start);
    label.set_margin_start(12);
    label.set_margin_end(12);
    label.set_margin_top(8);
    label.set_margin_bottom(8);
    row.set_child(Some(&label));
    row
}

fn normalize_url(url: &str) -> String {
    let url = url.trim();
    let url = if url.starts_with("ws://") {
        url.replace("ws://", "http://")
    } else if url.starts_with("wss://") {
        url.replace("wss://", "https://")
    } else {
        url.to_string()
    };
    url.trim_end_matches('/').to_string()
}
