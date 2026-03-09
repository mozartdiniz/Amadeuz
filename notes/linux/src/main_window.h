#pragma once
#include "note_view_model.h"

#include <gtk/gtk.h>
#include <memory>

class MainWindow {
public:
    explicit MainWindow(GtkApplication* app);

private:
    void update_text(const std::string& content);
    void update_status(bool connected);
    void show_settings_dialog();

    static void     cb_text_changed(GtkTextBuffer* buffer, gpointer data);
    static void     cb_settings_clicked(GtkButton* button, gpointer data);

    std::unique_ptr<NoteViewModel> vm_;

    GtkWidget*     window_{nullptr};
    GtkTextBuffer* buffer_{nullptr};
    GtkWidget*     status_label_{nullptr};

    // Prevents the text-changed signal from triggering the debounce when we
    // are programmatically updating the buffer with content from the server.
    bool suppress_changed_{false};
};
