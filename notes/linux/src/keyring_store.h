#pragma once
#include <string>

/// Stores the JWT token in the GNOME Keyring (libsecret).
/// Falls back gracefully if the keyring service is unavailable.
class KeyringStore {
public:
    static void        save(const std::string& token);
    static std::string load();
    static void        clear();
};
