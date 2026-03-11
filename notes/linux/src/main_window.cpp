#include "main_window.h"

#include <algorithm>
#include <cstring>
#include <ctime>
#include <string>
#include <vector>

#include <gtk/gtk.h>
#include <pango/pango.h>

static constexpr int WINDOW_W       = 1050;
static constexpr int WINDOW_H       = 700;
static constexpr int FOLDER_PANEL_W = 200;
static constexpr int NOTE_PANEL_W   = 280;
static constexpr int AUTH_WIDTH     = 400;

// ── Text helpers ──────────────────────────────────────────────────────────────

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
    std::string result = content.substr(s, 80);
    // Replace newlines so the label stays single-line
    for (char& c : result)
        if (c == '\n' || c == '\r') c = ' ';
    return result;
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
    struct tm ytm{};
    localtime_r(&yesterday, &ytm);
    if (note_tm.tm_year == ytm.tm_year && note_tm.tm_yday == ytm.tm_yday)
        return "Yesterday";

    strftime(buf, sizeof(buf), "%b %e, %Y", &note_tm);
    return buf;
}

// ── Row factories ─────────────────────────────────────────────────────────────

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

// show_folder_label: show folder name on note rows when in "All Notes" view
static GtkWidget* make_note_row(
    const Note& note, const std::string& folder_name, bool show_folder)
{
    GtkWidget* row = gtk_list_box_row_new();
    g_object_set_data_full(G_OBJECT(row), "note-id",
        g_strdup(note.id.c_str()), g_free);

    GtkWidget* vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);
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
    gtk_label_set_single_line_mode(GTK_LABEL(title_lbl), TRUE);
    gtk_widget_set_hexpand(title_lbl, TRUE);
    gtk_widget_add_css_class(title_lbl, "caption-heading");
    gtk_box_append(GTK_BOX(hbox), title_lbl);

    GtkWidget* date_lbl = gtk_label_new(format_note_date(note.updated_at).c_str());
    gtk_label_set_single_line_mode(GTK_LABEL(date_lbl), TRUE);
    gtk_widget_add_css_class(date_lbl, "caption");
    gtk_widget_add_css_class(date_lbl, "dim-label");
    gtk_box_append(GTK_BOX(hbox), date_lbl);

    // Preview — single line, newlines already stripped by note_preview()
    GtkWidget* preview_lbl = gtk_label_new(note_preview(note.content).c_str());
    gtk_label_set_xalign(GTK_LABEL(preview_lbl), 0.0f);
    gtk_label_set_ellipsize(GTK_LABEL(preview_lbl), PANGO_ELLIPSIZE_END);
    gtk_label_set_single_line_mode(GTK_LABEL(preview_lbl), TRUE);
    gtk_widget_add_css_class(preview_lbl, "caption");
    gtk_widget_add_css_class(preview_lbl, "dim-label");
    gtk_box_append(GTK_BOX(vbox), preview_lbl);

    // Folder label — always rendered to keep row height constant.
    // Use a non-empty placeholder (" ") when hidden so GTK allocates the same height.
    std::string flabel = show_folder
        ? (folder_name.empty() ? "—" : folder_name)
        : " ";
    GtkWidget* folder_lbl = gtk_label_new(flabel.c_str());
    gtk_label_set_xalign(GTK_LABEL(folder_lbl), 0.0f);
    gtk_label_set_ellipsize(GTK_LABEL(folder_lbl), PANGO_ELLIPSIZE_END);
    gtk_label_set_single_line_mode(GTK_LABEL(folder_lbl), TRUE);
    gtk_widget_add_css_class(folder_lbl, "caption");
    gtk_widget_add_css_class(folder_lbl, "dim-label");
    if (!show_folder) gtk_widget_set_opacity(folder_lbl, 0.0);
    gtk_box_append(GTK_BOX(vbox), folder_lbl);

    return row;
}

// ── MainWindow constructor ────────────────────────────────────────────────────

