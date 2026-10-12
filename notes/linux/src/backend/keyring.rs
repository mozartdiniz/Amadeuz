use anyhow::Result;
use keyring::Entry;

const SERVICE: &str = "com.amadeuz.Notes";
const USER: &str = "jwt";
const ICLOUD_USER: &str = "icloud-session";

/// Windows Credential Manager caps a secret at 2560 bytes; the iCloud session
/// (tokens + cookies) is larger, so on Windows it is split across entries.
/// 600 chars stays under the cap even if every char needs 4 bytes of UTF-8.
/// Elsewhere it stays one entry: on macOS every Keychain item asks for access
/// separately, so splitting would multiply the password prompts.
#[cfg(windows)]
const CHUNK: usize = 600;
#[cfg(not(windows))]
const CHUNK: usize = usize::MAX;

pub async fn load_token() -> Result<Option<String>> {
    load(USER).await
}

pub async fn save_token(token: &str) -> Result<()> {
    save(USER, token.to_owned()).await
}

pub async fn delete_token() -> Result<()> {
    delete(USER).await
}

pub async fn load_icloud_session() -> Result<Option<String>> {
    load_chunked(ICLOUD_USER).await
}

/// `previous` is what was loaded at startup, if known: an unchanged session is
/// not rewritten (each Keychain write can prompt on macOS).
pub async fn save_icloud_session(session: String, previous: Option<&str>) -> Result<()> {
    if previous == Some(session.as_str()) {
        return Ok(());
    }
    // Remove parts left by a previous split layout. Reads only the head entry,
    // which is the one about to be written anyway.
    if let Some(n) = load(ICLOUD_USER).await?.as_deref().and_then(chunk_count) {
        for i in 0..n {
            delete(&format!("{ICLOUD_USER}-{i}")).await?;
        }
    }
    if session.chars().count() <= CHUNK {
        return save(ICLOUD_USER, session).await;
    }
    let chars: Vec<char> = session.chars().collect();
    let chunks: Vec<String> = chars.chunks(CHUNK).map(|c| c.iter().collect()).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        save(&format!("{ICLOUD_USER}-{i}"), chunk.clone()).await?;
    }
    save(ICLOUD_USER, format!("chunks:{}", chunks.len())).await
}

fn chunk_count(head: &str) -> Option<usize> {
    head.strip_prefix("chunks:")?.parse().ok()
}

pub async fn delete_icloud_session() -> Result<()> {
    delete_chunked(ICLOUD_USER).await
}

async fn load_chunked(user: &str) -> Result<Option<String>> {
    let Some(head) = load(user).await? else { return Ok(None) };
    let Some(n) = chunk_count(&head) else {
        return Ok(Some(head));
    };
    let mut out = String::new();
    for i in 0..n {
        match load(&format!("{user}-{i}")).await? {
            Some(part) => out.push_str(&part),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

async fn delete_chunked(user: &str) -> Result<()> {
    if let Some(n) = load(user).await?.as_deref().and_then(chunk_count) {
        for i in 0..n {
            delete(&format!("{user}-{i}")).await?;
        }
    }
    delete(user).await
}

async fn load(user: &str) -> Result<Option<String>> {
    let user = user.to_owned();
    tokio::task::spawn_blocking(move || {
        let entry = Entry::new(SERVICE, &user)?;
        match entry.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    })
    .await?
}

async fn save(user: &str, secret: String) -> Result<()> {
    let user = user.to_owned();
    tokio::task::spawn_blocking(move || {
        let entry = Entry::new(SERVICE, &user)?;
        entry.set_password(&secret)?;
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}

async fn delete(user: &str) -> Result<()> {
    let user = user.to_owned();
    tokio::task::spawn_blocking(move || {
        let entry = Entry::new(SERVICE, &user)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    })
    .await??;
    Ok(())
}
