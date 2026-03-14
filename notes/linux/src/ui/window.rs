use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib, CompositeTemplate, TemplateChild};

use crate::config;
use crate::manager::{ManagerRef, NotesManager, WASTEBASKET_ID};
use crate::model::{AmzFolder, AmzNote};
use crate::ui::{AmzAuthView, AmzNoteRow};
use crate::ui::md_formatter;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/com/amadeuz/Notes/ui/window.ui")]
    pub struct AmzWindow {
        // Stack
        #[template_child] pub main_stack: TemplateChild<gtk::Stack>,

        // Welcome page
        #[template_child] pub offline_row:         TemplateChild<adw::ActionRow>,
        #[template_child] pub connect_row:         TemplateChild<adw::ActionRow>,
        #[template_child] pub quit_welcome_button: TemplateChild<gtk::Button>,

        // Auth page
        #[template_child] pub auth_view:       TemplateChild<AmzAuthView>,
        #[template_child] pub auth_back_button: TemplateChild<gtk::Button>,

        // Main layout
        #[template_child] pub outer_split: TemplateChild<adw::NavigationSplitView>,
        #[template_child] pub inner_split: TemplateChild<adw::NavigationSplitView>,

        // Toolbar
        #[template_child] pub menu_button: TemplateChild<gtk::MenuButton>,

        // Folders
        #[template_child] pub all_notes_list:       TemplateChild<gtk::ListBox>,
        #[template_child] pub wastebasket_list:     TemplateChild<gtk::ListBox>,
        #[template_child] pub folder_list:          TemplateChild<gtk::ListBox>,
        #[template_child] pub new_folder_button:    TemplateChild<gtk::Button>,
        #[template_child] pub new_folder_revealer:  TemplateChild<gtk::Revealer>,
        #[template_child] pub new_folder_entry:     TemplateChild<gtk::Entry>,

        // Notes list
        #[template_child] pub note_list:       TemplateChild<gtk::ListBox>,
        #[template_child] pub new_note_button: TemplateChild<gtk::Button>,
        #[template_child] pub search_button:   TemplateChild<gtk::ToggleButton>,
        #[template_child] pub search_stack:    TemplateChild<gtk::Stack>,
        #[template_child] pub search_entry:    TemplateChild<gtk::SearchEntry>,

        // Editor
        #[template_child] pub text_view:           TemplateChild<gtk::TextView>,
        #[template_child] pub delete_note_button:  TemplateChild<gtk::Button>,

        // Status bar
        #[template_child] pub status_icon:  TemplateChild<gtk::Image>,
        #[template_child] pub status_label: TemplateChild<gtk::Label>,

        // Manager
        pub manager: RefCell<Option<ManagerRef>>,
        pub loading_note: RefCell<bool>,

        // Markdown formatting
        /// Prevents re-entrant buffer modifications during image embedding.
        pub is_formatting: RefCell<bool>,
        /// Guards against scheduling multiple idle embed_images callbacks at once.
        pub embed_pending: RefCell<bool>,
        /// Child anchors we inserted for inline image display.
        pub image_anchors: RefCell<Vec<gtk::TextChildAnchor>>,

        // Note list filter state — updated on folder/search change.
        // The CustomFilter reads these; call filter.changed() after mutating.
        pub note_filter_folder: RefCell<Option<String>>,
        pub note_filter_search: RefCell<String>,
        // The live CustomFilter — stored so folder/search handlers can call .changed().
        pub note_filter: RefCell<Option<gtk::CustomFilter>>,
        // The FilterListModel wrapping note_store — stored so row-activated can look up note by index.
        pub note_filter_model: RefCell<Option<gtk::FilterListModel>>,
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
            mgr.borrow_mut().on_auth_error = Some(Rc::new(move |msg| win.on_auth_error(msg)));
        }
        {
            let win = self.clone();
            mgr.borrow_mut().on_recovery_code =
                Some(Rc::new(move |code| win.show_recovery_code(code)));
        }
        {
            let win = self.clone();
            mgr.borrow_mut().on_state_changed = Some(Rc::new(move || win.refresh_ui()));
        }
        {
            let win = self.clone();
            mgr.borrow_mut().on_note_ws_update = Some(Rc::new(move |note_id, title, content| {
                win.apply_ws_update_to_editor(&note_id, title, content);
            }));
        }

        // Bind folder store to folder_list.
        {
            let folder_store = mgr.borrow().folder_store.clone();
            let win_weak = self.downgrade();
            imp.folder_list.bind_model(Some(&folder_store), move |obj| {
                let folder = obj.downcast_ref::<AmzFolder>().unwrap();

                // ── Label (view mode) ──────────────────────────────────────
                let label = gtk::Label::new(Some(&folder.name()));
                label.set_halign(gtk::Align::Start);
                label.set_hexpand(true);
                label.set_margin_start(12);
                label.set_margin_end(12);
                label.set_margin_top(8);
                label.set_margin_bottom(8);

                // ── Entry (edit mode) ──────────────────────────────────────
                let entry = gtk::Entry::new();
                entry.set_text(&folder.name());
                entry.set_hexpand(true);
                entry.set_margin_start(8);
                entry.set_margin_end(8);
                entry.set_margin_top(4);
                entry.set_margin_bottom(4);

                // Stack toggles between the two.
                let stack = gtk::Stack::new();
                stack.set_transition_type(gtk::StackTransitionType::None);
                stack.add_named(&label, Some("label"));
                stack.add_named(&entry, Some("entry"));

                let row = gtk::ListBoxRow::new();
                row.set_child(Some(&stack));

                // Keep label + entry in sync when name changes (e.g. remote sync).
                folder.connect_name_notify({
                    let label = label.clone();
                    let entry = entry.clone();
                    move |f| {
                        label.set_text(&f.name());
                        entry.set_text(&f.name());
                    }
                });

                // ── Double-click → switch to edit mode ─────────────────────
                let dbl_click = gtk::GestureClick::new();
                dbl_click.set_button(1);
                {
                    let stack = stack.clone();
                    let entry = entry.clone();
                    dbl_click.connect_pressed(move |_, n_press, _, _| {
                        if n_press == 2 {
                            stack.set_visible_child_name("entry");
                            entry.grab_focus();
                            entry.select_region(0, -1);
                        }
                    });
                }
                row.add_controller(dbl_click);

                // ── Enter key → save & switch back ─────────────────────────
                {
                    let stack = stack.clone();
                    let win_weak = win_weak.clone();
                    let folder = folder.clone();
                    entry.connect_activate(move |e| {
                        let name = e.text().to_string();
                        stack.set_visible_child_name("label");
                        if !name.is_empty() {
                            if let Some(win) = win_weak.upgrade() {
                                if let Some(mgr) = win.imp().manager.borrow().clone() {
                                    mgr.borrow_mut().rename_folder(&folder.id(), name);
                                }
                                win.refresh_notes_for_folder(&folder.id());
                            }
                        }
                    });
                }

                // ── Focus-leave → save if changed, switch back ──────────────
                let focus_ctrl = gtk::EventControllerFocus::new();
                {
                    let stack = stack.clone();
                    let entry = entry.clone();
                    let win_weak = win_weak.clone();
                    let folder = folder.clone();
                    focus_ctrl.connect_leave(move |_| {
                        let name = entry.text().to_string();
                        stack.set_visible_child_name("label");
                        if !name.is_empty() && name != folder.name() {
                            if let Some(win) = win_weak.upgrade() {
                                if let Some(mgr) = win.imp().manager.borrow().clone() {
                                    mgr.borrow_mut().rename_folder(&folder.id(), name);
                                }
                                win.refresh_notes_for_folder(&folder.id());
                            }
                        }
                    });
                }
                entry.add_controller(focus_ctrl);

                // ── Escape → cancel edit ────────────────────────────────────
                let key_ctrl = gtk::EventControllerKey::new();
                {
                    let stack = stack.clone();
                    let entry = entry.clone();
                    let folder = folder.clone();
                    key_ctrl.connect_key_pressed(move |_, key, _, _| {
                        if key == gdk::Key::Escape {
                            entry.set_text(&folder.name());
                            stack.set_visible_child_name("label");
                            glib::Propagation::Stop
                        } else {
                            glib::Propagation::Proceed
                        }
                    });
                }
                entry.add_controller(key_ctrl);

                // ── Right-click → delete folder ─────────────────────────────
                let right_click = gtk::GestureClick::new();
                right_click.set_button(3);
                {
                    let win_weak = win_weak.clone();
                    let folder = folder.clone();
                    right_click.connect_pressed(move |_, _, _, _| {
                        if let Some(win) = win_weak.upgrade() {
                            win.prompt_folder_delete(&folder.id(), &folder.name());
                        }
                    });
                }
                row.add_controller(right_click);

                row.upcast()
            });
        }

        // Bind note store to note_list via a FilterListModel.
        // GTK4 explicitly states that ListBox::set_filter_func is incompatible
        // with bind_model — filtering must be done at the model level instead.
        {
            let note_store = mgr.borrow().note_store.clone();
            let folder_store = mgr.borrow().folder_store.clone();
            let win_weak = self.downgrade();

            // CustomFilter reads folder + search state directly from the window imp.
            // It receives the AmzNote GObject, so no need to inspect row widgets.
            let filter = gtk::CustomFilter::new({
                let win_weak = win_weak.clone();
                move |obj| {
                    let Some(win) = win_weak.upgrade() else { return true };
                    let imp = win.imp();
                    let Some(note) = obj.downcast_ref::<AmzNote>() else { return true };

                    let folder_ok = match imp.note_filter_folder.borrow().as_deref() {
                        None | Some("__all__") => note.folder_id() != WASTEBASKET_ID,
                        Some(fid) => note.folder_id() == fid,
                    };
                    if !folder_ok { return false; }

                    let search = imp.note_filter_search.borrow().clone();
                    if search.is_empty() { return true; }
                    note.title().to_lowercase().contains(&search)
                        || note.content().to_lowercase().contains(&search)
                }
            });
            imp.note_filter.replace(Some(filter.clone()));

            let filter_model = gtk::FilterListModel::new(Some(note_store.clone()), Some(filter.clone()));
            imp.note_filter_model.replace(Some(filter_model.clone()));
            imp.note_list.bind_model(Some(&filter_model), move |obj| {
                let note = obj.downcast_ref::<AmzNote>().unwrap();
                let folder_name = folder_name_for(&folder_store, &note.folder_id());
                let row = AmzNoteRow::new();
                row.bind_note(note, &folder_name);

                // Keep the row in sync whenever the note's properties change
                // (title/content edited, note moved to another folder).
                let refresh = glib::clone!(
                    #[weak] row,
                    #[weak] folder_store,
                    move |n: &AmzNote| {
                        let name = folder_name_for(&folder_store, &n.folder_id());
                        row.bind_note(n, &name);
                    }
                );
                note.connect_title_notify(refresh.clone());
                note.connect_content_notify(refresh.clone());
                note.connect_updated_at_notify(refresh.clone());
                note.connect_folder_id_notify(refresh);

                // Right-click → "Move to Folder" context menu.
                let right_click = gtk::GestureClick::new();
                right_click.set_button(3);
                {
                    let note_id = note.id();
                    let win_weak = win_weak.clone();
                    right_click.connect_pressed(move |g, _, x, y| {
                        let Some(win) = win_weak.upgrade() else { return };
                        let Some(widget) = g.widget() else { return };
                        win.show_note_context_menu(note_id.clone(), x, y, &widget);
                    });
                }
                row.add_controller(right_click);

                row.upcast()
            });
        }

        imp.manager.replace(Some(mgr));
        self.refresh_ui();
    }

    // ── State refresh ─────────────────────────────────────────────────────────

    fn refresh_ui(&self) {
        let imp = self.imp();
        let Some(mgr) = imp.manager.borrow().clone() else { return };

        let (is_authenticated, is_connected, offline_mode, selected_note_id, selected_folder_id) = {
            let m = mgr.borrow();
            (m.is_authenticated, m.is_connected, m.offline_mode, m.selected_note_id.clone(), m.selected_folder_id.clone())
        };

        // Rebuild menu to reflect current mode.
        {
            let section1 = gio::Menu::new();
            if offline_mode {
                section1.append(Some("_Return to Start"), Some("win.sign-out"));
            } else {
                section1.append(Some("_Server Settings"), Some("win.show-preferences"));
                section1.append(Some("_Sign Out"), Some("win.sign-out"));
            }
            let section2 = gio::Menu::new();
            section2.append(Some("_About Amadeuz Notes"), Some("win.show-about"));
            let menu = gio::Menu::new();
            menu.append_section(None, &section1);
            menu.append_section(None, &section2);
            imp.menu_button.set_menu_model(Some(&menu));
        }

        if is_authenticated || offline_mode {
            imp.main_stack.set_visible_child_name("main");
            if offline_mode {
                imp.status_icon.set_icon_name(Some("computer-symbolic"));
                imp.status_label.set_text("Offline Mode");
            } else if is_connected {
                imp.status_icon.set_icon_name(Some("network-transmit-receive-symbolic"));
                imp.status_label.set_text("Synced");
            } else {
                imp.status_icon.set_icon_name(Some("network-offline-symbolic"));
                imp.status_label.set_text("Offline");
            }

            // Auto-select All Notes + first note only on initial load
            // (before the user has selected any folder).
            if selected_folder_id.is_none() {
                // Highlight the All Notes row in the sidebar.
                if let Some(row) = imp.all_notes_list.row_at_index(0) {
                    imp.all_notes_list.select_row(Some(&row));
                }
                imp.folder_list.unselect_all();
                imp.wastebasket_list.unselect_all();
                *imp.note_filter_folder.borrow_mut() = Some("__all__".into());
                if let Some(f) = imp.note_filter.borrow().as_ref() {
                    f.changed(gtk::FilterChange::Different);
                }
                if let Some(mgr) = imp.manager.borrow().clone() {
                    mgr.borrow_mut().selected_folder_id = Some("__all__".into());
                }

                // Select the first note in the list, if any.
                let first_note_id = imp.note_filter_model.borrow()
                    .as_ref()
                    .and_then(|m| m.item(0))
                    .and_downcast::<AmzNote>()
                    .map(|n| n.id());
                if let Some(id) = first_note_id {
                    if let Some(row) = imp.note_list.row_at_index(0) {
                        imp.note_list.select_row(Some(&row));
                    }
                    self.on_note_selected(&id);
                }
            }
        } else {
            // After sign-out or on first launch without a token, go to the
            // welcome page — not directly to auth.  Only stay on auth if the
            // user already navigated there (i.e. the stack was set to "auth"
            // by on_connect_to_server and we haven't left yet).
            let current = imp.main_stack.visible_child_name();
            if current.as_deref() != Some("auth") {
                imp.main_stack.set_visible_child_name("welcome");
            }
        }
    }

    // ── Welcome page callbacks ────────────────────────────────────────────────

    fn use_offline(&self) {
        let imp = self.imp();
        if let Some(mgr) = imp.manager.borrow().clone() {
            let mut m = mgr.borrow_mut();
            m.offline_mode = true;
            m.load_local();
        }
        // Persist the choice so the welcome screen is skipped next launch.
        let mut data = crate::backend::local_store::load();
        data.offline_mode = true;
        crate::backend::local_store::save(&data).ok();

        self.refresh_ui();
    }

    fn on_connect_to_server(&self) {
        let imp = self.imp();
        imp.auth_view.imp().auth_stack.set_visible_child_name("login");
        imp.auth_view.clear_error();
        imp.main_stack.set_visible_child_name("auth");
    }

    fn back_to_welcome(&self) {
        self.imp().auth_view.clear_error();
        self.imp().main_stack.set_visible_child_name("welcome");
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
        imp.wastebasket_list.unselect_all();
        imp.new_note_button.set_sensitive(true);
        *imp.note_filter_folder.borrow_mut() = folder_id.clone();
        if let Some(f) = imp.note_filter.borrow().as_ref() {
            f.changed(gtk::FilterChange::Different);
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
        imp.wastebasket_list.unselect_all();
        imp.new_note_button.set_sensitive(true);
        *imp.note_filter_folder.borrow_mut() = Some("__all__".into());
        if let Some(f) = imp.note_filter.borrow().as_ref() {
            f.changed(gtk::FilterChange::Different);
        }
        if let Some(mgr) = imp.manager.borrow().clone() {
            let mut m = mgr.borrow_mut();
            m.selected_folder_id = Some("__all__".into());
            m.selected_note_id = None;
            m.disconnect_note_ws();
        }
        self.clear_editor();
    }

    fn on_wastebasket_selected(&self) {
        let imp = self.imp();
        imp.all_notes_list.unselect_all();
        imp.folder_list.unselect_all();
        imp.new_note_button.set_sensitive(false);
        *imp.note_filter_folder.borrow_mut() = Some(WASTEBASKET_ID.to_string());
        if let Some(f) = imp.note_filter.borrow().as_ref() {
            f.changed(gtk::FilterChange::Different);
        }
        if let Some(mgr) = imp.manager.borrow().clone() {
            let mut m = mgr.borrow_mut();
            m.selected_folder_id = Some(WASTEBASKET_ID.to_string());
            m.selected_note_id = None;
            m.disconnect_note_ws();
        }
        self.clear_editor();
    }

    fn on_note_selected(&self, note_id: &str) {
        let imp = self.imp();
        let Some(mgr) = imp.manager.borrow().clone() else { return };

        // Flush any pending save for the note we're leaving, then cancel the timer.
        // If we just cancelled without flushing, edits made within the debounce
        // window would be silently discarded.
        let prev_id = {
            let mut m = mgr.borrow_mut();
            let prev = m.selected_note_id.clone();
            if let Some(src) = m.debounce_source.take() {
                src.remove();
            }
            prev
        };
        if let Some(prev_id) = prev_id {
            let buf = imp.text_view.buffer();
            let full = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
            let (title, content) = split_title_content(&full);
            md_formatter::cleanup_orphaned_images(&prev_id, &content);
            mgr.borrow_mut().flush_note(&prev_id, title, content);
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

        let in_trash = note.folder_id() == WASTEBASKET_ID;
        imp.new_note_button.set_sensitive(!in_trash);
        imp.delete_note_button.set_tooltip_text(Some(if in_trash {
            "Delete Permanently"
        } else {
            "Move to Trash"
        }));

        // Combine title + content into one text blob (first line = title).
        let combined = {
            let t = note.title();
            let c = note.content();
            if t.is_empty() {
                c
            } else if c.is_empty() {
                t
            } else {
                format!("{}\n{}", t, c)
            }
        };

        *imp.loading_note.borrow_mut() = true;
        imp.text_view.buffer().set_text(&combined);
        *imp.loading_note.borrow_mut() = false;

        // Download any blobs referenced in this note that aren't in the local cache.
        self.prefetch_blobs(&combined);

        mgr.borrow_mut().connect_note_ws(note_id);
        imp.inner_split.set_show_content(true);
        imp.text_view.grab_focus();
    }

    fn on_editor_changed(&self) {
        let imp = self.imp();
        if *imp.loading_note.borrow() {
            return;
        }
        let Some(mgr) = imp.manager.borrow().clone() else { return };
        let note_id = mgr.borrow().selected_note_id.clone();
        let Some(note_id) = note_id else { return };

        let buf = imp.text_view.buffer();
        let full = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        let (title, content) = split_title_content(&full);
        NotesManager::schedule_save(mgr, note_id, title, content);
    }

    fn clear_editor(&self) {
        let imp = self.imp();
        *imp.loading_note.borrow_mut() = true;
        imp.text_view.buffer().set_text("");
        *imp.loading_note.borrow_mut() = false;
    }

    /// Called when a WS update arrives for a note. If the note is currently
    /// open in the editor, refreshes the buffer without triggering a save loop.
    fn apply_ws_update_to_editor(&self, note_id: &str, title: String, content: String) {
        let imp = self.imp();
        let Some(mgr) = imp.manager.borrow().clone() else { return };
        let selected = mgr.borrow().selected_note_id.clone();
        if selected.as_deref() != Some(note_id) {
            return;
        }
        let combined = if title.is_empty() {
            content
        } else if content.is_empty() {
            title
        } else {
            format!("{}\n{}", title, content)
        };
        let buf = imp.text_view.buffer();
        let current = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        if current != combined {
            *imp.loading_note.borrow_mut() = true;
            buf.set_text(&combined);
            *imp.loading_note.borrow_mut() = false;
            self.prefetch_blobs(&combined);
        }
    }

    // ── Actions ───────────────────────────────────────────────────────────────

    /// Emit notify::folder-id on every note in `folder_id` so their row
    /// refresh closures re-look up the (just-changed) folder name.
    fn refresh_notes_for_folder(&self, folder_id: &str) {
        let Some(mgr) = self.imp().manager.borrow().clone() else { return };
        let note_store = mgr.borrow().note_store.clone();
        for i in 0..note_store.n_items() {
            if let Some(note) = note_store.item(i).and_downcast::<AmzNote>() {
                if note.folder_id() == folder_id {
                    note.notify("folder-id");
                }
            }
        }
    }

    fn prompt_folder_delete(&self, folder_id: &str, folder_name: &str) {
        let dialog = adw::AlertDialog::new(
            Some(&format!("Delete \"{}\"?", folder_name)),
            Some("All notes in this folder will also be deleted."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));

        let win = self.clone();
        let fid = folder_id.to_owned();
        dialog.connect_response(Some("delete"), move |_, _| {
            if let Some(mgr) = win.imp().manager.borrow().clone() {
                mgr.borrow_mut().delete_folder(&fid);
            }
            win.on_folder_selected(None);
        });
        dialog.present(Some(self));
    }

    fn show_note_context_menu(&self, note_id: String, x: f64, y: f64, widget: &gtk::Widget) {
        let imp = self.imp();
        let mgr_opt = imp.manager.borrow().clone();
        let Some(mgr) = mgr_opt else { return };

        // Determine if this note is in the wastebasket.
        let in_trash = {
            let m = mgr.borrow();
            (0..m.note_store.n_items()).any(|i| {
                m.note_store.item(i).and_downcast::<AmzNote>()
                    .map(|n| n.id() == note_id && n.folder_id() == WASTEBASKET_ID)
                    .unwrap_or(false)
            })
        };

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("navigation-sidebar");

        let make_row = |label: &str, name: &str| {
            let lbl = gtk::Label::new(Some(label));
            lbl.set_halign(gtk::Align::Start);
            lbl.set_margin_start(12);
            lbl.set_margin_end(12);
            lbl.set_margin_top(6);
            lbl.set_margin_bottom(6);
            let row = gtk::ListBoxRow::new();
            row.set_widget_name(name);
            row.set_child(Some(&lbl));
            row
        };

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let popover = gtk::Popover::new();
        popover.set_parent(widget);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.set_has_arrow(false);
        popover.connect_closed(|p| p.unparent());

        let win_weak = self.downgrade();
        let popover_weak = popover.downgrade();

        if in_trash {
            // Wastebasket context: Restore + Delete Permanently
            list.append(&make_row("Restore", "__restore__"));
            list.append(&make_row("Delete Permanently", "__delete_perm__"));

            list.connect_row_activated(move |_, row| {
                let action = row.widget_name().to_string();
                if let Some(win) = win_weak.upgrade() {
                    let imp = win.imp();
                    match action.as_str() {
                        "__restore__" => {
                            if let Some(mgr) = imp.manager.borrow().clone() {
                                mgr.borrow_mut().restore_note(&note_id);
                            }
                            if let Some(f) = imp.note_filter.borrow().as_ref() {
                                f.changed(gtk::FilterChange::Different);
                            }
                            win.clear_editor();
                        }
                        "__delete_perm__" => {
                            win.prompt_permanent_delete(&note_id);
                        }
                        _ => {}
                    }
                }
                if let Some(p) = popover_weak.upgrade() {
                    p.popdown();
                }
            });
        } else {
            // Normal context: Move to Folder
            let folder_store = mgr.borrow().folder_store.clone();

            let header = gtk::Label::new(Some("Move to Folder"));
            header.add_css_class("heading");
            header.set_margin_top(8);
            header.set_margin_bottom(4);
            header.set_margin_start(12);
            header.set_margin_end(12);
            let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
            vbox.append(&header);
            vbox.append(&sep);

            // Index 0 = "No Folder"
            list.append(&make_row("No Folder", "__no_folder__"));
            for i in 0..folder_store.n_items() {
                if let Some(folder) = folder_store.item(i).and_downcast::<AmzFolder>() {
                    list.append(&make_row(&folder.name(), &folder.id()));
                }
            }

            list.connect_row_activated(move |_, row| {
                let name = row.widget_name().to_string();
                let folder_id = if name == "__no_folder__" || name.is_empty() {
                    None
                } else {
                    Some(name)
                };
                if let Some(win) = win_weak.upgrade() {
                    if let Some(mgr) = win.imp().manager.borrow().clone() {
                        mgr.borrow_mut().move_note(&note_id, folder_id);
                    }
                    let filter = win.imp().note_filter.borrow().clone();
                    if let Some(f) = filter.as_ref() {
                        f.changed(gtk::FilterChange::Different);
                    }
                }
                if let Some(p) = popover_weak.upgrade() {
                    p.popdown();
                }
            });
        }

        vbox.append(&list);
        popover.set_child(Some(&vbox));
        popover.popup();
    }

    fn prompt_permanent_delete(&self, note_id: &str) {
        let dialog = adw::AlertDialog::new(
            Some("Delete Permanently?"),
            Some("This note will be deleted forever and cannot be recovered."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete Permanently");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let win = self.clone();
        let id = note_id.to_owned();
        dialog.connect_response(Some("delete"), move |_, _| {
            let imp = win.imp();
            md_formatter::delete_note_images(&id);
            if let Some(mgr) = imp.manager.borrow().clone() {
                mgr.borrow_mut().delete_note(&id);
            }
            if let Some(f) = imp.note_filter.borrow().as_ref() {
                f.changed(gtk::FilterChange::Different);
            }
            win.clear_editor();
        });
        dialog.present(Some(self));
    }

    fn sign_out(&self) {
        let offline_mode = self.imp().manager.borrow()
            .as_ref()
            .map(|m| m.borrow().offline_mode)
            .unwrap_or(false);

        if offline_mode {
            // Offline mode: just clear the flag and return to the welcome screen.
            // No data loss, so no confirmation dialog needed.
            if let Some(mgr) = self.imp().manager.borrow().clone() {
                let mut m = mgr.borrow_mut();
                m.offline_mode = false;
                m.folder_store.remove_all();
                m.note_store.remove_all();
            }
            let mut data = crate::backend::local_store::load();
            data.offline_mode = false;
            crate::backend::local_store::save(&data).ok();
            self.clear_editor();
            self.refresh_ui();
            return;
        }

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
                // borrow_mut released here; now safe to call refresh_ui
            }
            win.clear_editor();
            win.refresh_ui();
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

    // ── Image paste ──────────────────────────────────────────────────────────

    fn handle_image_paste(&self) -> bool {
        use gtk::prelude::WidgetExt;
        let clipboard = WidgetExt::display(self).clipboard();
        let formats = clipboard.formats();

        let Some(_note_id) = self.imp().manager.borrow()
            .as_ref()
            .and_then(|m| m.borrow().selected_note_id.clone())
        else { return false };

        // ── Case 1: file(s) copied from file manager ──────────────────────
        if formats.contains_type(gdk::FileList::static_type()) {
            let win_weak = self.downgrade();
            clipboard.read_value_async(
                gdk::FileList::static_type(),
                glib::Priority::DEFAULT,
                gio::Cancellable::NONE,
                move |result: Result<glib::Value, glib::Error>| {
                    let Ok(value) = result else { return };
                    let Ok(file_list) = value.get::<gdk::FileList>() else { return };
                    let Some(win) = win_weak.upgrade() else { return };
                    let buf = win.imp().text_view.buffer();
                    for file in file_list.files() {
                        let Some(path) = file.path() else { continue };
                        if !md_formatter::is_image_file(&path) { continue }
                        let Ok(data) = std::fs::read(&path) else { continue };
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
                        let alt = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
                        let Some(blob_id) = win.save_blob_and_upload(data, ext) else { continue };
                        buf.insert_at_cursor(&format!("\n![{}](amadeuz://blob/{})\n", alt, blob_id));
                    }
                },
            );
            return true;
        }

        // ── Case 2: raw image data (screenshot, copy from browser, etc.) ──
        if formats.contains_type(gdk::Texture::static_type()) {
            let win_weak = self.downgrade();
            clipboard.read_texture_async(
                gio::Cancellable::NONE,
                move |result: Result<Option<gdk::Texture>, glib::Error>| {
                    let Ok(Some(texture)) = result else { return };
                    let Some(win) = win_weak.upgrade() else { return };
                    // Save texture to a temp file so we can read raw PNG bytes.
                    let tmp = std::env::temp_dir().join(format!("amz_paste_{}.png",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos()).unwrap_or(0)));
                    if texture.save_to_png(&tmp).is_err() { return }
                    let Ok(data) = std::fs::read(&tmp) else { return };
                    let _ = std::fs::remove_file(&tmp);
                    let Some(blob_id) = win.save_blob_and_upload(data, "png") else { return };
                    win.imp().text_view.buffer()
                        .insert_at_cursor(&format!("\n![image](amadeuz://blob/{})\n", blob_id));
                },
            );
            return true;
        }

        false
    }

    /// For each `amadeuz://blob/{id}` in `content` that isn't cached locally,
    /// download it from the server and save to the blob cache, then trigger
    /// a re-embed so the image appears without any user action.
    ///
    /// Retries up to 5 times with increasing delays to handle the race where
    /// the note content arrives via WebSocket before the sender's blob upload
    /// has finished.
    fn prefetch_blobs(&self, content: &str) {
        let ids = md_formatter::blob_ids_in_content(content);
        let missing: Vec<String> = ids.into_iter()
            .filter(|id| !md_formatter::blob_cache_path(id).exists())
            .collect();
        if missing.is_empty() { return; }

        let api = match self.imp().manager.borrow().as_ref().map(|m| m.borrow().api.clone()) {
            Some(api) if api.token.is_some() => api,
            _ => return,
        };
        // Fire-and-forget: download missing blobs on the tokio runtime.
        // No channel back to GTK — the GTK side polls independently (see below).
        let ids_for_download = missing.clone();
        crate::spawn(async move {
            // Retry delays in seconds: 1, 2, 4, 8, 16
            const DELAYS: &[u64] = &[1, 2, 4, 8, 16];
            let mut still_missing = ids_for_download;
            for &delay in DELAYS {
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                let mut failed = Vec::new();
                for id in still_missing {
                    match api.download_blob(&id).await {
                        Ok(data) => {
                            let path = md_formatter::blob_cache_path(&id);
                            if let Some(parent) = path.parent() {
                                std::fs::create_dir_all(parent).ok();
                            }
                            std::fs::write(&path, &data).ok();
                        }
                        Err(_) => failed.push(id),
                    }
                }
                still_missing = failed;
                if still_missing.is_empty() { break; }
            }
        });

        // Poll the blob cache every 500 ms on the GTK main thread.
        // When all blobs are present, trigger a re-embed. Give up after 30 s.
        let win_weak = self.downgrade();
        let mut ticks = 0u32;
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            ticks += 1;
            let all_cached = missing.iter().all(|id| md_formatter::blob_cache_path(id).exists());
            if all_cached {
                if let Some(win) = win_weak.upgrade() {
                    win.schedule_image_embed();
                }
                glib::ControlFlow::Break
            } else if ticks >= 60 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Save image bytes to the local blob cache and kick off an async upload
    /// to the server. Returns the blob ID on success.
    fn save_blob_and_upload(&self, data: Vec<u8>, _ext: &str) -> Option<String> {
        let blob_id = uuid::Uuid::new_v4().to_string();
        let cache_path = md_formatter::blob_cache_path(&blob_id);
        std::fs::create_dir_all(cache_path.parent()?).ok()?;
        std::fs::write(&cache_path, &data).ok()?;

        // Async upload — fire-and-forget, same pattern as note saves.
        let api = self.imp().manager.borrow()
            .as_ref()
            .map(|m| m.borrow().api.clone());
        if let Some(api) = api {
            let id = blob_id.clone();
            crate::spawn(async move {
                api.upload_blob(&id, data).await.ok();
            });
        }

        Some(blob_id)
    }

    // ── Image embedding ───────────────────────────────────────────────────────

    /// Schedule one `embed_images` call for the next GLib iteration.
    /// Uses `embed_pending` so rapid typing doesn't pile up callbacks.
    fn schedule_image_embed(&self) {
        let imp = self.imp();
        if *imp.embed_pending.borrow() {
            return;
        }
        *imp.embed_pending.borrow_mut() = true;

        let win_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(win) = win_weak.upgrade() {
                *win.imp().embed_pending.borrow_mut() = false;
                win.do_embed_images();
            }
        });
    }

    fn do_embed_images(&self) {
        let imp = self.imp();
        if *imp.is_formatting.borrow() {
            return;
        }
        let note_id = imp.manager.borrow()
            .as_ref()
            .and_then(|m| m.borrow().selected_note_id.clone())
            .unwrap_or_default();

        *imp.is_formatting.borrow_mut() = true;

        let buffer = imp.text_view.buffer();
        let view = imp.text_view.clone();
        md_formatter::embed_images(
            view.upcast_ref::<gtk::TextView>(),
            &buffer,
            &mut imp.image_anchors.borrow_mut(),
            &note_id,
        );

        *imp.is_formatting.borrow_mut() = false;
    }

    // ── setup_callbacks ───────────────────────────────────────────────────────

    fn setup_callbacks(&self) {
        let imp = self.imp();
        self.setup_auth_callbacks();

        // ── Welcome page ──────────────────────────────────────────────────
        imp.offline_row.connect_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.use_offline()
        ));
        imp.connect_row.connect_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.on_connect_to_server()
        ));
        imp.quit_welcome_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.close()
        ));
        imp.auth_back_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.back_to_welcome()
        ));

        imp.all_notes_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _| win.on_all_notes_selected()
        ));

        imp.wastebasket_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _| win.on_wastebasket_selected()
        ));

        // Folder list: look up the folder by row index in the folder_store.
        // Skip activation when the row is in inline-edit mode (entry is visible).
        imp.folder_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, row| {
                // If this row's stack is showing the entry, the user is editing — ignore.
                if let Some(stack) = row.child().and_downcast::<gtk::Stack>() {
                    if stack.visible_child_name().as_deref() == Some("entry") {
                        return;
                    }
                }
                let idx = row.index();
                if idx < 0 { return; }
                let imp = win.imp();
                let Some(mgr) = imp.manager.borrow().clone() else { return };
                let folder_store = mgr.borrow().folder_store.clone();
                let Some(obj) = folder_store.item(idx as u32) else { return };
                let Some(folder) = obj.downcast_ref::<AmzFolder>() else { return };
                win.on_folder_selected(Some(folder.id()));
            }
        ));

        // Note list: look up the note by row index in the filter model.
        imp.note_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, row| {
                let idx = row.index();
                if idx < 0 { return; }
                let imp = win.imp();
                let model_ref = imp.note_filter_model.borrow();
                let Some(model) = model_ref.as_ref() else { return };
                let Some(obj) = model.item(idx as u32) else { return };
                let Some(note) = obj.downcast_ref::<AmzNote>() else { return };
                let note_id = note.id();
                drop(model_ref);
                win.on_note_selected(&note_id);
            }
        ));

        imp.new_note_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| {
                let mgr = win.imp().manager.borrow().clone();
                if let Some(mgr) = mgr {
                    let folder_id = mgr.borrow().selected_folder_id.clone()
                        .filter(|id| id != "__all__");
                    let note_id = mgr.borrow_mut().create_note(folder_id);
                    if let Some(note_id) = note_id {
                        win.on_note_selected(&note_id);
                        // Select the row on the next GTK iteration once the filter model
                        // has had a chance to insert the new row.
                        let win2 = win.downgrade();
                        glib::idle_add_local_once(move || {
                            if let Some(w) = win2.upgrade() {
                                if let Some(row) = w.imp().note_list.row_at_index(0) {
                                    w.imp().note_list.select_row(Some(&row));
                                }
                            }
                        });
                    }
                }
            }
        ));

        imp.delete_note_button.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| {
                let imp = win.imp();
                let note_id = imp.manager.borrow().as_ref()
                    .and_then(|m| m.borrow().selected_note_id.clone());
                let Some(id) = note_id else { return };

                let in_trash = imp.manager.borrow().as_ref()
                    .and_then(|m| {
                        let m = m.borrow();
                        (0..m.note_store.n_items()).find_map(|i| {
                            m.note_store.item(i).and_downcast::<AmzNote>()
                                .filter(|n| n.id() == id)
                                .map(|n| n.folder_id() == WASTEBASKET_ID)
                        })
                    })
                    .unwrap_or(false);

                if in_trash {
                    win.prompt_permanent_delete(&id);
                } else {
                    if let Some(mgr) = imp.manager.borrow().clone() {
                        mgr.borrow_mut().trash_note(&id);
                    }
                    if let Some(f) = imp.note_filter.borrow().as_ref() {
                        f.changed(gtk::FilterChange::Different);
                    }
                    win.clear_editor();
                }
            }
        ));

        imp.new_folder_button.connect_clicked(glib::clone!(
            #[weak]
            imp,
            move |_| {
                imp.new_folder_entry.set_text("");
                imp.new_folder_revealer.set_reveal_child(true);
                imp.new_folder_entry.grab_focus();
            }
        ));

        // Enter → create folder and hide revealer.
        imp.new_folder_entry.connect_activate(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |entry| {
                let name = entry.text().to_string();
                let imp = win.imp();
                imp.new_folder_revealer.set_reveal_child(false);
                entry.set_text("");
                if !name.is_empty() {
                    if let Some(mgr) = imp.manager.borrow().clone() {
                        mgr.borrow_mut().create_folder(name);
                    }
                }
            }
        ));

        // Escape → cancel without creating.
        {
            let key_ctrl = gtk::EventControllerKey::new();
            key_ctrl.connect_key_pressed(glib::clone!(
                #[weak]
                imp,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, key, _, _| {
                    if key == gdk::Key::Escape {
                        imp.new_folder_revealer.set_reveal_child(false);
                        imp.new_folder_entry.set_text("");
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                }
            ));
            imp.new_folder_entry.add_controller(key_ctrl);
        }

        // Set up markdown text-tags once, on the initial buffer.
        md_formatter::setup_tags(&imp.text_view.buffer());

        // Key controller for smart list continuation on Enter.
        {
            let key_ctrl = gtk::EventControllerKey::new();
            let buf = imp.text_view.buffer();
            key_ctrl.connect_key_pressed(move |_, key, _, _| {
                if key == gdk::Key::Return || key == gdk::Key::KP_Enter {
                    if md_formatter::handle_enter_key(&buf) {
                        return glib::Propagation::Stop;
                    }
                }
                glib::Propagation::Proceed
            });
            imp.text_view.add_controller(key_ctrl);
        }

        // ── Image drag-and-drop ───────────────────────────────────────────
        {
            let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
            drop.connect_drop(glib::clone!(
                #[weak(rename_to = win)]
                self,
                #[upgrade_or]
                false,
                move |_, value, _, _| {
                    let Ok(file_list) = value.get::<gdk::FileList>() else { return false };
                    let note_id = win.imp().manager.borrow()
                        .as_ref()
                        .and_then(|m| m.borrow().selected_note_id.clone());
                    let Some(note_id) = note_id else { return false };
                    let buf = win.imp().text_view.buffer();
                    let mut handled = false;
                    for file in file_list.files() {
                        let Some(path) = file.path() else { continue };
                        if !md_formatter::is_image_file(&path) { continue }
                        let Ok(data) = std::fs::read(&path) else { continue };
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
                        let alt = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
                        let Some(blob_id) = win.save_blob_and_upload(data, ext) else { continue };
                        buf.insert_at_cursor(&format!("\n![{}](amadeuz://blob/{})\n", alt, blob_id));
                        handled = true;
                    }
                    handled
                }
            ));
            imp.text_view.add_controller(drop);
        }

        // ── Image paste (Ctrl+V with image files in clipboard) ────────────
        {
            let paste_ctrl = gtk::EventControllerKey::new();
            paste_ctrl.connect_key_pressed(glib::clone!(
                #[weak(rename_to = win)]
                self,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, key, _, mods| {
                    if key == gdk::Key::v && mods.contains(gdk::ModifierType::CONTROL_MASK) {
                        if win.handle_image_paste() {
                            return glib::Propagation::Stop;
                        }
                    }
                    glib::Propagation::Proceed
                }
            ));
            imp.text_view.add_controller(paste_ctrl);
        }

        imp.text_view.buffer().connect_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |buf| {
                // Guard: skip everything during our own buffer modifications
                if *win.imp().is_formatting.borrow() {
                    return;
                }
                // Debounced save
                win.on_editor_changed();
                // Live markdown tag formatting (TextTag ops don't re-trigger changed)
                md_formatter::apply_formatting(buf);
                // Image embedding deferred to next GLib iteration
                win.schedule_image_embed();
            }
        ));

        imp.search_button.connect_toggled(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |btn| {
                let imp = win.imp();
                if btn.is_active() {
                    imp.search_stack.set_visible_child_name("search");
                    imp.search_entry.grab_focus();
                } else {
                    imp.search_stack.set_visible_child_name("empty");
                    imp.search_entry.set_text("");
                    *imp.note_filter_search.borrow_mut() = String::new();
                    if let Some(f) = imp.note_filter.borrow().as_ref() {
                        f.changed(gtk::FilterChange::Different);
                    }
                }
            }
        ));
        imp.search_entry.connect_stop_search(glib::clone!(
            #[weak]
            imp,
            move |_| imp.search_button.set_active(false)
        ));
        imp.search_entry.connect_search_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |entry| {
                let imp = win.imp();
                *imp.note_filter_search.borrow_mut() = entry.text().to_lowercase().to_string();
                if let Some(f) = imp.note_filter.borrow().as_ref() {
                    f.changed(gtk::FilterChange::Different);
                };
            }
        ));

    }
}


fn split_title_content(text: &str) -> (String, String) {
    match text.find('\n') {
        Some(pos) => (text[..pos].to_string(), text[pos + 1..].to_string()),
        None => (text.to_string(), String::new()),
    }
}

fn folder_name_for(folder_store: &gio::ListStore, folder_id: &str) -> String {
    if folder_id.is_empty() {
        return String::new();
    }
    (0..folder_store.n_items())
        .find_map(|i| {
            folder_store
                .item(i)
                .and_downcast::<AmzFolder>()
                .filter(|f| f.id() == folder_id)
                .map(|f| f.name())
        })
        .unwrap_or_default()
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
