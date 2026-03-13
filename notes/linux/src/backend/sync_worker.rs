use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Message received from the per-note WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub title: String,
    pub content: String,
    pub updated_at: i64,
}

/// RAII handle for a per-note WebSocket connection.
/// Dropping this disconnects immediately.
pub struct NoteSync {
    _cancel: oneshot::Sender<()>,
}

impl NoteSync {
    /// Spawns a tokio task that connects to `ws_url` and forwards messages via
    /// `on_msg`. Auto-reconnects every 3 seconds. Drops when the struct drops.
    pub fn connect(
        ws_url: String,
        on_msg: impl Fn(WsMessage) + Send + Sync + 'static,
        on_status: impl Fn(bool) + Send + Sync + 'static,
    ) -> Self {
        let (tx, mut rx) = oneshot::channel::<()>();
        crate::TOKIO_HANDLE
            .get()
            .expect("tokio runtime not initialised")
            .spawn(async move {
                loop {
                    tokio::select! {
                        _ = &mut rx => break,
                        _ = run_once(&ws_url, &on_msg, &on_status) => {}
                    }
                    // Check for cancel before sleeping.
                    if rx.try_recv().is_ok() {
                        break;
                    }
                    on_status(false);
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
                on_status(false);
            });
        NoteSync { _cancel: tx }
    }

    /// Send an update to the server over the WebSocket.
    /// NOTE: The current architecture sends updates via REST PATCH only.
    /// This is a placeholder for future direct-WS sends if needed.
    pub fn send_update(&self, _title: &str, _content: &str, _updated_at: i64) {}
}

/// Runs one WebSocket session until it drops or errors.
async fn run_once(
    url: &str,
    on_msg: &impl Fn(WsMessage),
    on_status: &impl Fn(bool),
) {
    let Ok((mut ws, _)) = connect_async(url).await else {
        return;
    };
    on_status(true);

    while let Some(Ok(msg)) = ws.next().await {
        if let Message::Text(text) = msg {
            if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                on_msg(ws_msg);
            }
        }
    }
    // Close cleanly.
    let _ = ws.close(None).await;
    on_status(false);
}
