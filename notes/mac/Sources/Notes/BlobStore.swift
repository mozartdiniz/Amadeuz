import Foundation

/// Manages local blob cache, pending upload queue, and HTTP sync with the server.
///
/// Offline-first: images are saved locally with a client-generated ID the moment
/// the user inserts them. A pending queue tracks blobs not yet uploaded to the server.
/// On reconnect, NoteViewModel calls uploadPending() to flush the queue.
final class BlobStore {

    private let cacheDir: URL
    private let pendingURL: URL
    private var serverBase: String
    private var pendingIDs: Set<String>

    /// JWT token used for authenticated uploads. Set by NoteViewModel after login.
    var token: String?

    init(serverBase: String) {
        self.serverBase = serverBase
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let amadeuzDir = support.appendingPathComponent("amadeuz", isDirectory: true)
        cacheDir   = amadeuzDir.appendingPathComponent("blobs", isDirectory: true)
        pendingURL = amadeuzDir.appendingPathComponent("pending_blobs.json")
        try? FileManager.default.createDirectory(at: cacheDir, withIntermediateDirectories: true)
        pendingIDs = BlobStore.loadPending(from: pendingURL)
    }

    func updateServerBase(_ base: String) {
        serverBase = base
    }

    // MARK: - Local save (works offline)

    /// Saves image data locally with a client-generated UUID and queues it for upload.
    /// Returns the blob ID that should be embedded in the note content.
    func save(_ data: Data) throws -> String {
        let id = UUID().uuidString.lowercased()
        try data.write(to: cacheURL(for: id), options: .atomic)
        pendingIDs.insert(id)
        savePending()
        return id
    }

    // MARK: - Cache read

    func cachedData(for id: String) -> Data? {
        try? Data(contentsOf: cacheURL(for: id))
    }

    // MARK: - Upload (requires connection)

    /// Uploads a single locally cached blob to the server via PUT /blobs/:id.
    /// Idempotent — safe to retry. Removes from pending queue on success.
    func upload(id: String) async throws {
        guard let data = cachedData(for: id) else { return }
        guard let url = URL(string: "\(serverBase)/blobs/\(id)") else { throw URLError(.badURL) }
        var req = URLRequest(url: url)
        req.httpMethod = "PUT"
        req.httpBody = data
        if let tok = token {
            req.setValue("Bearer \(tok)", forHTTPHeaderField: "Authorization")
        }
        let (_, response) = try await URLSession.shared.data(for: req)
        guard let http = response as? HTTPURLResponse,
              http.statusCode == 200 || http.statusCode == 201 else {
            throw URLError(.badServerResponse)
        }
        pendingIDs.remove(id)
        savePending()
    }

    /// Uploads all blobs that haven't reached the server yet. Call on reconnect.
    func uploadPending() async {
        let ids = pendingIDs // snapshot — set may change during iteration
        for id in ids {
            try? await upload(id: id)
        }
    }

    // MARK: - Download

    /// Returns locally cached data if available; otherwise downloads from server and caches.
    func download(id: String) async throws -> Data {
        if let cached = cachedData(for: id) { return cached }
        guard let url = URL(string: "\(serverBase)/blobs/\(id)") else { throw URLError(.badURL) }
        let (data, _) = try await URLSession.shared.data(from: url)
        try? data.write(to: cacheURL(for: id), options: .atomic)
        return data
    }

    // MARK: - Pending queue persistence

    private func savePending() {
        guard let data = try? JSONEncoder().encode(Array(pendingIDs)) else { return }
        try? data.write(to: pendingURL, options: .atomic)
    }

    private static func loadPending(from url: URL) -> Set<String> {
        guard let data = try? Data(contentsOf: url),
              let ids  = try? JSONDecoder().decode([String].self, from: data) else { return [] }
        return Set(ids)
    }

    private func cacheURL(for id: String) -> URL {
        cacheDir.appendingPathComponent(id)
    }
}
