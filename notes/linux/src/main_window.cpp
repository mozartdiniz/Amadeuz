#include "main_window.h"

#include <algorithm>
#include <cstring>
#include <ctime>
#include <string>
#include <vector>

#include <gtk/gtk.h>
#include <pango/pango.h>

static constexpr int WINDOW_W        = 1000;
static constexpr int WINDOW_H        = 680;
static constexpr int FOLDER_PANEL_W  = 200;
static constexpr int NOTE_PANEL_W    = 260;

// ── Static text-formatting helpers ────────────────────────────────────────────

static std::string display_title(const std::string& title) {
    size_t s = title.find_first_not_of(" \t\n\r");
    if (s == std::string::npos) return "Untitled";
    size_t e = title.find_last_not_of(" \t\n\r");
    std::string t = title.substr(s, e - s + 1);
    return t.empty() ? "Untitled" : t;
}

static std::string note_preview(const std::string& content) {
    size_t s = content.find_first_not_of(" \t\n\r");
    if (s == std::string::npos) return "No additional text";
    return content.substr(s, 80);
}

static std::string format_note_date(int64_t ms) {
    if (ms == 0) return "";
    time_t ts = ms / 1000;
    struct tm note_tm{}, now_tm{};
    time_t now_t = time(nullptr);
    localtime_r(&ts,    &note_tm);
    localtime_r(&now_t, &now_tm);

    char buf[32];
    if (note_tm.tm_year == now_tm.tm_year && note_tm.tm_yday == now_tm.tm_yday) {
        strftime(buf, sizeof(buf), "%I:%M %p", &note_tm);
        return buf;
    }
    time_t yesterday = now_t - 86400;
    struct tm yesterday_tm{};
    localtime_r(&yesterday, &yesterday_tm);
    if (note_tm.tm_year == yesterday_tm.tm_year && note_tm.tm_yday == yesterday_tm.tm_yday)
        return "Yesterday";

    strftime(buf, sizeof(buf), "%b %e, %Y", &note_tm);
    return buf;
}

// ── Row factory helpers ───────────────────────────────────────────────────────

static GtkWidget* make_all_notes_row() {
    GtkWidget* row = gtk_list_box_row_new();
    g_object_set_data_full(G_OBJECT(row), "folder-id",
        g_strdup(NotesViewModel::ALL_NOTES_ID), g_free);

    GtkWidget* label = gtk_label_new("All Notes");
    gtk_label_set_xalign(GTK_LABEL(label), 0.0f);
    gtk_widget_set_margin_start(label, 12);
    gtk_widget_set_margin_end(label, 12);
    gtk_widget_set_margin_top(label, 7);
    gtk_widget_set_margin_bottom(label, 7);
    gtk_list_box_row_set_child(GTK_LIST_BOX_ROW(row), label);
    return row;
}

static GtkWidget* make_folder_row(const Folder& folder) {
    GtkWidget* row = gtk_list_box_row_new();
    g_object_set_data_full(G_OBJECT(row), "folder-id",
        g_strdup(folder.id.c_str()), g_free);

    GtkWidget* label = gtk_label_new(folder.name.c_str());
    gtk_label_set_xalign(GTK_LABEL(label), 0.0f);
    gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
    gtk_widget_set_hexpand(label, TRUE);
    gtk_widget_set_margin_start(label, 12);
    gtk_widget_set_margin_end(label, 12);
    gtk_widget_set_margin_top(label, 7);
    gtk_widget_set_margin_bottom(label, 7);
    gtk_list_box_row_set_child(GTK_LIST_BOX_ROW(row), label);
    return row;
}

static GtkWidget* make_note_row(const Note& note) {
    GtkWidget* row = gtk_list_box_row_new();
    g_object_set_data_full(G_OBJECT(row), "note-id",
        g_strdup(note.id.c_str()), g_free);

    GtkWidget* vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 3);
    gtk_widget_set_margin_start(vbox, 12);
    gtk_widget_set_margin_end(vbox, 12);
    gtk_widget_set_margin_top(vbox, 9);
    gtk_widget_set_margin_bottom(vbox, 9);
    gtk_list_box_row_set_child(GTK_LIST_BOX_ROW(row), vbox);

    // Title + date row
    GtkWidget* hbox = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_box_append(GTK_BOX(vbox), hbox);

    GtkWidget* title_lbl = gtk_label_new(display_title(note.title).c_str());
    gtk_label_set_xalign(GTK_LABEL(title_lbl), 0.0f);
    gtk_label_set_ellipsize(GTK_LABEL(title_lbl), PANGO_ELLIPSIZE_END);
    gtk_widget_set_hexpand(title_lbl, TRUE);
    gtk_widget_add_css_class(title_lbl, "caption-heading");
    gtk_box_append(GTK_BOX(hbox), title_lbl);

    GtkWidget* date_lbl = gtk_label_new(format_note_date(note.updated_at).c_str());
    gtk_widget_add_css_class(date_lbl, "caption");
    gtk_widget_add_css_class(date_lbl, "dim-label");
    gtk_box_append(GTK_BOX(hbox), date_lbl);

    // Preview
    GtkWidget* preview_lbl = gtk_label_new(note_preview(note.content).c_str());
    gtk_label_set_xalign(GTK_LABEL(preview_lbl), 0.0f);
    gtk_label_set_ellipsize(GTK_LABEL(preview_lbl), PANGO_ELLIPSIZE_END);
    gtk_widget_set_hexpand(preview_lbl, TRUE);
    gtk_widget_add_css_class(preview_lbl, "caption");
    gtk_widget_add_css_class(preview_lbl, "dim-label");
    gtk_box_append(GTK_BOX(vbox), preview_lbl);

    return row;
}

