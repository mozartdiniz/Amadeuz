#include "local_store.h"

#include <glib.h>
#include <json-glib/json-glib.h>

LocalStore::LocalStore() {
    // XDG_DATA_HOME defaults to ~/.local/share
    std::string dir = std::string(g_get_user_data_dir()) + "/amadeuz";
    g_mkdir_with_parents(dir.c_str(), 0755);
    note_path_ = dir + "/note.json";
}

NoteData LocalStore::load() const {
    GError* error   = nullptr;
    gchar*  raw     = nullptr;
    gsize   length  = 0;

    if (!g_file_get_contents(note_path_.c_str(), &raw, &length, &error)) {
        g_clear_error(&error);
        return {};
    }

    JsonParser* parser = json_parser_new();
    NoteData    result;

    if (json_parser_load_from_data(parser, raw, (gssize)length, &error)) {
        JsonNode* root = json_parser_get_root(parser);
        if (root && JSON_NODE_HOLDS_OBJECT(root)) {
            JsonObject* obj = json_node_get_object(root);
            if (json_object_has_member(obj, "content"))
                result.content    = json_object_get_string_member(obj, "content");
            if (json_object_has_member(obj, "updatedAt"))
                result.updated_at = json_object_get_int_member(obj, "updatedAt");
        }
    }

    g_clear_error(&error);
    g_object_unref(parser);
    g_free(raw);
    return result;
}

void LocalStore::save(const std::string& content, int64_t updated_at) const {
    JsonBuilder* builder = json_builder_new();
    json_builder_begin_object(builder);
    json_builder_set_member_name(builder, "content");
    json_builder_add_string_value(builder, content.c_str());
    json_builder_set_member_name(builder, "updatedAt");
    json_builder_add_int_value(builder, (gint64)updated_at);
    json_builder_end_object(builder);

    JsonNode*      node = json_builder_get_root(builder);
    JsonGenerator* gen  = json_generator_new();
    json_generator_set_root(gen, node);

    gchar*   json_str = json_generator_to_data(gen, nullptr);
    GError*  error    = nullptr;
    g_file_set_contents(note_path_.c_str(), json_str, -1, &error);

    g_clear_error(&error);
    g_free(json_str);
    g_object_unref(gen);
    json_node_free(node);
    g_object_unref(builder);
}
