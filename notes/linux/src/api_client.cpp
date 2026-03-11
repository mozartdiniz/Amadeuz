#include "api_client.h"

#include <cstring>

// ── JSON builder helpers ───────────────────────────────────────────────────────

namespace {

std::string json_build(std::function<void(JsonBuilder*)> fn) {
    JsonBuilder*   b    = json_builder_new();
    fn(b);
    JsonNode*      node = json_builder_get_root(b);
    JsonGenerator* gen  = json_generator_new();
    json_generator_set_root(gen, node);
    gchar*         str  = json_generator_to_data(gen, nullptr);
    std::string    result(str);
    g_free(str);
    g_object_unref(gen);
    json_node_free(node);
    g_object_unref(b);
    return result;
}

} // namespace

// ── ApiClient ─────────────────────────────────────────────────────────────────

ApiClient::ApiClient(std::string base_url)
    : base_url_(std::move(base_url))
    , session_(soup_session_new())
    , cancel_(g_cancellable_new())
{}

ApiClient::~ApiClient() {
    g_cancellable_cancel(cancel_);
    g_object_unref(cancel_);
    g_object_unref(session_);
}

void ApiClient::set_token(const std::string& token) {
    token_ = token;
}

std::string ApiClient::ws_url_for_note(const std::string& note_id) const {
    std::string base = base_url_;
    // Remove trailing slash
    if (!base.empty() && base.back() == '/') base.pop_back();

    // Convert http→ws, https→wss
    std::string ws;
    if (base.size() >= 8 && base.substr(0, 8) == "https://")
        ws = "wss://" + base.substr(8);
    else if (base.size() >= 7 && base.substr(0, 7) == "http://")
        ws = "ws://" + base.substr(7);
    else
        ws = base;

    return ws + "/notes/" + note_id + "/ws?token=" + token_;
}

// ── Internal helpers ──────────────────────────────────────────────────────────

SoupMessage* ApiClient::make_msg(const char* method, const char* path, const char* body) {
    std::string url = base_url_;
    if (!url.empty() && url.back() == '/') url.pop_back();
    url += path;

    SoupMessage* msg = soup_message_new(method, url.c_str());
    if (!msg) return nullptr;

    if (!token_.empty()) {
        std::string auth = "Bearer " + token_;
        soup_message_headers_append(
            soup_message_get_request_headers(msg), "Authorization", auth.c_str());
    }
    if (body) {
        GBytes* b = g_bytes_new(body, strlen(body));
        soup_message_set_request_body_from_bytes(msg, "application/json", b);
        g_bytes_unref(b);
    }
    return msg;
}

void ApiClient::send_obj(SoupMessage* msg, ObjCb cb) {
    auto* req   = new Req{msg, std::move(cb), {}, {}};
    g_object_ref(msg); // keep alive in Req
    soup_session_send_and_read_async(
        session_, msg, G_PRIORITY_DEFAULT, cancel_, cb_obj, req);
    g_object_unref(msg);
}

void ApiClient::send_bool(SoupMessage* msg, BoolCb cb) {
    auto* req = new Req{msg, {}, std::move(cb), {}};
    g_object_ref(msg);
    soup_session_send_and_read_async(
        session_, msg, G_PRIORITY_DEFAULT, cancel_, cb_bool, req);
    g_object_unref(msg);
}

void ApiClient::send_auth(SoupMessage* msg, AuthCb cb) {
    auto* req = new Req{msg, {}, {}, std::move(cb)};
    g_object_ref(msg);
    soup_session_send_and_read_async(
        session_, msg, G_PRIORITY_DEFAULT, cancel_, cb_auth, req);
    g_object_unref(msg);
}

// ── Async callbacks ───────────────────────────────────────────────────────────

void ApiClient::cb_obj(GObject* src, GAsyncResult* res, gpointer data) {
    auto*   req   = static_cast<Req*>(data);
    GError* err   = nullptr;
    GBytes* bytes = soup_session_send_and_read_finish(SOUP_SESSION(src), res, &err);

    if (err) {
        g_clear_error(&err);
        if (bytes) g_bytes_unref(bytes);
        g_object_unref(req->msg);
        if (req->obj_cb) req->obj_cb(nullptr);
        delete req;
        return;
    }

    guint status = soup_message_get_status(req->msg);
    g_object_unref(req->msg);

    if (status < 200 || status >= 300) {
        if (bytes) g_bytes_unref(bytes);
        if (req->obj_cb) req->obj_cb(nullptr);
        delete req;
        return;
    }

    JsonParser* parser  = json_parser_new();
    JsonObject* result  = nullptr;

    if (bytes) {
        gsize       len = 0;
        const char* raw = (const char*)g_bytes_get_data(bytes, &len);
        if (json_parser_load_from_data(parser, raw, (gssize)len, nullptr)) {
            JsonNode* root = json_parser_get_root(parser);
            if (root && JSON_NODE_HOLDS_OBJECT(root))
                result = json_node_get_object(root);
        }
    }

    if (req->obj_cb) req->obj_cb(result); // callback runs before parser is freed
    g_object_unref(parser);
    if (bytes) g_bytes_unref(bytes);
    delete req;
}

