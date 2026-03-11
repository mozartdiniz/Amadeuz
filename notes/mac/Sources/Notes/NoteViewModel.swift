import Combine
import Foundation

@MainActor
final class NotesViewModel: ObservableObject {

    /// Sentinel folder ID used for the hardcoded "All Notes" view.
    static let allNotesID = "__all__"

    // MARK: - Published state

    @Published var isAuthenticated = false
    @Published var authError: String?
    /// Set after register/recover. ContentView shows it as a sheet that survives the auth transition.
    @Published var pendingRecoveryCode: String?

    @Published var folders: [Folder] = []
    @Published var notes: [Note]     = []

    @Published var selectedFolderID: String?
    @Published var selectedNoteID: String?

    @Published var editingContent: String = ""
    @Published var editingTitle: String   = ""

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
            api.serverBase = serverAddress
            blobStore.updateServerBase(serverAddress)
            noteSync = nil
            if isAuthenticated { Task { await self.fullSync() } }
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
    private(set) var blobStore: BlobStore
    private(set) var api: APIClient
    private var noteSync: NoteSync?
    private var cancellables = Set<AnyCancellable>()
    private var isSyncing = false

    /// Tracks which note is currently loaded so we can flush before switching.
    private var editingNoteID: String?

    // MARK: - Init

    init() {
        let savedAddress = Self.normalizeServerAddress(
            UserDefaults.standard.string(forKey: "serverAddress") ?? "http://localhost:8080"
        )
        serverAddress = savedAddress
        let savedToken = KeychainStore.loadToken()
        api       = APIClient(serverBase: savedAddress, token: savedToken)
        blobStore = BlobStore(serverBase: savedAddress)
        blobStore.token = savedToken

        let saved = localStore.load()
        folders = saved.folders
        notes   = saved.notes
        selectedFolderID = Self.allNotesID

        if savedToken != nil {
            isAuthenticated = true
            Task { await self.fullSync() }
        }

        setupDebounce()
    }

    // MARK: - Auth

    func login(email: String, password: String) async {
        authError = nil
        do {
            let resp = try await api.login(email: email, password: password)
            applyToken(resp.token)
            await fullSync()
        } catch {
            authError = errorMessage(error)
        }
    }

    func register(email: String, password: String) async {
        authError = nil
        do {
            let resp = try await api.register(email: email, password: password)
            pendingRecoveryCode = resp.recoveryCode   // set BEFORE applyToken so ContentView shows it
            applyToken(resp.token)
            await fullSync()
        } catch {
            authError = errorMessage(error)
        }
    }

    func recover(email: String, code: String, newPassword: String) async {
        authError = nil
        do {
            let resp = try await api.recover(email: email, code: code, newPassword: newPassword)
            pendingRecoveryCode = resp.recoveryCode   // set BEFORE applyToken
            applyToken(resp.token)
            await fullSync()
        } catch {
            authError = errorMessage(error)
        }
    }

    func logout() {
        KeychainStore.deleteToken()
        api.token       = nil
        blobStore.token = nil
        isAuthenticated = false
        noteSync        = nil
        isConnected     = false
        folders         = []
        notes           = []
        clearEditor()
        selectedFolderID = Self.allNotesID
        localStore.save(folders: [], notes: [])
    }

    // MARK: - Full sync (REST)

