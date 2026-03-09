#include "note_view_model.h"

#include <chrono>
#include <glib.h>
#include <json-glib/json-glib.h>

static constexpr const char* DEFAULT_SERVER = "ws://localhost:8080/ws";

// ── Settings persistence (same directory as note, matching other platforms) ───

static std::string settings_path() {
    return std::string(g_get_user_data_dir()) + "/amadeuz/settings.json";
}

std::string NoteViewModel::load_server_address() {
    GError* error   = nullptr;
    gchar*  raw     = nullptr;
    gsize   length  = 0;
    std::string path = settings_path();

    if (!g_file_get_contents(path.c_str(), &raw, &length, &error)) {
        g_clear_error(&error);
        return DEFAULT_SERVER;
    }

    JsonParser* parser = json_parser_new();
    std::string result = DEFAULT_SERVER;

    if (json_parser_load_from_data(parser, raw, (gssize)length, &error)) {
        JsonNode* root = json_parser_get_root(parser);
        if (root && JSON_NODE_HOLDS_OBJECT(root)) {
            JsonObject* obj = json_node_get_object(root);
            if (json_object_has_member(obj, "serverAddress"))
                result = json_object_get_string_member(obj, "serverAddress");
        }
    }

    g_clear_error(&error);
    g_object_unref(parser);
    g_free(raw);
    return result;
}

void NoteViewModel::save_server_address(const std::string& address) {
    std::string path = settings_path();
    std::string dir  = path.substr(0, path.rfind('/'));
    g_mkdir_with_parents(dir.c_str(), 0755);

    JsonBuilder* builder = json_builder_new();
    json_builder_begin_object(builder);
    json_builder_set_member_name(builder, "serverAddress");
    json_builder_add_string_value(builder, address.c_str());
    json_builder_end_object(builder);

    JsonNode*      node = json_builder_get_root(builder);
    JsonGenerator* gen  = json_generator_new();
    json_generator_set_root(gen, node);
    gchar* json_str = json_generator_to_data(gen, nullptr);

    GError* error = nullptr;
    g_file_set_contents(path.c_str(), json_str, -1, &error);
    g_clear_error(&error);

    g_free(json_str);
    g_object_unref(gen);
    json_node_free(node);
    g_object_unref(builder);
}

// ── NoteViewModel ─────────────────────────────────────────────────────────────

NoteViewModel::NoteViewModel(ContentCb on_content, ConnectionCb on_connection)
    : on_content_cb_(std::move(on_content))
    , on_connection_cb_(std::move(on_connection))
{
    server_address_ = load_server_address();

    // Offline-first: show locally-stored note immediately.
    NoteData note       = local_store_.load();
    content_            = note.content;
    last_received_content_ = note.content;
    last_updated_at_    = note.updated_at;

    start_sync();
}

NoteViewModel::~NoteViewModel() {
    if (debounce_source_ != 0) {
        g_source_remove(debounce_source_);
        debounce_source_ = 0;
    }
}

void NoteViewModel::start_sync() {
    sync_service_ = std::make_unique<SyncService>(
        server_address_,
        [this](std::string content, int64_t updated_at) {
            handle_server_message(std::move(content), updated_at);
        },
        [this](bool connected) {
            on_connection_cb_(connected);
        });
}

void NoteViewModel::handle_server_message(std::string content, int64_t updated_at) {
    if (updated_at > last_updated_at_) {
        // Server has newer content — accept it.
        last_received_content_ = content;
        content_               = content;
        last_updated_at_       = updated_at;
        local_store_.save(content_, last_updated_at_);
        on_content_cb_(content_);

    } else if (last_updated_at_ > updated_at) {
        // We have newer content (wrote offline) — push it to the server.
        sync_service_->send(content_, last_updated_at_);
    }
    // Equal timestamps → already in sync, nothing to do.
}

void NoteViewModel::on_text_changed(const std::string& content) {
    content_ = content;

    // Cancel any pending debounce and start a fresh 500 ms window.
    if (debounce_source_ != 0) {
        g_source_remove(debounce_source_);
        debounce_source_ = 0;
    }
    debounce_source_ = g_timeout_add(500, cb_debounce, this);
}

gboolean NoteViewModel::cb_debounce(gpointer data) {
    auto* self = static_cast<NoteViewModel*>(data);
    self->debounce_source_ = 0;
    self->flush_debounce();
    return G_SOURCE_REMOVE;
}

void NoteViewModel::flush_debounce() {
    // Echo from server — don't loop it back.
    if (content_ == last_received_content_) return;

    auto now = std::chrono::duration_cast<std::chrono::milliseconds>(
                   std::chrono::system_clock::now().time_since_epoch()).count();
    last_updated_at_ = now;
    local_store_.save(content_, now);
    if (sync_service_) sync_service_->send(content_, now);
}

void NoteViewModel::set_server_address(const std::string& address) {
    server_address_ = address;
    save_server_address(address);
    on_connection_cb_(false);
    start_sync();
}
