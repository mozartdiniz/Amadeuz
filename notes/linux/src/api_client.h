#pragma once
#include "models.h"

#include <functional>
#include <string>

#include <libsoup/soup.h>
#include <json-glib/json-glib.h>

/// All REST calls against the amadeuz server.
/// Callbacks are invoked on the GLib main thread (safe to touch GTK from them).
/// On network error or non-2xx response, ObjCb receives nullptr.
class ApiClient {
public:
    using ObjCb  = std::function<void(JsonObject* obj)>;
    using BoolCb = std::function<void(bool ok)>;
    using AuthCb = std::function<void(AuthResponse auth, std::string error)>;

    explicit ApiClient(std::string base_url);
    ~ApiClient();

    void set_token(const std::string& token);

    /// Build the ws:// URL for a per-note WebSocket connection.
    std::string ws_url_for_note(const std::string& note_id) const;

    // ── Auth ──────────────────────────────────────────────────────────────────
    void do_login(std::string email, std::string password, AuthCb cb);
    void do_register(std::string email, std::string password, AuthCb cb);
    void do_recover(std::string email, std::string code, std::string new_pwd, AuthCb cb);

    // ── Folders ───────────────────────────────────────────────────────────────
    void get_folders(ObjCb cb);
    void create_folder(std::string id, std::string name, ObjCb cb);
    void patch_folder(std::string id, std::string name, ObjCb cb);
    void delete_folder(std::string id, BoolCb cb);

    // ── Notes ─────────────────────────────────────────────────────────────────
    void get_notes(ObjCb cb);
    void create_note(Note note, ObjCb cb);
    void patch_note(std::string id, std::string title, std::string content,
                    int64_t updated_at, ObjCb cb);
    void move_note(std::string id, std::string folder_id, ObjCb cb);
    void delete_note(std::string id, BoolCb cb);

private:
    struct Req {
        SoupMessage* msg; // kept alive for status check; unref'd by callback
        ObjCb        obj_cb;
        BoolCb       bool_cb;
        AuthCb       auth_cb;
    };

    SoupMessage* make_msg(const char* method, const char* path, const char* body = nullptr);
    void         send_obj (SoupMessage* msg, ObjCb  cb);
    void         send_bool(SoupMessage* msg, BoolCb cb);
    void         send_auth(SoupMessage* msg, AuthCb cb);

    static void cb_obj (GObject* src, GAsyncResult* res, gpointer data);
    static void cb_bool(GObject* src, GAsyncResult* res, gpointer data);
    static void cb_auth(GObject* src, GAsyncResult* res, gpointer data);

    std::string   base_url_;
    std::string   token_;
    SoupSession*  session_{nullptr};
    GCancellable* cancel_{nullptr};
};
