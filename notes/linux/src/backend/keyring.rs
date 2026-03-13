use anyhow::Result;
use secret_service::{EncryptionType, SecretService};
use std::collections::HashMap;

const LABEL: &str = "Amadeuz Notes JWT";
const ATTR_APP: &str = "application";
const ATTR_APP_VAL: &str = "com.amadeuz.Notes";
const ATTR_KEY: &str = "key";
const ATTR_KEY_VAL: &str = "jwt";

fn attrs() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert(ATTR_APP, ATTR_APP_VAL);
    m.insert(ATTR_KEY, ATTR_KEY_VAL);
    m
}

pub async fn load_token() -> Result<Option<String>> {
    let ss = SecretService::connect(EncryptionType::Dh).await?;
    let col = ss.get_default_collection().await?;
    col.ensure_unlocked().await?;
    let results = col.search_items(attrs()).await?;
    match results.first() {
        Some(item) => {
            let secret = item.get_secret().await?;
            Ok(Some(String::from_utf8(secret)?))
        }
        None => Ok(None),
    }
}

pub async fn save_token(token: &str) -> Result<()> {
    let ss = SecretService::connect(EncryptionType::Dh).await?;
    let col = ss.get_default_collection().await?;
    col.ensure_unlocked().await?;
    col.create_item(LABEL, attrs(), token.as_bytes(), true, "text/plain; charset=utf8")
        .await?;
    Ok(())
}

pub async fn delete_token() -> Result<()> {
    let ss = SecretService::connect(EncryptionType::Dh).await?;
    let col = ss.get_default_collection().await?;
    col.ensure_unlocked().await?;
    for item in col.search_items(attrs()).await? {
        item.delete().await?;
    }
    Ok(())
}
