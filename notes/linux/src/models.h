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
    std::string folder_id;   // empty string = unfoldered
    std::string title;
    std::string content;
    int64_t     updated_at{0};
    int64_t     created_at{0};
};

struct AuthResponse {
    std::string token;
    std::string recovery_code; // non-empty after register or recover
};

// Per-note WebSocket message (init / update)
struct NoteWsMsg {
    std::string type;       // "init" | "update"
    std::string title;
    std::string content;
    int64_t     updated_at{0};
};
