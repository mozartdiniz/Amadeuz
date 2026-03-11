#pragma once
#include "note_view_model.h"

#include <gtk/gtk.h>
#include <map>
#include <memory>
#include <string>

class MainWindow {
public:
    explicit MainWindow(GtkApplication* app);

private:
    // ── UI construction ───────────────────────────────────────────────────────
    GtkWidget* build_auth_page();
    GtkWidget* build_main_page();

    // ── UI rebuilds (called by VM callbacks) ──────────────────────────────────
    void show_auth_page(const std::string& recovery_code);
    void show_main_page();
    void rebuild_folder_list();
    void rebuild_note_list();
    void load_note(const Note* note);
    void update_status(bool connected);
    void update_editor_text(const std::string& title, const std::string& content);

    // ── Markdown styling ──────────────────────────────────────────────────────
    void apply_markdown_full();
    void apply_markdown_for_current_line();
    void ensure_markdown_tags();

    // ── Image blobs ───────────────────────────────────────────────────────────
    void insert_image_at_iter(GtkTextIter* iter, const std::string& uuid);
    void render_blob_images();
    static gboolean cb_image_drop(GtkDropTarget*, const GValue*, double, double, gpointer);

    // ── Dialogs ───────────────────────────────────────────────────────────────
    void show_settings_dialog();
    void show_new_folder_dialog();
    void show_rename_folder_dialog(const std::string& folder_id,
                                   const std::string& current_name);
    void show_recovery_code_dialog(const std::string& code);
    void show_folder_context_menu(GtkWidget* row, const char* folder_id, double x, double y);
    void show_note_context_menu  (GtkWidget* row, const char* note_id,   double x, double y);

    // ── Auth page helpers ─────────────────────────────────────────────────────
    void switch_auth_mode(const char* mode); // "login" | "register" | "recover"
    void set_auth_error(const char* msg);

    // ── Static callbacks ──────────────────────────────────────────────────────
    static void     cb_folder_row_selected (GtkListBox*, GtkListBoxRow*, gpointer);
    static void     cb_note_row_selected   (GtkListBox*, GtkListBoxRow*, gpointer);
    static void     cb_title_changed       (GtkEditable*, gpointer);
    static void     cb_content_changed     (GtkTextBuffer*, gpointer);
    static void     cb_new_folder_clicked  (GtkButton*, gpointer);
    static void     cb_new_note_clicked    (GtkButton*, gpointer);
    static void     cb_delete_note_clicked (GtkButton*, gpointer);
    static void     cb_settings_clicked    (GtkButton*, gpointer);
    static void     cb_sign_out_clicked    (GtkButton*, gpointer);
    static void     cb_search_changed      (GtkSearchEntry*, gpointer);
    static void     cb_folder_right_click  (GtkGestureClick*, int, double, double, gpointer);
    static void     cb_note_right_click    (GtkGestureClick*, int, double, double, gpointer);
    static void     folder_header_func     (GtkListBoxRow*, GtkListBoxRow*, gpointer);

    // ── Auth page callbacks ───────────────────────────────────────────────────
    static void cb_login_btn    (GtkButton*, gpointer);
    static void cb_register_btn (GtkButton*, gpointer);
    static void cb_recover_btn  (GtkButton*, gpointer);
    static void cb_mode_login   (GtkButton*, gpointer);
    static void cb_mode_register(GtkButton*, gpointer);
    static void cb_mode_recover (GtkButton*, gpointer);

    std::unique_ptr<NotesViewModel> vm_;

    // ── Top-level ─────────────────────────────────────────────────────────────
    GtkWidget*     window_{nullptr};
    GtkWidget*     root_stack_{nullptr};   // pages: "auth" | "main"

    // ── Auth page ─────────────────────────────────────────────────────────────
    GtkWidget*     auth_mode_stack_{nullptr};  // pages: "login" | "register" | "recover"
    GtkWidget*     auth_email_entry_{nullptr};
    GtkWidget*     auth_password_entry_{nullptr};
    GtkWidget*     auth_new_password_entry_{nullptr};
    GtkWidget*     auth_code_entry_{nullptr};
    GtkWidget*     auth_error_label_{nullptr};
    GtkWidget*     auth_login_btn_{nullptr};
    GtkWidget*     auth_register_btn_{nullptr};
    GtkWidget*     auth_recover_btn_{nullptr};
    GtkWidget*     auth_server_entry_{nullptr};

    // ── Main page ─────────────────────────────────────────────────────────────
    GtkWidget*     folder_list_box_{nullptr};
    GtkWidget*     note_list_box_{nullptr};
    GtkWidget*     editor_stack_{nullptr};
    GtkWidget*     title_entry_{nullptr};
    GtkWidget*     content_view_{nullptr};
    GtkTextBuffer* content_buffer_{nullptr};
    GtkWidget*     note_list_header_{nullptr};
    GtkWidget*     status_label_{nullptr};
    GtkWidget*     new_note_btn_{nullptr};
    GtkWidget*     delete_note_btn_{nullptr};
    GtkWidget*     search_entry_{nullptr};

    bool suppress_folder_selection_{false};
    bool suppress_note_selection_{false};
    bool suppress_title_changed_{false};
    bool suppress_content_changed_{false};
    bool markdown_tags_created_{false};

    // anchor → uuid mapping for inline images
    std::map<GtkTextChildAnchor*, std::string> blob_anchors_;
};
