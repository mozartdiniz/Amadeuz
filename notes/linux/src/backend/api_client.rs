use anyhow::Result;
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::backend::local_store::{FolderRecord, NoteRecord};

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Invalid email or password")]
    Unauthorized,
    #[error("Not found")]
    NotFound,
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Server error ({0})")]
    Server(u16),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub recovery_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FoldersResponse {
    folders: Vec<FolderRecord>,
}

#[derive(Debug, Deserialize)]
struct FolderResponse {
    folder: FolderRecord,
}

#[derive(Debug, Deserialize)]
struct NotesResponse {
    notes: Vec<NoteRecord>,
}

#[derive(Debug, Deserialize)]
struct NoteResponse {
    note: NoteRecord,
}

// ── Client ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ApiClient {
    http: Client,
    pub base_url: String,
    pub token: Option<String>,
}

impl ApiClient {
    pub fn new(base_url: String, token: Option<String>) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build reqwest client"),
            base_url,
            token,
        }
    }

    pub fn ws_url_for_note(&self, note_id: &str) -> Option<String> {
        let tok = self.token.as_ref()?;
        let ws_base = self
            .base_url
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        Some(format!("{ws_base}/notes/{note_id}/ws?token={tok}"))
    }

    // ── Auth ──────────────────────────────────────────────────────────────────

    pub async fn login(&self, email: &str, password: &str) -> Result<AuthResponse, ApiError> {
        self.post_unauth(
            "/auth/login",
            serde_json::json!({ "email": email, "password": password }),
        )
        .await
    }

    pub async fn register(&self, email: &str, password: &str) -> Result<AuthResponse, ApiError> {
        self.post_unauth(
            "/auth/register",
            serde_json::json!({ "email": email, "password": password }),
        )
        .await
    }

    pub async fn recover(
        &self,
        email: &str,
        code: &str,
        new_password: &str,
    ) -> Result<AuthResponse, ApiError> {
        self.post_unauth(
            "/auth/recover",
            serde_json::json!({
                "email": email,
                "recovery_code": code,
                "new_password": new_password,
            }),
        )
        .await
    }

    // ── Folders ───────────────────────────────────────────────────────────────

    pub async fn list_folders(&self) -> Result<Vec<FolderRecord>, ApiError> {
        let r: FoldersResponse = self.get("/folders").await?;
        Ok(r.folders)
    }

    pub async fn create_folder(
        &self,
        id: &str,
        name: &str,
        created_at: i64,
    ) -> Result<FolderRecord, ApiError> {
        let r: FolderResponse = self
            .post(
                "/folders",
                serde_json::json!({ "id": id, "name": name, "created_at": created_at }),
            )
            .await?;
        Ok(r.folder)
    }

    pub async fn rename_folder(&self, id: &str, name: &str) -> Result<FolderRecord, ApiError> {
        let r: FolderResponse = self
            .patch(&format!("/folders/{id}"), serde_json::json!({ "name": name }))
            .await?;
        Ok(r.folder)
    }

    pub async fn delete_folder(&self, id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/folders/{id}")).await
    }

    // ── Notes ─────────────────────────────────────────────────────────────────

    pub async fn list_notes(&self) -> Result<Vec<NoteRecord>, ApiError> {
        let r: NotesResponse = self.get("/notes").await?;
        Ok(r.notes)
    }

    pub async fn create_note(&self, note: &NoteRecord) -> Result<NoteRecord, ApiError> {
        let mut body = serde_json::json!({
            "id": note.id,
            "title": note.title,
            "content": note.content,
            "updated_at": note.updated_at,
            "created_at": note.created_at,
        });
        if let Some(fid) = &note.folder_id {
            if !fid.is_empty() {
                body["folder_id"] = serde_json::Value::String(fid.clone());
            }
        }
        let r: NoteResponse = self.post("/notes", body).await?;
        Ok(r.note)
    }

    pub async fn update_note(
        &self,
        id: &str,
        title: &str,
        content: &str,
        updated_at: i64,
    ) -> Result<NoteRecord, ApiError> {
        let r: NoteResponse = self
            .patch(
                &format!("/notes/{id}"),
                serde_json::json!({
                    "title": title,
                    "content": content,
                    "updated_at": updated_at,
                }),
            )
            .await?;
        Ok(r.note)
    }

    pub async fn move_note(&self, id: &str, folder_id: Option<&str>) -> Result<NoteRecord, ApiError> {
        let r: NoteResponse = self
            .patch(
                &format!("/notes/{id}/move"),
                serde_json::json!({ "folder_id": folder_id.unwrap_or("") }),
            )
            .await?;
        Ok(r.note)
    }

    pub async fn trash_note(&self, id: &str) -> Result<(), ApiError> {
        self.patch_empty(&format!("/notes/{id}/trash")).await
    }

    pub async fn restore_note(&self, id: &str) -> Result<(), ApiError> {
        self.patch_empty(&format!("/notes/{id}/restore")).await
    }

    pub async fn delete_note(&self, id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/notes/{id}")).await
    }

    // ── Blobs ─────────────────────────────────────────────────────────────────

    pub async fn upload_blob(&self, id: &str, data: Vec<u8>) -> Result<(), ApiError> {
        let url = format!("{}/blobs/{}", self.base_url, id);
        let mut req = self.http.put(&url).body(data);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        match resp.status() {
            s if s.is_success() => Ok(()),
            StatusCode::UNAUTHORIZED => Err(ApiError::Unauthorized),
            s => Err(ApiError::Server(s.as_u16())),
        }
    }

    pub async fn download_blob(&self, id: &str) -> Result<Vec<u8>, ApiError> {
        let url = format!("{}/blobs/{}", self.base_url, id);
        let resp = self.http.get(&url).send().await?;
        match resp.status() {
            s if s.is_success() => Ok(resp.bytes().await?.to_vec()),
            StatusCode::NOT_FOUND => Err(ApiError::NotFound),
            s => Err(ApiError::Server(s.as_u16())),
        }
    }

    // ── HTTP helpers ──────────────────────────────────────────────────────────

    fn auth_header(&self) -> Option<String> {
        self.token.as_ref().map(|t| format!("Bearer {t}"))
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ApiError> {
        let mut req = self.http.get(format!("{}{}", self.base_url, path));
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        self.parse(resp).await
    }

    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, ApiError> {
        let mut req = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .json(&body);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        self.parse(resp).await
    }

    async fn post_unauth<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, ApiError> {
        let resp = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .json(&body)
            .send()
            .await?;
        self.parse(resp).await
    }

    async fn patch<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, ApiError> {
        let mut req = self
            .http
            .patch(format!("{}{}", self.base_url, path))
            .json(&body);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        self.parse(resp).await
    }

    /// PATCH that expects 204 No Content (no response body).
    async fn patch_empty(&self, path: &str) -> Result<(), ApiError> {
        let mut req = self
            .http
            .patch(format!("{}{}", self.base_url, path))
            .header("Content-Length", "0");
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        match resp.status() {
            s if s.is_success() => Ok(()),
            StatusCode::UNAUTHORIZED => Err(ApiError::Unauthorized),
            StatusCode::NOT_FOUND => Err(ApiError::NotFound),
            s => Err(ApiError::Server(s.as_u16())),
        }
    }

    async fn delete(&self, path: &str) -> Result<(), ApiError> {
        let mut req = self.http.delete(format!("{}{}", self.base_url, path));
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        match resp.status() {
            StatusCode::NO_CONTENT | StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => Err(ApiError::Unauthorized),
            StatusCode::NOT_FOUND => Err(ApiError::NotFound),
            s => Err(ApiError::Server(s.as_u16())),
        }
    }

    async fn parse<T: for<'de> Deserialize<'de>>(
        &self,
        resp: reqwest::Response,
    ) -> Result<T, ApiError> {
        match resp.status() {
            s if s.is_success() => Ok(resp.json::<T>().await?),
            StatusCode::UNAUTHORIZED => Err(ApiError::Unauthorized),
            StatusCode::NOT_FOUND => Err(ApiError::NotFound),
            StatusCode::CONFLICT => {
                let msg = resp.text().await.unwrap_or_default();
                Err(ApiError::Conflict(msg))
            }
            s => Err(ApiError::Server(s.as_u16())),
        }
    }
}