    func fullSync() async {
        guard !isSyncing else { return }
        isSyncing = true
        defer { isSyncing = false }

        do {
            async let sf = api.listFolders()
            async let sn = api.listNotes()
            let (serverFolders, serverNotes) = try await (sf, sn)

            // Capture current local state before computing diffs.
            let localFolders = folders
            let localNotes   = notes

            let serverFolderIDs = Set(serverFolders.map { $0.id })
            let serverNoteMap   = Dictionary(uniqueKeysWithValues: serverNotes.map { ($0.id, $0) })
            let localNoteIDs    = Set(localNotes.map { $0.id })

            let localOnlyFolders = localFolders.filter { !serverFolderIDs.contains($0.id) }
            let localOnlyNotes   = localNotes.filter   { serverNoteMap[$0.id] == nil }
            let newerLocalNotes  = localNotes.filter   { n in
                guard let sv = serverNoteMap[n.id] else { return false }
                return n.updatedAt > sv.updatedAt
            }

            // Push offline-only and locally-newer items (failures are silent; next sync retries).
            await withTaskGroup(of: Void.self) { group in
                for f in localOnlyFolders {
                    group.addTask {
                        _ = try? await self.api.createFolder(id: f.id, name: f.name, createdAt: f.createdAt)
                    }
                }
                for n in localOnlyNotes {
                    group.addTask {
                        _ = try? await self.api.createNote(
                            id: n.id, folderID: n.folderID.isEmpty ? nil : n.folderID,
                            title: n.title, content: n.content,
                            updatedAt: n.updatedAt, createdAt: n.createdAt
                        )
                    }
                }
                for n in newerLocalNotes {
                    group.addTask {
                        _ = try? await self.api.updateNote(
                            id: n.id, title: n.title, content: n.content, updatedAt: n.updatedAt
                        )
                    }
                }
            }

            // Merge state.
            folders = serverFolders + localOnlyFolders

            var merged = [Note]()
            for local in localNotes {
                if let sv = serverNoteMap[local.id] {
                    merged.append(local.updatedAt > sv.updatedAt ? local : sv)
                } else {
                    merged.append(local)   // local-only
                }
            }
            for sv in serverNotes where !localNoteIDs.contains(sv.id) {
                merged.append(sv)
            }
            notes = merged

            // Refresh editor if the active note was superseded by the server.
            if let id = selectedNoteID, let n = notes.first(where: { $0.id == id }) {
                editingTitle   = n.title
                editingContent = n.content
            }

            localStore.save(folders: folders, notes: notes)
            isConnected = true
            await blobStore.uploadPending()
            connectNoteSync()

        } catch let err as APIError {
            if case .unauthorized = err { logout() }
        } catch {
            isConnected = false
        }
    }

    // MARK: - Per-note WebSocket

    private func connectNoteSync() {
        guard let noteID = selectedNoteID,
              let url = api.wsURL(forNoteID: noteID) else { return }
        noteSync = NoteSync(
            url: url,
            onInit: { [weak self] msg in
                Task { @MainActor [weak self] in self?.handleNoteWSInit(msg) }
            },
            onUpdate: { [weak self] msg in
                Task { @MainActor [weak self] in self?.handleNoteWSUpdate(msg) }
            },
            onConnectionChange: { [weak self] connected in
                Task { @MainActor [weak self] in self?.isConnected = connected }
            }
        )
    }

    private func handleNoteWSInit(_ msg: NoteWsMsg) {
        guard let id = selectedNoteID,
              let idx = notes.firstIndex(where: { $0.id == id }),
              let msgUpdatedAt = msg.updatedAt,
              msgUpdatedAt > notes[idx].updatedAt else { return }
        notes[idx].title     = msg.title   ?? notes[idx].title
        notes[idx].content   = msg.content ?? notes[idx].content
        notes[idx].updatedAt = msgUpdatedAt
        editingTitle   = notes[idx].title
        editingContent = notes[idx].content
        localStore.save(folders: folders, notes: notes)
    }

    private func handleNoteWSUpdate(_ msg: NoteWsMsg) {
        guard let id = selectedNoteID,
              let idx = notes.firstIndex(where: { $0.id == id }),
              let msgUpdatedAt = msg.updatedAt,
              msgUpdatedAt > notes[idx].updatedAt else { return }
        notes[idx].title     = msg.title   ?? notes[idx].title
        notes[idx].content   = msg.content ?? notes[idx].content
        notes[idx].updatedAt = msgUpdatedAt
        editingTitle   = notes[idx].title
        editingContent = notes[idx].content
        localStore.save(folders: folders, notes: notes)
    }

    // MARK: - Debounce

    private func setupDebounce() {
        Publishers.CombineLatest($editingTitle, $editingContent)
            .dropFirst()
            .debounce(for: .milliseconds(500), scheduler: RunLoop.main)
            .sink { [weak self] title, content in
                guard let self, let noteID = self.editingNoteID else { return }
                self.flushNote(id: noteID, title: title, content: content)
            }
            .store(in: &cancellables)
    }

    /// Persists and syncs the note only if its content actually changed.
    private func flushNote(id: String, title: String, content: String) {
        guard let idx = notes.firstIndex(where: { $0.id == id }) else { return }
        let stored = notes[idx]
        guard stored.title != title || stored.content != content else { return }

        let now = Int64(Date().timeIntervalSince1970 * 1000)
        notes[idx].title     = title
        notes[idx].content   = content
        notes[idx].updatedAt = now
        localStore.save(folders: folders, notes: notes)
        Task { _ = try? await self.api.updateNote(id: id, title: title, content: content, updatedAt: now) }
    }

    // MARK: - Selection

    func noteSelectionChanged(from oldID: String?, to newID: String?) {
        if let old = oldID, old == editingNoteID {
            flushNote(id: old, title: editingTitle, content: editingContent)
        }
        noteSync = nil
        loadNoteIntoEditor(newID.flatMap { id in notes.first { $0.id == id } })
        if newID != nil { connectNoteSync() }
    }

