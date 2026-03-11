#include "local_store.h"

#include <glib.h>
#include <json-glib/json-glib.h>

// ── JSON helpers ──────────────────────────────────────────────────────────────

namespace {

Folder parse_folder_obj(JsonObject* obj) {
    Folder f;
    if (json_object_has_member(obj, "id"))
        f.id = json_object_get_string_member(obj, "id");
    if (json_object_has_member(obj, "name"))
        f.name = json_object_get_string_member(obj, "name");
    if (json_object_has_member(obj, "created_at"))
        f.created_at = json_object_get_int_member(obj, "created_at");
    return f;
}

Note parse_note_obj(JsonObject* obj) {
    Note n;
    if (json_object_has_member(obj, "id"))
        n.id = json_object_get_string_member(obj, "id");
    if (json_object_has_member(obj, "folder_id"))
        n.folder_id = json_object_get_string_member(obj, "folder_id");
    if (json_object_has_member(obj, "title"))
        n.title = json_object_get_string_member(obj, "title");
    if (json_object_has_member(obj, "content"))
        n.content = json_object_get_string_member(obj, "content");
    if (json_object_has_member(obj, "updated_at"))
        n.updated_at = json_object_get_int_member(obj, "updated_at");
    if (json_object_has_member(obj, "created_at"))
        n.created_at = json_object_get_int_member(obj, "created_at");
    return n;
}

void add_folder_to_builder(JsonBuilder* b, const Folder& f) {
    json_builder_begin_object(b);
    json_builder_set_member_name(b, "id");         json_builder_add_string_value(b, f.id.c_str());
    json_builder_set_member_name(b, "name");       json_builder_add_string_value(b, f.name.c_str());
    json_builder_set_member_name(b, "created_at"); json_builder_add_int_value(b, (gint64)f.created_at);
    json_builder_end_object(b);
}

void add_note_to_builder(JsonBuilder* b, const Note& n) {
    json_builder_begin_object(b);
    json_builder_set_member_name(b, "id");         json_builder_add_string_value(b, n.id.c_str());
    json_builder_set_member_name(b, "folder_id");  json_builder_add_string_value(b, n.folder_id.c_str());
    json_builder_set_member_name(b, "title");      json_builder_add_string_value(b, n.title.c_str());
    json_builder_set_member_name(b, "content");    json_builder_add_string_value(b, n.content.c_str());
    json_builder_set_member_name(b, "updated_at"); json_builder_add_int_value(b, (gint64)n.updated_at);
    json_builder_set_member_name(b, "created_at"); json_builder_add_int_value(b, (gint64)n.created_at);
    json_builder_end_object(b);
}

// Migrate old ws://host/ws format → http://host
std::string migrate_address(const std::string& addr) {
    if (addr.size() >= 5 && addr.substr(0, 5) == "ws://") {
        std::string host = addr.substr(5);
        if (host.size() >= 3 && host.substr(host.size() - 3) == "/ws")
            host = host.substr(0, host.size() - 3);
        return "http://" + host;
    }
    if (addr.size() >= 6 && addr.substr(0, 6) == "wss://") {
        std::string host = addr.substr(6);
        if (host.size() >= 3 && host.substr(host.size() - 3) == "/ws")
            host = host.substr(0, host.size() - 3);
        return "https://" + host;
    }
    return addr;
}

} // namespace

// ── LocalStore ────────────────────────────────────────────────────────────────

LocalStore::LocalStore() {
    std::string dir = std::string(g_get_user_data_dir()) + "/amadeuz";
    g_mkdir_with_parents(dir.c_str(), 0755);
    data_path_     = dir + "/data.json";
    settings_path_ = dir + "/settings.json";
}

