#pragma once
#include "local_store.h"
#include "sync_service.h"

#include <cstdint>
#include <functional>
#include <memory>
#include <string>

#include <glib.h>

/// All business logic: offline-first load, 500 ms debounce, timestamp-based merge.
/// Mirror of the macOS NoteViewModel and Windows NoteViewModel.
/// All methods must be called on the GLib main thread.
class NoteViewModel {
public:
    using ContentCb    = std::function<void(std::string content)>;
    using ConnectionCb = std::function<void(bool connected)>;

    NoteViewModel(ContentCb on_content, ConnectionCb on_connection);
    ~NoteViewModel();

    const std::string& content()        const { return content_; }
    const std::string& server_address() const { return server_address_; }

    /// Called from the UI every time the text buffer changes.
    void on_text_changed(const std::string& content);

    /// Called from the settings dialog when the user saves a new server address.
    void set_server_address(const std::string& address);

private:
    void start_sync();
    void handle_server_message(std::string content, int64_t updated_at);
    void flush_debounce();

    static gboolean  cb_debounce(gpointer data);
    static std::string load_server_address();
    static void        save_server_address(const std::string& address);

    ContentCb    on_content_cb_;
    ConnectionCb on_connection_cb_;

    LocalStore   local_store_;
    std::unique_ptr<SyncService> sync_service_;

    std::string content_;
    std::string last_received_content_;
    int64_t     last_updated_at_{0};
    std::string server_address_;

    guint debounce_source_{0};
};
