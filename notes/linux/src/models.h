#pragma once
#include <cstdint>
#include <string>
#include <vector>

struct Folder {
    std::string id;
    std::string name;
    int64_t     created_at{0};
};

struct Note {
    std::string id;
    std::string folder_id;
    std::string title;
    std::string content;
    int64_t     updated_at{0};
    int64_t     created_at{0};
};

/// Wire protocol message. Optional fields are present/absent depending on type.
struct WireMessage {
    std::string         type;

    // Collections: init response
    std::vector<Folder> folders;
    std::vector<Note>   notes;

    // Single entity: create/update responses
    Folder folder;
    bool   has_folder{false};
    Note   note;
    bool   has_note{false};

    // Scalar params: requests + delete broadcasts
    std::string folder_id;
    std::string note_id;
    std::string name;
    std::string title;
    std::string content;
    int64_t     updated_at{0};
};
