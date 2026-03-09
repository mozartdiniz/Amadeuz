#include "main_window.h"

#include <gtk/gtk.h>
#include <memory>

static std::unique_ptr<MainWindow> g_window;

static void on_activate(GtkApplication* app, gpointer) {
    g_window = std::make_unique<MainWindow>(app);
}

int main(int argc, char* argv[]) {
    GtkApplication* app = gtk_application_new(
        "com.amadeuz.notes", G_APPLICATION_DEFAULT_FLAGS);

    g_signal_connect(app, "activate", G_CALLBACK(on_activate), nullptr);

    int status = g_application_run(G_APPLICATION(app), argc, argv);

    g_window.reset();
    g_object_unref(app);
    return status;
}
