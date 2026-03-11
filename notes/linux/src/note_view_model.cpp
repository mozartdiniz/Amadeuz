#include "note_view_model.h"
#include "keyring_store.h"

#include <algorithm>
#include <chrono>
#include <memory>
#include <unordered_map>

#include <glib.h>
#include <json-glib/json-glib.h>

// ── Static helpers ────────────────────────────────────────────────────────────

static Folder parse_folder_json(JsonObject* obj) {
    Folder f;
    if (json_object_has_member(obj, "id"))         f.id         = json_object_get_string_member(obj, "id");
    if (json_object_has_member(obj, "name"))       f.name       = json_object_get_string_member(obj, "name");
    if (json_object_has_member(obj, "created_at")) f.created_at = json_object_get_int_member   (obj, "created_at");
    return f;
}

static Note parse_note_json(JsonObject* obj) {
    Note n;
    if (json_object_has_member(obj, "id"))         n.id         = json_object_get_string_member(obj, "id");
    if (json_object_has_member(obj, "folder_id"))  n.folder_id  = json_object_get_string_member(obj, "folder_id");
    if (json_object_has_member(obj, "title"))      n.title      = json_object_get_string_member(obj, "title");
    if (json_object_has_member(obj, "content"))    n.content    = json_object_get_string_member(obj, "content");
    if (json_object_has_member(obj, "updated_at")) n.updated_at = json_object_get_int_member   (obj, "updated_at");
    if (json_object_has_member(obj, "created_at")) n.created_at = json_object_get_int_member   (obj, "created_at");
    return n;
}

// ── NotesViewModel ────────────────────────────────────────────────────────────

NotesViewModel::NotesViewModel(
    AuthStateCb      on_auth_state,
    AuthErrorCb      on_auth_error,
    FoldersChangedCb on_folders,
    NotesChangedCb   on_notes,
    NoteLoadedCb     on_note_loaded,
    EditorUpdatedCb  on_editor_updated,
    ConnectionCb     on_connection)
    : on_auth_state_(std::move(on_auth_state))
    , on_auth_error_(std::move(on_auth_error))
    , on_folders_changed_(std::move(on_folders))
    , on_notes_changed_(std::move(on_notes))
    , on_note_loaded_(std::move(on_note_loaded))
    , on_editor_updated_(std::move(on_editor_updated))
    , on_connection_(std::move(on_connection))
{
    server_address_ = local_store_.load_server_address();

    // Try to restore a saved JWT from the keyring
    std::string saved_token = KeyringStore::load();
    if (!saved_token.empty()) {
        token_ = saved_token;
        api_   = std::make_unique<ApiClient>(server_address_);
        api_->set_token(token_);

        // Load local state immediately (offline-first)
        local_store_.load(folders_, notes_);
        on_auth_state_(true, {});
        on_folders_changed_();
        on_notes_changed_();

        // Sync in background
        full_sync();
    } else {
        on_auth_state_(false, {});
    }
}

NotesViewModel::~NotesViewModel() {
    if (debounce_source_ != 0) {
        g_source_remove(debounce_source_);
        debounce_source_ = 0;
    }
}

// ── Utilities ─────────────────────────────────────────────────────────────────

std::string NotesViewModel::new_id() {
    gchar* uuid = g_uuid_string_random();
    std::string result = uuid;
    g_free(uuid);
    return result;
}

int64_t NotesViewModel::now_ms() {
    return std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::system_clock::now().time_since_epoch()).count();
}

// ── Auth ──────────────────────────────────────────────────────────────────────

void NotesViewModel::do_login(const std::string& email, const std::string& password) {
    if (!api_) api_ = std::make_unique<ApiClient>(server_address_);
    api_->do_login(email, password, [this](AuthResponse auth, std::string error) {
        if (!error.empty()) { on_auth_error_(error); return; }
        apply_token(auth.token);
    });
}

void NotesViewModel::do_register(const std::string& email, const std::string& password) {
    if (!api_) api_ = std::make_unique<ApiClient>(server_address_);
    api_->do_register(email, password, [this](AuthResponse auth, std::string error) {
        if (!error.empty()) { on_auth_error_(error); return; }
        apply_token(auth.token, auth.recovery_code);
    });
}

void NotesViewModel::do_recover(
    const std::string& email, const std::string& code, const std::string& new_pwd)
{
    if (!api_) api_ = std::make_unique<ApiClient>(server_address_);
    api_->do_recover(email, code, new_pwd, [this](AuthResponse auth, std::string error) {
        if (!error.empty()) { on_auth_error_(error); return; }
        apply_token(auth.token, auth.recovery_code);
    });
}