MainWindow::MainWindow(GtkApplication* app) {

    // ── View model ────────────────────────────────────────────────────────────
    vm_ = std::make_unique<NotesViewModel>(
        // on_auth_state
        [this](bool logged_in, std::string recovery_code) {
            if (logged_in) {
                show_main_page();
                if (!recovery_code.empty())
                    show_recovery_code_dialog(recovery_code);
            } else {
                show_auth_page({});
            }
        },
        // on_auth_error
        [this](std::string error) {
            set_auth_error(error.c_str());
        },
        [this]()                                            { rebuild_folder_list(); },
        [this]()                                            { rebuild_note_list(); },
        [this](const Note* n)                               { load_note(n); },
        [this](const std::string& t, const std::string& c) { update_editor_text(t, c); },
        [this](bool connected)                              { update_status(connected); }
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
        "listbox { background: transparent; }"
        ".auth-title { font-size: 28px; font-weight: bold; margin-bottom: 4px; }"
        ".auth-subtitle { font-size: 13px; margin-bottom: 24px; }"
        ".auth-mode-btn { border-radius: 0; }"
        ".error-label { color: #EF4444; font-size: 13px; }");
    gtk_style_context_add_provider_for_display(
        gdk_display_get_default(),
        GTK_STYLE_PROVIDER(css),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
    g_object_unref(css);

    // ── Root stack (auth | main) ───────────────────────────────────────────────
    root_stack_ = gtk_stack_new();
    gtk_stack_set_transition_type(GTK_STACK(root_stack_), GTK_STACK_TRANSITION_TYPE_CROSSFADE);
    gtk_stack_add_named(GTK_STACK(root_stack_), build_auth_page(), "auth");
    gtk_stack_add_named(GTK_STACK(root_stack_), build_main_page(), "main");

    gtk_window_set_child(GTK_WINDOW(window_), root_stack_);

    // The VM constructor may have fired on_auth_state_ while widgets were still
    // null (show_main_page / show_auth_page were no-ops at that point).
    // Now that everything is built, set the correct initial page explicitly.
    if (vm_->is_logged_in()) {
        show_main_page();
    } else {
        gtk_stack_set_visible_child_name(GTK_STACK(root_stack_), "auth");
        if (auth_server_entry_)
            gtk_editable_set_text(GTK_EDITABLE(auth_server_entry_),
                vm_->server_address().c_str());
    }

    gtk_window_present(GTK_WINDOW(window_));
}

// ── Auth page ─────────────────────────────────────────────────────────────────

GtkWidget* MainWindow::build_auth_page() {
    // Outer centering box
    GtkWidget* outer = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_widget_set_valign(outer, GTK_ALIGN_CENTER);
    gtk_widget_set_halign(outer, GTK_ALIGN_CENTER);

    GtkWidget* card = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_widget_set_size_request(card, AUTH_WIDTH, -1);
    gtk_widget_set_margin_start(card, 20);
    gtk_widget_set_margin_end(card, 20);
    gtk_box_append(GTK_BOX(outer), card);

    // Title
    GtkWidget* title = gtk_label_new("Amadeuz");
    gtk_widget_add_css_class(title, "auth-title");
    gtk_widget_set_halign(title, GTK_ALIGN_CENTER);
    gtk_box_append(GTK_BOX(card), title);

    GtkWidget* subtitle = gtk_label_new("Your notes, your server.");
    gtk_widget_add_css_class(subtitle, "auth-subtitle");
    gtk_widget_add_css_class(subtitle, "dim-label");
    gtk_widget_set_halign(subtitle, GTK_ALIGN_CENTER);
    gtk_box_append(GTK_BOX(card), subtitle);

    // Mode switcher: Login | Register | Recover
    GtkWidget* mode_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
    gtk_widget_set_halign(mode_row, GTK_ALIGN_CENTER);
    gtk_widget_set_margin_bottom(mode_row, 20);
    gtk_box_append(GTK_BOX(card), mode_row);

    auto make_mode_btn = [](const char* label) {
        GtkWidget* btn = gtk_toggle_button_new_with_label(label);
        gtk_widget_add_css_class(btn, "auth-mode-btn");
        return btn;
    };
    GtkWidget* btn_login    = make_mode_btn("Log In");
    GtkWidget* btn_register = make_mode_btn("Register");
    GtkWidget* btn_recover  = make_mode_btn("Recover");
    gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(btn_login), TRUE);
    gtk_box_append(GTK_BOX(mode_row), btn_login);
    gtk_box_append(GTK_BOX(mode_row), btn_register);
    gtk_box_append(GTK_BOX(mode_row), btn_recover);
    g_signal_connect(btn_login,    "clicked", G_CALLBACK(cb_mode_login),    this);
    g_signal_connect(btn_register, "clicked", G_CALLBACK(cb_mode_register), this);
    g_signal_connect(btn_recover,  "clicked", G_CALLBACK(cb_mode_recover),  this);

    // Forms stack
    auth_mode_stack_ = gtk_stack_new();
    gtk_stack_set_transition_type(GTK_STACK(auth_mode_stack_), GTK_STACK_TRANSITION_TYPE_NONE);
    gtk_box_append(GTK_BOX(card), auth_mode_stack_);

    // Shared email entry
    auth_email_entry_ = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(auth_email_entry_), "Email");
    gtk_entry_set_input_purpose(GTK_ENTRY(auth_email_entry_), GTK_INPUT_PURPOSE_EMAIL);

    // Shared password entry
    auth_password_entry_ = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(auth_password_entry_), "Password");
    gtk_entry_set_visibility(GTK_ENTRY(auth_password_entry_), FALSE);
    gtk_entry_set_input_purpose(GTK_ENTRY(auth_password_entry_), GTK_INPUT_PURPOSE_PASSWORD);

    // Recover: new password
    auth_new_password_entry_ = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(auth_new_password_entry_), "New password");
    gtk_entry_set_visibility(GTK_ENTRY(auth_new_password_entry_), FALSE);
    gtk_entry_set_input_purpose(GTK_ENTRY(auth_new_password_entry_), GTK_INPUT_PURPOSE_PASSWORD);

    // Recover: recovery code
    auth_code_entry_ = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(auth_code_entry_), "Recovery code");

    // ── Login form ─────────────────────────────────────────────────────────────
    {
        GtkWidget* form = gtk_box_new(GTK_ORIENTATION_VERTICAL, 10);
        gtk_box_append(GTK_BOX(form), auth_email_entry_);
        gtk_box_append(GTK_BOX(form), auth_password_entry_);
        auth_login_btn_ = gtk_button_new_with_mnemonic("_Log In");
        gtk_widget_add_css_class(auth_login_btn_, "suggested-action");
        g_signal_connect(auth_login_btn_, "clicked", G_CALLBACK(cb_login_btn), this);
        gtk_box_append(GTK_BOX(form), auth_login_btn_);
        gtk_stack_add_named(GTK_STACK(auth_mode_stack_), form, "login");
    }

    // ── Register form ──────────────────────────────────────────────────────────
    {
        GtkWidget* form = gtk_box_new(GTK_ORIENTATION_VERTICAL, 10);
        // Use cloned entries for register so they're independent widgets
        GtkWidget* email2 = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(email2), "Email");
        gtk_entry_set_input_purpose(GTK_ENTRY(email2), GTK_INPUT_PURPOSE_EMAIL);
        GtkWidget* pass2 = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(pass2), "Password");
        gtk_entry_set_visibility(GTK_ENTRY(pass2), FALSE);
        g_object_set_data(G_OBJECT(form), "email", email2);
        g_object_set_data(G_OBJECT(form), "password", pass2);
        gtk_box_append(GTK_BOX(form), email2);
        gtk_box_append(GTK_BOX(form), pass2);
        auth_register_btn_ = gtk_button_new_with_mnemonic("_Register");
        gtk_widget_add_css_class(auth_register_btn_, "suggested-action");
        g_signal_connect(auth_register_btn_, "clicked", G_CALLBACK(cb_register_btn), this);
        gtk_box_append(GTK_BOX(form), auth_register_btn_);
        gtk_stack_add_named(GTK_STACK(auth_mode_stack_), form, "register");
    }

    // ── Recover form ───────────────────────────────────────────────────────────
    {
        GtkWidget* form = gtk_box_new(GTK_ORIENTATION_VERTICAL, 10);
        GtkWidget* email3 = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(email3), "Email");
        GtkWidget* code3 = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(code3), "Recovery code");
        GtkWidget* pass3 = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(pass3), "New password");
        gtk_entry_set_visibility(GTK_ENTRY(pass3), FALSE);
        g_object_set_data(G_OBJECT(form), "email",    email3);
        g_object_set_data(G_OBJECT(form), "code",     code3);
        g_object_set_data(G_OBJECT(form), "password", pass3);
        gtk_box_append(GTK_BOX(form), email3);
        gtk_box_append(GTK_BOX(form), code3);
        gtk_box_append(GTK_BOX(form), pass3);
        auth_recover_btn_ = gtk_button_new_with_mnemonic("_Recover Account");
        gtk_widget_add_css_class(auth_recover_btn_, "suggested-action");
        g_signal_connect(auth_recover_btn_, "clicked", G_CALLBACK(cb_recover_btn), this);
        gtk_box_append(GTK_BOX(form), auth_recover_btn_);
        gtk_stack_add_named(GTK_STACK(auth_mode_stack_), form, "recover");
    }

    gtk_stack_set_visible_child_name(GTK_STACK(auth_mode_stack_), "login");

    // Error label
    auth_error_label_ = gtk_label_new("");
    gtk_widget_add_css_class(auth_error_label_, "error-label");
    gtk_label_set_wrap(GTK_LABEL(auth_error_label_), TRUE);
    gtk_widget_set_margin_top(auth_error_label_, 8);
    gtk_box_append(GTK_BOX(card), auth_error_label_);

    // Server address
    GtkWidget* sep = gtk_separator_new(GTK_ORIENTATION_HORIZONTAL);
    gtk_widget_set_margin_top(sep, 20);
    gtk_widget_set_margin_bottom(sep, 12);
    gtk_box_append(GTK_BOX(card), sep);

    GtkWidget* srv_lbl = gtk_label_new("Server");
    gtk_label_set_xalign(GTK_LABEL(srv_lbl), 0.0f);
    gtk_widget_add_css_class(srv_lbl, "caption");
    gtk_widget_add_css_class(srv_lbl, "dim-label");
    gtk_box_append(GTK_BOX(card), srv_lbl);

    auth_server_entry_ = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(auth_server_entry_), "http://hostname:8080");
    gtk_widget_set_margin_bottom(auth_server_entry_, 20);
    gtk_box_append(GTK_BOX(card), auth_server_entry_);

    return outer;
}