void ApiClient::cb_bool(GObject* src, GAsyncResult* res, gpointer data) {
    auto*   req   = static_cast<Req*>(data);
    GError* err   = nullptr;
    GBytes* bytes = soup_session_send_and_read_finish(SOUP_SESSION(src), res, &err);

    bool ok = (err == nullptr);
    g_clear_error(&err);
    if (bytes) g_bytes_unref(bytes);

    if (ok) {
        guint status = soup_message_get_status(req->msg);
        ok = (status >= 200 && status < 300);
    }

    g_object_unref(req->msg);
    if (req->bool_cb) req->bool_cb(ok);
    delete req;
}

void ApiClient::cb_auth(GObject* src, GAsyncResult* res, gpointer data) {
    auto*   req   = static_cast<Req*>(data);
    GError* err   = nullptr;
    GBytes* bytes = soup_session_send_and_read_finish(SOUP_SESSION(src), res, &err);

    if (err) {
        std::string msg = err->message ? err->message : "Network error";
        g_clear_error(&err);
        if (bytes) g_bytes_unref(bytes);
        g_object_unref(req->msg);
        if (req->auth_cb) req->auth_cb({}, msg);
        delete req;
        return;
    }

    guint       status = soup_message_get_status(req->msg);
    g_object_unref(req->msg);

    AuthResponse auth_res;
    std::string  error_str;

    if (bytes) {
        gsize       len    = 0;
        const char* raw    = (const char*)g_bytes_get_data(bytes, &len);
        JsonParser* parser = json_parser_new();

        if (json_parser_load_from_data(parser, raw, (gssize)len, nullptr)) {
            JsonNode* root = json_parser_get_root(parser);
            if (root && JSON_NODE_HOLDS_OBJECT(root)) {
                JsonObject* obj = json_node_get_object(root);
                if (json_object_has_member(obj, "token"))
                    auth_res.token = json_object_get_string_member(obj, "token");
                if (json_object_has_member(obj, "recovery_code"))
                    auth_res.recovery_code = json_object_get_string_member(obj, "recovery_code");
                if (status >= 400) {
                    if (json_object_has_member(obj, "error"))
                        error_str = json_object_get_string_member(obj, "error");
                    else if (json_object_has_member(obj, "message"))
                        error_str = json_object_get_string_member(obj, "message");
                }
            }
        }
        g_object_unref(parser);
        g_bytes_unref(bytes);
    }

    if (auth_res.token.empty() && error_str.empty()) {
        error_str = (status >= 400) ? "Authentication failed" : "Unexpected response";
    }

    if (req->auth_cb) req->auth_cb(auth_res, error_str);
    delete req;
}

// ── Auth endpoints ────────────────────────────────────────────────────────────

void ApiClient::do_login(std::string email, std::string password, AuthCb cb) {
    std::string body = json_build([&](JsonBuilder* b) {
        json_builder_begin_object(b);
        json_builder_set_member_name(b, "email");    json_builder_add_string_value(b, email.c_str());
        json_builder_set_member_name(b, "password"); json_builder_add_string_value(b, password.c_str());
        json_builder_end_object(b);
    });
    SoupMessage* msg = make_msg("POST", "/auth/login", body.c_str());
    if (!msg) { cb({}, "Failed to create request"); return; }
    send_auth(msg, std::move(cb));
}

void ApiClient::do_register(std::string email, std::string password, AuthCb cb) {
    std::string body = json_build([&](JsonBuilder* b) {
        json_builder_begin_object(b);
        json_builder_set_member_name(b, "email");    json_builder_add_string_value(b, email.c_str());
        json_builder_set_member_name(b, "password"); json_builder_add_string_value(b, password.c_str());
        json_builder_end_object(b);
    });
    SoupMessage* msg = make_msg("POST", "/auth/register", body.c_str());
    if (!msg) { cb({}, "Failed to create request"); return; }
    send_auth(msg, std::move(cb));
}

void ApiClient::do_recover(std::string email, std::string code, std::string new_pwd, AuthCb cb) {
    std::string body = json_build([&](JsonBuilder* b) {
        json_builder_begin_object(b);
        json_builder_set_member_name(b, "email");        json_builder_add_string_value(b, email.c_str());
        json_builder_set_member_name(b, "recovery_code"); json_builder_add_string_value(b, code.c_str());
        json_builder_set_member_name(b, "new_password");  json_builder_add_string_value(b, new_pwd.c_str());
        json_builder_end_object(b);
    });
    SoupMessage* msg = make_msg("POST", "/auth/recover", body.c_str());
    if (!msg) { cb({}, "Failed to create request"); return; }
    send_auth(msg, std::move(cb));
}

