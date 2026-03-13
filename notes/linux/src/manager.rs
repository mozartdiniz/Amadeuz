use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};
use uuid::Uuid;

use crate::backend::api_client::{ApiClient, ApiError};
use crate::backend::local_store::{self, FolderRecord, NoteRecord};
use crate::backend::sync_worker::NoteSync;
use crate::model::{AmzFolder, AmzNote};

/// Reserved folder ID for soft-deleted notes.
pub const WASTEBASKET_ID: &str = "__wastebasket__";

// ── AppEvent: sent from tokio tasks → GTK main thread ─────────────────────────

#[derive(Debug)]
pub enum AppEvent {
    AuthSuccess {
        token: String,
        recovery_code: Option<String>,
    },
    AuthError(String),
    SyncComplete {
        folders: Vec<FolderRecord>,
        notes: Vec<NoteRecord>,
    },
    SyncError,
    NoteWsUpdate {
        note_id: String,
        title: String,
        content: String,
        updated_at: i64,
    },
    WsConnected,
    WsDisconnected,
}

// ── NotesManager ──────────────────────────────────────────────────────────────

pub type ManagerRef = Rc<RefCell<NotesManager>>;

pub struct NotesManager {
    pub api: ApiClient,
    /// GTK list stores — updated on the main thread; observed by UI widgets.
    pub folder_store: gio::ListStore,
    pub note_store: gio::ListStore,

    pub is_authenticated: bool,
    pub is_connected: bool,
    pub selected_folder_id: Option<String>,
    pub selected_note_id: Option<String>,

    /// Channel sender — tokio tasks use this to push AppEvents to the main loop.
    event_tx: async_channel::Sender<AppEvent>,

    /// Live WebSocket for the currently selected note.
    note_sync: Option<NoteSync>,

    /// Pending debounce source ID.  Some = a save is scheduled.
    pub debounce_source: Option<glib::SourceId>,

    /// Callback invoked (on main thread) when any relevant state changes.
    /// Stored as Rc so it can be cloned out of the borrow before being called
    /// (calling while borrow_mut is held would cause a RefCell panic).
    pub on_state_changed: Option<std::rc::Rc<dyn Fn()>>,
    pub on_auth_error: Option<std::rc::Rc<dyn Fn(String)>>,
    pub on_recovery_code: Option<std::rc::Rc<dyn Fn(String)>>,
}

impl NotesManager {
    pub fn new(server_url: String, token: Option<String>) -> (ManagerRef, async_channel::Receiver<AppEvent>) {
        let (tx, rx) = async_channel::bounded::<AppEvent>(128);

        let mgr = Rc::new(RefCell::new(NotesManager {
            api: ApiClient::new(server_url, token),
            folder_store: gio::ListStore::new::<AmzFolder>(),
            note_store: gio::ListStore::new::<AmzNote>(),
            is_authenticated: false,
            is_connected: false,
            selected_folder_id: None,
            selected_note_id: None,
            event_tx: tx,
            note_sync: None,
            debounce_source: None,
            on_state_changed: None,
            on_auth_error: None,
            on_recovery_code: None,
        }));

        (mgr, rx)
    }

    // ── Auth ──────────────────────────────────────────────────────────────────

    pub fn login(&self, email: String, password: String) {
        let mut api = self.api.clone();
        api.token = None;
        let tx = self.event_tx.clone();
        crate::spawn(async move {
            let event = match api.login(&email, &password).await {
                Ok(resp) => AppEvent::AuthSuccess {
                    token: resp.token,
                    recovery_code: resp.recovery_code,
                },
                Err(e) => AppEvent::AuthError(e.to_string()),
            };
            tx.send(event).await.ok();
        });
    }

    pub fn register(&self, email: String, password: String) {
        let mut api = self.api.clone();
        api.token = None;
        let tx = self.event_tx.clone();
        crate::spawn(async move {
            let event = match api.register(&email, &password).await {
                Ok(resp) => AppEvent::AuthSuccess {
                    token: resp.token,
                    recovery_code: resp.recovery_code,
                },
                Err(e) => AppEvent::AuthError(e.to_string()),
            };
            tx.send(event).await.ok();
        });
    }

