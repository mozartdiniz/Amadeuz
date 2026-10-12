//! iCloud Notes in the main window: sidebar section, sign-in dialogs and the
//! read-only note view. iCloud notes use their own stores (see `ICloudManager`)
//! and are never mixed into the local/server note store.

use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::icloud_manager::{ICloudManager, ICloudRef, ICloudStatus, SaveStatus, SignInStep, WRITABLE_FOLDER};
use crate::model::{AmzFolder, AmzNote};
use crate::ui::AmzNoteRow;

use super::window::{folder_name_for, AmzWindow};

/// Sidebar selection marker so the window doesn't auto-select "All Notes"
/// while an iCloud folder is shown.
pub const ICLOUD_SELECTION: &str = "__icloud__";

impl AmzWindow {
    pub fn set_icloud(&self, icloud: ICloudRef) {
        {
            let win = self.downgrade();
            icloud.borrow_mut().on_changed = Some(Rc::new(move || {
                if let Some(w) = win.upgrade() {
                    w.refresh_icloud_ui();
                }
            }));
        }
        {
            let win = self.downgrade();
            icloud.borrow_mut().on_sign_in_step = Some(Rc::new(move |s| {
                if let Some(w) = win.upgrade() {
                    w.on_icloud_step(s);
                }
            }));
        }
        {
            let win = self.downgrade();
            icloud.borrow_mut().on_images_ready = Some(Rc::new(move || {
                if let Some(w) = win.upgrade() {
                    if w.imp().icloud_mode.get() {
                        w.schedule_image_embed();
                    }
                }
            }));
        }
        {
            let win = self.downgrade();
            icloud.borrow_mut().on_conflict = Some(Rc::new(move |id| {
                if let Some(w) = win.upgrade() {
                    w.on_icloud_conflict(&id);
                }
            }));
        }
        {
            let win = self.downgrade();
            icloud.borrow_mut().on_synced = Some(Rc::new(move || {
                if let Some(w) = win.upgrade() {
                    w.reload_open_icloud_note();
                }
            }));
        }
        {
            let win = self.downgrade();
            icloud.borrow_mut().on_created = Some(Rc::new(move |id| {
                if let Some(w) = win.upgrade() {
                    w.open_created_icloud_note(&id);
                }
            }));
        }
        self.build_icloud_sidebar(&icloud);
        self.imp().icloud.replace(Some(icloud));
        self.refresh_icloud_ui();
    }

    pub(super) fn icloud_connected(&self) -> bool {
        self.imp()
            .icloud
            .borrow()
            .as_ref()
            .is_some_and(|i| i.borrow().is_connected())
    }

    // ── Sidebar ───────────────────────────────────────────────────────────────