// ── MainWindow constructor ────────────────────────────────────────────────────

MainWindow::MainWindow(GtkApplication* app) {

    // ── View model ────────────────────────────────────────────────────────────
    vm_ = std::make_unique<NotesViewModel>(
        [this]()                                           { rebuild_folder_list(); },
        [this]()                                           { rebuild_note_list(); },
        [this](const Note* n)                              { load_note(n); },
        [this](const std::string& t, const std::string& c) { update_editor_text(t, c); },
        [this](bool connected)                             { update_status(connected); }
    );

    // ── Window ────────────────────────────────────────────────────────────────
    window_ = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(window_), "Amadeuz");
    gtk_window_set_default_size(GTK_WINDOW(window_), WINDOW_W, WINDOW_H);

    // ── CSS ───────────────────────────────────────────────────────────────────
    GtkCssProvider* css = gtk_css_provider_new();
    gtk_css_provider_load_from_string(css,
        "textview { font-size: 15px; }"
        ".note-title-entry { font-size: 20px; font-weight: bold; }"
        "listbox { background: transparent; }");
    gtk_style_context_add_provider_for_display(
        gdk_display_get_default(),
        GTK_STYLE_PROVIDER(css),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
    g_object_unref(css);

    // ── Header bar ────────────────────────────────────────────────────────────
    GtkWidget* header = gtk_header_bar_new();
    gtk_header_bar_set_show_title_buttons(GTK_HEADER_BAR(header), TRUE);

    status_label_ = gtk_label_new(nullptr);
    update_status(false);

    GtkWidget* settings_btn = gtk_button_new_with_label("Settings…");
    gtk_button_set_has_frame(GTK_BUTTON(settings_btn), FALSE);
    g_signal_connect(settings_btn, "clicked", G_CALLBACK(cb_settings_clicked), this);

    GtkWidget* end_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_box_append(GTK_BOX(end_box), status_label_);
    gtk_box_append(GTK_BOX(end_box), settings_btn);
    gtk_header_bar_pack_end(GTK_HEADER_BAR(header), end_box);

    gtk_window_set_titlebar(GTK_WINDOW(window_), header);

    // ── Folder sidebar ────────────────────────────────────────────────────────
    GtkWidget* folder_panel = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_widget_set_size_request(folder_panel, FOLDER_PANEL_W, -1);

    GtkWidget* folder_title = gtk_label_new("Folders");
    gtk_label_set_xalign(GTK_LABEL(folder_title), 0.0f);
    gtk_widget_set_margin_start(folder_title, 12);
    gtk_widget_set_margin_top(folder_title, 12);
    gtk_widget_set_margin_bottom(folder_title, 6);
    gtk_widget_add_css_class(folder_title, "title-4");
    gtk_box_append(GTK_BOX(folder_panel), folder_title);

    GtkWidget* folder_scroll = gtk_scrolled_window_new();
    gtk_widget_set_vexpand(folder_scroll, TRUE);
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(folder_scroll),
        GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
    gtk_box_append(GTK_BOX(folder_panel), folder_scroll);

    folder_list_box_ = gtk_list_box_new();
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(folder_list_box_), GTK_SELECTION_SINGLE);
    gtk_list_box_set_header_func(GTK_LIST_BOX(folder_list_box_),
        (GtkListBoxUpdateHeaderFunc)folder_header_func, nullptr, nullptr);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(folder_scroll), folder_list_box_);
    g_signal_connect(folder_list_box_, "row-selected",
        G_CALLBACK(cb_folder_row_selected), this);

    gtk_box_append(GTK_BOX(folder_panel), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));

    GtkWidget* new_folder_btn = gtk_button_new_with_label("+ New Folder");
    gtk_button_set_has_frame(GTK_BUTTON(new_folder_btn), FALSE);
    gtk_widget_set_halign(new_folder_btn, GTK_ALIGN_START);
    gtk_widget_set_margin_start(new_folder_btn, 8);
    gtk_widget_set_margin_end(new_folder_btn, 8);
    gtk_widget_set_margin_top(new_folder_btn, 6);
    gtk_widget_set_margin_bottom(new_folder_btn, 6);
    gtk_box_append(GTK_BOX(folder_panel), new_folder_btn);
    g_signal_connect(new_folder_btn, "clicked", G_CALLBACK(cb_new_folder_clicked), this);

    // ── Note list panel ───────────────────────────────────────────────────────
    GtkWidget* note_panel = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_widget_set_size_request(note_panel, NOTE_PANEL_W, -1);

    GtkWidget* note_header_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);
    gtk_widget_set_margin_start(note_header_row, 12);
    gtk_widget_set_margin_end(note_header_row, 8);
    gtk_widget_set_margin_top(note_header_row, 8);
    gtk_widget_set_margin_bottom(note_header_row, 8);
    gtk_box_append(GTK_BOX(note_panel), note_header_row);

    note_list_header_ = gtk_label_new("All Notes");
    gtk_label_set_xalign(GTK_LABEL(note_list_header_), 0.0f);
    gtk_widget_set_hexpand(note_list_header_, TRUE);
    gtk_widget_add_css_class(note_list_header_, "title-4");
    gtk_box_append(GTK_BOX(note_header_row), note_list_header_);

    delete_note_btn_ = gtk_button_new_from_icon_name("user-trash-symbolic");
    gtk_widget_set_tooltip_text(delete_note_btn_, "Delete Note");
    gtk_widget_set_sensitive(delete_note_btn_, FALSE);
    gtk_box_append(GTK_BOX(note_header_row), delete_note_btn_);
    g_signal_connect(delete_note_btn_, "clicked", G_CALLBACK(cb_delete_note_clicked), this);

    new_note_btn_ = gtk_button_new_from_icon_name("list-add-symbolic");
    gtk_widget_set_tooltip_text(new_note_btn_, "New Note");
    gtk_widget_set_sensitive(new_note_btn_, FALSE);
    gtk_box_append(GTK_BOX(note_header_row), new_note_btn_);
    g_signal_connect(new_note_btn_, "clicked", G_CALLBACK(cb_new_note_clicked), this);

    gtk_box_append(GTK_BOX(note_panel), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));

    GtkWidget* note_scroll = gtk_scrolled_window_new();
    gtk_widget_set_vexpand(note_scroll, TRUE);
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(note_scroll),
        GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
    gtk_box_append(GTK_BOX(note_panel), note_scroll);

    note_list_box_ = gtk_list_box_new();
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(note_list_box_), GTK_SELECTION_SINGLE);

    GtkWidget* placeholder = gtk_label_new("No Notes");
    gtk_widget_add_css_class(placeholder, "dim-label");
    gtk_widget_set_margin_top(placeholder, 32);
    gtk_list_box_set_placeholder(GTK_LIST_BOX(note_list_box_), placeholder);

    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(note_scroll), note_list_box_);
    g_signal_connect(note_list_box_, "row-selected",
        G_CALLBACK(cb_note_row_selected), this);

    // ── Editor panel ──────────────────────────────────────────────────────────
    GtkWidget* editor_panel = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_widget_set_hexpand(editor_panel, TRUE);

    editor_stack_ = gtk_stack_new();
    gtk_widget_set_vexpand(editor_stack_, TRUE);
    gtk_widget_set_hexpand(editor_stack_, TRUE);
    gtk_box_append(GTK_BOX(editor_panel), editor_stack_);

    // Empty page
    GtkWidget* empty_label = gtk_label_new("Select a note to start editing");
    gtk_widget_add_css_class(empty_label, "dim-label");
    gtk_widget_add_css_class(empty_label, "title-3");
    gtk_widget_set_valign(empty_label, GTK_ALIGN_CENTER);
    gtk_widget_set_halign(empty_label, GTK_ALIGN_CENTER);
    gtk_stack_add_named(GTK_STACK(editor_stack_), empty_label, "empty");

    // Editor page
    GtkWidget* editor_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_stack_add_named(GTK_STACK(editor_stack_), editor_box, "editor");

    title_entry_ = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(title_entry_), "Title");
    gtk_widget_add_css_class(title_entry_, "note-title-entry");
    gtk_widget_add_css_class(title_entry_, "flat");
    gtk_widget_set_margin_start(title_entry_, 16);
    gtk_widget_set_margin_end(title_entry_, 16);
    gtk_widget_set_margin_top(title_entry_, 16);
    gtk_widget_set_margin_bottom(title_entry_, 8);
    gtk_box_append(GTK_BOX(editor_box), title_entry_);
    g_signal_connect(title_entry_, "changed", G_CALLBACK(cb_title_changed), this);

    gtk_box_append(GTK_BOX(editor_box), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));

    GtkWidget* content_scroll = gtk_scrolled_window_new();
    gtk_widget_set_vexpand(content_scroll, TRUE);
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(content_scroll),
        GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
    gtk_box_append(GTK_BOX(editor_box), content_scroll);

    GtkWidget* content_view = gtk_text_view_new();
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(content_view), GTK_WRAP_WORD_CHAR);
    gtk_text_view_set_left_margin(GTK_TEXT_VIEW(content_view), 16);
    gtk_text_view_set_right_margin(GTK_TEXT_VIEW(content_view), 16);
    gtk_text_view_set_top_margin(GTK_TEXT_VIEW(content_view), 12);
    gtk_text_view_set_bottom_margin(GTK_TEXT_VIEW(content_view), 12);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(content_scroll), content_view);
    content_buffer_ = gtk_text_view_get_buffer(GTK_TEXT_VIEW(content_view));
    g_signal_connect(content_buffer_, "changed", G_CALLBACK(cb_content_changed), this);

    gtk_stack_set_visible_child_name(GTK_STACK(editor_stack_), "empty");

    // ── 3-column paned layout ─────────────────────────────────────────────────
    GtkWidget* right_paned = gtk_paned_new(GTK_ORIENTATION_HORIZONTAL);
    gtk_paned_set_start_child(GTK_PANED(right_paned), note_panel);
    gtk_paned_set_end_child(GTK_PANED(right_paned), editor_panel);
    gtk_paned_set_position(GTK_PANED(right_paned), NOTE_PANEL_W);
    gtk_paned_set_resize_start_child(GTK_PANED(right_paned), FALSE);
    gtk_paned_set_shrink_start_child(GTK_PANED(right_paned), FALSE);
    gtk_paned_set_resize_end_child(GTK_PANED(right_paned), TRUE);

    GtkWidget* main_paned = gtk_paned_new(GTK_ORIENTATION_HORIZONTAL);
    gtk_paned_set_start_child(GTK_PANED(main_paned), folder_panel);
    gtk_paned_set_end_child(GTK_PANED(main_paned), right_paned);
    gtk_paned_set_position(GTK_PANED(main_paned), FOLDER_PANEL_W);
    gtk_paned_set_resize_start_child(GTK_PANED(main_paned), FALSE);
    gtk_paned_set_shrink_start_child(GTK_PANED(main_paned), FALSE);
    gtk_paned_set_resize_end_child(GTK_PANED(main_paned), TRUE);

    gtk_window_set_child(GTK_WINDOW(window_), main_paned);

    // ── Initial state ─────────────────────────────────────────────────────────
    rebuild_folder_list();
    rebuild_note_list();

    gtk_window_present(GTK_WINDOW(window_));
}