    pub fn recover(&self, email: String, code: String, new_password: String) {
        let mut api = self.api.clone();
        api.token = None;
        let tx = self.event_tx.clone();
        crate::spawn(async move {
            let event = match api.recover(&email, &code, &new_password).await {
                Ok(resp) => AppEvent::AuthSuccess {
                    token: resp.token,
                    recovery_code: resp.recovery_code,
                },
                Err(e) => AppEvent::AuthError(e.to_string()),
            };
            tx.send(event).await.ok();
        });
    }

    pub fn logout(&mut self) {
        self.note_sync = None;
        self.is_authenticated = false;
        self.is_connected = false;
        self.api.token = None;
        self.selected_folder_id = None;
        self.selected_note_id = None;
        self.folder_store.remove_all();
        self.note_store.remove_all();
        local_store::save(&local_store::LocalData::default()).ok();

        crate::spawn(async move {
            crate::backend::keyring::delete_token().await.ok();
        });
        // on_state_changed is intentionally NOT called here: logout() is called
        // while mgr is already borrow_mut'd by the caller, so invoking the
        // callback (which calls refresh_ui → mgr.borrow()) would panic.
        // The caller (sign_out in window.rs) calls refresh_ui() after releasing
        // the borrow.
    }

    // ── Full sync (REST) ──────────────────────────────────────────────────────

    pub fn full_sync(mgr: ManagerRef) {
        let api = mgr.borrow().api.clone();
        let tx = mgr.borrow().event_tx.clone();
        crate::spawn(async move {
            let event = match do_full_sync(api).await {
                Ok((folders, notes)) => AppEvent::SyncComplete { folders, notes },
                Err(ApiError::Unauthorized) => {
                    AppEvent::AuthError("Session expired. Please log in again.".into())
                }
                Err(_) => AppEvent::SyncError,
            };
            tx.send(event).await.ok();
        });
    }

    // ── Event handler (called on GTK main thread) ─────────────────────────────

    pub fn handle_event(mgr: ManagerRef, event: AppEvent) {
        match event {
            AppEvent::AuthSuccess { token, recovery_code } => {
                // Release the borrow_mut BEFORE calling any callback.
                // on_state_changed = refresh_ui, which calls mgr.borrow() — that would
                // panic if we're still holding borrow_mut here.
                let (state_cb, recovery_cb) = {
                    let mut m = mgr.borrow_mut();
                    m.api.token = Some(token.clone());
                    m.is_authenticated = true;
                    let tok = token.clone();
                    crate::spawn(async move {
                        crate::backend::keyring::save_token(&tok).await.ok();
                    });
                    let state_cb = m.on_state_changed.clone();
                    let recovery_cb = recovery_code.and_then(|code| {
                        m.on_recovery_code.clone().map(|cb| (cb, code))
                    });
                    (state_cb, recovery_cb)
                }; // borrow_mut released here
                if let Some(cb) = state_cb { cb(); }
                if let Some((cb, code)) = recovery_cb { cb(code); }
                NotesManager::full_sync(mgr);
            }

            AppEvent::AuthError(msg) => {
                let cb = mgr.borrow().on_auth_error.clone();
                if let Some(cb) = cb { cb(msg); }
            }

            AppEvent::SyncComplete { folders, notes } => {
                let state_cb = {
                    let mut m = mgr.borrow_mut();
                    m.is_connected = true;
                    m.apply_sync_result(folders, notes);
                    m.on_state_changed.clone()
                };
                if let Some(cb) = state_cb { cb(); }
            }

            AppEvent::SyncError => {
                let state_cb = {
                    let mut m = mgr.borrow_mut();
                    m.is_connected = false;
                    m.on_state_changed.clone()
                };
                if let Some(cb) = state_cb { cb(); }
            }

            AppEvent::NoteWsUpdate { note_id, title, content, updated_at } => {
                let state_cb = {
                    let mut m = mgr.borrow_mut();
                    m.apply_ws_update(&note_id, title, content, updated_at);
                    m.on_state_changed.clone()
                };
                if let Some(cb) = state_cb { cb(); }
            }

            AppEvent::WsConnected => {
                let state_cb = {
                    let mut m = mgr.borrow_mut();
                    m.is_connected = true;
                    m.on_state_changed.clone()
                };
                if let Some(cb) = state_cb { cb(); }
            }

            AppEvent::WsDisconnected => {
                let state_cb = {
                    let mut m = mgr.borrow_mut();
                    m.is_connected = false;
                    m.on_state_changed.clone()
                };
                if let Some(cb) = state_cb { cb(); }
            }
        }
    }

