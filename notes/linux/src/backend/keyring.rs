use anyhow::Result;
use keyring::Entry;

const SERVICE: &str = "com.amadeuz.Notes";
const USER: &str = "jwt";
/// The iCloud session is shared with Leporello (iCloud Reminders): signing in
/// to either app signs in both.
const ICLOUD_SERVICE: &str = "io.github.mozartdiniz.iCloud";
const ICLOUD_USER: &str = "web-session";
/// Where this app kept its own session before the entry was shared.
const LEGACY_ICLOUD_USER: &str = "icloud-session";
/// The Apple ID password, also shared with Leporello: an expired session is
/// renewed with it in the background (no 2FA code while Apple trusts this
/// computer). Signing out deletes it.
const ICLOUD_PASSWORD_USER: &str = "apple-id-password";

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
    load(SERVICE, USER).await
}

pub async fn save_token(token: &str) -> Result<()> {
    save(SERVICE, USER, token.to_owned()).await
}

pub async fn delete_token() -> Result<()> {
    delete(SERVICE, USER).await
}

pub async fn load_icloud_session() -> Result<Option<String>> {
    if let Some(s) = load_chunked(ICLOUD_SERVICE, ICLOUD_USER).await? {
        return Ok(Some(s));
    }
    let Some(s) = load_chunked(SERVICE, LEGACY_ICLOUD_USER).await? else { return Ok(None) };
    save_icloud_session(s.clone(), None).await?;
    delete_chunked(SERVICE, LEGACY_ICLOUD_USER).await?;
    Ok(Some(s))
}

/// `previous` is what was loaded at startup, if known: an unchanged session is
/// not rewritten (each Keychain write can prompt on macOS).
pub async fn save_icloud_session(session: String, previous: Option<&str>) -> Result<()> {
    if previous == Some(session.as_str()) {
        return Ok(());
    }
    // Remove parts left by a previous split layout. Reads only the head entry,
    // which is the one about to be written anyway.
    if let Some(n) = load(ICLOUD_SERVICE, ICLOUD_USER).await?.as_deref().and_then(chunk_count) {
        for i in 0..n {
            delete(ICLOUD_SERVICE, &format!("{ICLOUD_USER}-{i}")).await?;
        }
    }
    if session.chars().count() <= CHUNK {
        return save(ICLOUD_SERVICE, ICLOUD_USER, session).await;
    }
    let chars: Vec<char> = session.chars().collect();
    let chunks: Vec<String> = chars.chunks(CHUNK).map(|c| c.iter().collect()).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        save(ICLOUD_SERVICE, &format!("{ICLOUD_USER}-{i}"), chunk.clone()).await?;
    }
    save(ICLOUD_SERVICE, ICLOUD_USER, format!("chunks:{}", chunks.len())).await
}

fn chunk_count(head: &str) -> Option<usize> {
    head.strip_prefix("chunks:")?.parse().ok()
}

pub async fn load_icloud_password() -> Result<Option<String>> {
    load(ICLOUD_SERVICE, ICLOUD_PASSWORD_USER).await
}

pub async fn save_icloud_password(password: String) -> Result<()> {
    save(ICLOUD_SERVICE, ICLOUD_PASSWORD_USER, password).await
}

/// Apple refused it: forget it, so renewals don't keep trying it.
pub async fn delete_icloud_password() -> Result<()> {
    delete(ICLOUD_SERVICE, ICLOUD_PASSWORD_USER).await
}

/// Signs out every app that shares the session.
pub async fn delete_icloud_session() -> Result<()> {
    delete_chunked(ICLOUD_SERVICE, ICLOUD_USER).await?;
    delete_icloud_password().await?;
    delete_chunked(SERVICE, LEGACY_ICLOUD_USER).await
}

async fn load_chunked(service: &'static str, user: &str) -> Result<Option<String>> {
    let Some(head) = load(service, user).await? else { return Ok(None) };
    let Some(n) = chunk_count(&head) else {
        return Ok(Some(head));
    };
    let mut out = String::new();
    for i in 0..n {
        match load(service, &format!("{user}-{i}")).await? {
            Some(part) => out.push_str(&part),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

async fn delete_chunked(service: &'static str, user: &str) -> Result<()> {
    if let Some(n) = load(service, user).await?.as_deref().and_then(chunk_count) {
        for i in 0..n {
            delete(service, &format!("{user}-{i}")).await?;
        }
    }
    delete(service, user).await
}

async fn load(service: &'static str, user: &str) -> Result<Option<String>> {
    let user = user.to_owned();
    tokio::task::spawn_blocking(move || {
        let entry = Entry::new(service, &user)?;
        match entry.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    })
    .await?
}

async fn save(service: &'static str, user: &str, secret: String) -> Result<()> {
    let user = user.to_owned();
    tokio::task::spawn_blocking(move || {
        let entry = Entry::new(service, &user)?;
        entry.set_password(&secret)?;
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}

async fn delete(service: &'static str, user: &str) -> Result<()> {
    let user = user.to_owned();
    tokio::task::spawn_blocking(move || {
        let entry = Entry::new(service, &user)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    })
    .await??;
    Ok(())
}
