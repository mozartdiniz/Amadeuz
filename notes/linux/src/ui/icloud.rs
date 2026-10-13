//! iCloud Notes in the main window: sidebar section, sign-in dialogs and the
//! read-only note view. iCloud notes use their own stores (see `ICloudManager`)
//! and are never mixed into the local/server note store.

use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::icloud_manager::{ICloudManager, ICloudRef, ICloudStatus, SaveStatus, SignInStep, DEFAULT_FOLDER};
use crate::model::{AmzFolder, AmzNote};
use crate::ui::AmzNoteRow;

use super::window::{folder_name_for, AmzWindow};

/// Sidebar selection marker so the window doesn't auto-select "All Notes"
/// while an iCloud folder is shown.
pub const ICLOUD_SELECTION: &str = "__icloud__";

/// Pseudo folder id for "All iCloud": every note outside Recently Deleted.
const ICLOUD_ALL: &str = "__icloud_all__";

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
            icloud.borrow_mut().on_action_failed = Some(Rc::new(move |title, msg| {
                if let Some(w) = win.upgrade() {
                    let dialog = adw::AlertDialog::new(Some(&title), Some(&msg));
                    dialog.add_response("ok", "OK");
                    dialog.present(Some(&w));
                }
            }));
        }
        {
            let win = self.downgrade();
            icloud.borrow_mut().on_folder_deleted = Some(Rc::new(move |id| {
                let Some(w) = win.upgrade() else { return };
                let imp = w.imp();
                if imp.icloud_mode.get() && imp.icloud_filter_folder.borrow().as_deref() == Some(id.as_str()) {
                    w.on_all_notes_selected();
                    if let Some(row) = imp.all_notes_list.row_at_index(0) {
                        imp.all_notes_list.select_row(Some(&row));
                    }
                }
            }));
        }
        {
            let win = self.downgrade();
            icloud.borrow_mut().on_folder_renamed = Some(Rc::new(move |id| {
                let Some(w) = win.upgrade() else { return };
                let imp = w.imp();
                // Re-sorting rebuilt the row; highlight it again without
                // reloading the folder.
                if imp.icloud_mode.get() && imp.icloud_filter_folder.borrow().as_deref() == Some(id.as_str()) {
                    let index = imp.icloud.borrow().as_ref().and_then(|i| {
                        let i = i.borrow();
                        (0..i.folder_store.n_items())
                            .find(|&k| i.folder_store.item(k).and_downcast::<AmzFolder>().is_some_and(|f| f.id() == id))
                    });
                    if let (Some(list), Some(k)) = (imp.icloud_folder_list.borrow().as_ref(), index) {
                        if let Some(row) = list.row_at_index(k as i32) {
                            list.select_row(Some(&row));
                        }
                    }
                }
            }));
        }
        {
            let win = self.downgrade();
            icloud.borrow_mut().on_note_removed = Some(Rc::new(move |id| {
                if let Some(w) = win.upgrade() {
                    w.on_icloud_note_removed(&id);
                }
            }));
        }
        {
            let win = self.downgrade();
            icloud.borrow_mut().on_folder_created = Some(Rc::new(move |id| {
                if let Some(w) = win.upgrade() {
                    w.select_icloud_folder(&id);
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

        // "All iCloud" is not a real folder, so it lives in its own list above
        // the model-bound one; the two keep a single selection between them.
        let all_list = gtk::ListBox::new();
        all_list.set_selection_mode(gtk::SelectionMode::Single);
        all_list.add_css_class("navigation-sidebar");
        {
            let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row_box.set_margin_start(12);
            row_box.set_margin_end(12);
            row_box.set_margin_top(8);
            row_box.set_margin_bottom(8);
            row_box.append(&gtk::Image::from_icon_name("weather-overcast-symbolic"));
            let label = gtk::Label::new(Some("All iCloud"));
            label.set_halign(gtk::Align::Start);
            label.set_hexpand(true);
            row_box.append(&label);
            let row = gtk::ListBoxRow::new();
            row.set_child(Some(&row_box));
            all_list.append(&row);
        }
        all_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _| win.on_icloud_folder_selected(ICLOUD_ALL.into())
        ));
        section.append(&all_list);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.add_css_class("navigation-sidebar");
        let folder_store = icloud.borrow().folder_store.clone();
        let win_weak = self.downgrade();
        list.bind_model(Some(&folder_store), move |obj| {
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

            // Right-click → folder actions.
            let right_click = gtk::GestureClick::new();
            right_click.set_button(3);
            {
                let win_weak = win_weak.clone();
                let (id, name) = (folder.id(), folder.name());
                right_click.connect_pressed(move |g, _, x, y| {
                    let (Some(win), Some(widget)) = (win_weak.upgrade(), g.widget()) else { return };
                    win.show_icloud_folder_menu(&id, &name, &widget, x, y);
                });
            }
            row.add_controller(right_click);
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
        imp.icloud_all_list.replace(Some(all_list));
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
        let reason = imp
            .icloud_open_note
            .borrow()
            .as_deref()
            .and_then(|id| icloud.borrow().read_only_reason(id));
        let text = {
            let i = icloud.borrow();
            match (&i.status, &i.save_status) {
                (ICloudStatus::Error(msg), _) => msg.clone(),
                (_, SaveStatus::Saving) => "iCloud · Saving…".to_string(),
                (_, SaveStatus::Failed(msg)) => format!("iCloud · Not saved: {msg}"),
                (ICloudStatus::Syncing, _) => "iCloud · Syncing…".to_string(),
                (_, SaveStatus::Saved) if editable => "iCloud · Saved".to_string(),
                _ if editable => "iCloud · Editing".to_string(),
                _ => match reason {
                    Some(r) => format!("iCloud · Read-only ({r})"),
                    None => "iCloud · Read-only".to_string(),
                },
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
        let other = if folder_id == ICLOUD_ALL { &imp.icloud_folder_list } else { &imp.icloud_all_list };
        if let Some(list) = other.borrow().as_ref() {
            list.unselect_all();
        }
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
        }
        imp.delete_note_button.set_sensitive(false);
        self.set_icloud_editable(false);
        imp.icloud_open_note.replace(None);
        // In "All iCloud", "+" creates in Notes, as on Apple devices.
        let writable = imp.icloud.borrow().as_ref().is_some_and(|i| {
            i.borrow().is_writable_folder(if folder_id == ICLOUD_ALL { DEFAULT_FOLDER } else { &folder_id })
        });
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
        for list in [&imp.icloud_folder_list, &imp.icloud_all_list] {
            if let Some(list) = list.borrow().as_ref() {
                list.unselect_all();
            }
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
            let folder_id = note.folder_id();
            let shown = match imp.icloud_filter_folder.borrow().as_deref() {
                Some(ICLOUD_ALL) => !folder_id.starts_with("TrashFolder"),
                f => f == Some(folder_id.as_str()),
            };
            if !shown {
                return false;
            }
            let search = imp.note_filter_search.borrow().clone();
            search.is_empty()
                || note.title().to_lowercase().contains(&search)
                || note.content().to_lowercase().contains(&search)
        });
        let model = gtk::FilterListModel::new(Some(note_store), Some(filter.clone()));
        let win_for_rows = self.downgrade();
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
            note.connect_folder_id_notify(refresh.clone());
            note.connect_updated_at_notify(refresh);

            let right_click = gtk::GestureClick::new();
            right_click.set_button(3);
            {
                let win_weak = win_for_rows.clone();
                let id = note.id();
                right_click.connect_pressed(move |g, _, x, y| {
                    let (Some(win), Some(widget)) = (win_weak.upgrade(), g.widget()) else { return };
                    win.show_icloud_note_menu(&id, &widget, x, y);
                });
            }
            row.add_controller(right_click);
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
        let in_trash = icloud.borrow().is_in_trash(id);
        imp.delete_note_button.set_sensitive(true);
        imp.delete_note_button.set_tooltip_text(Some(if in_trash { "Delete Permanently" } else { "Move to Trash" }));

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
        let Some(mut folder) = imp.icloud_filter_folder.borrow().clone() else { return };
        if folder == ICLOUD_ALL {
            folder = DEFAULT_FOLDER.into();
        }
        self.flush_icloud_save();
        if let Some(icloud) = imp.icloud.borrow().as_ref() {
            icloud.borrow().create_note(folder);
        }
    }

    /// "+" outside iCloud folders: a new amadeuz note, or a new iCloud note
    /// in a chosen folder.
    pub(super) fn show_new_note_menu(&self, anchor: &gtk::Widget) {
        let Some(icloud) = self.imp().icloud.borrow().clone() else { return };
        let folders: Vec<AmzFolder> = {
            let i = icloud.borrow();
            (0..i.folder_store.n_items())
                .filter_map(|k| i.folder_store.item(k).and_downcast::<AmzFolder>())
                .filter(|f| i.is_writable_folder(&f.id()))
                .collect()
        };

        let row = |icon: &str, label: &str, name: &str| {
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            b.set_margin_start(10);
            b.set_margin_end(10);
            b.set_margin_top(6);
            b.set_margin_bottom(6);
            b.append(&gtk::Image::from_icon_name(icon));
            let l = gtk::Label::new(Some(label));
            l.set_halign(gtk::Align::Start);
            b.append(&l);
            let r = gtk::ListBoxRow::new();
            r.set_widget_name(name);
            r.set_child(Some(&b));
            r
        };
        let list = |rows: Vec<gtk::ListBoxRow>| {
            let l = gtk::ListBox::new();
            l.set_selection_mode(gtk::SelectionMode::None);
            l.add_css_class("navigation-sidebar");
            for r in rows {
                l.append(&r);
            }
            l
        };

        let local = list(vec![row("document-new-symbolic", "New Amadeuz Note", "__local__")]);
        let header = gtk::Label::new(Some("New iCloud Note in"));
        header.add_css_class("heading");
        header.set_halign(gtk::Align::Start);
        header.set_margin_start(12);
        header.set_margin_top(8);
        header.set_margin_bottom(4);
        let remote = list(folders.iter().map(|f| row("folder-symbolic", &f.name(), &f.id())).collect());
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(360);
        scroll.set_child(Some(&remote));

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.append(&local);
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        vbox.append(&header);
        vbox.append(&scroll);

        let popover = gtk::Popover::new();
        popover.set_parent(anchor);
        popover.set_child(Some(&vbox));
        popover.connect_closed(|p| p.unparent());

        for l in [&local, &remote] {
            let win = self.downgrade();
            let pop = popover.downgrade();
            l.connect_row_activated(move |_, r| {
                if let Some(p) = pop.upgrade() {
                    p.popdown();
                }
                let Some(win) = win.upgrade() else { return };
                match r.widget_name().as_str() {
                    "__local__" => win.create_local_note(),
                    folder_id => win.create_icloud_note_in(folder_id),
                }
            });
        }
        popover.popup();
    }

    /// Switch to iCloud folder `folder_id` and create a note there.
    fn create_icloud_note_in(&self, folder_id: &str) {
        self.select_icloud_folder(folder_id);
        self.create_icloud_note();
    }

    /// Select iCloud folder `folder_id` in the sidebar and show its notes.
    fn select_icloud_folder(&self, folder_id: &str) {
        let imp = self.imp();
        let index = imp.icloud.borrow().as_ref().and_then(|i| {
            let i = i.borrow();
            (0..i.folder_store.n_items())
                .find(|&k| i.folder_store.item(k).and_downcast::<AmzFolder>().is_some_and(|f| f.id() == folder_id))
        });
        if let (Some(list), Some(k)) = (imp.icloud_folder_list.borrow().as_ref(), index) {
            if let Some(row) = list.row_at_index(k as i32) {
                list.select_row(Some(&row));
            }
        }
        self.on_icloud_folder_selected(folder_id.to_string());
    }

    fn show_icloud_folder_menu(&self, id: &str, name: &str, widget: &gtk::Widget, x: f64, y: f64) {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("navigation-sidebar");
        for (label, action) in [("Rename Folder…", "rename"), ("Delete Folder…", "delete")] {
            list.append(&menu_row(label, action));
        }
        let popover = context_popover(widget, x, y, &list);
        let win = self.downgrade();
        let pop = popover.downgrade();
        let (id, name) = (id.to_string(), name.to_string());
        list.connect_row_activated(move |_, row| {
            if let Some(p) = pop.upgrade() {
                p.popdown();
            }
            let Some(w) = win.upgrade() else { return };
            if row.widget_name() == "rename" {
                w.prompt_icloud_folder_rename(&id, &name, None);
            } else {
                w.prompt_icloud_folder_delete(&id, &name);
            }
        });
        popover.popup();
    }

    fn prompt_icloud_folder_rename(&self, id: &str, name: &str, error: Option<&str>) {
        let Some(icloud) = self.imp().icloud.borrow().clone() else { return };
        if let Some(why) = icloud.borrow().folder_rename_blocker(id) {
            let dialog = adw::AlertDialog::new(Some("Can't Rename This Folder"), Some(why));
            dialog.add_response("ok", "OK");
            dialog.present(Some(self));
            return;
        }
        let entry = adw::EntryRow::new();
        entry.set_title("Folder Name");
        entry.set_text(name);
        entry.set_activates_default(true);
        let group = adw::PreferencesGroup::new();
        group.add(&entry);
        let dialog = adw::AlertDialog::new(Some("Rename iCloud Folder"), error);
        dialog.set_extra_child(Some(&group));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("rename", "Rename");
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");
        let win = self.clone();
        let (id, old) = (id.to_string(), name.to_string());
        let focus = entry.clone();
        dialog.connect_response(Some("rename"), move |_, _| {
            let new = entry.text().trim().to_string();
            if new.is_empty() {
                win.prompt_icloud_folder_rename(&id, &old, Some("Enter a name for the folder."));
            } else if new != old {
                icloud.borrow().rename_folder(id.clone(), new);
            }
        });
        dialog.present(Some(self));
        focus.grab_focus();
    }

    /// Right-click on an iCloud note: move it to another folder, or (from
    /// Recently Deleted) recover it.
    fn show_icloud_note_menu(&self, note_id: &str, widget: &gtk::Widget, x: f64, y: f64) {
        let Some(icloud) = self.imp().icloud.borrow().clone() else { return };
        let (in_trash, folders) = {
            let i = icloud.borrow();
            let current = i.note_folder_id(note_id).unwrap_or_default();
            let folders: Vec<AmzFolder> = (0..i.folder_store.n_items())
                .filter_map(|k| i.folder_store.item(k).and_downcast::<AmzFolder>())
                .filter(|f| i.is_writable_folder(&f.id()) && f.id() != current)
                .collect();
            (i.is_in_trash(note_id), folders)
        };
        let header = gtk::Label::new(Some(if in_trash { "Recover to" } else { "Move to" }));
        header.add_css_class("heading");
        header.set_halign(gtk::Align::Start);
        header.set_margin_start(12);
        header.set_margin_top(8);
        header.set_margin_bottom(4);
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("navigation-sidebar");
        for f in &folders {
            list.append(&menu_row(&f.name(), &f.id()));
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(360);
        scroll.set_child(Some(&list));
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.append(&header);
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        vbox.append(&scroll);

        let popover = context_popover(widget, x, y, &vbox);
        let win = self.downgrade();
        let pop = popover.downgrade();
        let note_id = note_id.to_string();
        list.connect_row_activated(move |_, row| {
            if let Some(p) = pop.upgrade() {
                p.popdown();
            }
            let Some(w) = win.upgrade() else { return };
            if w.imp().icloud_open_note.borrow().as_deref() == Some(note_id.as_str()) {
                w.flush_icloud_save();
            }
            let title = if in_trash { "Couldn't Recover Note" } else { "Couldn't Move Note" };
            icloud.borrow().move_note(note_id.clone(), row.widget_name().to_string(), title);
        });
        popover.popup();
    }

    fn prompt_icloud_folder_delete(&self, id: &str, name: &str) {
        let Some(icloud) = self.imp().icloud.borrow().clone() else { return };
        let (blocker, count) = {
            let i = icloud.borrow();
            (i.folder_delete_blocker(id), i.notes_in_folder(id).len())
        };
        if let Some(why) = blocker {
            let dialog = adw::AlertDialog::new(Some("Can't Delete This Folder"), Some(why));
            dialog.add_response("ok", "OK");
            dialog.present(Some(self));
            return;
        }
        let body = match count {
            0 => "The folder is empty.".to_string(),
            1 => "Its note will be moved to Recently Deleted in iCloud.".to_string(),
            n => format!("Its {n} notes will be moved to Recently Deleted in iCloud."),
        };
        let dialog = adw::AlertDialog::new(Some(&format!("Delete \u{201c}{name}\u{201d}?")), Some(&body));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete Folder");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let id = id.to_string();
        let win = self.clone();
        dialog.connect_response(Some("delete"), move |_, _| {
            win.flush_icloud_save();
            icloud.borrow().delete_folder(id.clone());
        });
        dialog.present(Some(self));
    }

    /// Sidebar "+": a new amadeuz folder (inline entry, as before) or a new
    /// iCloud folder (asks for a name).
    pub(super) fn show_new_folder_menu(&self, anchor: &gtk::Widget) {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("navigation-sidebar");
        for (icon, label, name) in [
            ("folder-new-symbolic", "New Amadeuz Folder", "local"),
            ("weather-overcast-symbolic", "New iCloud Folder…", "icloud"),
        ] {
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            b.set_margin_start(10);
            b.set_margin_end(10);
            b.set_margin_top(6);
            b.set_margin_bottom(6);
            b.append(&gtk::Image::from_icon_name(icon));
            b.append(&gtk::Label::new(Some(label)));
            let r = gtk::ListBoxRow::new();
            r.set_widget_name(name);
            r.set_child(Some(&b));
            list.append(&r);
        }
        let popover = gtk::Popover::new();
        popover.set_parent(anchor);
        popover.set_child(Some(&list));
        popover.connect_closed(|p| p.unparent());
        let win = self.downgrade();
        let pop = popover.downgrade();
        list.connect_row_activated(move |_, r| {
            if let Some(p) = pop.upgrade() {
                p.popdown();
            }
            let Some(win) = win.upgrade() else { return };
            if r.widget_name() == "icloud" {
                win.prompt_icloud_folder_name(None);
            } else {
                win.start_local_folder();
            }
        });
        popover.popup();
    }

    fn prompt_icloud_folder_name(&self, error: Option<&str>) {
        let entry = adw::EntryRow::new();
        entry.set_title("Folder Name");
        entry.set_activates_default(true);
        let group = adw::PreferencesGroup::new();
        group.add(&entry);
        let dialog = adw::AlertDialog::new(Some("New iCloud Folder"), error);
        dialog.set_extra_child(Some(&group));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("create", "Create");
        dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("create"));
        dialog.set_close_response("cancel");
        let win = self.clone();
        let focus = entry.clone();
        dialog.connect_response(Some("create"), move |_, _| {
            let name = entry.text().trim().to_string();
            if name.is_empty() {
                win.prompt_icloud_folder_name(Some("Enter a name for the folder."));
                return;
            }
            if let Some(icloud) = win.imp().icloud.borrow().as_ref() {
                icloud.borrow().create_folder(name);
            }
        });
        dialog.present(Some(self));
        focus.grab_focus();
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

    /// Trash button in an iCloud folder: move to Recently Deleted, or delete
    /// for good (after asking) if it's already there.
    pub(super) fn delete_icloud_note(&self) {
        let imp = self.imp();
        let Some(id) = imp.icloud_open_note.borrow().clone() else { return };
        let Some(icloud) = imp.icloud.borrow().clone() else { return };
        self.flush_icloud_save();
        if !icloud.borrow().is_in_trash(&id) {
            imp.delete_note_button.set_sensitive(false);
            icloud.borrow().move_to_trash(id);
            return;
        }
        let dialog = adw::AlertDialog::new(
            Some("Delete Permanently?"),
            Some("This note will be deleted from iCloud on all your devices. This can't be undone."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete Permanently");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let win = self.clone();
        dialog.connect_response(Some("delete"), move |_, _| {
            win.imp().delete_note_button.set_sensitive(false);
            icloud.borrow().delete_permanently(id.clone());
        });
        dialog.present(Some(self));
    }

    /// A note was moved or deleted. Clear the editor if it left the list (a
    /// move between folders keeps it in "All iCloud").
    fn on_icloud_note_removed(&self, id: &str) {
        let imp = self.imp();
        self.refilter_icloud();
        let still_listed = imp.icloud_filter_model.borrow().as_ref().is_some_and(|m| {
            (0..m.n_items()).any(|i| m.item(i).and_downcast::<AmzNote>().is_some_and(|n| n.id() == id))
        });
        if !still_listed && imp.icloud_open_note.borrow().as_deref() == Some(id) {
            imp.icloud_open_note.replace(None);
            self.set_icloud_editable(false);
            self.clear_editor();
            imp.delete_note_button.set_sensitive(false);
        }
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
            // The list was rebuilt by the sync; highlight the open note again.
            let index = imp.icloud_filter_model.borrow().as_ref().and_then(|m| {
                (0..m.n_items()).find(|&i| m.item(i).and_downcast::<AmzNote>().is_some_and(|n| n.id() == id))
            });
            if let Some(row) = index.and_then(|i| imp.note_list.row_at_index(i as i32)) {
                imp.note_list.select_row(Some(&row));
            }
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

fn menu_row(label: &str, name: &str) -> gtk::ListBoxRow {
    let l = gtk::Label::new(Some(label));
    l.set_halign(gtk::Align::Start);
    l.set_margin_start(12);
    l.set_margin_end(12);
    l.set_margin_top(6);
    l.set_margin_bottom(6);
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(name);
    row.set_child(Some(&l));
    row
}

fn context_popover(widget: &gtk::Widget, x: f64, y: f64, child: &impl IsA<gtk::Widget>) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(widget);
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    popover.set_has_arrow(false);
    popover.set_child(Some(child));
    popover.connect_closed(|p| p.unparent());
    popover
}
