#pragma once
#include <cstdint>
#include <string>

struct NoteData {
    std::string content;
    int64_t     updated_at{0};
};

/// Persists note content to ~/.local/share/amadeuz/note.json.
/// Same JSON schema as the macOS and Windows clients.
class LocalStore {
public:
    LocalStore();
    NoteData load() const;
    void     save(const std::string& content, int64_t updated_at) const;

private:
    std::string note_path_;
};
