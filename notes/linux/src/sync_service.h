#pragma once
#include <cstdint>
#include <functional>
#include <string>

#include <libsoup/soup.h>

/// WebSocket client for the amadeuz sync server.
/// Automatically reconnects every 3 seconds on disconnect.
/// All callbacks are invoked on the GLib main thread (safe to touch GTK from them).
class SyncService {
public:
    using MessageCb    = std::function<void(std::string content, int64_t updated_at)>;
    using ConnectionCb = std::function<void(bool connected)>;

    SyncService(std::string url, MessageCb on_message, ConnectionCb on_connection);
    ~SyncService();

    void send(const std::string& content, int64_t updated_at);

private:
    void connect();
    void on_ws_opened(SoupWebsocketConnection* ws);
    void schedule_reconnect();

    // Static C-style callbacks for GLib signals
    static void      cb_connected(GObject* src, GAsyncResult* res, gpointer data);
    static void      cb_message(SoupWebsocketConnection*, gint type, GBytes* msg, gpointer data);
    static void      cb_closed(SoupWebsocketConnection*, gpointer data);
    static gboolean  cb_reconnect(gpointer data);

    std::string   url_;
    MessageCb     on_message_cb_;
    ConnectionCb  on_connection_cb_;

    SoupSession*             session_{nullptr};
    SoupWebsocketConnection* ws_{nullptr};
    GCancellable*            cancellable_{nullptr};
    bool                     alive_{true};
    guint                    reconnect_source_{0};
};