void NotesViewModel::apply_token(const std::string& token, const std::string& recovery_code) {
    token_ = token;
    KeyringStore::save(token);

    if (!api_) api_ = std::make_unique<ApiClient>(server_address_);
    api_->set_token(token);

    // Clear old session state before syncing
    close_note_sync();
    folders_.clear();
    notes_.clear();
    clear_editor();
    selected_folder_id_ = ALL_NOTES_ID;
    selected_note_id_   = "";

    on_auth_state_(true, recovery_code);

    // Load local state then sync with server
    local_store_.load(folders_, notes_);
    on_folders_changed_();
    on_notes_changed_();
    full_sync();
}

void NotesViewModel::sign_out() {
    reset_debounce();
    close_note_sync();

    token_ = "";
    KeyringStore::clear();

    folders_.clear();
    notes_.clear();
    clear_editor();
    selected_folder_id_ = ALL_NOTES_ID;
    selected_note_id_   = "";

    on_auth_state_(false, {});
    on_connection_(false);
    on_folders_changed_();
    on_notes_changed_();
    on_note_loaded_(nullptr);
}

// ── Sync ──────────────────────────────────────────────────────────────────────

void NotesViewModel::full_sync() {
    if (!api_ || token_.empty()) return;

    api_->get_folders([this](JsonObject* obj) {
        if (!obj) {
            // Server unreachable — stay offline with local state
            on_connection_(false);
            on_folders_changed_();
            on_notes_changed_();
            return;
        }
        on_connection_(true);

        // Parse server folders
        auto server_folders = std::make_shared<std::vector<Folder>>();
        if (json_object_has_member(obj, "folders")) {
            JsonArray* arr = json_object_get_array_member(obj, "folders");
            guint len = json_array_get_length(arr);
            for (guint i = 0; i < len; i++) {
                JsonNode* elem = json_array_get_element(arr, i);
                if (JSON_NODE_HOLDS_OBJECT(elem))
                    server_folders->push_back(parse_folder_json(json_node_get_object(elem)));
            }
        }

        api_->get_notes([this, server_folders](JsonObject* obj2) {
            if (!obj2) {
                on_connection_(false);
                return;
            }

            // Parse server notes
            std::vector<Note> server_notes;
            if (json_object_has_member(obj2, "notes")) {
                JsonArray* arr = json_object_get_array_member(obj2, "notes");
                guint len = json_array_get_length(arr);
                for (guint i = 0; i < len; i++) {
                    JsonNode* elem = json_array_get_element(arr, i);
                    if (JSON_NODE_HOLDS_OBJECT(elem))
                        server_notes.push_back(parse_note_json(json_node_get_object(elem)));
                }
            }

            merge_from_server(*server_folders, std::move(server_notes));
        });
    });
}

void NotesViewModel::merge_from_server(
    std::vector<Folder> server_folders, std::vector<Note> server_notes)
{
    // ── Folders ───────────────────────────────────────────────────────────────
    // Push any local-only folders to server (created while offline)
    std::unordered_map<std::string, bool> server_folder_ids;
    for (const auto& f : server_folders) server_folder_ids[f.id] = true;

    for (const auto& lf : folders_) {
        if (server_folder_ids.find(lf.id) == server_folder_ids.end()) {
            api_->create_folder(lf.id, lf.name, nullptr);
        }
    }

    // Accept server folder list as source of truth
    folders_ = std::move(server_folders);
    on_folders_changed_();

    // ── Notes ─────────────────────────────────────────────────────────────────
    std::unordered_map<std::string, Note> server_map;
    for (auto& n : server_notes) server_map[n.id] = std::move(n);

    std::vector<Note> merged;

    for (const auto& ln : notes_) {
        auto it = server_map.find(ln.id);
        if (it != server_map.end()) {
            if (ln.updated_at > it->second.updated_at) {
                // Local is newer — keep and push to server
                merged.push_back(ln);
                api_->patch_note(ln.id, ln.title, ln.content, ln.updated_at, nullptr);
            } else {
                merged.push_back(it->second);
            }
            server_map.erase(it);
        } else {
            // Local-only note (created offline) — push to server
            merged.push_back(ln);
            api_->create_note(ln, nullptr);
        }
    }

    // Notes only on server (created by other clients)
    for (auto& [id, n] : server_map)
        merged.push_back(std::move(n));

    notes_ = std::move(merged);
    on_notes_changed_();
    local_store_.save(folders_, notes_);

    // Refresh editor if active note was updated
    if (!editing_note_id_.empty()) {
        auto it = std::find_if(notes_.begin(), notes_.end(),
            [&](const Note& n) { return n.id == editing_note_id_; });
        if (it != notes_.end()) {
            editing_title_   = it->title;
            editing_content_ = it->content;
            on_editor_updated_(it->title, it->content);
        }
    }
}