// ── UI rebuild ────────────────────────────────────────────────────────────────

void MainWindow::rebuild_folder_list() {
    suppress_folder_selection_ = true;

    // Remove all existing rows
    std::vector<GtkWidget*> to_remove;
    for (int i = 0; ; i++) {
        GtkListBoxRow* row = gtk_list_box_get_row_at_index(
            GTK_LIST_BOX(folder_list_box_), i);
        if (!row) break;
        to_remove.push_back(GTK_WIDGET(row));
    }
    for (auto* w : to_remove)
        gtk_list_box_remove(GTK_LIST_BOX(folder_list_box_), w);

    // Add "All Notes" row
    gtk_list_box_append(GTK_LIST_BOX(folder_list_box_), make_all_notes_row());

    // Add folder rows with right-click gesture
    for (const auto& folder : vm_->folders()) {
        GtkWidget* row = make_folder_row(folder);

        GtkGesture* gesture = gtk_gesture_click_new();
        gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), 3);
        g_object_set_data(G_OBJECT(gesture), "mw", this);
        g_object_set_data_full(G_OBJECT(gesture), "folder-id",
            g_strdup(folder.id.c_str()), g_free);
        g_signal_connect(gesture, "pressed",
            G_CALLBACK(cb_folder_right_click), nullptr);
        gtk_widget_add_controller(row, GTK_EVENT_CONTROLLER(gesture));

        gtk_list_box_append(GTK_LIST_BOX(folder_list_box_), row);
    }

    // Restore selection
    const auto& sel = vm_->selected_folder_id();
    for (int i = 0; ; i++) {
        GtkListBoxRow* row = gtk_list_box_get_row_at_index(
            GTK_LIST_BOX(folder_list_box_), i);
        if (!row) break;
        const char* id = (const char*)g_object_get_data(G_OBJECT(row), "folder-id");
        if (id && sel == id) {
            gtk_list_box_select_row(GTK_LIST_BOX(folder_list_box_), row);
            break;
        }
    }

    suppress_folder_selection_ = false;
}

