#include "main_window.h"

#include <algorithm>
#include <gtk/gtk.h>

static constexpr int WINDOW_W = 700;
static constexpr int WINDOW_H = 500;

MainWindow::MainWindow(GtkApplication* app) {
    // ── View model ──────────────────────────────────────────────────────────
    vm_ = std::make_unique<NoteViewModel>(
        [this](std::string content) { update_text(content); },
        [this](bool connected)      { update_status(connected); });

    // ── Window ──────────────────────────────────────────────────────────────
    window_ = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(window_), "Amadeuz");
    gtk_window_set_default_size(GTK_WINDOW(window_), WINDOW_W, WINDOW_H);

    // ── CSS: set editor font size ────────────────────────────────────────────
    GtkCssProvider* css = gtk_css_provider_new();
    gtk_css_provider_load_from_data(css, "textview { font-size: 16px; }", -1);
    gtk_style_context_add_provider_for_display(
        gdk_display_get_default(),
        GTK_STYLE_PROVIDER(css),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
    g_object_unref(css);

    // ── Root layout: text area + separator + status bar ─────────────────────
    GtkWidget* vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_window_set_child(GTK_WINDOW(window_), vbox);

    // Scrolled text view
    GtkWidget* scroll = gtk_scrolled_window_new();
    gtk_widget_set_vexpand(scroll, TRUE);
    gtk_widget_set_hexpand(scroll, TRUE);
    gtk_box_append(GTK_BOX(vbox), scroll);

    GtkWidget* text_view = gtk_text_view_new();
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(text_view), GTK_WRAP_WORD_CHAR);
    gtk_text_view_set_left_margin(GTK_TEXT_VIEW(text_view), 12);
    gtk_text_view_set_right_margin(GTK_TEXT_VIEW(text_view), 12);
    gtk_text_view_set_top_margin(GTK_TEXT_VIEW(text_view), 12);
    gtk_text_view_set_bottom_margin(GTK_TEXT_VIEW(text_view), 12);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroll), text_view);

    buffer_ = gtk_text_view_get_buffer(GTK_TEXT_VIEW(text_view));

    // Populate with the locally-stored note without triggering the debounce.
    suppress_changed_ = true;
    gtk_text_buffer_set_text(buffer_, vm_->content().c_str(), -1);
    suppress_changed_ = false;

    g_signal_connect(buffer_, "changed", G_CALLBACK(cb_text_changed), this);

    // Separator
    gtk_box_append(GTK_BOX(vbox), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));

    // Status bar
    GtkWidget* hbox = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_widget_set_margin_start(hbox, 12);
    gtk_widget_set_margin_end(hbox, 12);
    gtk_widget_set_margin_top(hbox, 6);
    gtk_widget_set_margin_bottom(hbox, 6);
    gtk_box_append(GTK_BOX(vbox), hbox);

    status_label_ = gtk_label_new(nullptr);
    gtk_label_set_xalign(GTK_LABEL(status_label_), 0.0f);
    gtk_widget_set_hexpand(status_label_, TRUE);
    gtk_box_append(GTK_BOX(hbox), status_label_);
    update_status(false); // initial state

    GtkWidget* settings_btn = gtk_button_new_with_label("Settings");
    gtk_button_set_has_frame(GTK_BUTTON(settings_btn), FALSE);
    gtk_box_append(GTK_BOX(hbox), settings_btn);
    g_signal_connect(settings_btn, "clicked", G_CALLBACK(cb_settings_clicked), this);

    gtk_window_present(GTK_WINDOW(window_));
}

// ── Signal handlers ───────────────────────────────────────────────────────────

void MainWindow::cb_text_changed(GtkTextBuffer*, gpointer data) {
    auto* self = static_cast<MainWindow*>(data);
    if (self->suppress_changed_) return;

    GtkTextIter start, end;
    gtk_text_buffer_get_bounds(self->buffer_, &start, &end);
    gchar* text = gtk_text_buffer_get_text(self->buffer_, &start, &end, FALSE);
    self->vm_->on_text_changed(text);
    g_free(text);
}

void MainWindow::cb_settings_clicked(GtkButton*, gpointer data) {
    static_cast<MainWindow*>(data)->show_settings_dialog();
}

// ── Settings dialog ───────────────────────────────────────────────────────────

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

    GtkWidget* label = gtk_label_new("WebSocket URL");
    gtk_label_set_xalign(GTK_LABEL(label), 0.0f);
    gtk_box_append(GTK_BOX(vbox), label);

    GtkWidget* entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(entry), "ws://hostname:8080/ws");
    gtk_entry_buffer_set_text(
        gtk_entry_get_buffer(GTK_ENTRY(entry)),
        vm_->server_address().c_str(), -1);
    gtk_box_append(GTK_BOX(vbox), entry);

    // Buttons row (right-aligned)
    GtkWidget* btn_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_widget_set_halign(btn_row, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(vbox), btn_row);

    GtkWidget* cancel_btn  = gtk_button_new_with_mnemonic("_Cancel");
    GtkWidget* connect_btn = gtk_button_new_with_mnemonic("_Connect");
    gtk_widget_add_css_class(connect_btn, "suggested-action");
    gtk_box_append(GTK_BOX(btn_row), cancel_btn);
    gtk_box_append(GTK_BOX(btn_row), connect_btn);

    // Store references on the dialog for use in the lambda-style callbacks.
    g_object_set_data(G_OBJECT(dialog), "entry", entry);
    g_object_set_data(G_OBJECT(dialog), "vm",    vm_.get());

    g_signal_connect(cancel_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            gtk_window_destroy(GTK_WINDOW(d));
        }), dialog);

    g_signal_connect(connect_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer d) {
            auto* dlg   = GTK_WIDGET(d);
            auto* entry = GTK_WIDGET(g_object_get_data(G_OBJECT(dlg), "entry"));
            auto* vm    = static_cast<NoteViewModel*>(g_object_get_data(G_OBJECT(dlg), "vm"));
            const char* text = gtk_entry_buffer_get_text(
                gtk_entry_get_buffer(GTK_ENTRY(entry)));
            if (text && *text)
                vm->set_server_address(text);
            gtk_window_destroy(GTK_WINDOW(dlg));
        }), dialog);

    gtk_window_present(GTK_WINDOW(dialog));
}

// ── UI update helpers (always called on the GLib main thread) ─────────────────

void MainWindow::update_text(const std::string& content) {
    // Save cursor offset so the view doesn't jump unexpectedly.
    GtkTextMark* insert = gtk_text_buffer_get_insert(buffer_);
    GtkTextIter  cursor;
    gtk_text_buffer_get_iter_at_mark(buffer_, &cursor, insert);
    int offset = gtk_text_iter_get_offset(&cursor);

    suppress_changed_ = true;
    gtk_text_buffer_set_text(buffer_, content.c_str(), -1);
    suppress_changed_ = false;

    // Restore cursor, clamped to the new content length.
    GtkTextIter new_pos;
    gtk_text_buffer_get_iter_at_offset(
        buffer_, &new_pos,
        std::min(offset, gtk_text_buffer_get_char_count(buffer_)));
    gtk_text_buffer_place_cursor(buffer_, &new_pos);
}

void MainWindow::update_status(bool connected) {
    const char* markup = connected
        ? "<span foreground='#22C55E'>●</span>  "
          "<span foreground='#888888' size='small'>Synced</span>"
        : "<span foreground='#EF4444'>●</span>  "
          "<span foreground='#888888' size='small'>Offline</span>";
    gtk_label_set_markup(GTK_LABEL(status_label_), markup);
}
