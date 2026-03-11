import Combine
import Foundation

final class NotesViewModel: ObservableObject {

    /// Sentinel folder ID used for the hardcoded "All Notes" view.
    static let allNotesID = "__all__"

    // MARK: - Published state

    @Published var folders: [Folder] = []
    @Published var notes: [Note] = []

    @Published var selectedFolderID: String?
    @Published var selectedNoteID: String?

    /// Content and title currently shown in the editor.
    @Published var editingContent: String = ""
    @Published var editingTitle: String = ""

    @Published var searchText: String = "" {
        didSet {
            if !searchText.isEmpty && selectedFolderID != Self.allNotesID {
                selectedFolderID = Self.allNotesID
            }
        }
    }

    @Published var isConnected: Bool = false
    @Published var serverAddress: String {
        didSet {
            UserDefaults.standard.set(serverAddress, forKey: "serverAddress")
            reconnect()
        }
    }

    // MARK: - Computed

    var selectedFolder: Folder? {
        folders.first { $0.id == selectedFolderID }
    }

    var notesInSelectedFolder: [Note] {
        let sorted = notes.sorted { $0.updatedAt > $1.updatedAt }
        let folderFiltered: [Note]
        if let id = selectedFolderID, id != Self.allNotesID {
            folderFiltered = sorted.filter { $0.folderID == id }
        } else {
            folderFiltered = sorted
        }
        guard !searchText.isEmpty else { return folderFiltered }
        let query = searchText.lowercased()
        return folderFiltered.filter {
            $0.title.lowercased().contains(query) || $0.content.lowercased().contains(query)
        }
    }

    // MARK: - Private

    private let localStore = LocalStore()
    private var syncService: SyncService?
    private var cancellables = Set<AnyCancellable>()

    /// Tracks which note ID is currently loaded in the editor,
    /// so we can flush it before switching to another note.
    private var editingNoteID: String?

    // MARK: - Init

    init() {
        serverAddress = UserDefaults.standard.string(forKey: "serverAddress")
            ?? "ws://localhost:8080/ws"

        let saved = localStore.load()
        folders = saved.folders
        notes   = saved.notes

        selectedFolderID = Self.allNotesID   // always start on "All Notes"

        setupDebounce()
        startSync()
    }

    // MARK: - Debounce

    private func setupDebounce() {
        // Fire 500 ms after the last change to either title or content.
        Publishers.CombineLatest($editingTitle, $editingContent)
            .dropFirst()
            .debounce(for: .milliseconds(500), scheduler: RunLoop.main)
            .sink { [weak self] title, content in
                guard let self, let noteID = self.editingNoteID else { return }
                self.flushNote(id: noteID, title: title, content: content)
            }
            .store(in: &cancellables)
    }

    /// Saves and syncs the note only if its content actually changed.
    private func flushNote(id: String, title: String, content: String) {
        guard let idx = notes.firstIndex(where: { $0.id == id }) else { return }
        let stored = notes[idx]
        guard stored.title != title || stored.content != content else { return }

        let now = Int64(Date().timeIntervalSince1970 * 1000)
        notes[idx].title     = title
        notes[idx].content   = content
        notes[idx].updatedAt = now
        localStore.save(folders: folders, notes: notes)
        syncService?.send(WSMsg(
            type: "update_note",
            noteID: id, title: title, content: content, updatedAt: now
        ))
    }

    // MARK: - Sync

    private func startSync() {
        guard let url = URL(string: serverAddress) else { return }
        syncService = SyncService(
            url: url,
            onMessage: { [weak self] msg in self?.handleMessage(msg) },
            onConnectionChange: { [weak self] connected in
                DispatchQueue.main.async { self?.isConnected = connected }
            }
        )
    }

