import Foundation

private struct LocalData: Codable {
    var folders: [Folder]
    var notes: [Note]
}

/// Persists all folders and notes to
/// ~/Library/Application Support/amadeuz/data.json
final class LocalStore {
    private let fileURL: URL

    init() {
        let appSupport = FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let dir = appSupport.appendingPathComponent("amadeuz", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        fileURL = dir.appendingPathComponent("data.json")
    }

    func load() -> (folders: [Folder], notes: [Note]) {
        guard
            let data  = try? Data(contentsOf: fileURL),
            let local = try? JSONDecoder().decode(LocalData.self, from: data)
        else {
            return ([], [])
        }
        return (local.folders, local.notes)
    }

    func save(folders: [Folder], notes: [Note]) {
        guard let data = try? JSONEncoder().encode(LocalData(folders: folders, notes: notes))
        else { return }
        try? data.write(to: fileURL, options: .atomic)
    }
}
