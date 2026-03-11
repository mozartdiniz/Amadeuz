#include "note_sync.h"

#include <json-glib/json-glib.h>

// ── JSON helpers ──────────────────────────────────────────────────────────────

namespace {

NoteWsMsg parse_ws_msg(const char* data, gsize length) {
    NoteWsMsg   msg;
    GError*     error  = nullptr;
    JsonParser* parser = json_parser_new();

    if (!json_parser_load_from_data(parser, data, (gssize)length, &error)) {
        g_clear_error(&error);
        g_object_unref(parser);
        return msg;
    }

    JsonNode* root = json_parser_get_root(parser);
    if (root && JSON_NODE_HOLDS_OBJECT(root)) {
        JsonObject* obj = json_node_get_object(root);
        if (json_object_has_member(obj, "type"))
            msg.type = json_object_get_string_member(obj, "type");
        if (json_object_has_member(obj, "title"))
            msg.title = json_object_get_string_member(obj, "title");
        if (json_object_has_member(obj, "content"))
            msg.content = json_object_get_string_member(obj, "content");
        if (json_object_has_member(obj, "updated_at"))
            msg.updated_at = json_object_get_int_member(obj, "updated_at");
    }

    g_object_unref(parser);
    return msg;
}

std::string serialize_update(const std::string& title, const std::string& content, int64_t ts) {
    JsonBuilder*   b   = json_builder_new();
    json_builder_begin_object(b);
    json_builder_set_member_name(b, "type");       json_builder_add_string_value(b, "update");
    json_builder_set_member_name(b, "title");      json_builder_add_string_value(b, title.c_str());
    json_builder_set_member_name(b, "content");    json_builder_add_string_value(b, content.c_str());
    json_builder_set_member_name(b, "updated_at"); json_builder_add_int_value(b, (gint64)ts);
    json_builder_end_object(b);

    JsonNode*      node = json_builder_get_root(b);
    JsonGenerator* gen  = json_generator_new();
    json_generator_set_root(gen, node);
    gchar* str = json_generator_to_data(gen, nullptr);
    std::string result(str);
    g_free(str);
    g_object_unref(gen);
    json_node_free(node);
    g_object_unref(b);
    return result;
}

} // namespace

// ── NoteSync ──────────────────────────────────────────────────────────────────

NoteSync::NoteSync(std::string ws_url, MsgCb on_message)
    : url_(std::move(ws_url))
    , on_message_cb_(std::move(on_message))
    , session_(soup_session_new())
    , cancel_(g_cancellable_new())
{
    connect();
}

NoteSync::~NoteSync() {
    alive_ = false;

    if (reconnect_source_ != 0) {
        g_source_remove(reconnect_source_);
        reconnect_source_ = 0;
    }

    g_cancellable_cancel(cancel_);
    g_object_unref(cancel_);

    if (ws_) {
        g_signal_handlers_disconnect_by_data(ws_, this);
        soup_websocket_connection_close(ws_, SOUP_WEBSOCKET_CLOSE_GOING_AWAY, nullptr);
        g_object_unref(ws_);
        ws_ = nullptr;
    }

    g_object_unref(session_);
}

bool NoteSync::is_connected() const {
    return ws_ && soup_websocket_connection_get_state(ws_) == SOUP_WEBSOCKET_STATE_OPEN;
}

void NoteSync::connect() {
    if (!alive_) return;

    SoupMessage* msg = soup_message_new(SOUP_METHOD_GET, url_.c_str());
    if (!msg) { schedule_reconnect(); return; }

    soup_session_websocket_connect_async(
        session_, msg, nullptr, nullptr, G_PRIORITY_DEFAULT,
        cancel_, cb_connected, this);

    g_object_unref(msg);
}

void NoteSync::cb_connected(GObject* src, GAsyncResult* res, gpointer data) {
    auto*   self  = static_cast<NoteSync*>(data);
    GError* error = nullptr;

    SoupWebsocketConnection* ws =
        soup_session_websocket_connect_finish(SOUP_SESSION(src), res, &error);

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
    g_signal_connect(ws, "message", G_CALLBACK(cb_message), self);
    g_signal_connect(ws, "closed",  G_CALLBACK(cb_closed),  self);
}

void NoteSync::cb_message(SoupWebsocketConnection*, gint type, GBytes* bytes, gpointer data) {
    if (type != SOUP_WEBSOCKET_DATA_TEXT) return;

    auto*       self   = static_cast<NoteSync*>(data);
    gsize       length = 0;
    const char* raw    = static_cast<const char*>(g_bytes_get_data(bytes, &length));
    auto        msg    = parse_ws_msg(raw, length);

    if (!msg.type.empty())
        self->on_message_cb_(msg.type, msg.title, msg.content, msg.updated_at);
}

void NoteSync::cb_closed(SoupWebsocketConnection* ws, gpointer data) {
    auto* self = static_cast<NoteSync*>(data);
    g_signal_handlers_disconnect_by_data(ws, self);
    g_object_unref(ws);
    self->ws_ = nullptr;
    if (self->alive_) self->schedule_reconnect();
}

void NoteSync::schedule_reconnect() {
    if (!alive_) return;
    reconnect_source_ = g_timeout_add_seconds(3, cb_reconnect, this);
}

gboolean NoteSync::cb_reconnect(gpointer data) {
    auto* self = static_cast<NoteSync*>(data);
    self->reconnect_source_ = 0;
    self->connect();
    return G_SOURCE_REMOVE;
}

void NoteSync::send_update(const std::string& title, const std::string& content, int64_t ts) {
    if (!is_connected()) return;
    auto json = serialize_update(title, content, ts);
    soup_websocket_connection_send_text(ws_, json.c_str());
}