void MainWindow::rebuild_note_list() {
    suppress_note_selection_ = true;

    // Update header label
    const auto& sel_folder = vm_->selected_folder_id();
    if (sel_folder == NotesViewModel::ALL_NOTES_ID || sel_folder.empty()) {
        gtk_label_set_text(GTK_LABEL(note_list_header_), "All Notes");
    } else {
        const auto& folders = vm_->folders();
        auto it = std::find_if(folders.begin(), folders.end(),
            [&](const Folder& f) { return f.id == sel_folder; });
        gtk_label_set_text(GTK_LABEL(note_list_header_),
            it != folders.end() ? it->name.c_str() : "Notes");
    }

    // Update button sensitivity
    bool can_create = !sel_folder.empty() && sel_folder != NotesViewModel::ALL_NOTES_ID;
    gtk_widget_set_sensitive(new_note_btn_, can_create ? TRUE : FALSE);

    // Remove existing rows
    std::vector<GtkWidget*> to_remove;
    for (int i = 0; ; i++) {
        GtkListBoxRow* row = gtk_list_box_get_row_at_index(
            GTK_LIST_BOX(note_list_box_), i);
        if (!row) break;
        to_remove.push_back(GTK_WIDGET(row));
    }
    for (auto* w : to_remove)
        gtk_list_box_remove(GTK_LIST_BOX(note_list_box_), w);

    // Add note rows with right-click gesture
    for (const auto& note : vm_->notes_in_view()) {
        GtkWidget* row = make_note_row(note);

        GtkGesture* gesture = gtk_gesture_click_new();
        gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), 3);
        g_object_set_data(G_OBJECT(gesture), "mw", this);
        g_object_set_data_full(G_OBJECT(gesture), "note-id",
            g_strdup(note.id.c_str()), g_free);
        g_signal_connect(gesture, "pressed",
            G_CALLBACK(cb_note_right_click), nullptr);
        gtk_widget_add_controller(row, GTK_EVENT_CONTROLLER(gesture));

        gtk_list_box_append(GTK_LIST_BOX(note_list_box_), row);
    }

    // Restore selection
    const auto& sel_note = vm_->selected_note_id();
    if (!sel_note.empty()) {
        for (int i = 0; ; i++) {
            GtkListBoxRow* row = gtk_list_box_get_row_at_index(
                GTK_LIST_BOX(note_list_box_), i);
            if (!row) break;
            const char* id = (const char*)g_object_get_data(G_OBJECT(row), "note-id");
            if (id && sel_note == id) {
                gtk_list_box_select_row(GTK_LIST_BOX(note_list_box_), row);
                break;
            }
        }
    }

    suppress_note_selection_ = false;
}