// ── Per-note WebSocket ────────────────────────────────────────────────────────

void NotesViewModel::open_note_sync(const std::string& note_id) {
    close_note_sync();
    if (!api_ || token_.empty()) return;

    std::string ws_url = api_->ws_url_for_note(note_id);

    note_sync_ = std::make_unique<NoteSync>(ws_url,
        [this, note_id](std::string type, std::string title, std::string content, int64_t ts) {
            auto it = std::find_if(notes_.begin(), notes_.end(),
                [&](const Note& n) { return n.id == note_id; });
            if (it == notes_.end()) return;
            if (ts <= it->updated_at) return; // stale

            it->title      = title;
            it->content    = content;
            it->updated_at = ts;

            on_notes_changed_();
            local_store_.save(folders_, notes_);

            if (editing_note_id_ == note_id) {
                editing_title_   = title;
                editing_content_ = content;
                on_editor_updated_(title, content);
            }
        });
}

void NotesViewModel::close_note_sync() {
    note_sync_.reset();
}

// ── Getters ───────────────────────────────────────────────────────────────────

std::vector<Note> NotesViewModel::notes_in_view() const {
    std::vector<Note> result;

    for (const auto& n : notes_) {
        bool folder_match = (selected_folder_id_ == ALL_NOTES_ID)
                         || (n.folder_id == selected_folder_id_);
        if (!folder_match) continue;

        if (!search_query_.empty()) {
            auto to_lower = [](std::string s) {
                std::transform(s.begin(), s.end(), s.begin(), ::tolower);
                return s;
            };
            std::string q = to_lower(search_query_);
            if (to_lower(n.title).find(q) == std::string::npos &&
                to_lower(n.content).find(q) == std::string::npos)
                continue;
        }

        result.push_back(n);
    }

    std::sort(result.begin(), result.end(), [](const Note& a, const Note& b) {
        return a.updated_at > b.updated_at;
    });
    return result;
}

// ── Selection ─────────────────────────────────────────────────────────────────

void NotesViewModel::select_folder(const std::string& folder_id) {
    if (!editing_note_id_.empty())
        flush_note(editing_note_id_, editing_title_, editing_content_);
    reset_debounce();
    close_note_sync();

    selected_folder_id_ = folder_id;
    selected_note_id_   = "";
    clear_editor();
    on_note_loaded_(nullptr);
    on_notes_changed_();
}

void NotesViewModel::select_note(const std::string& note_id) {
    if (!editing_note_id_.empty())
        flush_note(editing_note_id_, editing_title_, editing_content_);
    reset_debounce();
    close_note_sync();

    selected_note_id_ = note_id;

    if (note_id.empty()) {
        clear_editor();
        on_note_loaded_(nullptr);
        return;
    }

    auto it = std::find_if(notes_.begin(), notes_.end(),
        [&](const Note& n) { return n.id == note_id; });

    if (it == notes_.end()) {
        selected_note_id_ = "";
        clear_editor();
        on_note_loaded_(nullptr);
        return;
    }

    editing_note_id_ = note_id;
    editing_title_   = it->title;
    editing_content_ = it->content;
    on_note_loaded_(&*it);

    open_note_sync(note_id);
}

// ── Editor changes ────────────────────────────────────────────────────────────

void NotesViewModel::on_title_changed(const std::string& title) {
    editing_title_ = title;
    if (!editing_note_id_.empty()) reset_debounce();
}

void NotesViewModel::on_content_changed(const std::string& content) {
    editing_content_ = content;
    if (!editing_note_id_.empty()) reset_debounce();
}

void NotesViewModel::set_search_query(const std::string& query) {
    search_query_ = query;
    on_notes_changed_();
}

// ── Folder actions ────────────────────────────────────────────────────────────

void NotesViewModel::create_folder(const std::string& name) {
    Folder f{new_id(), name, now_ms()};
    folders_.push_back(f);
    on_folders_changed_();
    local_store_.save(folders_, notes_);

    if (api_ && !token_.empty())
        api_->create_folder(f.id, f.name, nullptr);
}

void NotesViewModel::rename_folder(const std::string& id, const std::string& name) {
    for (auto& f : folders_) {
        if (f.id == id) { f.name = name; break; }
    }
    on_folders_changed_();
    local_store_.save(folders_, notes_);

    if (api_ && !token_.empty())
        api_->patch_folder(id, name, nullptr);
}

void NotesViewModel::delete_folder(const std::string& id) {
    folders_.erase(std::remove_if(folders_.begin(), folders_.end(),
        [&](const Folder& f) { return f.id == id; }), folders_.end());
    notes_.erase(std::remove_if(notes_.begin(), notes_.end(),
        [&](const Note& n) { return n.folder_id == id; }), notes_.end());

    if (selected_folder_id_ == id) {
        selected_folder_id_ = ALL_NOTES_ID;
        close_note_sync();
        clear_editor();
        on_note_loaded_(nullptr);
    }

    on_folders_changed_();
    on_notes_changed_();
    local_store_.save(folders_, notes_);

    if (api_ && !token_.empty())
        api_->delete_folder(id, nullptr);
}