    /// Populate the list stores from local disk immediately.
    /// Called on startup (when a saved token exists) so the UI is usable
    /// before the first server sync completes or if the server is unreachable.
    pub fn load_local(&mut self) {
        let data = local_store::load();

        self.folder_store.remove_all();
        for f in &data.folders {
            self.folder_store
                .append(&AmzFolder::new(&f.id, &f.name, f.created_at));
        }

        let mut notes = data.notes.clone();
        notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        self.note_store.remove_all();
        for n in &notes {
            self.note_store.append(&AmzNote::new(
                &n.id,
                &n.title,
                &n.content,
                n.folder_id.as_deref().unwrap_or(""),
                n.updated_at,
                n.created_at,
            ));
        }
    }

    // ── Folder actions ────────────────────────────────────────────────────────

    pub fn create_folder(&mut self, name: String) {
        let now = now_ms();
        let id = Uuid::new_v4().to_string();
        self.folder_store
            .append(&AmzFolder::new(&id, &name, now));
        self.persist();
        let api = self.api.clone();
        crate::spawn(async move {
            api.create_folder(&id, &name, now).await.ok();
        });
    }

    pub fn rename_folder(&mut self, id: &str, new_name: String) {
        if let Some(folder) = self.find_folder(id) {
            folder.set_name(new_name.clone());
        }
        self.persist();
        let api = self.api.clone();
        let id = id.to_owned();
        crate::spawn(async move {
            api.rename_folder(&id, &new_name).await.ok();
        });
    }

    pub fn delete_folder(&mut self, id: &str) {
        self.remove_folder_from_store(id);
        // Remove notes that belong to this folder.
        let to_remove: Vec<u32> = (0..self.note_store.n_items())
            .rev()
            .filter(|&i| {
                self.note_store
                    .item(i)
                    .and_downcast::<AmzNote>()
                    .map(|n| n.folder_id() == id)
                    .unwrap_or(false)
            })
            .collect();
        for i in to_remove {
            self.note_store.remove(i);
        }
        if self.selected_folder_id.as_deref() == Some(id) {
            self.selected_folder_id = None;
            self.selected_note_id = None;
        }
        self.persist();
        let api = self.api.clone();
        let id = id.to_owned();
        crate::spawn(async move {
            api.delete_folder(&id).await.ok();
        });
    }

    // ── Note actions ──────────────────────────────────────────────────────────

    pub fn create_note(&mut self, folder_id: Option<String>) -> Option<String> {
        let now = now_ms();
        let id = Uuid::new_v4().to_string();
        let fid = folder_id.unwrap_or_default();
        let note = AmzNote::new(&id, "", "", &fid, now, now);
        self.note_store.insert(0, &note);
        self.persist();
        let api = self.api.clone();
        let record = NoteRecord {
            id: id.clone(),
            title: String::new(),
            content: String::new(),
            folder_id: if fid.is_empty() { None } else { Some(fid) },
            updated_at: now,
            created_at: now,
        };
        crate::spawn(async move {
            api.create_note(&record).await.ok();
        });
        Some(id)
    }

    /// Move a note to the wastebasket (soft delete).
    pub fn trash_note(&mut self, id: &str) {
        self.move_note(id, Some(WASTEBASKET_ID.to_string()));
    }

    /// Move a note out of the wastebasket back to "No Folder".
    pub fn restore_note(&mut self, id: &str) {
        self.move_note(id, None);
    }

    pub fn delete_note(&mut self, id: &str) {
        self.note_sync = None;
        self.remove_note_from_store(id);
        if self.selected_note_id.as_deref() == Some(id) {
            self.selected_note_id = None;
        }
        self.persist();
        let api = self.api.clone();
        let id = id.to_owned();
        crate::spawn(async move {
            api.delete_note(&id).await.ok();
        });
    }