// ── Main page ─────────────────────────────────────────────────────────────────

GtkWidget* MainWindow::build_main_page() {
    // ── Header bar ────────────────────────────────────────────────────────────
    GtkWidget* header = gtk_header_bar_new();
    gtk_header_bar_set_show_title_buttons(GTK_HEADER_BAR(header), TRUE);

    status_label_ = gtk_label_new(nullptr);
    update_status(false);

    GtkWidget* sign_out_btn = gtk_button_new_with_label("Sign Out");
    gtk_button_set_has_frame(GTK_BUTTON(sign_out_btn), FALSE);
    g_signal_connect(sign_out_btn, "clicked", G_CALLBACK(cb_sign_out_clicked), this);

    GtkWidget* settings_btn = gtk_button_new_with_label("Settings…");
    gtk_button_set_has_frame(GTK_BUTTON(settings_btn), FALSE);
    g_signal_connect(settings_btn, "clicked", G_CALLBACK(cb_settings_clicked), this);

    GtkWidget* end_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_box_append(GTK_BOX(end_box), status_label_);
    gtk_box_append(GTK_BOX(end_box), settings_btn);
    gtk_box_append(GTK_BOX(end_box), sign_out_btn);
    gtk_header_bar_pack_end(GTK_HEADER_BAR(header), end_box);

    // We need to attach the header to the window later; build returns the body only.
    // Store it so show_main_page can attach it.
    g_object_set_data(G_OBJECT(window_), "main-header", header);

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

    // Header row: label + delete + new note
    GtkWidget* note_header_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);
    gtk_widget_set_margin_start(note_header_row, 12);
    gtk_widget_set_margin_end(note_header_row, 8);
    gtk_widget_set_margin_top(note_header_row, 8);
    gtk_widget_set_margin_bottom(note_header_row, 4);
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
    gtk_box_append(GTK_BOX(note_header_row), new_note_btn_);
    g_signal_connect(new_note_btn_, "clicked", G_CALLBACK(cb_new_note_clicked), this);

    // Search bar
    search_entry_ = gtk_search_entry_new();
    gtk_search_entry_set_placeholder_text(GTK_SEARCH_ENTRY(search_entry_), "Search…");
    gtk_widget_set_margin_start(search_entry_, 8);
    gtk_widget_set_margin_end(search_entry_, 8);
    gtk_widget_set_margin_bottom(search_entry_, 6);
    gtk_box_append(GTK_BOX(note_panel), search_entry_);
    g_signal_connect(search_entry_, "search-changed", G_CALLBACK(cb_search_changed), this);

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

    GtkWidget* empty_label = gtk_label_new("Select a note to start editing");
    gtk_widget_add_css_class(empty_label, "dim-label");
    gtk_widget_add_css_class(empty_label, "title-3");
    gtk_widget_set_valign(empty_label, GTK_ALIGN_CENTER);
    gtk_widget_set_halign(empty_label, GTK_ALIGN_CENTER);
    gtk_stack_add_named(GTK_STACK(editor_stack_), empty_label, "empty");

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

    content_view_ = gtk_text_view_new();
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(content_view_), GTK_WRAP_WORD_CHAR);
    gtk_text_view_set_left_margin(GTK_TEXT_VIEW(content_view_), 16);
    gtk_text_view_set_right_margin(GTK_TEXT_VIEW(content_view_), 16);
    gtk_text_view_set_top_margin(GTK_TEXT_VIEW(content_view_), 12);
    gtk_text_view_set_bottom_margin(GTK_TEXT_VIEW(content_view_), 12);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(content_scroll), content_view_);
    content_buffer_ = gtk_text_view_get_buffer(GTK_TEXT_VIEW(content_view_));
    ensure_markdown_tags();
    g_signal_connect(content_buffer_, "changed", G_CALLBACK(cb_content_changed), this);

    // Drop target for images
    GtkDropTarget* drop_target = gtk_drop_target_new(GDK_TYPE_FILE_LIST, GDK_ACTION_COPY);
    g_signal_connect(drop_target, "drop", G_CALLBACK(cb_image_drop), this);
    gtk_widget_add_controller(content_view_, GTK_EVENT_CONTROLLER(drop_target));

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

    rebuild_folder_list();
    rebuild_note_list();

    return main_paned;
}

// ── Page transitions ──────────────────────────────────────────────────────────

void MainWindow::show_auth_page(const std::string&) {
    // Detach main header; restore no-titlebar
    GtkWidget* main_header = (GtkWidget*)g_object_get_data(G_OBJECT(window_), "main-header");
    if (gtk_window_get_titlebar(GTK_WINDOW(window_)) == main_header)
        gtk_window_set_titlebar(GTK_WINDOW(window_), nullptr);

    gtk_stack_set_visible_child_name(GTK_STACK(root_stack_), "auth");
    if (auth_server_entry_)
        gtk_editable_set_text(GTK_EDITABLE(auth_server_entry_),
            vm_->server_address().c_str());
    set_auth_error("");
}

