// Async sync worker: REST API + per-note WebSocket.
// Stub — will be wired in Phase 2 (auth + sync).

use anyhow::Result;

pub struct SyncWorker {
    pub server_url: String,
    pub token: Option<String>,
}

impl SyncWorker {
    pub fn new(server_url: String, token: Option<String>) -> Self {
        Self { server_url, token }
    }
}