    fn build_icloud_sidebar(&self, icloud: &ICloudRef) {
        let imp = self.imp();
        let Some(sidebar_box) = imp.folder_list.parent().and_downcast::<gtk::Box>() else {
            log::warn!("folder list is not inside a Gtk.Box; iCloud section not added");
            return;
        };

        let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        section.set_visible(false);
        section.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        header.set_margin_start(12);
        header.set_margin_end(6);
        header.set_margin_top(8);
        header.set_margin_bottom(2);
        let title = gtk::Label::new(Some("iCloud"));
        title.set_halign(gtk::Align::Start);
        title.set_hexpand(true);
        title.add_css_class("caption-heading");
        title.add_css_class("dim-label");
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Refresh iCloud Notes"));
        refresh.set_valign(gtk::Align::Center);
        refresh.add_css_class("flat");
        header.append(&title);
        header.append(&refresh);
        section.append(&header);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.add_css_class("navigation-sidebar");
        let folder_store = icloud.borrow().folder_store.clone();
        list.bind_model(Some(&folder_store), |obj| {
            let folder = obj.downcast_ref::<AmzFolder>().unwrap();
            let icon = if folder.id().starts_with("TrashFolder") {
                "user-trash-symbolic"
            } else {
                "folder-symbolic"
            };
            let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row_box.set_margin_start(12);
            row_box.set_margin_end(12);
            row_box.set_margin_top(8);
            row_box.set_margin_bottom(8);
            row_box.append(&gtk::Image::from_icon_name(icon));
            let label = gtk::Label::new(Some(&folder.name()));
            label.set_halign(gtk::Align::Start);
            label.set_hexpand(true);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            row_box.append(&label);
            let row = gtk::ListBoxRow::new();
            row.set_child(Some(&row_box));
            row.upcast()
        });
        section.append(&list);
        sidebar_box.append(&section);

        list.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            #[weak]
            folder_store,
            move |_, row| {
                let Some(folder) = folder_store.item(row.index() as u32).and_downcast::<AmzFolder>() else {
                    return;
                };
                win.on_icloud_folder_selected(folder.id());
            }
        ));
        refresh.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| {
                let icloud = win.imp().icloud.borrow().clone();
                if let Some(icloud) = icloud {
                    ICloudManager::refresh(&icloud);
                }
            }
        ));

        imp.icloud_section.replace(Some(section));
        imp.icloud_header.replace(Some(title));
        imp.icloud_folder_list.replace(Some(list));
    }

    pub(super) fn refresh_icloud_ui(&self) {
        let imp = self.imp();
        let Some(icloud) = imp.icloud.borrow().clone() else { return };
        let status = icloud.borrow().status.clone();

        if let Some(section) = imp.icloud_section.borrow().as_ref() {
            section.set_visible(status != ICloudStatus::Disconnected);
        }
        if let Some(header) = imp.icloud_header.borrow().as_ref() {
            match &status {
                ICloudStatus::Syncing => {
                    header.set_text("iCloud · Syncing…");
                    header.set_tooltip_text(None);
                }
                ICloudStatus::Error(msg) => {
                    header.set_text("iCloud · Error");
                    header.set_tooltip_text(Some(msg));
                }
                _ => {
                    header.set_text("iCloud");
                    header.set_tooltip_text(None);
                }
            }
        }

        if status == ICloudStatus::Disconnected && imp.icloud_mode.get() {
            self.leave_icloud_mode();
            self.on_all_notes_selected();
            if let Some(row) = imp.all_notes_list.row_at_index(0) {
                imp.all_notes_list.select_row(Some(&row));
            }
        }
        if imp.icloud_mode.get() {
            self.update_icloud_status_bar();
        }
        self.rebuild_menu();
    }

    pub(super) fn update_icloud_status_bar(&self) {
        let imp = self.imp();
        let Some(icloud) = imp.icloud.borrow().clone() else { return };
        let editable = imp.text_view.is_editable();
        let text = {
            let i = icloud.borrow();
            match (&i.status, &i.save_status) {
                (ICloudStatus::Error(msg), _) => msg.clone(),
                (_, SaveStatus::Saving) => "iCloud · Saving…".to_string(),
                (_, SaveStatus::Failed(msg)) => format!("iCloud · Not saved: {msg}"),
                (ICloudStatus::Syncing, _) => "iCloud · Syncing…".to_string(),
                (_, SaveStatus::Saved) if editable => "iCloud · Saved".to_string(),
                _ if editable => "iCloud · Editing".to_string(),
                _ => format!("iCloud · Read-only (editing is enabled in \"{WRITABLE_FOLDER}\" only)"),
            }
        };
        imp.status_icon.set_icon_name(Some("weather-overcast-symbolic"));
        imp.status_label.set_text(&text);
    }

    // ── Selection ─────────────────────────────────────────────────────────────

    fn on_icloud_folder_selected(&self, folder_id: String) {
        let imp = self.imp();
        imp.all_notes_list.unselect_all();
        imp.folder_list.unselect_all();
        imp.wastebasket_list.unselect_all();
        if let Some(mgr) = imp.manager.borrow().clone() {
            let mut m = mgr.borrow_mut();
            m.selected_folder_id = Some(ICLOUD_SELECTION.into());
            m.selected_note_id = None;
            m.disconnect_note_ws();
        }

        self.flush_icloud_save();
        if !imp.icloud_mode.get() {
            imp.icloud_mode.set(true);
            self.bind_icloud_note_list();
            imp.delete_note_button.set_sensitive(false);
        }
        self.set_icloud_editable(false);
        imp.icloud_open_note.replace(None);
        let writable = imp.icloud.borrow().as_ref().is_some_and(|i| i.borrow().is_writable_folder(&folder_id));
        imp.new_note_button.set_sensitive(writable);
        imp.icloud_filter_folder.replace(Some(folder_id));
        self.refilter_icloud();
        self.clear_editor();
        self.update_icloud_status_bar();
    }

    /// Restore the local/server note list and editing. Called whenever a
    /// non-iCloud sidebar entry is selected.
    pub(super) fn leave_icloud_mode(&self) {
        let imp = self.imp();
        if !imp.icloud_mode.get() {
            return;
        }
        self.flush_icloud_save();
        imp.icloud_open_note.replace(None);
        imp.icloud_mode.set(false);
        if let Some(list) = imp.icloud_folder_list.borrow().as_ref() {
            list.unselect_all();
        }
        imp.delete_note_button.set_sensitive(true);
        imp.text_view.set_editable(true);
        imp.text_view.set_cursor_visible(true);
        if let Some(mgr) = imp.manager.borrow().clone() {
            self.bind_local_note_list(&mgr);
        }
        self.clear_editor();
        self.refresh_ui();
    }

    fn bind_icloud_note_list(&self) {
        let imp = self.imp();
        let Some(icloud) = imp.icloud.borrow().clone() else { return };
        let (note_store, folder_store) = {
            let i = icloud.borrow();
            (i.note_store.clone(), i.folder_store.clone())
        };

        let win_weak = self.downgrade();
        let filter = gtk::CustomFilter::new(move |obj| {
            let Some(win) = win_weak.upgrade() else { return true };
            let imp = win.imp();
            let Some(note) = obj.downcast_ref::<AmzNote>() else { return true };
            if imp.icloud_filter_folder.borrow().as_deref() != Some(note.folder_id().as_str()) {
                return false;
            }
            let search = imp.note_filter_search.borrow().clone();
            search.is_empty()
                || note.title().to_lowercase().contains(&search)
                || note.content().to_lowercase().contains(&search)
        });
        let model = gtk::FilterListModel::new(Some(note_store), Some(filter.clone()));
        imp.note_list.bind_model(Some(&model), move |obj| {
            let note = obj.downcast_ref::<AmzNote>().unwrap();
            let row = AmzNoteRow::new();
            row.bind_note(note, &folder_name_for(&folder_store, &note.folder_id()));
            // Saves update the note's title/content/date; keep the row in step.
            let refresh = glib::clone!(
                #[weak]
                row,
                #[weak]
                folder_store,
                move |n: &AmzNote| row.bind_note(n, &folder_name_for(&folder_store, &n.folder_id()))
            );
            note.connect_title_notify(refresh.clone());
            note.connect_content_notify(refresh.clone());
            note.connect_updated_at_notify(refresh);
            row.upcast()
        });
        imp.icloud_filter.replace(Some(filter));
        imp.icloud_filter_model.replace(Some(model));
    }

    pub(super) fn refilter_icloud(&self) {
        if let Some(f) = self.imp().icloud_filter.borrow().as_ref() {
            f.changed(gtk::FilterChange::Different);
        }
    }

    pub(super) fn on_icloud_note_activated(&self, index: u32) {
        let imp = self.imp();
        let Some(note) = imp
            .icloud_filter_model
            .borrow()
            .as_ref()
            .and_then(|m| m.item(index))
            .and_downcast::<AmzNote>()
        else {
            return;
        };
        self.open_icloud_note(&note.id());
    }

    fn open_icloud_note(&self, id: &str) {
        let imp = self.imp();
        self.flush_icloud_save();
        let Some(icloud) = imp.icloud.borrow().clone() else { return };
        let (body, editable) = {
            let i = icloud.borrow();
            (i.body(id).unwrap_or_default().to_string(), i.can_edit(id))
        };
        icloud.borrow_mut().save_status = SaveStatus::Idle;
        imp.icloud_open_note.replace(Some(id.to_string()));

        imp.loading_note.replace(true);
        imp.text_view.buffer().set_text(&body);
        imp.loading_note.replace(false);
        self.set_icloud_editable(editable);
        imp.inner_split.set_show_content(true);
        if editable {
            imp.text_view.grab_focus();
        }
        self.update_icloud_status_bar();
        icloud.borrow().fetch_images(super::md_formatter::icloud_image_ids(&body));
    }

    fn set_icloud_editable(&self, editable: bool) {
        let imp = self.imp();
        imp.text_view.set_editable(editable);
        imp.text_view.set_cursor_visible(editable);
    }

    // ── Editing ───────────────────────────────────────────────────────────────

    /// Save the open iCloud note one second after the last keystroke.
    pub(super) fn schedule_icloud_save(&self) {
        let imp = self.imp();
        if !imp.text_view.is_editable() || imp.icloud_open_note.borrow().is_none() {
            return;
        }
        if let Some(src) = imp.icloud_debounce.take() {
            src.remove();
        }
        let win = self.downgrade();
        let src = glib::timeout_add_local_once(std::time::Duration::from_secs(1), move || {
            if let Some(w) = win.upgrade() {
                w.imp().icloud_debounce.replace(None);
                w.save_open_icloud_note();
            }
        });
        imp.icloud_debounce.replace(Some(src));
    }

    /// Save immediately if a save is pending (switching notes/folders).
    fn flush_icloud_save(&self) {
        if let Some(src) = self.imp().icloud_debounce.take() {
            src.remove();
            self.save_open_icloud_note();
        }
    }

    fn save_open_icloud_note(&self) {
        let imp = self.imp();
        let Some(id) = imp.icloud_open_note.borrow().clone() else { return };
        let Some(icloud) = imp.icloud.borrow().clone() else { return };
        let buf = imp.text_view.buffer();
        let markdown = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        ICloudManager::save_note(&icloud, id, markdown);
    }

    pub(super) fn create_icloud_note(&self) {
        let imp = self.imp();
        let Some(folder) = imp.icloud_filter_folder.borrow().clone() else { return };
        self.flush_icloud_save();
        if let Some(icloud) = imp.icloud.borrow().as_ref() {
            icloud.borrow().create_note(folder);
        }
    }

    fn open_created_icloud_note(&self, id: &str) {
        let imp = self.imp();
        if !imp.icloud_mode.get() {
            return;
        }
        self.refilter_icloud();
        if let Some(row) = imp.note_list.row_at_index(0) {
            imp.note_list.select_row(Some(&row));
        }
        self.open_icloud_note(id);
    }

    /// After a refresh the editor may show an older version than the stores.
    /// Saving it would diff against the new version and undo edits made on
    /// other devices, so drop the pending save and show the fresh version.
    fn reload_open_icloud_note(&self) {
        let imp = self.imp();
        let Some(id) = imp.icloud_open_note.borrow().clone() else { return };
        if let Some(src) = imp.icloud_debounce.take() {
            src.remove();
        }
        let still_there = imp.icloud.borrow().as_ref().is_some_and(|i| i.borrow().body(&id).is_some());
        if still_there {
            self.open_icloud_note(&id);
        } else {
            imp.icloud_open_note.replace(None);
            self.set_icloud_editable(false);
            self.clear_editor();
        }
    }

    fn on_icloud_conflict(&self, id: &str) {
        let imp = self.imp();
        if imp.icloud_open_note.borrow().as_deref() == Some(id) {
            self.set_icloud_editable(false);
            imp.icloud_open_note.replace(None);
        }
        let dialog = adw::AlertDialog::new(
            Some("Note Changed on Another Device"),
            Some("This note was edited elsewhere before your changes were saved. It will reload; \
                  please re-apply your last edits."),
        );
        dialog.add_response("ok", "OK");
        dialog.present(Some(self));
    }

    // ── Sign-in dialogs ───────────────────────────────────────────────────────

    pub(super) fn show_icloud_sign_in(&self, error: Option<String>) {
        let group = adw::PreferencesGroup::new();
        let id_row = adw::EntryRow::new();
        id_row.set_title("Apple ID");
        id_row.set_input_purpose(gtk::InputPurpose::Email);
        let pw_row = adw::PasswordEntryRow::new();
        pw_row.set_title("Password");
        pw_row.set_activates_default(true);
        group.add(&id_row);
        group.add(&pw_row);

        let body = error.unwrap_or_else(|| {
            "Your notes stay in iCloud and are shown read-only. Your password goes only to \
             Apple; the sign-in session is kept in your system keyring."
                .into()
        });
        let dialog = adw::AlertDialog::new(Some("Connect iCloud"), Some(&body));
        dialog.set_extra_child(Some(&group));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("signin", "Sign In");
        dialog.set_response_appearance("signin", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("signin"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        let focus_row = id_row.clone();
        dialog.connect_response(Some("signin"), move |_, _| {
            let apple_id = id_row.text().trim().to_string();
            let password = pw_row.text().to_string();
            if apple_id.is_empty() || password.is_empty() {
                win.show_icloud_sign_in(Some("Enter your Apple ID and password.".into()));
                return;
            }
            win.show_icloud_progress("Signing in to iCloud…");
            if let Some(icloud) = win.imp().icloud.borrow().as_ref() {
                icloud.borrow().sign_in(apple_id, password);
            }
        });
        dialog.present(Some(self));
        focus_row.grab_focus();
    }

    fn show_icloud_code(&self, message: Option<String>) {
        let phones = self.imp().icloud_phones.borrow().clone();
        let entry = adw::EntryRow::new();
        entry.set_title("Verification Code");
        entry.set_input_purpose(gtk::InputPurpose::Digits);
        entry.set_activates_default(true);
        let group = adw::PreferencesGroup::new();
        group.add(&entry);

        let body = message.unwrap_or_else(|| "Enter the code shown on your iPhone, iPad or Mac.".into());
        let dialog = adw::AlertDialog::new(Some("Two-Factor Authentication"), Some(&body));
        dialog.set_extra_child(Some(&group));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("resend", "Resend Code");
        for (i, p) in phones.iter().enumerate().take(2) {
            dialog.add_response(&format!("sms-{i}"), &format!("Text {}", p.number));
        }
        dialog.add_response("verify", "Verify");
        dialog.set_response_appearance("verify", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("verify"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        let focus_entry = entry.clone();
        dialog.connect_response(None, move |_, response| {
            let Some(icloud) = win.imp().icloud.borrow().clone() else { return };
            match response {
                "verify" => {
                    let code: String = entry.text().chars().filter(|c| c.is_ascii_digit()).collect();
                    if code.len() != 6 {
                        win.show_icloud_code(Some("The code has 6 digits.".into()));
                        return;
                    }
                    win.show_icloud_progress("Verifying…");
                    icloud.borrow().submit_code(code);
                }
                "resend" => {
                    win.show_icloud_progress("Requesting a new code…");
                    icloud.borrow().resend_code();
                }
                r => {
                    let phone = r
                        .strip_prefix("sms-")
                        .and_then(|i| i.parse::<usize>().ok())
                        .and_then(|i| phones.get(i).cloned());
                    if let Some(phone) = phone {
                        win.show_icloud_progress("Sending SMS…");
                        icloud.borrow().request_sms(phone);
                    }
                }
            }
        });
        dialog.present(Some(self));
        focus_entry.grab_focus();
    }

    fn show_icloud_progress(&self, text: &str) {
        self.close_icloud_progress();
        let spinner = gtk::Spinner::new();
        spinner.start();
        let label = gtk::Label::new(Some(text));
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.set_margin_start(32);
        content.set_margin_end(32);
        content.append(&spinner);
        content.append(&label);
        let dialog = adw::Dialog::new();
        dialog.set_can_close(false);
        dialog.set_child(Some(&content));
        dialog.present(Some(self));
        self.imp().icloud_progress.replace(Some(dialog));
    }

    fn close_icloud_progress(&self) {
        if let Some(d) = self.imp().icloud_progress.take() {
            d.force_close();
        }
    }

    fn on_icloud_step(&self, step: SignInStep) {
        self.close_icloud_progress();
        match step {
            SignInStep::NeedsCode(phones) => {
                self.imp().icloud_phones.replace(phones);
                self.show_icloud_code(None);
            }
            SignInStep::WrongCode => self.show_icloud_code(Some("That code was incorrect. Try again.".into())),
            SignInStep::CodeSent(msg) => self.show_icloud_code(Some(msg)),
            SignInStep::Done => {}
            SignInStep::Failed(msg) => self.show_icloud_sign_in(Some(msg)),
        }
    }

    pub(super) fn prompt_icloud_disconnect(&self) {
        let dialog = adw::AlertDialog::new(
            Some("Disconnect iCloud?"),
            Some("Your notes stay in iCloud. They will no longer be shown in Amadeuz Notes."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("disconnect", "Disconnect");
        dialog.set_response_appearance("disconnect", adw::ResponseAppearance::Destructive);
        dialog.set_close_response("cancel");
        let win = self.clone();
        dialog.connect_response(Some("disconnect"), move |_, _| {
            let Some(icloud) = win.imp().icloud.borrow().clone() else { return };
            icloud.borrow_mut().disconnect();
            win.refresh_icloud_ui();
        });
        dialog.present(Some(self));
    }
}