// ── Note actions ──────────────────────────────────────────────────────────────

void NotesViewModel::create_note() {
    // Allow creation in "All Notes" → creates unfoldered note
    std::string folder_id = (selected_folder_id_ == ALL_NOTES_ID) ? "" : selected_folder_id_;

    int64_t ts = now_ms();
    Note n;
    n.id         = new_id();
    n.folder_id  = folder_id;
    n.title      = "";
    n.content    = "";
    n.updated_at = ts;
    n.created_at = ts;

    notes_.push_back(n);

    // Select immediately
    close_note_sync();
    selected_note_id_ = n.id;
    editing_note_id_  = n.id;
    editing_title_    = "";
    editing_content_  = "";

    on_notes_changed_();

    auto it = std::find_if(notes_.begin(), notes_.end(),
        [&](const Note& note) { return note.id == n.id; });
    if (it != notes_.end()) on_note_loaded_(&*it);

    local_store_.save(folders_, notes_);

    if (api_ && !token_.empty()) {
        Note note_copy = n;
        std::string nid = n.id;
        api_->create_note(note_copy, [this, nid](JsonObject*) {
            // Open per-note WS after server confirms creation
            if (editing_note_id_ == nid && !token_.empty())
                open_note_sync(nid);
        });
    }
}

void NotesViewModel::delete_note(const std::string& id) {
    bool was_editing = (editing_note_id_ == id);

    if (was_editing) {
        reset_debounce();
        close_note_sync();
    }

    notes_.erase(std::remove_if(notes_.begin(), notes_.end(),
        [&](const Note& n) { return n.id == id; }), notes_.end());

    if (was_editing) {
        selected_note_id_ = "";
        clear_editor();
    }

    on_notes_changed_();
    if (was_editing) on_note_loaded_(nullptr);
    local_store_.save(folders_, notes_);

    if (api_ && !token_.empty())
        api_->delete_note(id, nullptr);
}

void NotesViewModel::move_note(const std::string& note_id, const std::string& target_folder_id) {
    auto it = std::find_if(notes_.begin(), notes_.end(),
        [&](const Note& n) { return n.id == note_id; });
    if (it == notes_.end()) return;

    // ALL_NOTES_ID as target means "unfoldered"
    it->folder_id = (target_folder_id == ALL_NOTES_ID) ? "" : target_folder_id;
    on_notes_changed_();
    local_store_.save(folders_, notes_);

    if (api_ && !token_.empty())
        api_->move_note(note_id, it->folder_id, nullptr);
}

// ── Server address ────────────────────────────────────────────────────────────

void NotesViewModel::set_server_address(const std::string& address) {
    server_address_ = address;
    local_store_.save_server_address(address);

    close_note_sync();
    on_connection_(false);

    // Rebuild API client with new URL (but keep token)
    api_ = std::make_unique<ApiClient>(address);
    if (!token_.empty()) {
        api_->set_token(token_);
        full_sync();
    }
}

// ── Debounce & flush ──────────────────────────────────────────────────────────

void NotesViewModel::reset_debounce() {
    if (debounce_source_ != 0) {
        g_source_remove(debounce_source_);
        debounce_source_ = 0;
    }
    if (!editing_note_id_.empty())
        debounce_source_ = g_timeout_add(500, cb_debounce, this);
}

gboolean NotesViewModel::cb_debounce(gpointer data) {
    auto* self = static_cast<NotesViewModel*>(data);
    self->debounce_source_ = 0;
    self->flush_note(self->editing_note_id_, self->editing_title_, self->editing_content_);
    return G_SOURCE_REMOVE;
}

void NotesViewModel::flush_note(
    const std::string& id, const std::string& title, const std::string& content)
{
    auto it = std::find_if(notes_.begin(), notes_.end(),
        [&](const Note& n) { return n.id == id; });
    if (it == notes_.end()) return;
    if (it->title == title && it->content == content) return;

    int64_t ts     = now_ms();
    it->title      = title;
    it->content    = content;
    it->updated_at = ts;

    local_store_.save(folders_, notes_);
    on_notes_changed_();

    if (api_ && !token_.empty()) {
        api_->patch_note(id, title, content, ts, nullptr);
        if (note_sync_) note_sync_->send_update(title, content, ts);
    }
}

void NotesViewModel::clear_editor() {
    editing_note_id_ = "";
    editing_title_   = "";
    editing_content_ = "";
}