void MainWindow::load_note(const Note* note) {
    if (!note) {
        gtk_stack_set_visible_child_name(GTK_STACK(editor_stack_), "empty");
        gtk_widget_set_sensitive(delete_note_btn_, FALSE);
        return;
    }

    gtk_stack_set_visible_child_name(GTK_STACK(editor_stack_), "editor");
    gtk_widget_set_sensitive(delete_note_btn_, TRUE);

    suppress_title_changed_ = true;
    gtk_editable_set_text(GTK_EDITABLE(title_entry_), note->title.c_str());
    suppress_title_changed_ = false;

    suppress_content_changed_ = true;
    gtk_text_buffer_set_text(content_buffer_, note->content.c_str(), -1);
    suppress_content_changed_ = false;
}

void MainWindow::update_editor_text(const std::string& title, const std::string& content) {
    // Preserve cursor position in text buffer before updating.
    GtkTextMark* insert = gtk_text_buffer_get_insert(content_buffer_);
    GtkTextIter  cursor;
    gtk_text_buffer_get_iter_at_mark(content_buffer_, &cursor, insert);
    int offset = gtk_text_iter_get_offset(&cursor);

    suppress_title_changed_   = true;
    suppress_content_changed_ = true;

    gtk_editable_set_text(GTK_EDITABLE(title_entry_), title.c_str());
    gtk_text_buffer_set_text(content_buffer_, content.c_str(), -1);

    suppress_title_changed_   = false;
    suppress_content_changed_ = false;

    GtkTextIter new_pos;
    gtk_text_buffer_get_iter_at_offset(content_buffer_, &new_pos,
        std::min(offset, gtk_text_buffer_get_char_count(content_buffer_)));
    gtk_text_buffer_place_cursor(content_buffer_, &new_pos);
}

void MainWindow::update_status(bool connected) {
    const char* markup = connected
        ? "<span foreground='#22C55E'>●</span>  "
          "<span foreground='#888888' size='small'>Synced</span>"
        : "<span foreground='#EF4444'>●</span>  "
          "<span foreground='#888888' size='small'>Offline</span>";
    gtk_label_set_markup(GTK_LABEL(status_label_), markup);
}

// ── Signal callbacks ──────────────────────────────────────────────────────────

void MainWindow::cb_folder_row_selected(GtkListBox*, GtkListBoxRow* row, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    if (self->suppress_folder_selection_ || !row) return;

    const char* folder_id =
        (const char*)g_object_get_data(G_OBJECT(row), "folder-id");
    if (folder_id) self->vm_->select_folder(folder_id);
}

void MainWindow::cb_note_row_selected(GtkListBox*, GtkListBoxRow* row, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    if (self->suppress_note_selection_) return;

    if (!row) {
        self->vm_->select_note("");
        return;
    }
    const char* note_id =
        (const char*)g_object_get_data(G_OBJECT(row), "note-id");
    if (note_id) self->vm_->select_note(note_id);
}

void MainWindow::cb_title_changed(GtkEditable*, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    if (self->suppress_title_changed_) return;

    const char* text = gtk_editable_get_text(GTK_EDITABLE(self->title_entry_));
    self->vm_->on_title_changed(text ? text : "");
}

void MainWindow::cb_content_changed(GtkTextBuffer*, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    if (self->suppress_content_changed_) return;

    GtkTextIter start, end;
    gtk_text_buffer_get_bounds(self->content_buffer_, &start, &end);
    gchar* text = gtk_text_buffer_get_text(self->content_buffer_, &start, &end, FALSE);
    self->vm_->on_content_changed(text ? text : "");
    g_free(text);
}

void MainWindow::cb_new_folder_clicked(GtkButton*, gpointer data) {
    static_cast<MainWindow*>(data)->show_new_folder_dialog();
}

void MainWindow::cb_new_note_clicked(GtkButton*, gpointer data) {
    static_cast<MainWindow*>(data)->vm_->create_note();
}

void MainWindow::cb_delete_note_clicked(GtkButton*, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    const auto& id = self->vm_->selected_note_id();
    if (!id.empty()) self->vm_->delete_note(id);
}

void MainWindow::cb_settings_clicked(GtkButton*, gpointer data) {
    static_cast<MainWindow*>(data)->show_settings_dialog();
}