    func folderSelectionChanged() {
        // Setting selectedNoteID = nil triggers onChange → noteSelectionChanged,
        // which flushes before clearing the editor.
        selectedNoteID = nil
    }

    // MARK: - Folder actions

    func createFolder(name: String) {
        let now = Int64(Date().timeIntervalSince1970 * 1000)
        let folder = Folder(id: UUID().uuidString.lowercased(), name: name, createdAt: now)
        folders.append(folder)
        localStore.save(folders: folders, notes: notes)
        Task { try? await self.api.createFolder(id: folder.id, name: folder.name, createdAt: folder.createdAt) }
    }

    func renameFolder(id: String, name: String) {
        if let idx = folders.firstIndex(where: { $0.id == id }) {
            folders[idx].name = name
        }
        localStore.save(folders: folders, notes: notes)
        Task { try? await self.api.renameFolder(id: id, name: name) }
    }

    func deleteFolder(id: String) {
        folders.removeAll { $0.id == id }
        notes.removeAll   { $0.folderID == id }
        if selectedFolderID == id {
            selectedFolderID = nil
            clearEditor()
        }
        localStore.save(folders: folders, notes: notes)
        Task { try? await self.api.deleteFolder(id: id) }
    }

    // MARK: - Note actions

    func createNote() {
        guard selectedFolderID != nil else { return }
        let folderID: String? = (selectedFolderID == Self.allNotesID) ? nil : selectedFolderID
        let now = Int64(Date().timeIntervalSince1970 * 1000)
        let note = Note(
            id: UUID().uuidString.lowercased(),
            folderID: folderID ?? "",
            title: "",
            content: "",
            updatedAt: now,
            createdAt: now
        )
        notes.append(note)
        localStore.save(folders: folders, notes: notes)
        if let old = editingNoteID {
            flushNote(id: old, title: editingTitle, content: editingContent)
        }
        noteSync = nil
        loadNoteIntoEditor(note)
        selectedNoteID = note.id
        connectNoteSync()
        Task {
            try? await self.api.createNote(
                id: note.id, folderID: folderID,
                title: "", content: "",
                updatedAt: now, createdAt: now
            )
        }
    }

    func moveNote(id: String, toFolderID: String?) {
        let folderID = toFolderID ?? ""
        guard let idx = notes.firstIndex(where: { $0.id == id }) else { return }
        let now = Int64(Date().timeIntervalSince1970 * 1000)
        notes[idx].folderID  = folderID
        notes[idx].updatedAt = now
        if selectedFolderID != Self.allNotesID,
           let fid = selectedFolderID,
           folderID != fid,
           selectedNoteID == id
        {
            clearEditor()
        }
        localStore.save(folders: folders, notes: notes)
        Task { try? await self.api.moveNote(id: id, folderID: folderID.isEmpty ? nil : folderID) }
    }

    func deleteNote(id: String) {
        notes.removeAll { $0.id == id }
        if selectedNoteID == id { clearEditor() }
        localStore.save(folders: folders, notes: notes)
        Task { try? await self.api.deleteNote(id: id) }
    }

    // MARK: - Helpers

    private func applyToken(_ token: String) {
        KeychainStore.saveToken(token)
        api.token       = token
        blobStore.token = token
        isAuthenticated = true
        // Wipe any previous user's data so fullSync starts clean.
        // (init-path resume keeps local data; only explicit auth calls go through here.)
        folders = []
        notes   = []
        clearEditor()
        localStore.save(folders: [], notes: [])
    }

    private func loadNoteIntoEditor(_ note: Note?) {
        editingNoteID  = note?.id
        editingTitle   = note?.title   ?? ""
        editingContent = note?.content ?? ""
    }

    private func clearEditor() {
        selectedNoteID = nil
        editingNoteID  = nil
        editingTitle   = ""
        editingContent = ""
    }

    private func errorMessage(_ error: Error) -> String {
        (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
    }

    static func normalizeServerAddress(_ addr: String) -> String {
        // Migrate old ws:// / wss:// format from previous version.
        if addr.hasPrefix("wss://") || addr.hasPrefix("ws://") {
            let http = addr
                .replacingOccurrences(of: "wss://", with: "https://")
                .replacingOccurrences(of: "ws://",  with: "http://")
            return http.components(separatedBy: "/ws").first ?? http
        }
        return addr.hasSuffix("/") ? String(addr.dropLast()) : addr
    }
}
