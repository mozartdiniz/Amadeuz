#include "note_view_model.h"

#include <algorithm>
#include <chrono>
#include <unordered_map>

// ── NotesViewModel ────────────────────────────────────────────────────────────

NotesViewModel::NotesViewModel(
    FoldersChangedCb on_folders,
    NotesChangedCb   on_notes,
    NoteLoadedCb     on_note_loaded,
    EditorUpdatedCb  on_editor_updated,
    ConnectionCb     on_connection)
    : on_folders_changed_(std::move(on_folders))
    , on_notes_changed_(std::move(on_notes))
    , on_note_loaded_(std::move(on_note_loaded))
    , on_editor_updated_(std::move(on_editor_updated))
    , on_connection_(std::move(on_connection))
{
    server_address_ = local_store_.load_server_address();
    local_store_.load(folders_, notes_);
    start_sync();
}

NotesViewModel::~NotesViewModel() {
    if (debounce_source_ != 0) {
        g_source_remove(debounce_source_);
        debounce_source_ = 0;
    }
}

// ── Getters ───────────────────────────────────────────────────────────────────

std::vector<Note> NotesViewModel::notes_in_view() const {
    std::vector<Note> result;
    for (const auto& n : notes_) {
        if (selected_folder_id_ == ALL_NOTES_ID || n.folder_id == selected_folder_id_)
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

// ── Folder actions ────────────────────────────────────────────────────────────

void NotesViewModel::create_folder(const std::string& name) {
    WireMessage msg;
    msg.type = "create_folder";
    msg.name = name;
    if (sync_service_) sync_service_->send(msg);
}

void NotesViewModel::rename_folder(const std::string& id, const std::string& name) {
    for (auto& f : folders_) {
        if (f.id == id) { f.name = name; break; }
    }
    on_folders_changed_();
    local_store_.save(folders_, notes_);

    WireMessage msg;
    msg.type      = "rename_folder";
    msg.folder_id = id;
    msg.name      = name;
    if (sync_service_) sync_service_->send(msg);
}

void NotesViewModel::delete_folder(const std::string& id) {
    folders_.erase(std::remove_if(folders_.begin(), folders_.end(),
        [&](const Folder& f) { return f.id == id; }), folders_.end());
    notes_.erase(std::remove_if(notes_.begin(), notes_.end(),
        [&](const Note& n) { return n.folder_id == id; }), notes_.end());

    if (selected_folder_id_ == id) {
        selected_folder_id_ = ALL_NOTES_ID;
        clear_editor();
        on_note_loaded_(nullptr);
    }

    on_folders_changed_();
    on_notes_changed_();
    local_store_.save(folders_, notes_);

    WireMessage msg;
    msg.type      = "delete_folder";
    msg.folder_id = id;
    if (sync_service_) sync_service_->send(msg);
}

// ── Note actions ──────────────────────────────────────────────────────────────

void NotesViewModel::create_note() {
    if (selected_folder_id_.empty() || selected_folder_id_ == ALL_NOTES_ID) return;

    WireMessage msg;
    msg.type      = "create_note";
    msg.folder_id = selected_folder_id_;
    msg.title     = "";
    if (sync_service_) sync_service_->send(msg);
}

void NotesViewModel::delete_note(const std::string& id) {
    bool was_editing = (editing_note_id_ == id);

    notes_.erase(std::remove_if(notes_.begin(), notes_.end(),
        [&](const Note& n) { return n.id == id; }), notes_.end());

    if (was_editing) {
        selected_note_id_ = "";
        clear_editor();
    }

    on_notes_changed_();

    if (was_editing)
        on_note_loaded_(nullptr);

    local_store_.save(folders_, notes_);

    WireMessage msg;
    msg.type    = "delete_note";
    msg.note_id = id;
    if (sync_service_) sync_service_->send(msg);
}

// ── Server address ────────────────────────────────────────────────────────────

void NotesViewModel::set_server_address(const std::string& address) {
    server_address_ = address;
    local_store_.save_server_address(address);
    on_connection_(false);
    start_sync();
}

// ── Sync ──────────────────────────────────────────────────────────────────────

void NotesViewModel::start_sync() {
    sync_service_ = std::make_unique<SyncService>(
        server_address_,
        [this](WireMessage msg) { handle_message(std::move(msg)); },
        [this](bool connected)  { on_connection_(connected); });
}

void NotesViewModel::handle_message(WireMessage msg) {
    if (msg.type == "init") {
        handle_init(std::move(msg.folders), std::move(msg.notes));
        return;
    }

    if (msg.type == "folder_created") {
        if (msg.has_folder) {
            bool exists = std::any_of(folders_.begin(), folders_.end(),
                [&](const Folder& f) { return f.id == msg.folder.id; });
            if (!exists) {
                folders_.push_back(msg.folder);
                on_folders_changed_();
            }
        }

    } else if (msg.type == "folder_renamed") {
        if (msg.has_folder) {
            for (auto& f : folders_) {
                if (f.id == msg.folder.id) { f.name = msg.folder.name; break; }
            }
            on_folders_changed_();
        }

    } else if (msg.type == "folder_deleted") {
        if (!msg.folder_id.empty()) {
            folders_.erase(std::remove_if(folders_.begin(), folders_.end(),
                [&](const Folder& f) { return f.id == msg.folder_id; }), folders_.end());
            notes_.erase(std::remove_if(notes_.begin(), notes_.end(),
                [&](const Note& n) { return n.folder_id == msg.folder_id; }), notes_.end());

            if (selected_folder_id_ == msg.folder_id) {
                selected_folder_id_ = ALL_NOTES_ID;
                clear_editor();
                on_note_loaded_(nullptr);
            }
            on_folders_changed_();
            on_notes_changed_();
        }

    } else if (msg.type == "note_created") {
        if (msg.has_note) {
            bool exists = std::any_of(notes_.begin(), notes_.end(),
                [&](const Note& n) { return n.id == msg.note.id; });
            if (!exists) {
                notes_.push_back(msg.note);

                bool visible = (msg.note.folder_id == selected_folder_id_)
                    || (selected_folder_id_ == ALL_NOTES_ID);

                if (visible) {
                    // Set selection state before notifying UI so the note list
                    // can restore the row highlight in rebuild.
                    selected_note_id_ = msg.note.id;
                    editing_note_id_  = msg.note.id;
                    editing_title_    = msg.note.title;
                    editing_content_  = msg.note.content;
                }

                on_notes_changed_();

                if (visible) {
                    auto it = std::find_if(notes_.begin(), notes_.end(),
                        [&](const Note& n) { return n.id == msg.note.id; });
                    if (it != notes_.end()) on_note_loaded_(&*it);
                }
            }
        }

    } else if (msg.type == "note_updated") {
        if (msg.has_note) {
            for (auto& n : notes_) {
                if (n.id == msg.note.id && msg.note.updated_at > n.updated_at) {
                    n = msg.note;
                    on_notes_changed_();
                    if (editing_note_id_ == msg.note.id) {
                        editing_title_   = n.title;
                        editing_content_ = n.content;
                        on_editor_updated_(n.title, n.content);
                    }
                    break;
                }
            }
        }

    } else if (msg.type == "note_deleted") {
        if (!msg.note_id.empty()) {
            bool was_editing = (editing_note_id_ == msg.note_id);
            notes_.erase(std::remove_if(notes_.begin(), notes_.end(),
                [&](const Note& n) { return n.id == msg.note_id; }), notes_.end());

            if (was_editing) {
                selected_note_id_ = "";
                clear_editor();
            }
            on_notes_changed_();
            if (was_editing) on_note_loaded_(nullptr);
        }
    }

    local_store_.save(folders_, notes_);
}

void NotesViewModel::handle_init(
    std::vector<Folder> server_folders,
    std::vector<Note>   server_notes)
{
    std::string saved_editing_id = editing_note_id_;

    folders_ = std::move(server_folders);
    on_folders_changed_();

    // Bootstrap: create a default folder if the server has none.
    if (folders_.empty()) {
        WireMessage msg;
        msg.type = "create_folder";
        msg.name = "Notes";
        if (sync_service_) sync_service_->send(msg);
    }

    // Merge notes: last-write-wins by timestamp.
    std::unordered_map<std::string, Note> server_map;
    for (auto& n : server_notes) server_map[n.id] = std::move(n);

    std::vector<Note> merged;
    for (const auto& local : notes_) {
        auto it = server_map.find(local.id);
        if (it != server_map.end()) {
            if (local.updated_at > it->second.updated_at) {
                // Local is newer — keep it and push to server.
                merged.push_back(local);
                WireMessage msg;
                msg.type       = "update_note";
                msg.note_id    = local.id;
                msg.title      = local.title;
                msg.content    = local.content;
                msg.updated_at = local.updated_at;
                if (sync_service_) sync_service_->send(msg);
            } else {
                merged.push_back(it->second);
            }
            server_map.erase(it);
        }
        // Local-only notes (no server ID): dropped — they were never confirmed.
    }

    // Notes only on server (created by other clients): add them.
    for (auto& [id, n] : server_map)
        merged.push_back(std::move(n));

    notes_ = std::move(merged);

    // Refresh editor if the active note was updated by server.
    if (!saved_editing_id.empty()) {
        auto it = std::find_if(notes_.begin(), notes_.end(),
            [&](const Note& n) { return n.id == saved_editing_id; });
        if (it != notes_.end()) {
            editing_note_id_ = saved_editing_id;
            editing_title_   = it->title;
            editing_content_ = it->content;
            on_editor_updated_(it->title, it->content);
        }
    }

    on_notes_changed_();
    local_store_.save(folders_, notes_);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

void NotesViewModel::flush_note(
    const std::string& id,
    const std::string& title,
    const std::string& content)
{
    auto it = std::find_if(notes_.begin(), notes_.end(),
        [&](const Note& n) { return n.id == id; });
    if (it == notes_.end()) return;
    if (it->title == title && it->content == content) return;

    auto now = std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::system_clock::now().time_since_epoch()).count();

    it->title      = title;
    it->content    = content;
    it->updated_at = now;

    local_store_.save(folders_, notes_);
    on_notes_changed_();

    WireMessage msg;
    msg.type       = "update_note";
    msg.note_id    = id;
    msg.title      = title;
    msg.content    = content;
    msg.updated_at = now;
    if (sync_service_) sync_service_->send(msg);
}

void NotesViewModel::clear_editor() {
    editing_note_id_ = "";
    editing_title_   = "";
    editing_content_ = "";
}