    pub fn move_note(&mut self, id: &str, folder_id: Option<String>) {
        if let Some(note) = self.find_note(id) {
            let fid = folder_id.clone().unwrap_or_default();
            note.set_folder_id(fid);
            note.set_updated_at(now_ms());
        }
        self.persist();
        let api = self.api.clone();
        let id = id.to_owned();
        crate::spawn(async move {
            api.move_note(&id, folder_id.as_deref()).await.ok();
        });
    }

    /// Schedule a debounced save (500ms). Cancels any pending save first.
    pub fn schedule_save(mgr: ManagerRef, id: String, title: String, content: String) {
        // Cancel existing debounce.
        if let Some(src) = mgr.borrow_mut().debounce_source.take() {
            src.remove();
        }

        let mgr_weak = Rc::downgrade(&mgr);
        let src = glib::timeout_add_local_once(
            std::time::Duration::from_millis(500),
            move || {
                if let Some(mgr) = mgr_weak.upgrade() {
                    let mut m = mgr.borrow_mut();
                    m.debounce_source = None;
                    m.flush_note(&id, title, content);
                }
            },
        );
        mgr.borrow_mut().debounce_source = Some(src);
    }

    pub fn flush_note(&mut self, id: &str, title: String, content: String) {
        let now = now_ms();
        if let Some(note) = self.find_note(id) {
            // Skip if unchanged.
            if note.title() == title && note.content() == content {
                return;
            }
            note.set_title(title.clone());
            note.set_content(content.clone());
            note.set_updated_at(now);
        }
        self.persist();
        let api = self.api.clone();
        let id = id.to_owned();
        crate::spawn(async move {
            api.update_note(&id, &title, &content, now).await.ok();
        });
    }

    // ── WebSocket for selected note ────────────────────────────────────────────

    pub fn connect_note_ws(&mut self, note_id: &str) {
        self.note_sync = None;

        let Some(url) = self.api.ws_url_for_note(note_id) else {
            return;
        };

        let tx = self.event_tx.clone();
        let note_id = note_id.to_owned();
        let tx2 = tx.clone();
        self.note_sync = Some(NoteSync::connect(
            url,
            move |msg| {
                if msg.msg_type == "init" || msg.msg_type == "update" {
                    tx.try_send(AppEvent::NoteWsUpdate {
                        note_id: note_id.clone(),
                        title: msg.title,
                        content: msg.content,
                        updated_at: msg.updated_at,
                    })
                    .ok();
                }
            },
            move |connected| {
                tx2.try_send(if connected {
                    AppEvent::WsConnected
                } else {
                    AppEvent::WsDisconnected
                })
                .ok();
            },
        ));
    }