void MainWindow::cb_folder_right_click(
    GtkGestureClick* gesture, int, double x, double y, gpointer)
{
    auto* mw = static_cast<MainWindow*>(
        g_object_get_data(G_OBJECT(gesture), "mw"));
    const char* folder_id =
        (const char*)g_object_get_data(G_OBJECT(gesture), "folder-id");

    if (!mw || !folder_id ||
        strcmp(folder_id, NotesViewModel::ALL_NOTES_ID) == 0) return;

    GtkWidget* row = gtk_event_controller_get_widget(GTK_EVENT_CONTROLLER(gesture));
    mw->show_folder_context_menu(row, folder_id, x, y);
}

void MainWindow::cb_note_right_click(
    GtkGestureClick* gesture, int, double x, double y, gpointer)
{
    auto* mw = static_cast<MainWindow*>(
        g_object_get_data(G_OBJECT(gesture), "mw"));
    const char* note_id =
        (const char*)g_object_get_data(G_OBJECT(gesture), "note-id");
    if (!mw || !note_id) return;

    GtkWidget* row = gtk_event_controller_get_widget(GTK_EVENT_CONTROLLER(gesture));
    mw->show_note_context_menu(row, note_id, x, y);
}

void MainWindow::folder_header_func(GtkListBoxRow* row, GtkListBoxRow* before, gpointer) {
    if (!before) {
        gtk_list_box_row_set_header(row, nullptr);
        return;
    }
    const char* before_id =
        (const char*)g_object_get_data(G_OBJECT(before), "folder-id");
    if (before_id && strcmp(before_id, NotesViewModel::ALL_NOTES_ID) == 0) {
        GtkWidget* label = gtk_label_new("Folders");
        gtk_label_set_xalign(GTK_LABEL(label), 0.0f);
        gtk_widget_set_margin_start(label, 12);
        gtk_widget_set_margin_end(label, 12);
        gtk_widget_set_margin_top(label, 8);
        gtk_widget_set_margin_bottom(label, 4);
        gtk_widget_add_css_class(label, "caption");
        gtk_widget_add_css_class(label, "dim-label");
        gtk_list_box_row_set_header(row, label);
    } else {
        gtk_list_box_row_set_header(row, nullptr);
    }
}

// ── Dialogs ───────────────────────────────────────────────────────────────────

void MainWindow::show_settings_dialog() {
    GtkWidget* dialog = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(dialog), "Server Settings");
    gtk_window_set_transient_for(GTK_WINDOW(dialog), GTK_WINDOW(window_));
    gtk_window_set_modal(GTK_WINDOW(dialog), TRUE);
    gtk_window_set_resizable(GTK_WINDOW(dialog), FALSE);
    gtk_window_set_default_size(GTK_WINDOW(dialog), 400, -1);

    GtkWidget* vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
    gtk_widget_set_margin_start(vbox, 20);
    gtk_widget_set_margin_end(vbox, 20);
    gtk_widget_set_margin_top(vbox, 20);
    gtk_widget_set_margin_bottom(vbox, 20);
    gtk_window_set_child(GTK_WINDOW(dialog), vbox);

    GtkWidget* lbl = gtk_label_new("WebSocket URL");
    gtk_label_set_xalign(GTK_LABEL(lbl), 0.0f);
    gtk_box_append(GTK_BOX(vbox), lbl);

    GtkWidget* entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(entry), "ws://hostname:8080/ws");
    gtk_entry_buffer_set_text(gtk_entry_get_buffer(GTK_ENTRY(entry)),
        vm_->server_address().c_str(), -1);
    gtk_box_append(GTK_BOX(vbox), entry);

    GtkWidget* btn_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_widget_set_halign(btn_row, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(vbox), btn_row);

    GtkWidget* cancel_btn  = gtk_button_new_with_mnemonic("_Cancel");
    GtkWidget* connect_btn = gtk_button_new_with_mnemonic("_Connect");
    gtk_widget_add_css_class(connect_btn, "suggested-action");
    gtk_box_append(GTK_BOX(btn_row), cancel_btn);
    gtk_box_append(GTK_BOX(btn_row), connect_btn);

    g_object_set_data(G_OBJECT(dialog), "entry", entry);
    g_object_set_data(G_OBJECT(dialog), "vm",    vm_.get());

    g_signal_connect(cancel_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            gtk_window_destroy(GTK_WINDOW(d));
        }), dialog);

    g_signal_connect(connect_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            auto* dlg  = GTK_WIDGET(d);
            auto* ent  = GTK_WIDGET(g_object_get_data(G_OBJECT(dlg), "entry"));
            auto* vm   = static_cast<NotesViewModel*>(g_object_get_data(G_OBJECT(dlg), "vm"));
            const char* text = gtk_entry_buffer_get_text(
                gtk_entry_get_buffer(GTK_ENTRY(ent)));
            if (text && *text) vm->set_server_address(text);
            gtk_window_destroy(GTK_WINDOW(dlg));
        }), dialog);

    gtk_window_present(GTK_WINDOW(dialog));
}