// ── Folder endpoints ──────────────────────────────────────────────────────────

void ApiClient::get_folders(ObjCb cb) {
    SoupMessage* msg = make_msg("GET", "/folders");
    if (!msg) { if (cb) cb(nullptr); return; }
    send_obj(msg, std::move(cb));
}

void ApiClient::create_folder(std::string id, std::string name, ObjCb cb) {
    std::string body = json_build([&](JsonBuilder* b) {
        json_builder_begin_object(b);
        json_builder_set_member_name(b, "id");   json_builder_add_string_value(b, id.c_str());
        json_builder_set_member_name(b, "name"); json_builder_add_string_value(b, name.c_str());
        json_builder_end_object(b);
    });
    SoupMessage* msg = make_msg("POST", "/folders", body.c_str());
    if (!msg) { if (cb) cb(nullptr); return; }
    send_obj(msg, std::move(cb));
}

void ApiClient::patch_folder(std::string id, std::string name, ObjCb cb) {
    std::string path = "/folders/" + id;
    std::string body = json_build([&](JsonBuilder* b) {
        json_builder_begin_object(b);
        json_builder_set_member_name(b, "name"); json_builder_add_string_value(b, name.c_str());
        json_builder_end_object(b);
    });
    SoupMessage* msg = make_msg("PATCH", path.c_str(), body.c_str());
    if (!msg) { if (cb) cb(nullptr); return; }
    send_obj(msg, std::move(cb));
}

void ApiClient::delete_folder(std::string id, BoolCb cb) {
    std::string  path = "/folders/" + id;
    SoupMessage* msg  = make_msg("DELETE", path.c_str());
    if (!msg) { if (cb) cb(false); return; }
    send_bool(msg, std::move(cb));
}

// ── Note endpoints ────────────────────────────────────────────────────────────

void ApiClient::get_notes(ObjCb cb) {
    SoupMessage* msg = make_msg("GET", "/notes");
    if (!msg) { if (cb) cb(nullptr); return; }
    send_obj(msg, std::move(cb));
}

void ApiClient::create_note(Note note, ObjCb cb) {
    std::string body = json_build([&](JsonBuilder* b) {
        json_builder_begin_object(b);
        json_builder_set_member_name(b, "id");         json_builder_add_string_value(b, note.id.c_str());
        json_builder_set_member_name(b, "folder_id");  json_builder_add_string_value(b, note.folder_id.c_str());
        json_builder_set_member_name(b, "title");      json_builder_add_string_value(b, note.title.c_str());
        json_builder_set_member_name(b, "content");    json_builder_add_string_value(b, note.content.c_str());
        json_builder_set_member_name(b, "updated_at"); json_builder_add_int_value(b, (gint64)note.updated_at);
        json_builder_set_member_name(b, "created_at"); json_builder_add_int_value(b, (gint64)note.created_at);
        json_builder_end_object(b);
    });
    SoupMessage* msg = make_msg("POST", "/notes", body.c_str());
    if (!msg) { if (cb) cb(nullptr); return; }
    send_obj(msg, std::move(cb));
}

void ApiClient::patch_note(std::string id, std::string title, std::string content,
                            int64_t updated_at, ObjCb cb) {
    std::string path = "/notes/" + id;
    std::string body = json_build([&](JsonBuilder* b) {
        json_builder_begin_object(b);
        json_builder_set_member_name(b, "title");      json_builder_add_string_value(b, title.c_str());
        json_builder_set_member_name(b, "content");    json_builder_add_string_value(b, content.c_str());
        json_builder_set_member_name(b, "updated_at"); json_builder_add_int_value(b, (gint64)updated_at);
        json_builder_end_object(b);
    });
    SoupMessage* msg = make_msg("PATCH", path.c_str(), body.c_str());
    if (!msg) { if (cb) cb(nullptr); return; }
    send_obj(msg, std::move(cb));
}

void ApiClient::move_note(std::string id, std::string folder_id, ObjCb cb) {
    std::string path = "/notes/" + id + "/move";
    std::string body = json_build([&](JsonBuilder* b) {
        json_builder_begin_object(b);
        json_builder_set_member_name(b, "folder_id"); json_builder_add_string_value(b, folder_id.c_str());
        json_builder_end_object(b);
    });
    SoupMessage* msg = make_msg("PATCH", path.c_str(), body.c_str());
    if (!msg) { if (cb) cb(nullptr); return; }
    send_obj(msg, std::move(cb));
}

void ApiClient::delete_note(std::string id, BoolCb cb) {
    std::string  path = "/notes/" + id;
    SoupMessage* msg  = make_msg("DELETE", path.c_str());
    if (!msg) { if (cb) cb(false); return; }
    send_bool(msg, std::move(cb));
}
