#include "sync_service.h"

#include <json-glib/json-glib.h>

// ── JSON helpers ─────────────────────────────────────────────────────────────

namespace {

struct WireMessage {
    std::string type;
    std::string content;
    int64_t     updated_at{0};
};

WireMessage parse_wire(const char* data, gsize length) {
    WireMessage msg;
    GError*     error  = nullptr;
    JsonParser* parser = json_parser_new();

    if (json_parser_load_from_data(parser, data, (gssize)length, &error)) {
        JsonNode* root = json_parser_get_root(parser);
        if (root && JSON_NODE_HOLDS_OBJECT(root)) {
            JsonObject* obj = json_node_get_object(root);
            if (json_object_has_member(obj, "type"))
                msg.type = json_object_get_string_member(obj, "type");
            if (json_object_has_member(obj, "content"))
                msg.content = json_object_get_string_member(obj, "content");
            if (json_object_has_member(obj, "updated_at"))
                msg.updated_at = json_object_get_int_member(obj, "updated_at");
        }
    }

    g_clear_error(&error);
    g_object_unref(parser);
    return msg;
}

std::string make_update(const std::string& content, int64_t updated_at) {
    JsonBuilder* builder = json_builder_new();
    json_builder_begin_object(builder);
    json_builder_set_member_name(builder, "type");
    json_builder_add_string_value(builder, "update");
    json_builder_set_member_name(builder, "content");
    json_builder_add_string_value(builder, content.c_str());
    json_builder_set_member_name(builder, "updated_at");
    json_builder_add_int_value(builder, (gint64)updated_at);
    json_builder_end_object(builder);

    JsonNode*      node = json_builder_get_root(builder);
    JsonGenerator* gen  = json_generator_new();
    json_generator_set_root(gen, node);
    gchar* str = json_generator_to_data(gen, nullptr);

    std::string result(str);
    g_free(str);
    g_object_unref(gen);
    json_node_free(node);
    g_object_unref(builder);
    return result;
}

} // namespace

// ── SyncService ───────────────────────────────────────────────────────────────

SyncService::SyncService(std::string url, MessageCb on_message, ConnectionCb on_connection)
    : url_(std::move(url))
    , on_message_cb_(std::move(on_message))
    , on_connection_cb_(std::move(on_connection))
    , session_(soup_session_new())
    , cancellable_(g_cancellable_new())
{
    connect();
}

SyncService::~SyncService() {
    alive_ = false;

    if (reconnect_source_ != 0) {
        g_source_remove(reconnect_source_);
        reconnect_source_ = 0;
    }

    // Cancel any in-flight connection attempt; cb_connected will see the
    // cancellation error and return without touching `this`.
    g_cancellable_cancel(cancellable_);
    g_object_unref(cancellable_);

    if (ws_) {
        g_signal_handlers_disconnect_by_data(ws_, this);
        soup_websocket_connection_close(ws_, SOUP_WEBSOCKET_CLOSE_GOING_AWAY, nullptr);
        g_object_unref(ws_);
        ws_ = nullptr;
    }

    g_object_unref(session_);
}

void SyncService::connect() {
    if (!alive_) return;

    SoupMessage* msg = soup_message_new(SOUP_METHOD_GET, url_.c_str());
    if (!msg) { schedule_reconnect(); return; }

    soup_session_websocket_connect_async(
        session_, msg, nullptr, nullptr, G_PRIORITY_DEFAULT,
        cancellable_, cb_connected, this);

    g_object_unref(msg);
}

void SyncService::cb_connected(GObject* src, GAsyncResult* res, gpointer data) {
    auto*   self  = static_cast<SyncService*>(data);
    GError* error = nullptr;

    SoupWebsocketConnection* ws =
        soup_session_websocket_connect_finish(SOUP_SESSION(src), res, &error);

    // Cancelled means the SyncService is being destroyed — do not touch self.
    if (g_error_matches(error, G_IO_ERROR, G_IO_ERROR_CANCELLED)) {
        g_error_free(error);
        if (ws) g_object_unref(ws);
        return;
    }

    if (error || !ws) {
        g_clear_error(&error);
        if (ws) g_object_unref(ws);
        if (self->alive_) self->schedule_reconnect();
        return;
    }

    if (!self->alive_) {
        soup_websocket_connection_close(ws, SOUP_WEBSOCKET_CLOSE_GOING_AWAY, nullptr);
        g_object_unref(ws);
        return;
    }

    self->ws_ = ws;
    self->on_connection_cb_(true);
    g_signal_connect(ws, "message", G_CALLBACK(cb_message), self);
    g_signal_connect(ws, "closed",  G_CALLBACK(cb_closed),  self);
}

void SyncService::cb_message(SoupWebsocketConnection*, gint type, GBytes* bytes, gpointer data) {
    if (type != SOUP_WEBSOCKET_DATA_TEXT) return;

    auto*       self   = static_cast<SyncService*>(data);
    gsize       length = 0;
    const char* raw    = static_cast<const char*>(g_bytes_get_data(bytes, &length));
    auto        msg    = parse_wire(raw, length);

    if (msg.type == "init" || msg.type == "update")
        self->on_message_cb_(std::move(msg.content), msg.updated_at);
}

void SyncService::cb_closed(SoupWebsocketConnection* ws, gpointer data) {
    auto* self = static_cast<SyncService*>(data);
    g_signal_handlers_disconnect_by_data(ws, self);
    g_object_unref(ws);
    self->ws_ = nullptr;

    if (self->alive_) self->schedule_reconnect();
}

void SyncService::schedule_reconnect() {
    on_connection_cb_(false);
    if (!alive_) return;
    reconnect_source_ = g_timeout_add_seconds(3, cb_reconnect, this);
}

gboolean SyncService::cb_reconnect(gpointer data) {
    auto* self = static_cast<SyncService*>(data);
    self->reconnect_source_ = 0;
    self->connect();
    return G_SOURCE_REMOVE;
}

void SyncService::send(const std::string& content, int64_t updated_at) {
    if (!ws_ || soup_websocket_connection_get_state(ws_) != SOUP_WEBSOCKET_STATE_OPEN)
        return;
    auto json = make_update(content, updated_at);
    soup_websocket_connection_send_text(ws_, json.c_str());
}