void MainWindow::show_main_page() {
    GtkWidget* main_header = (GtkWidget*)g_object_get_data(G_OBJECT(window_), "main-header");
    if (main_header && gtk_window_get_titlebar(GTK_WINDOW(window_)) != main_header)
        gtk_window_set_titlebar(GTK_WINDOW(window_), main_header);

    gtk_stack_set_visible_child_name(GTK_STACK(root_stack_), "main");
    rebuild_folder_list();
    rebuild_note_list();
}

// ── Auth UI helpers ───────────────────────────────────────────────────────────

void MainWindow::switch_auth_mode(const char* mode) {
    if (auth_mode_stack_)
        gtk_stack_set_visible_child_name(GTK_STACK(auth_mode_stack_), mode);
    set_auth_error("");
}

void MainWindow::set_auth_error(const char* msg) {
    if (auth_error_label_)
        gtk_label_set_text(GTK_LABEL(auth_error_label_), msg ? msg : "");
}

// ── UI rebuilds ───────────────────────────────────────────────────────────────

void MainWindow::rebuild_folder_list() {
    if (!folder_list_box_) return;
    suppress_folder_selection_ = true;

    std::vector<GtkWidget*> to_remove;
    for (int i = 0; ; i++) {
        GtkListBoxRow* row = gtk_list_box_get_row_at_index(
            GTK_LIST_BOX(folder_list_box_), i);
        if (!row) break;
        to_remove.push_back(GTK_WIDGET(row));
    }
    for (auto* w : to_remove)
        gtk_list_box_remove(GTK_LIST_BOX(folder_list_box_), w);

    gtk_list_box_append(GTK_LIST_BOX(folder_list_box_), make_all_notes_row());

    for (const auto& folder : vm_->folders()) {
        GtkWidget* row = make_folder_row(folder);

        GtkGesture* gesture = gtk_gesture_click_new();
        gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), 3);
        g_object_set_data(G_OBJECT(gesture), "mw", this);
        g_object_set_data_full(G_OBJECT(gesture), "folder-id",
            g_strdup(folder.id.c_str()), g_free);
        g_signal_connect(gesture, "pressed", G_CALLBACK(cb_folder_right_click), nullptr);
        gtk_widget_add_controller(row, GTK_EVENT_CONTROLLER(gesture));

        gtk_list_box_append(GTK_LIST_BOX(folder_list_box_), row);
    }

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
    if (!note_list_box_) return;
    suppress_note_selection_ = true;

    // Update header
    const auto& sel_folder = vm_->selected_folder_id();
    bool is_all = (sel_folder == NotesViewModel::ALL_NOTES_ID || sel_folder.empty());
    if (is_all) {
        gtk_label_set_text(GTK_LABEL(note_list_header_), "All Notes");
    } else {
        const auto& folders = vm_->folders();
        auto it = std::find_if(folders.begin(), folders.end(),
            [&](const Folder& f) { return f.id == sel_folder; });
        gtk_label_set_text(GTK_LABEL(note_list_header_),
            it != folders.end() ? it->name.c_str() : "Notes");
    }

    // New note button always enabled (unfoldered notes allowed from All Notes)
    gtk_widget_set_sensitive(new_note_btn_, TRUE);

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

    // Build folder name lookup
    const auto& folders = vm_->folders();
    auto folder_name_for = [&](const std::string& fid) -> std::string {
        if (fid.empty()) return ""; // unfoldered
        for (const auto& f : folders)
            if (f.id == fid) return f.name;
        return "";
    };

    for (const auto& note : vm_->notes_in_view()) {
        std::string fname = folder_name_for(note.folder_id);
        GtkWidget* row = make_note_row(note, fname, is_all);

        GtkGesture* gesture = gtk_gesture_click_new();
        gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), 3);
        g_object_set_data(G_OBJECT(gesture), "mw", this);
        g_object_set_data_full(G_OBJECT(gesture), "note-id",
            g_strdup(note.id.c_str()), g_free);
        g_signal_connect(gesture, "pressed", G_CALLBACK(cb_note_right_click), nullptr);
        gtk_widget_add_controller(row, GTK_EVENT_CONTROLLER(gesture));

        gtk_list_box_append(GTK_LIST_BOX(note_list_box_), row);
    }

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
    blob_anchors_.clear();
    gtk_text_buffer_set_text(content_buffer_, note->content.c_str(), -1);
    render_blob_images();
    apply_markdown_full();
    suppress_content_changed_ = false;
}

void MainWindow::update_editor_text(const std::string& title, const std::string& content) {
    GtkTextMark* insert = gtk_text_buffer_get_insert(content_buffer_);
    GtkTextIter  cursor;
    gtk_text_buffer_get_iter_at_mark(content_buffer_, &cursor, insert);
    int offset = gtk_text_iter_get_offset(&cursor);

    suppress_title_changed_   = true;
    suppress_content_changed_ = true;

    gtk_editable_set_text(GTK_EDITABLE(title_entry_), title.c_str());
    blob_anchors_.clear();
    gtk_text_buffer_set_text(content_buffer_, content.c_str(), -1);
    render_blob_images();
    apply_markdown_full();

    suppress_title_changed_   = false;
    suppress_content_changed_ = false;

    GtkTextIter new_pos;
    gtk_text_buffer_get_iter_at_offset(content_buffer_, &new_pos,
        std::min(offset, gtk_text_buffer_get_char_count(content_buffer_)));
    gtk_text_buffer_place_cursor(content_buffer_, &new_pos);
}

void MainWindow::update_status(bool connected) {
    if (!status_label_) return;
    const char* markup = connected
        ? "<span foreground='#22C55E'>●</span>  "
          "<span foreground='#888888' size='small'>Synced</span>"
        : "<span foreground='#EF4444'>●</span>  "
          "<span foreground='#888888' size='small'>Offline</span>";
    gtk_label_set_markup(GTK_LABEL(status_label_), markup);
}

// ── Markdown styling ──────────────────────────────────────────────────────────

void MainWindow::ensure_markdown_tags() {
    if (markdown_tags_created_) return;
    markdown_tags_created_ = true;

    gtk_text_buffer_create_tag(content_buffer_, "h1",
        "weight", PANGO_WEIGHT_BOLD, "scale", 1.6, nullptr);
    gtk_text_buffer_create_tag(content_buffer_, "h2",
        "weight", PANGO_WEIGHT_BOLD, "scale", 1.3, nullptr);
    gtk_text_buffer_create_tag(content_buffer_, "h3",
        "weight", PANGO_WEIGHT_BOLD, "scale", 1.1, nullptr);
    gtk_text_buffer_create_tag(content_buffer_, "done",
        "strikethrough", TRUE, "foreground", "#888888", nullptr);
}

