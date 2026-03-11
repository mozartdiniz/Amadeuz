#pragma once
#include "local_store.h"
#include "api_client.h"
#include "note_sync.h"

#include <functional>
#include <memory>
#include <string>
#include <vector>

#include <glib.h>

/// All business logic: auth, offline-first sync, folder/note CRUD,
/// 500 ms debounce, per-note WebSocket, search filter, move note.
/// All methods must be called on the GLib main thread.
class NotesViewModel {
public:
    static constexpr const char* ALL_NOTES_ID = "__all__";

    using AuthStateCb      = std::function<void(bool logged_in, std::string recovery_code)>;
    using AuthErrorCb      = std::function<void(std::string error)>;
    using FoldersChangedCb = std::function<void()>;
    using NotesChangedCb   = std::function<void()>;
    using NoteLoadedCb     = std::function<void(const Note*)>; // nullptr = clear editor
    using EditorUpdatedCb  = std::function<void(const std::string& title, const std::string& content)>;
    using ConnectionCb     = std::function<void(bool connected)>;

    NotesViewModel(AuthStateCb, AuthErrorCb,
                   FoldersChangedCb, NotesChangedCb,
                   NoteLoadedCb, EditorUpdatedCb, ConnectionCb);
    ~NotesViewModel();

    // ── Auth ──────────────────────────────────────────────────────────────────
    void do_login   (const std::string& email, const std::string& password);
    void do_register(const std::string& email, const std::string& password);
    void do_recover (const std::string& email, const std::string& code,
                     const std::string& new_pwd);
    void sign_out();

    bool is_logged_in() const { return !token_.empty(); }

    // ── Getters ───────────────────────────────────────────────────────────────
    const std::vector<Folder>& folders()           const { return folders_; }
    std::vector<Note>          notes_in_view()     const;
    const std::string&         selected_folder_id()const { return selected_folder_id_; }
    const std::string&         selected_note_id()  const { return selected_note_id_; }
    const std::string&         server_address()    const { return server_address_; }
    const std::string&         search_query()      const { return search_query_; }

    // ── UI actions ────────────────────────────────────────────────────────────
    void select_folder(const std::string& folder_id);
    void select_note  (const std::string& note_id);
    void on_title_changed  (const std::string& title);
    void on_content_changed(const std::string& content);
    void set_search_query  (const std::string& query);

    void create_folder(const std::string& name);
    void rename_folder(const std::string& id, const std::string& name);
    void delete_folder(const std::string& id);

    void create_note();
    void delete_note(const std::string& id);
    void move_note  (const std::string& note_id, const std::string& target_folder_id);

    void set_server_address(const std::string& address);

private:
    void apply_token(const std::string& token, const std::string& recovery_code = {});
    void full_sync();
    void merge_from_server(std::vector<Folder> server_folders, std::vector<Note> server_notes);

    void open_note_sync(const std::string& note_id);
    void close_note_sync();

    void flush_note(const std::string& id, const std::string& title, const std::string& content);
    void reset_debounce();
    void clear_editor();

    static std::string new_id();
    static int64_t     now_ms();

    static gboolean cb_debounce(gpointer data);

    // ── Callbacks ─────────────────────────────────────────────────────────────
    AuthStateCb      on_auth_state_;
    AuthErrorCb      on_auth_error_;
    FoldersChangedCb on_folders_changed_;
    NotesChangedCb   on_notes_changed_;
    NoteLoadedCb     on_note_loaded_;
    EditorUpdatedCb  on_editor_updated_;
    ConnectionCb     on_connection_;

    // ── State ─────────────────────────────────────────────────────────────────
    LocalStore                 local_store_;
    std::unique_ptr<ApiClient> api_;
    std::unique_ptr<NoteSync>  note_sync_;

    std::vector<Folder> folders_;
    std::vector<Note>   notes_;

    std::string token_;
    std::string server_address_;

    std::string selected_folder_id_{ALL_NOTES_ID};
    std::string selected_note_id_;
    std::string editing_note_id_;
    std::string editing_title_;
    std::string editing_content_;

    std::string search_query_;
    guint       debounce_source_{0};
};
