#pragma once
#include "local_store.h"
#include "sync_service.h"

#include <functional>
#include <memory>
#include <string>
#include <vector>

#include <glib.h>

/// All business logic: offline-first load, folder/note CRUD, 500 ms debounce,
/// timestamp-based merge. Mirror of the macOS and Windows NotesViewModel.
/// All methods must be called on the GLib main thread.
class NotesViewModel {
public:
    static constexpr const char* ALL_NOTES_ID = "__all__";

    using FoldersChangedCb = std::function<void()>;
    using NotesChangedCb   = std::function<void()>;
    using NoteLoadedCb     = std::function<void(const Note*)>; // nullptr = clear editor
    using EditorUpdatedCb  = std::function<void(const std::string& title, const std::string& content)>;
    using ConnectionCb     = std::function<void(bool connected)>;

    NotesViewModel(FoldersChangedCb, NotesChangedCb, NoteLoadedCb, EditorUpdatedCb, ConnectionCb);
    ~NotesViewModel();

    // ── Getters ───────────────────────────────────────────────────────────────

    const std::vector<Folder>& folders()           const { return folders_; }
    std::vector<Note>          notes_in_view()      const;
    const std::string&         selected_folder_id() const { return selected_folder_id_; }
    const std::string&         selected_note_id()   const { return selected_note_id_; }
    const std::string&         server_address()     const { return server_address_; }

    // ── UI actions ────────────────────────────────────────────────────────────

    void select_folder(const std::string& folder_id);
    void select_note(const std::string& note_id);
    void on_title_changed(const std::string& title);
    void on_content_changed(const std::string& content);

    void create_folder(const std::string& name);
    void rename_folder(const std::string& id, const std::string& name);
    void delete_folder(const std::string& id);

    void create_note();
    void delete_note(const std::string& id);

    void set_server_address(const std::string& address);

private:
    void start_sync();
    void handle_message(WireMessage msg);
    void handle_init(std::vector<Folder> server_folders, std::vector<Note> server_notes);
    void flush_note(const std::string& id, const std::string& title, const std::string& content);
    void reset_debounce();
    void clear_editor();

    static gboolean cb_debounce(gpointer data);

    FoldersChangedCb on_folders_changed_;
    NotesChangedCb   on_notes_changed_;
    NoteLoadedCb     on_note_loaded_;
    EditorUpdatedCb  on_editor_updated_;
    ConnectionCb     on_connection_;

    LocalStore                   local_store_;
    std::unique_ptr<SyncService> sync_service_;

    std::vector<Folder> folders_;
    std::vector<Note>   notes_;

    std::string selected_folder_id_{ALL_NOTES_ID};
    std::string selected_note_id_;
    std::string editing_note_id_;
    std::string editing_title_;
    std::string editing_content_;

    std::string server_address_;
    guint       debounce_source_{0};
};
