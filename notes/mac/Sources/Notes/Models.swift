import Foundation

// MARK: - Domain models

struct Folder: Codable, Identifiable, Equatable {
    let id: String
    var name: String
    let createdAt: Int64

    enum CodingKeys: String, CodingKey {
        case id, name
        case createdAt = "created_at"
    }
}

struct Note: Codable, Identifiable, Equatable {
    let id: String
    var folderID: String
    var title: String
    var content: String
    var updatedAt: Int64
    let createdAt: Int64

    enum CodingKeys: String, CodingKey {
        case id, title, content
        case folderID  = "folder_id"
        case updatedAt = "updated_at"
        case createdAt = "created_at"
    }
}

// MARK: - Auth response

struct AuthResponse: Decodable {
    let token: String
    let recoveryCode: String?

    enum CodingKeys: String, CodingKey {
        case token
        case recoveryCode = "recovery_code"
    }
}

// MARK: - Per-note WebSocket message

struct NoteWsMsg: Codable {
    let type: String
    var title: String?
    var content: String?
    var updatedAt: Int64?

    enum CodingKeys: String, CodingKey {
        case type, title, content
        case updatedAt = "updated_at"
    }
}
