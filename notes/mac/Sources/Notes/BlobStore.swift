import Foundation

/// Manages local blob cache and HTTP upload/download against the amadeuz server.
final class BlobStore {

    private let cacheDir: URL
    private var serverBase: String   // e.g. "http://192.168.1.10:8080"

    init(serverBase: String) {
        self.serverBase = serverBase
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        cacheDir = support.appendingPathComponent("amadeuz/blobs", isDirectory: true)
        try? FileManager.default.createDirectory(at: cacheDir, withIntermediateDirectories: true)
    }

    func updateServerBase(_ base: String) {
        serverBase = base
    }

    // MARK: - Cache

    func cachedData(for id: String) -> Data? {
        try? Data(contentsOf: cacheURL(for: id))
    }

    // MARK: - Upload

    /// Uploads raw image data to the server and caches it locally.
    /// Returns the assigned blob ID.
    func upload(_ data: Data) async throws -> String {
        guard let url = URL(string: "\(serverBase)/blobs") else { throw URLError(.badURL) }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.httpBody = data
        let (responseData, _) = try await URLSession.shared.data(for: req)
        let decoded = try JSONDecoder().decode(UploadResponse.self, from: responseData)
        try? data.write(to: cacheURL(for: decoded.id), options: .atomic)
        return decoded.id
    }

    // MARK: - Download

    /// Downloads a blob, caching it locally. Returns cached data on subsequent calls.
    func download(id: String) async throws -> Data {
        if let cached = cachedData(for: id) { return cached }
        guard let url = URL(string: "\(serverBase)/blobs/\(id)") else { throw URLError(.badURL) }
        let (data, _) = try await URLSession.shared.data(from: url)
        try? data.write(to: cacheURL(for: id), options: .atomic)
        return data
    }

    // MARK: - Helpers

    private func cacheURL(for id: String) -> URL {
        cacheDir.appendingPathComponent(id)
    }
}

private struct UploadResponse: Decodable {
    let id: String
}

// MARK: - Derive HTTP base from WebSocket URL

extension BlobStore {
    /// Converts a WebSocket URL like "ws://host:8080/ws" to "http://host:8080".
    static func httpBase(from wsURL: String) -> String {
        wsURL
            .replacingOccurrences(of: "wss://", with: "https://")
            .replacingOccurrences(of: "ws://", with: "http://")
            .components(separatedBy: "/ws").first ?? wsURL
    }
}