static const char* classify_line(const char* line) {
    if (strncmp(line, "### ", 4) == 0) return "h3";
    if (strncmp(line, "## ",  3) == 0) return "h2";
    if (strncmp(line, "# ",   2) == 0) return "h1";
    if (strncmp(line, "- [x] ", 6) == 0 || strncmp(line, "- [X] ", 6) == 0) return "done";
    return nullptr;
}

void MainWindow::apply_markdown_for_current_line() {
    GtkTextMark* insert = gtk_text_buffer_get_insert(content_buffer_);
    GtkTextIter  cursor;
    gtk_text_buffer_get_iter_at_mark(content_buffer_, &cursor, insert);

    GtkTextIter para_start = cursor, para_end = cursor;
    gtk_text_iter_set_line_offset(&para_start, 0);
    if (!gtk_text_iter_ends_line(&para_end))
        gtk_text_iter_forward_to_line_end(&para_end);

    for (const char* tag : {"h1", "h2", "h3", "done"})
        gtk_text_buffer_remove_tag_by_name(content_buffer_, tag, &para_start, &para_end);

    gchar* line = gtk_text_buffer_get_text(content_buffer_, &para_start, &para_end, FALSE);
    if (line) {
        const char* tag = classify_line(line);
        if (tag) gtk_text_buffer_apply_tag_by_name(content_buffer_, tag, &para_start, &para_end);
        g_free(line);
    }
}

void MainWindow::apply_markdown_full() {
    GtkTextIter start, end;
    gtk_text_buffer_get_bounds(content_buffer_, &start, &end);

    for (const char* tag : {"h1", "h2", "h3", "done"})
        gtk_text_buffer_remove_tag_by_name(content_buffer_, tag, &start, &end);

    GtkTextIter line_start = start;
    while (!gtk_text_iter_is_end(&line_start)) {
        GtkTextIter line_end = line_start;
        if (!gtk_text_iter_ends_line(&line_end))
            gtk_text_iter_forward_to_line_end(&line_end);

        gchar* line = gtk_text_buffer_get_text(content_buffer_, &line_start, &line_end, FALSE);
        if (line) {
            const char* tag = classify_line(line);
            if (tag) gtk_text_buffer_apply_tag_by_name(content_buffer_, tag, &line_start, &line_end);
            g_free(line);
        }
        if (!gtk_text_iter_forward_line(&line_start)) break;
    }
}

// ── Image blob helpers ────────────────────────────────────────────────────────

static std::string blob_dir_path() {
    return std::string(g_get_user_data_dir()) + "/amadeuz/blobs";
}

void MainWindow::insert_image_at_iter(GtkTextIter* iter, const std::string& uuid) {
    std::string path = blob_dir_path() + "/" + uuid;

    GError*    err = nullptr;
    GdkPixbuf* pb  = gdk_pixbuf_new_from_file(path.c_str(), &err);
    if (!pb) { g_clear_error(&err); return; }

    // Scale to max 400 px wide, preserving aspect ratio
    constexpr int MAX_W = 400;
    int orig_w = gdk_pixbuf_get_width(pb);
    int orig_h = gdk_pixbuf_get_height(pb);
    int disp_w, disp_h;
    GdkPixbuf* scaled;
    if (orig_w > MAX_W) {
        disp_h = (int)((double)orig_h * MAX_W / orig_w);
        disp_w = MAX_W;
        scaled = gdk_pixbuf_scale_simple(pb, disp_w, disp_h, GDK_INTERP_BILINEAR);
        g_object_unref(pb);
    } else {
        disp_w = orig_w;
        disp_h = orig_h;
        scaled = pb;
    }

    GdkTexture* texture = gdk_texture_new_for_pixbuf(scaled);
    g_object_unref(scaled);

    GtkTextChildAnchor* anchor = gtk_text_buffer_create_child_anchor(content_buffer_, iter);

    // GtkPicture renders images at the right size; explicit size_request ensures
    // the widget gets proper allocation inside the text view child anchor.
    GtkWidget* img = gtk_picture_new_for_paintable(GDK_PAINTABLE(texture));
    gtk_picture_set_can_shrink(GTK_PICTURE(img), FALSE);
    gtk_widget_set_size_request(img, disp_w, disp_h);
    gtk_widget_set_margin_top(img, 4);
    gtk_widget_set_margin_bottom(img, 4);
    g_object_unref(texture);

    gtk_text_view_add_child_at_anchor(GTK_TEXT_VIEW(content_view_), img, anchor);
    gtk_widget_set_visible(img, TRUE);

    blob_anchors_[anchor] = uuid;
}

void MainWindow::render_blob_images() {
    struct Match { int start_off, end_off; std::string uuid; };
    std::vector<Match> matches;

    static const char* PREFIX = "![](amadeuz://blob/";

    GtkTextIter search_from;
    gtk_text_buffer_get_start_iter(content_buffer_, &search_from);

    GtkTextIter ms, me;
    while (gtk_text_iter_forward_search(&search_from, PREFIX,
            GTK_TEXT_SEARCH_TEXT_ONLY, &ms, &me, nullptr)) {
        // me is right after PREFIX — find the closing ")"
        GtkTextIter cs, ce;
        if (!gtk_text_iter_forward_search(&me, ")",
                GTK_TEXT_SEARCH_TEXT_ONLY, &cs, &ce, nullptr))
            break;

        gchar* uuid = gtk_text_buffer_get_text(content_buffer_, &me, &cs, FALSE);
        if (uuid && *uuid) {
            matches.push_back({
                gtk_text_iter_get_offset(&ms),
                gtk_text_iter_get_offset(&ce),
                std::string(uuid)
            });
        }
        g_free(uuid);
        search_from = ce;
    }

    // Process in reverse so earlier offsets stay valid
    for (int i = (int)matches.size() - 1; i >= 0; --i) {
        const auto& m = matches[i];
        GtkTextIter del_start, del_end;
        gtk_text_buffer_get_iter_at_offset(content_buffer_, &del_start, m.start_off);
        gtk_text_buffer_get_iter_at_offset(content_buffer_, &del_end,   m.end_off);
        gtk_text_buffer_delete(content_buffer_, &del_start, &del_end);
        // del_start is now the insertion point
        insert_image_at_iter(&del_start, m.uuid);
    }
}

