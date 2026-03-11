#include "keyring_store.h"

#include <libsecret/secret.h>

static const SecretSchema AMADEUZ_SCHEMA = {
    "com.amadeuz.jwt",
    SECRET_SCHEMA_NONE,
    {
        { "application", SECRET_SCHEMA_ATTRIBUTE_STRING },
        { nullptr,       (SecretSchemaAttributeType)0   }
    }
};

void KeyringStore::save(const std::string& token) {
    GError* error = nullptr;
    secret_password_store_sync(
        &AMADEUZ_SCHEMA, SECRET_COLLECTION_DEFAULT,
        "Amadeuz JWT Token", token.c_str(),
        nullptr, &error,
        "application", "amadeuz",
        nullptr);
    g_clear_error(&error);
}

std::string KeyringStore::load() {
    GError* error = nullptr;
    gchar*  pwd   = secret_password_lookup_sync(
        &AMADEUZ_SCHEMA, nullptr, &error,
        "application", "amadeuz",
        nullptr);
    g_clear_error(&error);

    if (!pwd) return {};
    std::string result = pwd;
    secret_password_free(pwd);
    return result;
}

void KeyringStore::clear() {
    GError* error = nullptr;
    secret_password_clear_sync(
        &AMADEUZ_SCHEMA, nullptr, &error,
        "application", "amadeuz",
        nullptr);
    g_clear_error(&error);
}