void LocalStore::load(std::vector<Folder>& folders, std::vector<Note>& notes) const {
    folders.clear();
    notes.clear();

    GError* error  = nullptr;
    gchar*  raw    = nullptr;
    gsize   length = 0;

    if (!g_file_get_contents(data_path_.c_str(), &raw, &length, &error)) {
        g_clear_error(&error);
        return;
    }

    JsonParser* parser = json_parser_new();
    if (json_parser_load_from_data(parser, raw, (gssize)length, &error)) {
        JsonNode* root = json_parser_get_root(parser);
        if (root && JSON_NODE_HOLDS_OBJECT(root)) {
            JsonObject* obj = json_node_get_object(root);

            if (json_object_has_member(obj, "folders")) {
                JsonArray* arr = json_object_get_array_member(obj, "folders");
                guint len = json_array_get_length(arr);
                for (guint i = 0; i < len; i++) {
                    JsonNode* elem = json_array_get_element(arr, i);
                    if (JSON_NODE_HOLDS_OBJECT(elem))
                        folders.push_back(parse_folder_obj(json_node_get_object(elem)));
                }
            }

            if (json_object_has_member(obj, "notes")) {
                JsonArray* arr = json_object_get_array_member(obj, "notes");
                guint len = json_array_get_length(arr);
                for (guint i = 0; i < len; i++) {
                    JsonNode* elem = json_array_get_element(arr, i);
                    if (JSON_NODE_HOLDS_OBJECT(elem))
                        notes.push_back(parse_note_obj(json_node_get_object(elem)));
                }
            }
        }
    }

    g_clear_error(&error);
    g_object_unref(parser);
    g_free(raw);
}

void LocalStore::save(const std::vector<Folder>& folders, const std::vector<Note>& notes) const {
    JsonBuilder* b = json_builder_new();
    json_builder_begin_object(b);

    json_builder_set_member_name(b, "folders");
    json_builder_begin_array(b);
    for (const auto& f : folders) add_folder_to_builder(b, f);
    json_builder_end_array(b);

    json_builder_set_member_name(b, "notes");
    json_builder_begin_array(b);
    for (const auto& n : notes) add_note_to_builder(b, n);
    json_builder_end_array(b);

    json_builder_end_object(b);

    JsonNode*      node = json_builder_get_root(b);
    JsonGenerator* gen  = json_generator_new();
    json_generator_set_root(gen, node);
    gchar* str = json_generator_to_data(gen, nullptr);

    // Atomic write via temp file
    std::string tmp = data_path_ + ".tmp";
    GError* error = nullptr;
    g_file_set_contents(tmp.c_str(), str, -1, &error);
    g_clear_error(&error);
    rename(tmp.c_str(), data_path_.c_str());

    g_free(str);
    g_object_unref(gen);
    json_node_free(node);
    g_object_unref(b);
}

std::string LocalStore::load_server_address() const {
    static constexpr const char* DEFAULT = "http://localhost:8080";

    GError* error  = nullptr;
    gchar*  raw    = nullptr;
    gsize   length = 0;

    if (!g_file_get_contents(settings_path_.c_str(), &raw, &length, &error)) {
        g_clear_error(&error);
        return DEFAULT;
    }

    JsonParser* parser = json_parser_new();
    std::string result = DEFAULT;

    if (json_parser_load_from_data(parser, raw, (gssize)length, &error)) {
        JsonNode* root = json_parser_get_root(parser);
        if (root && JSON_NODE_HOLDS_OBJECT(root)) {
            JsonObject* obj = json_node_get_object(root);
            if (json_object_has_member(obj, "serverAddress"))
                result = migrate_address(json_object_get_string_member(obj, "serverAddress"));
        }
    }

    g_clear_error(&error);
    g_object_unref(parser);
    g_free(raw);
    return result;
}

void LocalStore::save_server_address(const std::string& address) const {
    JsonBuilder* b = json_builder_new();
    json_builder_begin_object(b);
    json_builder_set_member_name(b, "serverAddress");
    json_builder_add_string_value(b, address.c_str());
    json_builder_end_object(b);

    JsonNode*      node = json_builder_get_root(b);
    JsonGenerator* gen  = json_generator_new();
    json_generator_set_root(gen, node);
    gchar* str = json_generator_to_data(gen, nullptr);

    GError* error = nullptr;
    g_file_set_contents(settings_path_.c_str(), str, -1, &error);
    g_clear_error(&error);

    g_free(str);
    g_object_unref(gen);
    json_node_free(node);
    g_object_unref(b);
}