void MainWindow::show_new_folder_dialog() {
    GtkWidget* dialog = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(dialog), "New Folder");
    gtk_window_set_transient_for(GTK_WINDOW(dialog), GTK_WINDOW(window_));
    gtk_window_set_modal(GTK_WINDOW(dialog), TRUE);
    gtk_window_set_resizable(GTK_WINDOW(dialog), FALSE);
    gtk_window_set_default_size(GTK_WINDOW(dialog), 320, -1);

    GtkWidget* vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
    gtk_widget_set_margin_start(vbox, 20);
    gtk_widget_set_margin_end(vbox, 20);
    gtk_widget_set_margin_top(vbox, 20);
    gtk_widget_set_margin_bottom(vbox, 20);
    gtk_window_set_child(GTK_WINDOW(dialog), vbox);

    GtkWidget* lbl = gtk_label_new("Folder name");
    gtk_label_set_xalign(GTK_LABEL(lbl), 0.0f);
    gtk_box_append(GTK_BOX(vbox), lbl);

    GtkWidget* entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(entry), "Folder name");
    gtk_box_append(GTK_BOX(vbox), entry);

    GtkWidget* btn_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_widget_set_halign(btn_row, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(vbox), btn_row);

    GtkWidget* cancel_btn = gtk_button_new_with_mnemonic("_Cancel");
    GtkWidget* create_btn = gtk_button_new_with_mnemonic("C_reate");
    gtk_widget_add_css_class(create_btn, "suggested-action");
    gtk_box_append(GTK_BOX(btn_row), cancel_btn);
    gtk_box_append(GTK_BOX(btn_row), create_btn);

    g_object_set_data(G_OBJECT(dialog), "entry", entry);
    g_object_set_data(G_OBJECT(dialog), "vm",    vm_.get());

    g_signal_connect(cancel_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            gtk_window_destroy(GTK_WINDOW(d));
        }), dialog);

    g_signal_connect(create_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            auto* dlg  = GTK_WIDGET(d);
            auto* ent  = GTK_WIDGET(g_object_get_data(G_OBJECT(dlg), "entry"));
            auto* vm   = static_cast<NotesViewModel*>(g_object_get_data(G_OBJECT(dlg), "vm"));
            const char* text = gtk_entry_buffer_get_text(
                gtk_entry_get_buffer(GTK_ENTRY(ent)));
            if (text && *text) vm->create_folder(text);
            gtk_window_destroy(GTK_WINDOW(dlg));
        }), dialog);

    gtk_window_present(GTK_WINDOW(dialog));
}

void MainWindow::show_rename_folder_dialog(
    const std::string& folder_id, const std::string& current_name)
{
    GtkWidget* dialog = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(dialog), "Rename Folder");
    gtk_window_set_transient_for(GTK_WINDOW(dialog), GTK_WINDOW(window_));
    gtk_window_set_modal(GTK_WINDOW(dialog), TRUE);
    gtk_window_set_resizable(GTK_WINDOW(dialog), FALSE);
    gtk_window_set_default_size(GTK_WINDOW(dialog), 320, -1);

    GtkWidget* vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
    gtk_widget_set_margin_start(vbox, 20);
    gtk_widget_set_margin_end(vbox, 20);
    gtk_widget_set_margin_top(vbox, 20);
    gtk_widget_set_margin_bottom(vbox, 20);
    gtk_window_set_child(GTK_WINDOW(dialog), vbox);

    GtkWidget* lbl = gtk_label_new("New folder name");
    gtk_label_set_xalign(GTK_LABEL(lbl), 0.0f);
    gtk_box_append(GTK_BOX(vbox), lbl);

    GtkWidget* entry = gtk_entry_new();
    gtk_entry_buffer_set_text(gtk_entry_get_buffer(GTK_ENTRY(entry)),
        current_name.c_str(), -1);
    gtk_box_append(GTK_BOX(vbox), entry);

    GtkWidget* btn_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_widget_set_halign(btn_row, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(vbox), btn_row);

    GtkWidget* cancel_btn = gtk_button_new_with_mnemonic("_Cancel");
    GtkWidget* rename_btn = gtk_button_new_with_mnemonic("_Rename");
    gtk_widget_add_css_class(rename_btn, "suggested-action");
    gtk_box_append(GTK_BOX(btn_row), cancel_btn);
    gtk_box_append(GTK_BOX(btn_row), rename_btn);

    g_object_set_data(G_OBJECT(dialog), "entry",     entry);
    g_object_set_data(G_OBJECT(dialog), "vm",        vm_.get());
    g_object_set_data_full(G_OBJECT(dialog), "folder-id",
        g_strdup(folder_id.c_str()), g_free);

    g_signal_connect(cancel_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            gtk_window_destroy(GTK_WINDOW(d));
        }), dialog);

    g_signal_connect(rename_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            auto* dlg = GTK_WIDGET(d);
            auto* ent = GTK_WIDGET(g_object_get_data(G_OBJECT(dlg), "entry"));
            auto* vm  = static_cast<NotesViewModel*>(g_object_get_data(G_OBJECT(dlg), "vm"));
            const char* fid  = (const char*)g_object_get_data(G_OBJECT(dlg), "folder-id");
            const char* text = gtk_entry_buffer_get_text(
                gtk_entry_get_buffer(GTK_ENTRY(ent)));
            if (text && *text && fid) vm->rename_folder(fid, text);
            gtk_window_destroy(GTK_WINDOW(dlg));
        }), dialog);

    gtk_window_present(GTK_WINDOW(dialog));
}

