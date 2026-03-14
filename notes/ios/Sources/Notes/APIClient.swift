import Foundation

enum APIError: Error, LocalizedError {
    case unauthorized
    case notFound
    case conflict(String)
    case server(Int)

    var errorDescription: String? {
        switch self {
        case .unauthorized:      return "Invalid email or password"
        case .notFound:          return "Not found"
        case .conflict(let msg): return msg
        case .server(let c):     return "Server error (\(c))"
        }
    }
}

final class APIClient {
    var serverBase: String
    var token: String?

    private let decoder = JSONDecoder()
    private let session: URLSession = {
        let cfg = URLSessionConfiguration.default
        cfg.timeoutIntervalForRequest  = 10
        cfg.timeoutIntervalForResource = 30
        return URLSession(configuration: cfg)
    }()

    init(serverBase: String, token: String? = nil) {
        self.serverBase = serverBase
        self.token      = token
    }

    // MARK: - Auth (unauthenticated)

    func register(email: String, password: String) async throws -> AuthResponse {
        try await post("/auth/register",
                       body: ["email": email, "password": password],
                       authenticated: false)
    }

    func login(email: String, password: String) async throws -> AuthResponse {
        try await post("/auth/login",
                       body: ["email": email, "password": password],
                       authenticated: false)
    }

    func recover(email: String, code: String, newPassword: String) async throws -> AuthResponse {
        try await post("/auth/recover",
                       body: ["email": email, "recovery_code": code, "new_password": newPassword],
                       authenticated: false)
    }

    // MARK: - Folders

    func listFolders() async throws -> [Folder] {
        struct R: Decodable { let folders: [Folder] }
        let r: R = try await get("/folders")
        return r.folders
    }

    func createFolder(id: String, name: String, createdAt: Int64) async throws -> Folder {
        struct R: Decodable { let folder: Folder }
        let r: R = try await post("/folders",
                                  body: ["id": id, "name": name, "created_at": createdAt])
        return r.folder
    }

    func renameFolder(id: String, name: String) async throws -> Folder {
        struct R: Decodable { let folder: Folder }
        let r: R = try await patch("/folders/\(id)", body: ["name": name])
        return r.folder
    }

    func deleteFolder(id: String) async throws {
        try await delete("/folders/\(id)")
    }

    // MARK: - Notes

    func listNotes() async throws -> [Note] {
        struct R: Decodable { let notes: [Note] }
        let r: R = try await get("/notes")
        return r.notes
    }

    func createNote(
        id: String, folderID: String?,
        title: String, content: String,
        updatedAt: Int64, createdAt: Int64
    ) async throws -> Note {
        var body: [String: Any] = [
            "id": id, "title": title, "content": content,
            "updated_at": updatedAt, "created_at": createdAt,
        ]
        if let fid = folderID, !fid.isEmpty { body["folder_id"] = fid }
        struct R: Decodable { let note: Note }
        let r: R = try await post("/notes", body: body)
        return r.note
    }

    func updateNote(id: String, title: String, content: String, updatedAt: Int64) async throws -> Note {
        struct R: Decodable { let note: Note }
        let r: R = try await patch("/notes/\(id)",
                                   body: ["title": title, "content": content, "updated_at": updatedAt])
        return r.note
    }

    func moveNote(id: String, folderID: String?) async throws -> Note {
        struct R: Decodable { let note: Note }
        let r: R = try await patch("/notes/\(id)/move", body: ["folder_id": folderID ?? ""])
        return r.note
    }

    func trashNote(id: String) async throws {
        try await performVoid(try makeRequest(method: "PATCH", path: "/notes/\(id)/trash"))
    }

    func restoreNote(id: String) async throws -> Note {
        struct R: Decodable { let note: Note }
        let r: R = try await perform(try makeRequest(method: "PATCH", path: "/notes/\(id)/restore"))
        return r.note
    }

    func deleteNote(id: String) async throws {
        try await delete("/notes/\(id)")
    }

    // MARK: - WebSocket URL

    func wsURL(forNoteID noteID: String) -> URL? {
        guard let tok = token else { return nil }
        let wsBase = serverBase
            .replacingOccurrences(of: "https://", with: "wss://")
            .replacingOccurrences(of: "http://",  with: "ws://")
        return URL(string: "\(wsBase)/notes/\(noteID)/ws?token=\(tok)")
    }

    // MARK: - Private helpers

    private func makeRequest(
        method: String,
        path: String,
        body: [String: Any]? = nil,
        authenticated: Bool = true
    ) throws -> URLRequest {
        guard let url = URL(string: serverBase + path) else { throw URLError(.badURL) }
        var req = URLRequest(url: url)
        req.httpMethod = method
        if authenticated, let tok = token {
            req.setValue("Bearer \(tok)", forHTTPHeaderField: "Authorization")
        }
        if let body {
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        return req
    }

    private func checkStatus(_ response: URLResponse, body: Data) throws {
        guard let http = response as? HTTPURLResponse else { throw URLError(.badServerResponse) }
        switch http.statusCode {
        case 200, 201, 204: return
        case 401: throw APIError.unauthorized
        case 404: throw APIError.notFound
        case 409: throw APIError.conflict(serverMessage(body) ?? "Conflict")
        default:  throw APIError.server(http.statusCode)
        }
    }

    private func serverMessage(_ data: Data) -> String? {
        let s = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
        return (s?.isEmpty == false) ? s : nil
    }

    private func perform<T: Decodable>(_ req: URLRequest) async throws -> T {
        let (data, response) = try await session.data(for: req)
        try checkStatus(response, body: data)
        return try decoder.decode(T.self, from: data)
    }

    private func performVoid(_ req: URLRequest) async throws {
        let (data, response) = try await session.data(for: req)
        try checkStatus(response, body: data)
    }

    private func get<T: Decodable>(_ path: String) async throws -> T {
        try await perform(try makeRequest(method: "GET", path: path))
    }

    private func post<T: Decodable>(
        _ path: String,
        body: [String: Any],
        authenticated: Bool = true
    ) async throws -> T {
        try await perform(try makeRequest(method: "POST", path: path, body: body, authenticated: authenticated))
    }

    private func patch<T: Decodable>(_ path: String, body: [String: Any]) async throws -> T {
        try await perform(try makeRequest(method: "PATCH", path: path, body: body))
    }

    private func delete(_ path: String) async throws {
        try await performVoid(try makeRequest(method: "DELETE", path: path))
    }
}
