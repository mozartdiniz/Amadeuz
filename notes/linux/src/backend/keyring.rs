// JWT token storage via the Secret Service (libsecret).
// Stub — will be wired in Phase 2 (auth).

use anyhow::Result;

const SERVICE: &str = "com.amadeuz.Notes";
const LABEL: &str = "Amadeuz Notes JWT";

pub async fn load_token() -> Result<Option<String>> {
    // TODO: implement via secret-service crate
    Ok(None)
}

pub async fn save_token(token: &str) -> Result<()> {
    // TODO: implement via secret-service crate
    let _ = token;
    Ok(())
}

pub async fn delete_token() -> Result<()> {
    // TODO: implement via secret-service crate
    Ok(())
}