    pub fn disconnect_note_ws(&mut self) {
        self.note_sync = None;
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn apply_sync_result(&mut self, folders: Vec<FolderRecord>, notes: Vec<NoteRecord>) {
        // Merge: load current local state for diff.
        let local = local_store::load();

        // Build merged folders: server + local-only.
        let server_fids: std::collections::HashSet<&str> =
            folders.iter().map(|f| f.id.as_str()).collect();
        let mut merged_folders = folders.clone();
        for lf in &local.folders {
            if !server_fids.contains(lf.id.as_str()) {
                merged_folders.push(lf.clone());
            }
        }

        // Build merged notes: last-write-wins.
        let server_nmap: std::collections::HashMap<&str, &NoteRecord> =
            notes.iter().map(|n| (n.id.as_str(), n)).collect();
        let local_nids: std::collections::HashSet<&str> =
            local.notes.iter().map(|n| n.id.as_str()).collect();

        let mut merged_notes: Vec<NoteRecord> = Vec::new();
        for ln in &local.notes {
            if let Some(sn) = server_nmap.get(ln.id.as_str()) {
                merged_notes.push(if ln.updated_at > sn.updated_at {
                    ln.clone()
                } else {
                    (*sn).clone()
                });
            } else {
                merged_notes.push(ln.clone());
            }
        }
        for sn in &notes {
            if !local_nids.contains(sn.id.as_str()) {
                merged_notes.push(sn.clone());
            }
        }

        // Push offline-only items to server (fire and forget).
        let api = self.api.clone();
        let local_only_folders: Vec<FolderRecord> = local
            .folders
            .iter()
            .filter(|f| !server_fids.contains(f.id.as_str()))
            .cloned()
            .collect();
        let local_only_notes: Vec<NoteRecord> = local
            .notes
            .iter()
            .filter(|n| server_nmap.get(n.id.as_str()).is_none())
            .cloned()
            .collect();
        let newer_local_notes: Vec<NoteRecord> = local
            .notes
            .iter()
            .filter(|n| {
                server_nmap
                    .get(n.id.as_str())
                    .map(|sn| n.updated_at > sn.updated_at)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        crate::spawn(async move {
            for f in local_only_folders {
                api.create_folder(&f.id, &f.name, f.created_at).await.ok();
            }
            for n in local_only_notes {
                api.create_note(&n).await.ok();
            }
            for n in newer_local_notes {
                api.update_note(&n.id, &n.title, &n.content, n.updated_at)
                    .await
                    .ok();
            }
        });

        // Update list stores.
        self.folder_store.remove_all();
        for f in &merged_folders {
            self.folder_store
                .append(&AmzFolder::new(&f.id, &f.name, f.created_at));
        }

        merged_notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        self.note_store.remove_all();
        for n in &merged_notes {
            self.note_store.append(&AmzNote::new(
                &n.id,
                &n.title,
                &n.content,
                n.folder_id.as_deref().unwrap_or(""),
                n.updated_at,
                n.created_at,
            ));
        }

        // Persist merged result.
        local_store::save(&local_store::LocalData {
            folders: merged_folders,
            notes: merged_notes,
        })
        .ok();
    }

    fn apply_ws_update(&mut self, note_id: &str, title: String, content: String, updated_at: i64) {
        if let Some(note) = self.find_note(note_id) {
            if updated_at > note.updated_at() {
                note.set_title(title);
                note.set_content(content);
                note.set_updated_at(updated_at);
                self.persist();
            }
        }
    }

    fn find_note(&self, id: &str) -> Option<AmzNote> {
        (0..self.note_store.n_items()).find_map(|i| {
            self.note_store
                .item(i)
                .and_downcast::<AmzNote>()
                .filter(|n| n.id() == id)
        })
    }

    fn find_folder(&self, id: &str) -> Option<AmzFolder> {
        (0..self.folder_store.n_items()).find_map(|i| {
            self.folder_store
                .item(i)
                .and_downcast::<AmzFolder>()
                .filter(|f| f.id() == id)
        })
    }

    fn remove_note_from_store(&self, id: &str) {
        for i in (0..self.note_store.n_items()).rev() {
            if self
                .note_store
                .item(i)
                .and_downcast::<AmzNote>()
                .map(|n| n.id() == id)
                .unwrap_or(false)
            {
                self.note_store.remove(i);
                return;
            }
        }
    }

    fn remove_folder_from_store(&self, id: &str) {
        for i in (0..self.folder_store.n_items()).rev() {
            if self
                .folder_store
                .item(i)
                .and_downcast::<AmzFolder>()
                .map(|f| f.id() == id)
                .unwrap_or(false)
            {
                self.folder_store.remove(i);
                return;
            }
        }
    }

    fn persist(&self) {
        let folders: Vec<FolderRecord> = (0..self.folder_store.n_items())
            .filter_map(|i| {
                self.folder_store
                    .item(i)
                    .and_downcast::<AmzFolder>()
                    .map(|f| FolderRecord {
                        id: f.id(),
                        name: f.name(),
                        created_at: f.created_at(),
                    })
            })
            .collect();

        let notes: Vec<NoteRecord> = (0..self.note_store.n_items())
            .filter_map(|i| {
                self.note_store
                    .item(i)
                    .and_downcast::<AmzNote>()
                    .map(|n| NoteRecord {
                        id: n.id(),
                        title: n.title(),
                        content: n.content(),
                        folder_id: {
                            let fid = n.folder_id();
                            if fid.is_empty() { None } else { Some(fid) }
                        },
                        updated_at: n.updated_at(),
                        created_at: n.created_at(),
                    })
            })
            .collect();

        local_store::save(&local_store::LocalData { folders, notes }).ok();
    }
}

async fn do_full_sync(
    api: ApiClient,
) -> Result<(Vec<FolderRecord>, Vec<NoteRecord>), ApiError> {
    let (folders, notes) = tokio::try_join!(api.list_folders(), api.list_notes())?;
    Ok((folders, notes))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
