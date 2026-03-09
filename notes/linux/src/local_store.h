#pragma once
#include "models.h"

/// Persists all folders and notes to ~/.local/share/amadeuz/data.json.
/// Server address is stored in ~/.local/share/amadeuz/settings.json.
class LocalStore {
public:
    LocalStore();

    void load(std::vector<Folder>& folders, std::vector<Note>& notes) const;
    void save(const std::vector<Folder>& folders, const std::vector<Note>& notes) const;

    std::string load_server_address() const;
    void        save_server_address(const std::string& address) const;

private:
    std::string data_path_;
    std::string settings_path_;
};
