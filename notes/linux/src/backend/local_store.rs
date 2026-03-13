use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LocalData {
    pub folders: Vec<FolderRecord>,
    pub notes: Vec<NoteRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderRecord {
    pub id: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRecord {
    pub id: String,
    pub title: String,
    pub content: String,
    pub folder_id: Option<String>,
    pub updated_at: i64,
    pub created_at: i64,
}

fn data_path() -> PathBuf {
    glib::user_data_dir().join("amadeuz").join("data.json")
}

pub fn load() -> LocalData {
    let path = data_path();
    if !path.exists() {
        return LocalData::default();
    }
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(e) => {
            log::warn!("Could not read local data: {e}");
            LocalData::default()
        }
    }
}

pub fn save(data: &LocalData) -> Result<()> {
    let path = data_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(data)?;
    std::fs::write(&path, json)?;
    Ok(())
}
