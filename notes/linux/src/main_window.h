#pragma once
#include "note_view_model.h"

#include <gtk/gtk.h>
#include <memory>
#include <string>

class MainWindow {
public:
    explicit MainWindow(GtkApplication* app);

private:
    // ── UI rebuilds (called by VM callbacks) ──────────────────────────────────
    void rebuild_folder_list();
    void rebuild_note_list();
    void load_note(const Note* note);
    void update_status(bool connected);
    void update_editor_text(const std::string& title, const std::string& content);

    // ── Dialogs ───────────────────────────────────────────────────────────────
    void show_settings_dialog();
    void show_new_folder_dialog();
    void show_rename_folder_dialog(const std::string& folder_id,
                                   const std::string& current_name);
    void show_folder_context_menu(GtkWidget* row, const char* folder_id, double x, double y);
    void show_note_context_menu(GtkWidget* row, const char* note_id, double x, double y);

    // ── Static GTK signal callbacks ───────────────────────────────────────────
    static void     cb_folder_row_selected(GtkListBox*, GtkListBoxRow*, gpointer);
    static void     cb_note_row_selected(GtkListBox*, GtkListBoxRow*, gpointer);
    static void     cb_title_changed(GtkEditable*, gpointer);
    static void     cb_content_changed(GtkTextBuffer*, gpointer);
    static void     cb_new_folder_clicked(GtkButton*, gpointer);
    static void     cb_new_note_clicked(GtkButton*, gpointer);
    static void     cb_delete_note_clicked(GtkButton*, gpointer);
    static void     cb_settings_clicked(GtkButton*, gpointer);
    static void     cb_folder_right_click(GtkGestureClick*, int, double, double, gpointer);
    static void     cb_note_right_click(GtkGestureClick*, int, double, double, gpointer);
    static void     folder_header_func(GtkListBoxRow*, GtkListBoxRow*, gpointer);

    std::unique_ptr<NotesViewModel> vm_;

    GtkWidget*     window_{nullptr};
    GtkWidget*     folder_list_box_{nullptr};
    GtkWidget*     note_list_box_{nullptr};
    GtkWidget*     editor_stack_{nullptr};   // pages: "empty" | "editor"
    GtkWidget*     title_entry_{nullptr};
    GtkTextBuffer* content_buffer_{nullptr};
    GtkWidget*     note_list_header_{nullptr};
    GtkWidget*     status_label_{nullptr};
    GtkWidget*     new_note_btn_{nullptr};
    GtkWidget*     delete_note_btn_{nullptr};

    bool suppress_folder_selection_{false};
    bool suppress_note_selection_{false};
    bool suppress_title_changed_{false};
    bool suppress_content_changed_{false};
};
