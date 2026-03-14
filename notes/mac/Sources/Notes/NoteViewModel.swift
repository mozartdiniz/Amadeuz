import Combine
import Foundation

// MARK: - Date section model

struct NoteSection: Identifiable {
    let id: String
    let title: String
    let notes: [Note]
}

@MainActor
final class NotesViewModel: ObservableObject {

    /// Sentinel folder IDs for virtual views.
    static let allNotesID = "__all__"
    static let trashID    = "__trash__"

    // MARK: - Published state

    @Published var isAuthenticated = false
    @Published var authError: String?
    @Published var pendingRecoveryCode: String?

    @Published var folders: [Folder] = []
    @Published var notes: [Note]     = []

    @Published var selectedFolderID: String?
    @Published var selectedNoteID: String?

    /// Single editor body: first line is the title, rest is content.
    @Published var editingBody: String = ""

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

    var isTrashView: Bool { selectedFolderID == Self.trashID }

    var currentFolderTitle: String {
        switch selectedFolderID {
        case Self.allNotesID: return "All Notes"
        case Self.trashID:    return "Recently Deleted"
        case let id?:         return folders.first { $0.id == id }?.name ?? "Notes"
        case nil:             return "Notes"
        }
    }

    var currentFolderSubtitle: String {
        let count = notesInSelectedFolder.count
        return "\(count) \(count == 1 ? "note" : "notes")"
    }

    var allNotesCount: Int {
        notes.filter { !$0.isTrashed }.count
    }

    var trashCount: Int {
        notes.filter { $0.isTrashed }.count
    }

    func noteCount(for folderID: String) -> Int {
        notes.filter { !$0.isTrashed && $0.folderID == folderID }.count
    }

    var notesInSelectedFolder: [Note] {
        let sorted = notes.sorted { $0.updatedAt > $1.updatedAt }

        if selectedFolderID == Self.trashID {
            return sorted.filter { $0.isTrashed }
        }

        let active = sorted.filter { !$0.isTrashed }
        let folderFiltered: [Note]
        if let id = selectedFolderID, id != Self.allNotesID {
            folderFiltered = active.filter { $0.folderID == id }
        } else {
            folderFiltered = active
        }

        guard !searchText.isEmpty else { return folderFiltered }
        let query = searchText.lowercased()
        return folderFiltered.filter {
            $0.title.lowercased().contains(query) || $0.content.lowercased().contains(query)
        }
    }

    /// Notes grouped by recency for the middle column.
    var noteSections: [NoteSection] {
        let items = notesInSelectedFolder
        guard !items.isEmpty else { return [] }

        let now = Date()
        let cal = Calendar.current

        var today:  [Note] = []
        var week:   [Note] = []
        var month:  [Note] = []
        var older:  [Note] = []

        for note in items {
            let date = Date(timeIntervalSince1970: Double(note.updatedAt) / 1000)
            if cal.isDateInToday(date) {
                today.append(note)
            } else if let weekAgo = cal.date(byAdding: .day, value: -7, to: now), date > weekAgo {
                week.append(note)
            } else if let monthAgo = cal.date(byAdding: .day, value: -30, to: now), date > monthAgo {
                month.append(note)
            } else {
                older.append(note)
            }
        }

        return [
            NoteSection(id: "today",  title: "Today",            notes: today),
            NoteSection(id: "week",   title: "Previous 7 Days",  notes: week),
            NoteSection(id: "month",  title: "Previous 30 Days", notes: month),
            NoteSection(id: "older",  title: "Older",            notes: older),
        ].filter { !$0.notes.isEmpty }
    }

    // MARK: - Private

    private let localStore = LocalStore()
    private(set) var blobStore: BlobStore
    private(set) var api: APIClient
    private var noteSync: NoteSync?
    private var cancellables = Set<AnyCancellable>()
    private var isSyncing = false
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
        startPeriodicSync()
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
            pendingRecoveryCode = resp.recoveryCode
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
            pendingRecoveryCode = resp.recoveryCode
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

            let localFolders = folders
            let localNotes   = notes

            let serverFolderIDs = Set(serverFolders.map { $0.id })
            let serverNoteMap   = Dictionary(uniqueKeysWithValues: serverNotes.map { ($0.id, $0) })
            let localNoteIDs    = Set(localNotes.map { $0.id })

