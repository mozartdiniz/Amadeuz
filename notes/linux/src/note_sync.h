#pragma once
#include "models.h"

#include <functional>
#include <string>
#include <cstdint>

#include <libsoup/soup.h>

/// Per-note WebSocket client.
/// Connects to GET /notes/:id/ws?token=<jwt>.
/// Receives "init" and "update" messages; sends "update" on keystroke.
/// Auto-reconnects every 3 seconds on disconnect (while alive).
/// All callbacks fire on the GLib main thread.
class NoteSync {
public:
    using MsgCb = std::function<void(std::string type,
                                     std::string title,
                                     std::string content,
                                     int64_t     updated_at)>;

    NoteSync(std::string ws_url, MsgCb on_message);
    ~NoteSync();

    void send_update(const std::string& title, const std::string& content, int64_t updated_at);
    bool is_connected() const;

private:
    void connect();
    void schedule_reconnect();

    static void     cb_connected(GObject* src, GAsyncResult* res, gpointer data);
    static void     cb_message  (SoupWebsocketConnection*, gint type, GBytes* msg, gpointer data);
    static void     cb_closed   (SoupWebsocketConnection*, gpointer data);
    static gboolean cb_reconnect(gpointer data);

    std::string  url_;
    MsgCb        on_message_cb_;

    SoupSession*             session_{nullptr};
    SoupWebsocketConnection* ws_{nullptr};
    GCancellable*            cancel_{nullptr};
    bool                     alive_{true};
    guint                    reconnect_source_{0};
};
