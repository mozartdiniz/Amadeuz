#include "sync_service.h"

#include <json-glib/json-glib.h>

// ── JSON helpers ──────────────────────────────────────────────────────────────

namespace {

Folder parse_folder_obj(JsonObject* obj) {
    Folder f;
    if (json_object_has_member(obj, "id"))         f.id         = json_object_get_string_member(obj, "id");
    if (json_object_has_member(obj, "name"))       f.name       = json_object_get_string_member(obj, "name");
    if (json_object_has_member(obj, "created_at")) f.created_at = json_object_get_int_member(obj, "created_at");
    return f;
}

Note parse_note_obj(JsonObject* obj) {
    Note n;
    if (json_object_has_member(obj, "id"))         n.id         = json_object_get_string_member(obj, "id");
    if (json_object_has_member(obj, "folder_id"))  n.folder_id  = json_object_get_string_member(obj, "folder_id");
    if (json_object_has_member(obj, "title"))      n.title      = json_object_get_string_member(obj, "title");
    if (json_object_has_member(obj, "content"))    n.content    = json_object_get_string_member(obj, "content");
    if (json_object_has_member(obj, "updated_at")) n.updated_at = json_object_get_int_member(obj, "updated_at");
    if (json_object_has_member(obj, "created_at")) n.created_at = json_object_get_int_member(obj, "created_at");
    return n;
}

WireMessage parse_wire(const char* data, gsize length) {
    WireMessage msg;
    GError*     error  = nullptr;
    JsonParser* parser = json_parser_new();

    if (!json_parser_load_from_data(parser, data, (gssize)length, &error)) {
        g_clear_error(&error);
        g_object_unref(parser);
        return msg;
    }

    JsonNode* root = json_parser_get_root(parser);
    if (!root || !JSON_NODE_HOLDS_OBJECT(root)) {
        g_object_unref(parser);
        return msg;
    }

    JsonObject* obj = json_node_get_object(root);

    if (json_object_has_member(obj, "type"))
        msg.type = json_object_get_string_member(obj, "type");
    if (json_object_has_member(obj, "folder_id"))
        msg.folder_id = json_object_get_string_member(obj, "folder_id");
    if (json_object_has_member(obj, "note_id"))
        msg.note_id = json_object_get_string_member(obj, "note_id");
    if (json_object_has_member(obj, "name"))
        msg.name = json_object_get_string_member(obj, "name");
    if (json_object_has_member(obj, "title"))
        msg.title = json_object_get_string_member(obj, "title");
    if (json_object_has_member(obj, "content"))
        msg.content = json_object_get_string_member(obj, "content");
    if (json_object_has_member(obj, "updated_at"))
        msg.updated_at = json_object_get_int_member(obj, "updated_at");

    if (json_object_has_member(obj, "folder")) {
        JsonNode* fn = json_object_get_member(obj, "folder");
        if (JSON_NODE_HOLDS_OBJECT(fn)) {
            msg.folder     = parse_folder_obj(json_node_get_object(fn));
            msg.has_folder = true;
        }
    }

    if (json_object_has_member(obj, "note")) {
        JsonNode* nn = json_object_get_member(obj, "note");
        if (JSON_NODE_HOLDS_OBJECT(nn)) {
            msg.note     = parse_note_obj(json_node_get_object(nn));
            msg.has_note = true;
        }
    }

    if (json_object_has_member(obj, "folders")) {
        JsonArray* arr = json_object_get_array_member(obj, "folders");
        guint len = json_array_get_length(arr);
        for (guint i = 0; i < len; i++) {
            JsonNode* elem = json_array_get_element(arr, i);
            if (JSON_NODE_HOLDS_OBJECT(elem))
                msg.folders.push_back(parse_folder_obj(json_node_get_object(elem)));
        }
    }

    if (json_object_has_member(obj, "notes")) {
        JsonArray* arr = json_object_get_array_member(obj, "notes");
        guint len = json_array_get_length(arr);
        for (guint i = 0; i < len; i++) {
            JsonNode* elem = json_array_get_element(arr, i);
            if (JSON_NODE_HOLDS_OBJECT(elem))
                msg.notes.push_back(parse_note_obj(json_node_get_object(elem)));
        }
    }

    g_object_unref(parser);
    return msg;
}

std::string serialize_wire(const WireMessage& msg) {
    JsonBuilder* b = json_builder_new();
    json_builder_begin_object(b);

    json_builder_set_member_name(b, "type");
    json_builder_add_string_value(b, msg.type.c_str());

    if (!msg.folder_id.empty()) {
        json_builder_set_member_name(b, "folder_id");
        json_builder_add_string_value(b, msg.folder_id.c_str());
    }
    if (!msg.note_id.empty()) {
        json_builder_set_member_name(b, "note_id");
        json_builder_add_string_value(b, msg.note_id.c_str());
    }
    if (!msg.name.empty()) {
        json_builder_set_member_name(b, "name");
        json_builder_add_string_value(b, msg.name.c_str());
    }

    // Always include title/content for note write operations (even if empty string).
    bool is_note_write = (msg.type == "create_note" || msg.type == "update_note");
    if (is_note_write || !msg.title.empty()) {
        json_builder_set_member_name(b, "title");
        json_builder_add_string_value(b, msg.title.c_str());
    }
    if (msg.type == "update_note" || !msg.content.empty()) {
        json_builder_set_member_name(b, "content");
        json_builder_add_string_value(b, msg.content.c_str());
    }
    if (msg.updated_at != 0) {
        json_builder_set_member_name(b, "updated_at");
        json_builder_add_int_value(b, (gint64)msg.updated_at);
    }

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

    if (!msg.type.empty())
        self->on_message_cb_(std::move(msg));
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

void SyncService::send(const WireMessage& msg) {
    if (!ws_ || soup_websocket_connection_get_state(ws_) != SOUP_WEBSOCKET_STATE_OPEN)
        return;
    auto json = serialize_wire(msg);
    soup_websocket_connection_send_text(ws_, json.c_str());
}