            let localOnlyFolders = localFolders.filter { !serverFolderIDs.contains($0.id) }
            let localOnlyNotes   = localNotes.filter   { serverNoteMap[$0.id] == nil && !$0.isTrashed }
            let newerLocalNotes  = localNotes.filter   { n in
                guard let sv = serverNoteMap[n.id] else { return false }
                return n.updatedAt > sv.updatedAt && !n.isTrashed
            }

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

            folders = serverFolders + localOnlyFolders

            var merged = [Note]()
            for local in localNotes {
                if let sv = serverNoteMap[local.id] {
                    merged.append(local.updatedAt > sv.updatedAt ? local : sv)
                } else {
                    merged.append(local)
                }
            }
            for sv in serverNotes where !localNoteIDs.contains(sv.id) {
                merged.append(sv)
            }
            notes = merged

            if let id = selectedNoteID, let n = notes.first(where: { $0.id == id }) {
                editingBody = bodyFrom(n)
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
        editingBody = bodyFrom(notes[idx])
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
        editingBody = bodyFrom(notes[idx])
        localStore.save(folders: folders, notes: notes)
    }

    // MARK: - Debounce

    private func setupDebounce() {
        $editingBody
            .dropFirst()
            .debounce(for: .milliseconds(500), scheduler: RunLoop.main)
            .sink { [weak self] body in
                guard let self, let noteID = self.editingNoteID else { return }
                self.flushNote(id: noteID, body: body)
            }
            .store(in: &cancellables)
    }

    private func startPeriodicSync() {
        Timer.publish(every: 30, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                guard let self, self.isAuthenticated else { return }
                Task { await self.fullSync() }
            }
            .store(in: &cancellables)
    }

    private func flushNote(id: String, body: String) {
        let (title, content) = splitBody(body)
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
            flushNote(id: old, body: editingBody)
        }
        noteSync = nil
        loadNoteIntoEditor(newID.flatMap { id in notes.first { $0.id == id } })
        if newID != nil { connectNoteSync() }
    }

    func folderSelectionChanged() {
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
        notes = notes.map { note in
            var n = note
            if n.folderID == id { n.folderID = "" }
            return n
        }
        if selectedFolderID == id {
            selectedFolderID = Self.allNotesID
            clearEditor()
        }
        localStore.save(folders: folders, notes: notes)
        Task { try? await self.api.deleteFolder(id: id) }
    }

    // MARK: - Note actions

    func createNote() {
        guard selectedFolderID != nil, !isTrashView else { return }
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
            flushNote(id: old, body: editingBody)
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

    func trashNote(id: String) {
        let now = Int64(Date().timeIntervalSince1970 * 1000)
        if let idx = notes.firstIndex(where: { $0.id == id }) {
            notes[idx].deletedAt = now
        }
        if selectedNoteID == id { clearEditor() }
        localStore.save(folders: folders, notes: notes)
        Task { try? await self.api.trashNote(id: id) }
    }

    func restoreNote(id: String) {
        if let idx = notes.firstIndex(where: { $0.id == id }) {
            notes[idx].deletedAt = nil
        }
        if selectedNoteID == id { clearEditor() }
        localStore.save(folders: folders, notes: notes)
        Task { try? await self.api.restoreNote(id: id) }
    }

    func permanentlyDeleteNote(id: String) {
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
        folders = []
        notes   = []
        clearEditor()
        localStore.save(folders: [], notes: [])
    }

    private func loadNoteIntoEditor(_ note: Note?) {
        editingNoteID = note?.id
        editingBody   = note.map { bodyFrom($0) } ?? ""
    }

    private func clearEditor() {
        selectedNoteID = nil
        editingNoteID  = nil
        editingBody    = ""
    }

    private func bodyFrom(_ note: Note) -> String {
        note.content.isEmpty ? note.title : note.title + "\n" + note.content
    }

    private func splitBody(_ body: String) -> (title: String, content: String) {
        if let nl = body.firstIndex(of: "\n") {
            return (String(body[body.startIndex..<nl]),
                    String(body[body.index(after: nl)...]))
        }
        return (body, "")
    }

    private func errorMessage(_ error: Error) -> String {
        (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
    }

    static func normalizeServerAddress(_ addr: String) -> String {
        if addr.hasPrefix("wss://") || addr.hasPrefix("ws://") {
            let http = addr
                .replacingOccurrences(of: "wss://", with: "https://")
                .replacingOccurrences(of: "ws://",  with: "http://")
            return http.components(separatedBy: "/ws").first ?? http
        }
        return addr.hasSuffix("/") ? String(addr.dropLast()) : addr
    }
}
