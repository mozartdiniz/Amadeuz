//! On-disk copy of the iCloud notes, shown at launch while a fresh sync runs.
//!
//! Lives next to `data.json`. Holds what's displayed (folders, note list,
//! Markdown bodies). Editing data is not cached: cached notes are read-only
//! until the background sync brings fresh state. Deleted on "Disconnect iCloud".

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Bump when the format changes; older caches are ignored.
const VERSION: u32 = 2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cache {
    pub version: u32,
    pub folders: Vec<CachedFolder>,
    /// Newest first.
    pub notes: Vec<CachedNote>,
    /// Attachment id → image download URL (signed; may have expired).
    pub images: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFolder {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedNote {
    pub id: String,
    pub title: String,
    pub content: String,
    pub folder_id: String,
    pub created_at: i64,
    pub modified_at: i64,
    /// Full Markdown shown in the editor.
    pub body: String,
}

fn path() -> PathBuf {
    glib::user_data_dir().join("amadeuz").join("icloud-cache.json")
}

pub fn load() -> Option<Cache> {
    let text = std::fs::read_to_string(path()).ok()?;
    match serde_json::from_str::<Cache>(&text) {
        Ok(c) if c.version == VERSION => Some(c),
        Ok(_) => None,
        Err(e) => {
            log::warn!("ignoring unreadable iCloud cache: {e}");
            None
        }
    }
}

/// Write in the background (the cache can be a few MB); a half-written file is
/// never visible because the write goes through a temporary file.
pub fn save(mut cache: Cache) {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    // Writes can finish out of order; never let an older one win.
    static NEXT: AtomicU64 = AtomicU64::new(1);
    static WRITTEN: Mutex<u64> = Mutex::new(0);

    cache.version = VERSION;
    let generation = NEXT.fetch_add(1, Ordering::SeqCst);
    std::thread::spawn(move || {
        let Ok(mut written) = WRITTEN.lock() else { return };
        if generation < *written {
            return;
        }
        *written = generation;
        let path = path();
        let Ok(json) = serde_json::to_vec(&cache) else { return };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let tmp = path.with_extension("json.part");
        if std::fs::write(&tmp, json).is_ok() {
            std::fs::rename(&tmp, &path).ok();
        }
    });
}

pub fn delete() {
    std::fs::remove_file(path()).ok();
}