void MainWindow::show_folder_context_menu(
    GtkWidget* row, const char* folder_id, double x, double y)
{
    GtkWidget* popover = gtk_popover_new();
    gtk_widget_set_parent(popover, row);
    gtk_popover_set_has_arrow(GTK_POPOVER(popover), FALSE);

    GdkRectangle rect = {(int)x, (int)y, 1, 1};
    gtk_popover_set_pointing_to(GTK_POPOVER(popover), &rect);

    GtkWidget* vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);
    gtk_widget_set_margin_start(vbox, 4);
    gtk_widget_set_margin_end(vbox, 4);
    gtk_widget_set_margin_top(vbox, 4);
    gtk_widget_set_margin_bottom(vbox, 4);
    gtk_popover_set_child(GTK_POPOVER(popover), vbox);

    GtkWidget* rename_btn = gtk_button_new_with_label("Rename…");
    gtk_button_set_has_frame(GTK_BUTTON(rename_btn), FALSE);
    gtk_widget_set_halign(rename_btn, GTK_ALIGN_FILL);
    gtk_box_append(GTK_BOX(vbox), rename_btn);

    GtkWidget* delete_btn = gtk_button_new_with_label("Delete Folder");
    gtk_button_set_has_frame(GTK_BUTTON(delete_btn), FALSE);
    gtk_widget_add_css_class(delete_btn, "destructive-action");
    gtk_widget_set_halign(delete_btn, GTK_ALIGN_FILL);
    gtk_box_append(GTK_BOX(vbox), delete_btn);

    g_object_set_data(G_OBJECT(popover), "mw", this);
    g_object_set_data_full(G_OBJECT(popover), "folder-id",
        g_strdup(folder_id), g_free);

    g_signal_connect(rename_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            auto* pop = GTK_WIDGET(d);
            auto* mw  = static_cast<MainWindow*>(g_object_get_data(G_OBJECT(pop), "mw"));
            const char* fid = (const char*)g_object_get_data(G_OBJECT(pop), "folder-id");
            gtk_popover_popdown(GTK_POPOVER(pop));
            // Find the current folder name
            const auto& folders = mw->vm_->folders();
            for (const auto& f : folders) {
                if (f.id == fid) {
                    mw->show_rename_folder_dialog(fid, f.name);
                    return;
                }
            }
            mw->show_rename_folder_dialog(fid, "");
        }), popover);

    g_signal_connect(delete_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            auto* pop = GTK_WIDGET(d);
            auto* mw  = static_cast<MainWindow*>(g_object_get_data(G_OBJECT(pop), "mw"));
            const char* fid = (const char*)g_object_get_data(G_OBJECT(pop), "folder-id");
            gtk_popover_popdown(GTK_POPOVER(pop));
            if (fid) mw->vm_->delete_folder(fid);
        }), popover);

    // Clean up popover when it closes (it is parented to the row)
    g_signal_connect(popover, "closed",
        G_CALLBACK(+[](GtkPopover* pop, gpointer) {
            gtk_widget_unparent(GTK_WIDGET(pop));
        }), nullptr);

    gtk_popover_popup(GTK_POPOVER(popover));
}

void MainWindow::show_note_context_menu(
    GtkWidget* row, const char* note_id, double x, double y)
{
    GtkWidget* popover = gtk_popover_new();
    gtk_widget_set_parent(popover, row);
    gtk_popover_set_has_arrow(GTK_POPOVER(popover), FALSE);

    GdkRectangle rect = {(int)x, (int)y, 1, 1};
    gtk_popover_set_pointing_to(GTK_POPOVER(popover), &rect);

    GtkWidget* vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);
    gtk_widget_set_margin_start(vbox, 4);
    gtk_widget_set_margin_end(vbox, 4);
    gtk_widget_set_margin_top(vbox, 4);
    gtk_widget_set_margin_bottom(vbox, 4);
    gtk_popover_set_child(GTK_POPOVER(popover), vbox);

    GtkWidget* delete_btn = gtk_button_new_with_label("Delete Note");
    gtk_button_set_has_frame(GTK_BUTTON(delete_btn), FALSE);
    gtk_widget_add_css_class(delete_btn, "destructive-action");
    gtk_widget_set_halign(delete_btn, GTK_ALIGN_FILL);
    gtk_box_append(GTK_BOX(vbox), delete_btn);

    g_object_set_data(G_OBJECT(popover), "mw", this);
    g_object_set_data_full(G_OBJECT(popover), "note-id",
        g_strdup(note_id), g_free);

    g_signal_connect(delete_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            auto* pop = GTK_WIDGET(d);
            auto* mw  = static_cast<MainWindow*>(g_object_get_data(G_OBJECT(pop), "mw"));
            const char* nid = (const char*)g_object_get_data(G_OBJECT(pop), "note-id");
            gtk_popover_popdown(GTK_POPOVER(pop));
            if (nid) mw->vm_->delete_note(nid);
        }), popover);

    g_signal_connect(popover, "closed",
        G_CALLBACK(+[](GtkPopover* pop, gpointer) {
            gtk_widget_unparent(GTK_WIDGET(pop));
        }), nullptr);

    gtk_popover_popup(GTK_POPOVER(popover));
}
