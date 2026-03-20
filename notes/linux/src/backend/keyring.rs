use anyhow::Result;
use keyring::Entry;

const SERVICE: &str = "com.amadeuz.Notes";
const USER: &str = "jwt";

pub async fn load_token() -> Result<Option<String>> {
    tokio::task::spawn_blocking(|| {
        let entry = Entry::new(SERVICE, USER)?;
        match entry.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    })
    .await?
}

pub async fn save_token(token: &str) -> Result<()> {
    let token = token.to_owned();
    tokio::task::spawn_blocking(move || {
        let entry = Entry::new(SERVICE, USER)?;
        entry.set_password(&token)?;
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}

pub async fn delete_token() -> Result<()> {
    tokio::task::spawn_blocking(|| {
        let entry = Entry::new(SERVICE, USER)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    })
    .await??;
    Ok(())
}
