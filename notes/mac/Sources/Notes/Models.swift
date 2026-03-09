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
        case folderID = "folder_id"
        case updatedAt = "updated_at"
        case createdAt = "created_at"
    }
}

// MARK: - Wire message

/// Generic WebSocket message. Optional fields are omitted from JSON when nil.
struct WSMsg: Codable {
    var type: String

    // Collections (init response)
    var folders: [Folder]?
    var notes: [Note]?

    // Single entities (create/update responses)
    var folder: Folder?
    var note: Note?

    // Scalar params (requests + delete broadcasts)
    var folderID: String?
    var noteID: String?
    var name: String?
    var title: String?
    var content: String?
    var updatedAt: Int64?

    enum CodingKeys: String, CodingKey {
        case type, folders, notes, folder, note, name, title, content
        case folderID  = "folder_id"
        case noteID    = "note_id"
        case updatedAt = "updated_at"
    }

    // Custom init with defaults so call sites only specify what they need.
    init(
        type: String,
        folders: [Folder]?  = nil,
        notes: [Note]?      = nil,
        folder: Folder?     = nil,
        note: Note?         = nil,
        folderID: String?   = nil,
        noteID: String?     = nil,
        name: String?       = nil,
        title: String?      = nil,
        content: String?    = nil,
        updatedAt: Int64?   = nil
    ) {
        self.type      = type
        self.folders   = folders
        self.notes     = notes
        self.folder    = folder
        self.note      = note
        self.folderID  = folderID
        self.noteID    = noteID
        self.name      = name
        self.title     = title
        self.content   = content
        self.updatedAt = updatedAt
    }

    // Encode: omit nil fields instead of writing null.
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(type, forKey: .type)
        try c.encodeIfPresent(folders,   forKey: .folders)
        try c.encodeIfPresent(notes,     forKey: .notes)
        try c.encodeIfPresent(folder,    forKey: .folder)
        try c.encodeIfPresent(note,      forKey: .note)
        try c.encodeIfPresent(folderID,  forKey: .folderID)
        try c.encodeIfPresent(noteID,    forKey: .noteID)
        try c.encodeIfPresent(name,      forKey: .name)
        try c.encodeIfPresent(title,     forKey: .title)
        try c.encodeIfPresent(content,   forKey: .content)
        try c.encodeIfPresent(updatedAt, forKey: .updatedAt)
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        type      = try  c.decode(String.self,    forKey: .type)
        folders   = try? c.decodeIfPresent([Folder].self, forKey: .folders)
        notes     = try? c.decodeIfPresent([Note].self,   forKey: .notes)
        folder    = try? c.decodeIfPresent(Folder.self,   forKey: .folder)
        note      = try? c.decodeIfPresent(Note.self,     forKey: .note)
        folderID  = try? c.decodeIfPresent(String.self,   forKey: .folderID)
        noteID    = try? c.decodeIfPresent(String.self,   forKey: .noteID)
        name      = try? c.decodeIfPresent(String.self,   forKey: .name)
        title     = try? c.decodeIfPresent(String.self,   forKey: .title)
        content   = try? c.decodeIfPresent(String.self,   forKey: .content)
        updatedAt = try? c.decodeIfPresent(Int64.self,    forKey: .updatedAt)
    }
}