    private func handleMessage(_ msg: WSMsg) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }

            switch msg.type {
            case "init":
                self.handleInit(
                    serverFolders: msg.folders ?? [],
                    serverNotes:   msg.notes   ?? []
                )

            case "folder_created":
                if let f = msg.folder, !self.folders.contains(where: { $0.id == f.id }) {
                    self.folders.append(f)
                }

            case "folder_renamed":
                if let f = msg.folder,
                   let idx = self.folders.firstIndex(where: { $0.id == f.id }) {
                    self.folders[idx].name = f.name
                }

            case "folder_deleted":
                if let fid = msg.folderID {
                    self.folders.removeAll { $0.id == fid }
                    self.notes.removeAll   { $0.folderID == fid }
                    if self.selectedFolderID == fid {
                        self.selectedFolderID = nil
                        self.clearEditor()
                    }
                }

            case "note_created":
                if let n = msg.note, !self.notes.contains(where: { $0.id == n.id }) {
                    self.notes.append(n)
                    // Auto-select our own creation. Let onChange → noteSelectionChanged
                    // handle the flush of the current note before loading the new one.
                    let isVisible = n.folderID == self.selectedFolderID
                        || self.selectedFolderID == Self.allNotesID
                    if isVisible {
                        self.selectedNoteID = n.id
                    }
                }

            case "note_updated":
                if let n = msg.note,
                   let idx = self.notes.firstIndex(where: { $0.id == n.id }),
                   n.updatedAt > self.notes[idx].updatedAt
                {
                    self.notes[idx] = n
                    // Refresh editor only if this is the active note.
                    if self.selectedNoteID == n.id {
                        self.editingTitle   = n.title
                        self.editingContent = n.content
                    }
                }

            case "note_moved":
                if let n = msg.note,
                   let idx = self.notes.firstIndex(where: { $0.id == n.id })
                {
                    self.notes[idx].folderID = n.folderID
                    self.notes[idx].updatedAt = n.updatedAt
                    // If we're viewing a specific folder and this note moved out, deselect it.
                    let fid = self.selectedFolderID
                    if fid != Self.allNotesID,
                       let fid,
                       n.folderID != fid,
                       self.selectedNoteID == n.id
                    {
                        self.clearEditor()
                    }
                }

            case "note_deleted":
                if let nid = msg.noteID {
                    self.notes.removeAll { $0.id == nid }
                    if self.selectedNoteID == nid { self.clearEditor() }
                }

            default:
                break
            }

            self.localStore.save(folders: self.folders, notes: self.notes)
        }
    }

    /// On reconnect: server state is authoritative for folders.
    /// Per note: last-write-wins by timestamp; push local if ahead.
    private func handleInit(serverFolders: [Folder], serverNotes: [Note]) {
        folders = serverFolders

        var serverMap = Dictionary(uniqueKeysWithValues: serverNotes.map { ($0.id, $0) })
        var merged: [Note] = []

        for localNote in notes {
            if let serverNote = serverMap[localNote.id] {
                serverMap.removeValue(forKey: localNote.id)
                if localNote.updatedAt > serverNote.updatedAt {
                    merged.append(localNote)
                    syncService?.send(WSMsg(
                        type: "update_note",
                        noteID: localNote.id,
                        title: localNote.title,
                        content: localNote.content,
                        updatedAt: localNote.updatedAt
                    ))
                } else {
                    merged.append(serverNote)
                }
            }
            // Local-only notes (created offline without server confirmation): dropped.
        }

        // Notes only on server (from other clients): add them.
        merged.append(contentsOf: serverMap.values)
        notes = merged

        // Refresh editor if the active note was updated by server.
        if let id = selectedNoteID, let n = notes.first(where: { $0.id == id }) {
            editingTitle   = n.title
            editingContent = n.content
        }

        localStore.save(folders: folders, notes: notes)
    }

    // MARK: - Selection (called from view's onChange)

    /// Called when the note list selection changes.
    func noteSelectionChanged(from oldID: String?, to newID: String?) {
        // Flush any unsaved changes to the outgoing note immediately.
        if let old = oldID {
            flushNote(id: old, title: editingTitle, content: editingContent)
        }
        loadNoteIntoEditor(newID.flatMap { id in notes.first { $0.id == id } })
    }

    /// Called when the folder selection changes (clears note selection).
    func folderSelectionChanged() {
        // Setting selectedNoteID = nil triggers onChange → noteSelectionChanged,
        // which flushes the current note with the correct editor values before clearing.
        // Do NOT call clearEditor() here — that would zero out editingTitle/editingContent
        // before the flush runs, causing content loss.
        selectedNoteID = nil
    }

    // MARK: - Folder actions

    func createFolder(name: String) {
        syncService?.send(WSMsg(type: "create_folder", name: name))
    }

    func renameFolder(id: String, name: String) {
        if let idx = folders.firstIndex(where: { $0.id == id }) {
            folders[idx].name = name
        }
        localStore.save(folders: folders, notes: notes)
        syncService?.send(WSMsg(type: "rename_folder", folderID: id, name: name))
    }

    func deleteFolder(id: String) {
        folders.removeAll { $0.id == id }
        notes.removeAll   { $0.folderID == id }
        if selectedFolderID == id {
            selectedFolderID = nil
            clearEditor()
        }
        localStore.save(folders: folders, notes: notes)
        syncService?.send(WSMsg(type: "delete_folder", folderID: id))
    }

    // MARK: - Note actions

    func createNote() {
        guard selectedFolderID != nil else { return }
        // When in "All Notes", create an unfoldered note (no folder_id sent).
        let folderID: String? = (selectedFolderID == Self.allNotesID) ? nil : selectedFolderID
        syncService?.send(WSMsg(type: "create_note", folderID: folderID, title: ""))
    }

    func moveNote(id: String, toFolderID: String?) {
        let folderID = toFolderID ?? ""
        guard let idx = notes.firstIndex(where: { $0.id == id }) else { return }
        let now = Int64(Date().timeIntervalSince1970 * 1000)
        notes[idx].folderID = folderID
        notes[idx].updatedAt = now
        // If viewing a specific folder and the note moved out of it, deselect.
        if selectedFolderID != Self.allNotesID,
           let fid = selectedFolderID,
           folderID != fid,
           selectedNoteID == id
        {
            clearEditor()
        }
        localStore.save(folders: folders, notes: notes)
        syncService?.send(WSMsg(type: "move_note", folderID: folderID.isEmpty ? nil : folderID, noteID: id))
    }

    func deleteNote(id: String) {
        notes.removeAll { $0.id == id }
        if selectedNoteID == id { clearEditor() }
        localStore.save(folders: folders, notes: notes)
        syncService?.send(WSMsg(type: "delete_note", noteID: id))
    }

    // MARK: - Public

    func reconnect() {
        syncService = nil
        isConnected = false
        startSync()
    }

    // MARK: - Helpers

    private func loadNoteIntoEditor(_ note: Note?) {
        editingNoteID   = note?.id
        editingTitle    = note?.title   ?? ""
        editingContent  = note?.content ?? ""
    }

    private func clearEditor() {
        selectedNoteID = nil
        editingNoteID  = nil
        editingTitle   = ""
        editingContent = ""
    }
}