gboolean MainWindow::cb_image_drop(
    GtkDropTarget*, const GValue* value, double x, double y, gpointer data)
{
    auto* self = static_cast<MainWindow*>(data);
    if (!G_VALUE_HOLDS(value, GDK_TYPE_FILE_LIST)) return FALSE;

    GSList* files = (GSList*)g_value_get_boxed(value);
    if (!files) return FALSE;

    std::string bdir = blob_dir_path();
    g_mkdir_with_parents(bdir.c_str(), 0755);

    // Determine buffer position from drop coordinates
    GtkTextIter drop_iter;
    gtk_text_view_get_iter_at_location(
        GTK_TEXT_VIEW(self->content_view_), &drop_iter, (int)x, (int)y);

    bool handled = false;
    for (GSList* l = files; l; l = l->next) {
        GFile* src = G_FILE(l->data);

        // Check MIME type — only handle images
        GFileInfo* info = g_file_query_info(src,
            G_FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE,
            G_FILE_QUERY_INFO_NONE, nullptr, nullptr);
        if (!info) continue;
        const char* ct = g_file_info_get_content_type(info);
        bool is_image = ct && g_str_has_prefix(ct, "image/");
        g_object_unref(info);
        if (!is_image) continue;

        // Generate UUID and copy file to blobs dir
        gchar* uuid_raw = g_uuid_string_random();
        std::string uuid(uuid_raw);
        g_free(uuid_raw);

        std::string dst_path = bdir + "/" + uuid;
        GFile* dst = g_file_new_for_path(dst_path.c_str());
        GError* err = nullptr;
        gboolean ok = g_file_copy(src, dst, G_FILE_COPY_OVERWRITE,
            nullptr, nullptr, nullptr, &err);
        g_object_unref(dst);
        if (!ok) { g_clear_error(&err); continue; }

        // Insert newline before image if not at start of line
        if (gtk_text_iter_get_line_offset(&drop_iter) != 0)
            gtk_text_buffer_insert(self->content_buffer_, &drop_iter, "\n", -1);

        self->insert_image_at_iter(&drop_iter, uuid);

        // Insert newline after image
        gtk_text_buffer_insert(self->content_buffer_, &drop_iter, "\n", -1);

        handled = true;
    }

    return handled ? TRUE : FALSE;
}

// ── Signal callbacks ──────────────────────────────────────────────────────────

void MainWindow::cb_folder_row_selected(GtkListBox*, GtkListBoxRow* row, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    if (self->suppress_folder_selection_ || !row) return;
    const char* fid = (const char*)g_object_get_data(G_OBJECT(row), "folder-id");
    if (fid) self->vm_->select_folder(fid);
}

