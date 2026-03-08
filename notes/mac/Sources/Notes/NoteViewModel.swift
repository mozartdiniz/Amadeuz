import Combine
import Foundation

final class NoteViewModel: ObservableObject {
    @Published var content: String = ""
    @Published var isConnected: Bool = false
    @Published var serverAddress: String {
        didSet {
            UserDefaults.standard.set(serverAddress, forKey: "serverAddress")
            reconnect()
        }
    }

    private let localStore = LocalStore()
    private var syncService: SyncService?
    private var cancellables = Set<AnyCancellable>()

    /// The last content value received from (or confirmed by) the server.
    /// Used to avoid echo-sending server updates back.
    private var lastReceivedContent: String = ""
    private var lastUpdatedAt: Int64 = 0

    init() {
        serverAddress = UserDefaults.standard.string(forKey: "serverAddress")
            ?? "ws://localhost:8080/ws"

        let saved = localStore.load()
        content = saved.content
        lastReceivedContent = saved.content
        lastUpdatedAt = saved.updatedAt

        startSync()
        setupDebounce()
    }

    // MARK: - Private

    private func setupDebounce() {
        $content
            .dropFirst()
            .debounce(for: .milliseconds(500), scheduler: RunLoop.main)
            .sink { [weak self] newContent in
                guard let self else { return }
                // Skip if this change came from the server, not the user.
                guard newContent != self.lastReceivedContent else { return }

                let now = Int64(Date().timeIntervalSince1970 * 1000)
                self.lastUpdatedAt = now
                self.localStore.save(content: newContent, updatedAt: now)
                self.syncService?.send(content: newContent, updatedAt: now)
            }
            .store(in: &cancellables)
    }

    private func startSync() {
        guard let url = URL(string: serverAddress) else { return }
        syncService = SyncService(
            url: url,
            onMessage: { [weak self] content, updatedAt in
                self?.handleServerMessage(content: content, updatedAt: updatedAt)
            },
            onConnectionChange: { [weak self] connected in
                DispatchQueue.main.async { self?.isConnected = connected }
            }
        )
    }

    /// Handles a message received from the server (init or update).
    /// All timestamp comparisons use Unix milliseconds.
    private func handleServerMessage(content: String, updatedAt: Int64) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }

            if updatedAt > self.lastUpdatedAt {
                // Server has newer content — accept it.
                self.lastReceivedContent = content
                self.content = content
                self.lastUpdatedAt = updatedAt
                self.localStore.save(content: content, updatedAt: updatedAt)

            } else if self.lastUpdatedAt > updatedAt {
                // We have newer content (wrote offline) — push it to the server.
                self.syncService?.send(content: self.content, updatedAt: self.lastUpdatedAt)
            }
            // Equal timestamps → already in sync, nothing to do.
        }
    }

    // MARK: - Public

    func reconnect() {
        syncService = nil
        isConnected = false
        startSync()
    }
}
