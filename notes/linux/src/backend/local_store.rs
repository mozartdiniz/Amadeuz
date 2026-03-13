// Local persistence: reads/writes ~/.local/share/amadeuz/data.json
// Same JSON format as all other Amadeuz clients.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
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
    let base = glib::user_data_dir();
    base.join("amadeuz").join("data.json")
}

pub fn load() -> Result<LocalData> {
    let path = data_path();
    if !path.exists() {
        return Ok(LocalData::default());
    }
    let bytes = std::fs::read(&path)?;
    Ok(serde_json::from_slice(&bytes)?)
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