void MainWindow::cb_note_row_selected(GtkListBox*, GtkListBoxRow* row, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    if (self->suppress_note_selection_) return;
    if (!row) { self->vm_->select_note(""); return; }
    const char* nid = (const char*)g_object_get_data(G_OBJECT(row), "note-id");
    if (nid) self->vm_->select_note(nid);
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

    // Serialize buffer, replacing child anchors with their blob markdown references
    std::string content;
    GtkTextIter it;
    gtk_text_buffer_get_start_iter(self->content_buffer_, &it);
    while (!gtk_text_iter_is_end(&it)) {
        GtkTextChildAnchor* anchor = gtk_text_iter_get_child_anchor(&it);
        if (anchor) {
            auto found = self->blob_anchors_.find(anchor);
            if (found != self->blob_anchors_.end())
                content += "![](amadeuz://blob/" + found->second + ")";
        } else {
            gunichar ch = gtk_text_iter_get_char(&it);
            char buf[7] = {};
            int len = g_unichar_to_utf8(ch, buf);
            content.append(buf, len);
        }
        gtk_text_iter_forward_char(&it);
    }
    self->vm_->on_content_changed(content);

    self->apply_markdown_for_current_line();
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

void MainWindow::cb_sign_out_clicked(GtkButton*, gpointer data) {
    static_cast<MainWindow*>(data)->vm_->sign_out();
}

void MainWindow::cb_search_changed(GtkSearchEntry* entry, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    const char* text = gtk_editable_get_text(GTK_EDITABLE(entry));
    self->vm_->set_search_query(text ? text : "");
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
    if (!before) { gtk_list_box_row_set_header(row, nullptr); return; }
    const char* before_id =
        (const char*)g_object_get_data(G_OBJECT(before), "folder-id");
    if (before_id && strcmp(before_id, NotesViewModel::ALL_NOTES_ID) == 0) {
        GtkWidget* label = gtk_label_new("Folders");
        gtk_label_set_xalign(GTK_LABEL(label), 0.0f);
        gtk_widget_set_margin_start(label, 12);
        gtk_widget_set_margin_top(label, 8);
        gtk_widget_set_margin_bottom(label, 4);
        gtk_widget_add_css_class(label, "caption");
        gtk_widget_add_css_class(label, "dim-label");
        gtk_list_box_row_set_header(row, label);
    } else {
        gtk_list_box_row_set_header(row, nullptr);
    }
}

// ── Auth callbacks ────────────────────────────────────────────────────────────

void MainWindow::cb_mode_login(GtkButton*, gpointer data) {
    static_cast<MainWindow*>(data)->switch_auth_mode("login");
}
void MainWindow::cb_mode_register(GtkButton*, gpointer data) {
    static_cast<MainWindow*>(data)->switch_auth_mode("register");
}
void MainWindow::cb_mode_recover(GtkButton*, gpointer data) {
    static_cast<MainWindow*>(data)->switch_auth_mode("recover");
}

void MainWindow::cb_login_btn(GtkButton*, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    self->set_auth_error("");

    // Read server address first
    if (self->auth_server_entry_) {
        const char* srv = gtk_editable_get_text(GTK_EDITABLE(self->auth_server_entry_));
        if (srv && *srv) self->vm_->set_server_address(srv);
    }

    const char* email = gtk_editable_get_text(GTK_EDITABLE(self->auth_email_entry_));
    const char* pass  = gtk_editable_get_text(GTK_EDITABLE(self->auth_password_entry_));
    if (!email || !*email || !pass || !*pass) {
        self->set_auth_error("Please fill in email and password.");
        return;
    }
    self->vm_->do_login(email, pass);
}

void MainWindow::cb_register_btn(GtkButton*, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    self->set_auth_error("");

    if (self->auth_server_entry_) {
        const char* srv = gtk_editable_get_text(GTK_EDITABLE(self->auth_server_entry_));
        if (srv && *srv) self->vm_->set_server_address(srv);
    }

    // Get entries from the register form stack page
    GtkWidget* form = gtk_stack_get_child_by_name(GTK_STACK(self->auth_mode_stack_), "register");
    if (!form) return;
    auto* email_ent = GTK_WIDGET(g_object_get_data(G_OBJECT(form), "email"));
    auto* pass_ent  = GTK_WIDGET(g_object_get_data(G_OBJECT(form), "password"));
    const char* email = gtk_editable_get_text(GTK_EDITABLE(email_ent));
    const char* pass  = gtk_editable_get_text(GTK_EDITABLE(pass_ent));
    if (!email || !*email || !pass || !*pass) {
        self->set_auth_error("Please fill in email and password.");
        return;
    }
    self->vm_->do_register(email, pass);
}

void MainWindow::cb_recover_btn(GtkButton*, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    self->set_auth_error("");

    if (self->auth_server_entry_) {
        const char* srv = gtk_editable_get_text(GTK_EDITABLE(self->auth_server_entry_));
        if (srv && *srv) self->vm_->set_server_address(srv);
    }

    GtkWidget* form = gtk_stack_get_child_by_name(GTK_STACK(self->auth_mode_stack_), "recover");
    if (!form) return;
    auto* email_ent = GTK_WIDGET(g_object_get_data(G_OBJECT(form), "email"));
    auto* code_ent  = GTK_WIDGET(g_object_get_data(G_OBJECT(form), "code"));
    auto* pass_ent  = GTK_WIDGET(g_object_get_data(G_OBJECT(form), "password"));
    const char* email = gtk_editable_get_text(GTK_EDITABLE(email_ent));
    const char* code  = gtk_editable_get_text(GTK_EDITABLE(code_ent));
    const char* pass  = gtk_editable_get_text(GTK_EDITABLE(pass_ent));
    if (!email || !*email || !code || !*code || !pass || !*pass) {
        self->set_auth_error("Please fill in all fields.");
        return;
    }
    self->vm_->do_recover(email, code, pass);
}

// ── Dialogs ───────────────────────────────────────────────────────────────────

void MainWindow::show_recovery_code_dialog(const std::string& code) {
    GtkWidget* dialog = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(dialog), "Recovery Code");
    gtk_window_set_transient_for(GTK_WINDOW(dialog), GTK_WINDOW(window_));
    gtk_window_set_modal(GTK_WINDOW(dialog), TRUE);
    gtk_window_set_resizable(GTK_WINDOW(dialog), FALSE);
    gtk_window_set_default_size(GTK_WINDOW(dialog), 440, -1);

    GtkWidget* vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
    gtk_widget_set_margin_start(vbox, 20);
    gtk_widget_set_margin_end(vbox, 20);
    gtk_widget_set_margin_top(vbox, 20);
    gtk_widget_set_margin_bottom(vbox, 20);
    gtk_window_set_child(GTK_WINDOW(dialog), vbox);

    GtkWidget* lbl = gtk_label_new(
        "Save this recovery code. It will only be shown once.\n"
        "You will need it to recover your account if you forget your password.");
    gtk_label_set_wrap(GTK_LABEL(lbl), TRUE);
    gtk_label_set_xalign(GTK_LABEL(lbl), 0.0f);
    gtk_box_append(GTK_BOX(vbox), lbl);

    GtkWidget* code_lbl = gtk_label_new(code.c_str());
    gtk_widget_add_css_class(code_lbl, "title-3");
    gtk_label_set_selectable(GTK_LABEL(code_lbl), TRUE);
    gtk_label_set_xalign(GTK_LABEL(code_lbl), 0.0f);
    gtk_widget_set_margin_top(code_lbl, 8);
    gtk_widget_set_margin_bottom(code_lbl, 8);
    gtk_box_append(GTK_BOX(vbox), code_lbl);

    GtkWidget* btn_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_widget_set_halign(btn_row, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(vbox), btn_row);

    GtkWidget* copy_btn = gtk_button_new_with_label("Copy Code");
    g_object_set_data_full(G_OBJECT(copy_btn), "code", g_strdup(code.c_str()), g_free);
    g_signal_connect(copy_btn, "clicked",
        G_CALLBACK(+[](GtkButton* btn, gpointer d) {
            const char* c = (const char*)g_object_get_data(G_OBJECT(btn), "code");
            GdkClipboard* clip = gdk_display_get_clipboard(gdk_display_get_default());
            gdk_clipboard_set_text(clip, c);
        }), nullptr);
    gtk_box_append(GTK_BOX(btn_row), copy_btn);

    GtkWidget* done_btn = gtk_button_new_with_mnemonic("_Done");
    gtk_widget_add_css_class(done_btn, "suggested-action");
    g_signal_connect(done_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            gtk_window_destroy(GTK_WINDOW(d));
        }), dialog);
    gtk_box_append(GTK_BOX(btn_row), done_btn);

    gtk_window_present(GTK_WINDOW(dialog));
}

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

    GtkWidget* lbl = gtk_label_new("Server URL");
    gtk_label_set_xalign(GTK_LABEL(lbl), 0.0f);
    gtk_box_append(GTK_BOX(vbox), lbl);

    GtkWidget* entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(entry), "http://hostname:8080");
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
            auto* dlg = GTK_WIDGET(d);
            auto* ent = GTK_WIDGET(g_object_get_data(G_OBJECT(dlg), "entry"));
            auto* vm  = static_cast<NotesViewModel*>(g_object_get_data(G_OBJECT(dlg), "vm"));
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

    GtkWidget* lbl   = gtk_label_new("Folder name");
    gtk_label_set_xalign(GTK_LABEL(lbl), 0.0f);
    gtk_box_append(GTK_BOX(vbox), lbl);

    GtkWidget* entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(entry), "Folder name");
    gtk_box_append(GTK_BOX(vbox), entry);

    GtkWidget* btn_row    = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
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
        G_CALLBACK(+[](GtkButton*, gpointer d) { gtk_window_destroy(GTK_WINDOW(d)); }), dialog);
    g_signal_connect(create_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            auto* dlg = GTK_WIDGET(d);
            auto* ent = GTK_WIDGET(g_object_get_data(G_OBJECT(dlg), "entry"));
            auto* vm  = static_cast<NotesViewModel*>(g_object_get_data(G_OBJECT(dlg), "vm"));
            const char* text = gtk_entry_buffer_get_text(gtk_entry_get_buffer(GTK_ENTRY(ent)));
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

    GtkWidget* btn_row    = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
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
        G_CALLBACK(+[](GtkButton*, gpointer d) { gtk_window_destroy(GTK_WINDOW(d)); }), dialog);
    g_signal_connect(rename_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            auto* dlg = GTK_WIDGET(d);
            auto* ent = GTK_WIDGET(g_object_get_data(G_OBJECT(dlg), "entry"));
            auto* vm  = static_cast<NotesViewModel*>(g_object_get_data(G_OBJECT(dlg), "vm"));
            const char* fid  = (const char*)g_object_get_data(G_OBJECT(dlg), "folder-id");
            const char* text = gtk_entry_buffer_get_text(gtk_entry_get_buffer(GTK_ENTRY(ent)));
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
    g_object_set_data_full(G_OBJECT(popover), "folder-id", g_strdup(folder_id), g_free);

    g_signal_connect(rename_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            auto* pop = GTK_WIDGET(d);
            auto* mw  = static_cast<MainWindow*>(g_object_get_data(G_OBJECT(pop), "mw"));
            const char* fid = (const char*)g_object_get_data(G_OBJECT(pop), "folder-id");
            gtk_popover_popdown(GTK_POPOVER(pop));
            for (const auto& f : mw->vm_->folders()) {
                if (f.id == fid) { mw->show_rename_folder_dialog(fid, f.name); return; }
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

    // ── Move to… ──────────────────────────────────────────────────────────────
    GtkWidget* move_btn = gtk_button_new_with_label("Move to…");
    gtk_button_set_has_frame(GTK_BUTTON(move_btn), FALSE);
    gtk_widget_set_halign(move_btn, GTK_ALIGN_FILL);
    gtk_box_append(GTK_BOX(vbox), move_btn);

    // Build submenu popover for folder list (shown on "Move to…" click)
    GtkWidget* sub_popover = gtk_popover_new();
    gtk_widget_set_parent(sub_popover, row);
    gtk_popover_set_has_arrow(GTK_POPOVER(sub_popover), FALSE);
    gtk_popover_set_pointing_to(GTK_POPOVER(sub_popover), &rect);

    GtkWidget* sub_vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);
    gtk_widget_set_margin_start(sub_vbox, 4);
    gtk_widget_set_margin_end(sub_vbox, 4);
    gtk_widget_set_margin_top(sub_vbox, 4);
    gtk_widget_set_margin_bottom(sub_vbox, 4);
    gtk_popover_set_child(GTK_POPOVER(sub_popover), sub_vbox);

    // "No folder" option
    GtkWidget* unfolder_btn = gtk_button_new_with_label("No Folder (unfoldered)");
    gtk_button_set_has_frame(GTK_BUTTON(unfolder_btn), FALSE);
    gtk_widget_set_halign(unfolder_btn, GTK_ALIGN_FILL);
    g_object_set_data(G_OBJECT(unfolder_btn), "mw", this);
    g_object_set_data_full(G_OBJECT(unfolder_btn), "note-id", g_strdup(note_id), g_free);
    g_object_set_data_full(G_OBJECT(unfolder_btn), "folder-id",
        g_strdup(NotesViewModel::ALL_NOTES_ID), g_free);
    g_signal_connect(unfolder_btn, "clicked",
        G_CALLBACK(+[](GtkButton* btn, gpointer d) {
            auto* mw  = static_cast<MainWindow*>(g_object_get_data(G_OBJECT(btn), "mw"));
            const char* nid = (const char*)g_object_get_data(G_OBJECT(btn), "note-id");
            const char* fid = (const char*)g_object_get_data(G_OBJECT(btn), "folder-id");
            gtk_popover_popdown(GTK_POPOVER(d));
            if (nid && fid) mw->vm_->move_note(nid, fid);
        }), sub_popover);
    gtk_box_append(GTK_BOX(sub_vbox), unfolder_btn);

    // One button per folder
    for (const auto& folder : vm_->folders()) {
        GtkWidget* fb = gtk_button_new_with_label(folder.name.c_str());
        gtk_button_set_has_frame(GTK_BUTTON(fb), FALSE);
        gtk_widget_set_halign(fb, GTK_ALIGN_FILL);
        g_object_set_data(G_OBJECT(fb), "mw", this);
        g_object_set_data_full(G_OBJECT(fb), "note-id",   g_strdup(note_id),         g_free);
        g_object_set_data_full(G_OBJECT(fb), "folder-id", g_strdup(folder.id.c_str()), g_free);
        g_signal_connect(fb, "clicked",
            G_CALLBACK(+[](GtkButton* btn, gpointer d) {
                auto* mw  = static_cast<MainWindow*>(g_object_get_data(G_OBJECT(btn), "mw"));
                const char* nid = (const char*)g_object_get_data(G_OBJECT(btn), "note-id");
                const char* fid = (const char*)g_object_get_data(G_OBJECT(btn), "folder-id");
                gtk_popover_popdown(GTK_POPOVER(d));
                if (nid && fid) mw->vm_->move_note(nid, fid);
            }), sub_popover);
        gtk_box_append(GTK_BOX(sub_vbox), fb);
    }

    g_signal_connect(sub_popover, "closed",
        G_CALLBACK(+[](GtkPopover* pop, gpointer) {
            gtk_widget_unparent(GTK_WIDGET(pop));
        }), nullptr);

    g_object_set_data(G_OBJECT(move_btn), "sub", sub_popover);
    g_object_set_data(G_OBJECT(move_btn), "main-pop", popover);
    g_signal_connect(move_btn, "clicked",
        G_CALLBACK(+[](GtkButton* btn, gpointer) {
            auto* sub  = GTK_POPOVER(g_object_get_data(G_OBJECT(btn), "sub"));
            auto* main_pop = GTK_POPOVER(g_object_get_data(G_OBJECT(btn), "main-pop"));
            gtk_popover_popdown(main_pop);
            gtk_popover_popup(sub);
        }), nullptr);

    gtk_box_append(GTK_BOX(vbox), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));

    // ── Delete ────────────────────────────────────────────────────────────────
    GtkWidget* delete_btn = gtk_button_new_with_label("Delete Note");
    gtk_button_set_has_frame(GTK_BUTTON(delete_btn), FALSE);
    gtk_widget_add_css_class(delete_btn, "destructive-action");
    gtk_widget_set_halign(delete_btn, GTK_ALIGN_FILL);
    gtk_box_append(GTK_BOX(vbox), delete_btn);

    g_object_set_data(G_OBJECT(popover), "mw", this);
    g_object_set_data_full(G_OBJECT(popover), "note-id", g_strdup(note_id), g_free);

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
