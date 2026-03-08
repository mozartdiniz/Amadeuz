import Foundation

struct NoteData: Codable {
    var content: String
    var updatedAt: Int64

    static let empty = NoteData(content: "", updatedAt: 0)
}

/// Persists note content to ~/Library/Application Support/amadeuz/note.json
final class LocalStore {
    private let fileURL: URL

    init() {
        let appSupport = FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let dir = appSupport.appendingPathComponent("amadeuz", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        fileURL = dir.appendingPathComponent("note.json")
    }

    func load() -> NoteData {
        guard
            let data = try? Data(contentsOf: fileURL),
            let note = try? JSONDecoder().decode(NoteData.self, from: data)
        else {
            return .empty
        }
        return note
    }

    func save(content: String, updatedAt: Int64) {
        let note = NoteData(content: content, updatedAt: updatedAt)
        guard let data = try? JSONEncoder().encode(note) else { return }
        try? data.write(to: fileURL, options: .atomic)
    }
}
