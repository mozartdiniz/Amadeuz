use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use amadeuz_icloud::notebody::{self, Rendered};
use amadeuz_icloud::{Error as ICloudError, ICloudClient, SignInOutcome, Snapshot, TrustedPhone};
use gtk::prelude::*;
use gtk::{gio, glib};
use tokio::sync::Mutex;

use crate::backend::icloud_cache::{self, Cache, CachedFolder, CachedNote};
use crate::model::{AmzFolder, AmzNote};

/// Apple's built-in "Notes" folder; notes without a folder land here.
pub const DEFAULT_FOLDER: &str = "DefaultFolder-CloudKit";

/// What the editor needs to turn Markdown edits into note-text edits.
#[derive(Clone)]
struct EditState {
    raw: String,
    rendered: Rendered,
    /// Why the note can't be edited, if it can't.
    read_only_reason: Option<String>,
}

/// Save progress for the open note, shown in the status bar.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveStatus {
    Idle,
    Saving,
    Saved,
    Failed(String),
}

// ── Events: tokio tasks → GTK main thread ─────────────────────────────────────

#[derive(Debug)]
pub enum ICloudEvent {
    /// A saved session was found and is being validated.
    Restoring,
    SignIn(Result<SignInOutcome, String>),
    Code(Result<(), CodeError>),
    CodeSent(String),
    Synced(Snapshot),
    /// Images for the open note finished downloading into the cache.
    ImagesReady,
    Saved { id: String, raw: String, rendered: Rendered },
    SaveFailed { id: String, message: String, conflict: bool },
    Created { id: String, folder_id: String },
    CreateFailed(String),
    SyncFailed { message: String, expired: bool },
}

#[derive(Debug)]
pub enum CodeError {
    Wrong,
    Other(String),
}

/// Sign-in progress reported to the window, which drives the dialogs.
pub enum SignInStep {
    NeedsCode(Vec<TrustedPhone>),
    WrongCode,
    CodeSent(String),
    Done,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ICloudStatus {
    /// No iCloud account connected.
    Disconnected,
    Syncing,
    Ready,
    Error(String),
}

// ── ICloudManager ─────────────────────────────────────────────────────────────

pub type ICloudRef = Rc<RefCell<ICloudManager>>;

/// Read-only view of the user's iCloud notes. Kept completely separate from
/// `NotesManager`: these notes are never written to the local store or pushed
/// to the Amadeuz server.
pub struct ICloudManager {
    client: Arc<Mutex<ICloudClient>>,
    pub folder_store: gio::ListStore,
    pub note_store: gio::ListStore,
    pub status: ICloudStatus,
    /// Full Markdown body per note record name (the note store keeps title/content split).
    bodies: HashMap<String, String>,
    /// Attachment id → signed download URL for `icloud://` images.
    images: HashMap<String, String>,
    edits: HashMap<String, EditState>,
    /// A save is in flight; edits made meanwhile wait in `queued`.
    saving: bool,
    queued: Option<(String, String)>,
    pub save_status: SaveStatus,
    event_tx: async_channel::Sender<ICloudEvent>,

    pub on_changed: Option<Rc<dyn Fn()>>,
    pub on_sign_in_step: Option<Rc<dyn Fn(SignInStep)>>,
    pub on_images_ready: Option<Rc<dyn Fn()>>,
    /// A conflict forced a reload of this note id.
    pub on_conflict: Option<Rc<dyn Fn(String)>>,
    /// A note was created; the window opens it.
    pub on_created: Option<Rc<dyn Fn(String)>>,
    /// A fresh snapshot replaced the stores (the open note may have changed).
    pub on_synced: Option<Rc<dyn Fn()>>,
}

impl ICloudManager {
    pub fn new() -> (ICloudRef, async_channel::Receiver<ICloudEvent>) {
        let (tx, rx) = async_channel::bounded(32);
        let mgr = Rc::new(RefCell::new(ICloudManager {
            client: Arc::new(Mutex::new(ICloudClient::new())),
            folder_store: gio::ListStore::new::<AmzFolder>(),
            note_store: gio::ListStore::new::<AmzNote>(),
            status: ICloudStatus::Disconnected,
            bodies: HashMap::new(),
            images: HashMap::new(),
            edits: HashMap::new(),
            saving: false,
            queued: None,
            save_status: SaveStatus::Idle,
            event_tx: tx,
            on_changed: None,
            on_sign_in_step: None,
            on_images_ready: None,
            on_conflict: None,
            on_created: None,
            on_synced: None,
        }));
        (mgr, rx)
    }

    pub fn is_connected(&self) -> bool {
        self.status != ICloudStatus::Disconnected
    }

    pub fn body(&self, note_id: &str) -> Option<&str> {
        self.bodies.get(note_id).map(String::as_str)
    }

    /// On launch: show the cached notes at once, then reuse the session saved
    /// in the keyring (if any) to sync in the background.
    pub fn restore(mgr: &ICloudRef) {
        if let Some(cache) = icloud_cache::load() {
            {
                let mut m = mgr.borrow_mut();
                m.load_cache(cache);
                m.status = ICloudStatus::Syncing;
            }
            notify(mgr);
        }
        let client = mgr.borrow().client.clone();
        let tx = mgr.borrow().event_tx.clone();
        crate::spawn(async move {
            let saved = crate::backend::keyring::load_icloud_session().await.ok().flatten();
            let Some(restored) = saved.as_deref().and_then(|s| ICloudClient::restore(s).ok()) else {
                tx.send(ICloudEvent::SyncFailed { message: String::new(), expired: false }).await.ok();
                return;
            };
            *client.lock().await = restored;
            tx.send(ICloudEvent::Restoring).await.ok();
            let event = match validate_and_fetch(&client, saved.as_deref()).await {
                Ok(snap) => ICloudEvent::Synced(snap),
                Err(e) => sync_failed(e),
            };
            tx.send(event).await.ok();
        });
    }

    pub fn sign_in(&self, apple_id: String, password: String) {
        let client = self.client.clone();
        let tx = self.event_tx.clone();
        crate::spawn(async move {
            let res = client.lock().await.sign_in(&apple_id, &password).await;
            tx.send(ICloudEvent::SignIn(res.map_err(|e| e.to_string()))).await.ok();
        });
    }

    pub fn submit_code(&self, code: String) {
        let client = self.client.clone();
        let tx = self.event_tx.clone();
        crate::spawn(async move {
            let res = client.lock().await.submit_code(&code).await.map_err(|e| match e {
                ICloudError::BadCode => CodeError::Wrong,
                e => CodeError::Other(e.to_string()),
            });
            tx.send(ICloudEvent::Code(res)).await.ok();
        });
    }

    pub fn resend_code(&self) {
        let client = self.client.clone();
        let tx = self.event_tx.clone();
        crate::spawn(async move {
            let event = match client.lock().await.resend_code().await {
                Ok(()) => ICloudEvent::CodeSent("A new code was sent to your Apple devices.".into()),
                Err(e) => ICloudEvent::Code(Err(CodeError::Other(e.to_string()))),
            };
            tx.send(event).await.ok();
        });
    }

    pub fn request_sms(&self, phone: TrustedPhone) {
        let client = self.client.clone();
        let tx = self.event_tx.clone();
        crate::spawn(async move {
            let event = match client.lock().await.request_sms(phone.id).await {
                Ok(()) => ICloudEvent::CodeSent(format!("A code was sent by SMS to {}.", phone.number)),
                Err(e) => ICloudEvent::Code(Err(CodeError::Other(e.to_string()))),
            };
            tx.send(event).await.ok();
        });
    }

    pub fn refresh(mgr: &ICloudRef) {
        let (client, tx) = {
            let mut m = mgr.borrow_mut();
            if m.status == ICloudStatus::Disconnected || m.status == ICloudStatus::Syncing {
                return;
            }
            m.status = ICloudStatus::Syncing;
            (m.client.clone(), m.event_tx.clone())
        };
        notify(mgr);
        crate::spawn(async move {
            let event = match client.lock().await.fetch_all().await {
                Ok(snap) => ICloudEvent::Synced(snap),
                Err(e) => sync_failed(e),
            };
            tx.send(event).await.ok();
        });
    }

    pub fn can_edit(&self, note_id: &str) -> bool {
        self.read_only_reason(note_id).is_none()
    }

    /// Why `note_id` can't be edited right now; `None` if it can.
    pub fn read_only_reason(&self, note_id: &str) -> Option<String> {
        if self.note_folder(note_id).is_some_and(|f| !self.is_writable_folder(&f)) {
            return Some("in Recently Deleted".into());
        }
        match self.edits.get(note_id) {
            // Cached notes carry no editing data; the background sync brings it.
            None => Some("waiting for sync".into()),
            Some(e) => e.read_only_reason.clone(),
        }
    }

    /// Any folder except Apple's "Recently Deleted" (as in Apple Notes).
    pub fn is_writable_folder(&self, folder_id: &str) -> bool {
        (0..self.folder_store.n_items()).any(|i| {
            self.folder_store
                .item(i)
                .and_downcast::<AmzFolder>()
                .is_some_and(|f| f.id() == folder_id && !f.id().starts_with("TrashFolder"))
        })
    }

    fn note_folder(&self, note_id: &str) -> Option<String> {
        self.find_note(note_id).map(|n| n.folder_id())
    }

    fn find_note(&self, note_id: &str) -> Option<AmzNote> {
        (0..self.note_store.n_items())
            .find_map(|i| self.note_store.item(i).and_downcast::<AmzNote>().filter(|n| n.id() == note_id))
    }

    /// Save the editor's Markdown for `note_id`. Only one save runs at a time;
    /// newer text waits and is saved against the result of the running one.
    pub fn save_note(mgr: &ICloudRef, note_id: String, markdown: String) {
        let (client, tx, base_raw, edit, rendered) = {
            let mut m = mgr.borrow_mut();
            if !m.can_edit(&note_id) {
                return;
            }
            if m.saving {
                m.queued = Some((note_id, markdown));
                return;
            }
            let Some(state) = m.edits.get(&note_id).cloned() else { return };
            let mut edit = notebody::apply_markdown_edit(&state.raw, &state.rendered, &markdown);
            let rendered = Rendered { text: markdown, map: std::mem::take(&mut edit.map), typed: std::mem::take(&mut edit.typed) };
            if edit.text == state.raw && edit.paragraphs.is_empty() && edit.inline.is_empty() {
                // Nothing that changes the note: just track the editor's text.
                m.edits.get_mut(&note_id).unwrap().rendered = rendered;
                return;
            }
            m.saving = true;
            m.save_status = SaveStatus::Saving;
            (m.client.clone(), m.event_tx.clone(), state.raw, edit, rendered)
        };
        notify(mgr);
        crate::spawn(async move {
            let client = client.lock().await.clone();
            let event = match client.edit_note(&note_id, &base_raw, &edit).await {
                Ok(()) => ICloudEvent::Saved { id: note_id, raw: edit.text, rendered },
                Err(e) => ICloudEvent::SaveFailed {
                    conflict: matches!(e, ICloudError::Conflict),
                    message: e.to_string(),
                    id: note_id,
                },
            };
            tx.send(event).await.ok();
        });
    }

    /// Create an empty note in `folder_id` (writable folder only).
    pub fn create_note(&self, folder_id: String) {
        if !self.is_writable_folder(&folder_id) {
            return;
        }
        let client = self.client.clone();
        let tx = self.event_tx.clone();
        crate::spawn(async move {
            let client = client.lock().await.clone();
            let event = match client.create_note(&folder_id, "").await {
                Ok(id) => ICloudEvent::Created { id, folder_id },
                Err(e) => ICloudEvent::CreateFailed(e.to_string()),
            };
            tx.send(event).await.ok();
        });
    }

    /// Download any of `ids` not yet in the image cache; fires `on_images_ready`.
    pub fn fetch_images(&self, ids: Vec<String>) {
        let wanted: Vec<(std::path::PathBuf, String)> = ids
            .iter()
            .map(|id| (crate::ui::md_formatter::icloud_image_path(id), id))
            .filter(|(path, _)| !path.exists())
            .filter_map(|(path, id)| Some((path, self.images.get(id)?.clone())))
            .collect();
        if wanted.is_empty() {
            return;
        }
        let client = self.client.clone();
        let tx = self.event_tx.clone();
        crate::spawn(async move {
            let client = client.lock().await.clone();
            for (path, url) in wanted {
                match client.download_asset(&url).await {
                    Ok(bytes) => {
                        if let Some(dir) = path.parent() {
                            std::fs::create_dir_all(dir).ok();
                        }
                        // Write via a temp file so a half-written image is never displayed.
                        let tmp = path.with_extension("part");
                        if std::fs::write(&tmp, &bytes).is_ok() {
                            std::fs::rename(&tmp, &path).ok();
                        }
                    }
                    // Signed URLs expire; the next refresh brings fresh ones.
                    Err(e) => log::warn!("iCloud image download failed: {e}"),
                }
            }
            tx.send(ICloudEvent::ImagesReady).await.ok();
        });
    }

    /// Forget the account: clears the keyring entry and all iCloud data in memory.
    pub fn disconnect(&mut self) {
        self.client = Arc::new(Mutex::new(ICloudClient::new()));
        self.status = ICloudStatus::Disconnected;
        self.folder_store.remove_all();
        self.note_store.remove_all();
        self.bodies.clear();
        self.images.clear();
        self.edits.clear();
        self.queued = None;
        icloud_cache::delete();
        std::fs::remove_dir_all(crate::ui::md_formatter::icloud_image_path("")).ok();
        crate::spawn(async move {
            crate::backend::keyring::delete_icloud_session().await.ok();
        });
    }

    // ── Event handler (GTK main thread) ───────────────────────────────────────

    pub fn handle_event(mgr: ICloudRef, event: ICloudEvent) {
        match event {
            ICloudEvent::Restoring => {
                mgr.borrow_mut().status = ICloudStatus::Syncing;
                notify(&mgr);
            }
            ICloudEvent::SignIn(Ok(SignInOutcome::SignedIn)) | ICloudEvent::Code(Ok(())) => {
                Self::after_sign_in(&mgr);
                step(&mgr, SignInStep::Done);
            }
            ICloudEvent::SignIn(Ok(SignInOutcome::NeedsCode { phones })) => {
                step(&mgr, SignInStep::NeedsCode(phones));
            }
            ICloudEvent::SignIn(Err(msg)) => step(&mgr, SignInStep::Failed(msg)),
            ICloudEvent::Code(Err(CodeError::Wrong)) => step(&mgr, SignInStep::WrongCode),
            ICloudEvent::Code(Err(CodeError::Other(msg))) => step(&mgr, SignInStep::Failed(msg)),
            ICloudEvent::CodeSent(msg) => step(&mgr, SignInStep::CodeSent(msg)),

            ICloudEvent::Synced(snap) => {
                {
                    let mut m = mgr.borrow_mut();
                    m.apply_snapshot(snap);
                    m.status = ICloudStatus::Ready;
                    icloud_cache::save(m.to_cache());
                }
                notify(&mgr);
                let cb = mgr.borrow().on_synced.clone();
                if let Some(cb) = cb {
                    cb();
                }
            }
            ICloudEvent::Saved { id, raw, rendered } => {
                let queued = {
                    let mut m = mgr.borrow_mut();
                    m.saving = false;
                    m.save_status = SaveStatus::Saved;
                    m.apply_saved(&id, raw, rendered);
                    icloud_cache::save(m.to_cache());
                    m.queued.take()
                };
                notify(&mgr);
                if let Some((id, md)) = queued {
                    ICloudManager::save_note(&mgr, id, md);
                }
            }
            ICloudEvent::SaveFailed { id, message, conflict } => {
                {
                    let mut m = mgr.borrow_mut();
                    m.saving = false;
                    m.queued = None;
                    m.save_status = SaveStatus::Failed(message);
                }
                notify(&mgr);
                if conflict {
                    let cb = mgr.borrow().on_conflict.clone();
                    if let Some(cb) = cb {
                        cb(id);
                    }
                    ICloudManager::refresh(&mgr);
                }
            }
            ICloudEvent::Created { id, folder_id } => {
                {
                    let mut m = mgr.borrow_mut();
                    let now = now_ms();
                    m.note_store.insert(0, &AmzNote::new(&id, "", "", &folder_id, now, now));
                    m.bodies.insert(id.clone(), String::new());
                    m.edits.insert(id.clone(), EditState { raw: String::new(), rendered: Rendered::default(), read_only_reason: None });
                    icloud_cache::save(m.to_cache());
                }
                let cb = mgr.borrow().on_created.clone();
                if let Some(cb) = cb {
                    cb(id);
                }
            }
            ICloudEvent::CreateFailed(message) => {
                mgr.borrow_mut().save_status = SaveStatus::Failed(message);
                notify(&mgr);
            }
            ICloudEvent::ImagesReady => {
                let cb = mgr.borrow().on_images_ready.clone();
                if let Some(cb) = cb {
                    cb();
                }
            }
            ICloudEvent::SyncFailed { message, expired } => {
                {
                    let mut m = mgr.borrow_mut();
                    m.status = if message.is_empty() {
                        // No saved session: nothing to report, and any cached
                        // notes belong to an account that's no longer connected.
                        m.folder_store.remove_all();
                        m.note_store.remove_all();
                        m.bodies.clear();
                        m.edits.clear();
                        icloud_cache::delete();
                        ICloudStatus::Disconnected
                    } else if expired {
                        ICloudStatus::Error("iCloud session expired. Reconnect from the main menu.".into())
                    } else {
                        ICloudStatus::Error(message)
                    };
                }
                notify(&mgr);
            }
        }
    }

    /// Persist the new session, then load the notes.
    fn after_sign_in(mgr: &ICloudRef) {
        let (client, tx) = {
            let mut m = mgr.borrow_mut();
            m.status = ICloudStatus::Syncing;
            (m.client.clone(), m.event_tx.clone())
        };
        notify(mgr);
        crate::spawn(async move {
            let saved = client.lock().await.save();
            if let Err(e) = crate::backend::keyring::save_icloud_session(saved, None).await {
                log::warn!("could not save iCloud session to keyring: {e}");
            }
            let event = match client.lock().await.fetch_all().await {
                Ok(snap) => ICloudEvent::Synced(snap),
                Err(e) => sync_failed(e),
            };
            tx.send(event).await.ok();
        });
    }

    /// Snapshot of what's shown, for the on-disk cache.
    fn to_cache(&self) -> Cache {
        let folders = (0..self.folder_store.n_items())
            .filter_map(|i| self.folder_store.item(i).and_downcast::<AmzFolder>())
            .map(|f| CachedFolder { id: f.id(), title: f.name() })
            .collect();
        let notes = (0..self.note_store.n_items())
            .filter_map(|i| self.note_store.item(i).and_downcast::<AmzNote>())
            .map(|n| {
                let id = n.id();
                CachedNote {
                    body: self.bodies.get(&id).cloned().unwrap_or_default(),
                    title: n.title(),
                    content: n.content(),
                    folder_id: n.folder_id(),
                    created_at: n.created_at(),
                    modified_at: n.updated_at(),
                    id,
                }
            })
            .collect();
        Cache { version: 0, folders, notes, images: self.images.clone() }
    }

    fn load_cache(&mut self, cache: Cache) {
        self.folder_store.remove_all();
        for f in &cache.folders {
            self.folder_store.append(&AmzFolder::new(&f.id, &f.title, 0));
        }
        self.note_store.remove_all();
        self.bodies.clear();
        self.edits.clear();
        for n in cache.notes {
            self.note_store.append(&AmzNote::new(&n.id, &n.title, &n.content, &n.folder_id, n.modified_at, n.created_at));
            self.bodies.insert(n.id, n.body);
        }
        self.images = cache.images;
    }

    fn apply_saved(&mut self, id: &str, raw: String, rendered: Rendered) {
        let markdown = rendered.text.clone();
        if let Some(state) = self.edits.get_mut(id) {
            state.raw = raw;
            state.rendered = rendered;
        }
        if let Some(note) = self.find_note(id) {
            let (title, content) = markdown.split_once('\n').unwrap_or((markdown.as_str(), ""));
            note.set_title(title.trim_start_matches('#').trim().to_string());
            note.set_content(content.to_string());
            note.set_updated_at(now_ms());
        }
        self.bodies.insert(id.to_string(), markdown);
    }

    fn apply_snapshot(&mut self, snap: Snapshot) {
        self.images = snap.images;
        let mut folders = snap.folders;
        // Apple's order: "Notes" first, user folders A→Z, "Recently Deleted" last.
        folders.sort_by_key(|f| (f.id != DEFAULT_FOLDER, f.is_trash, f.title.to_lowercase()));
        self.folder_store.remove_all();
        for f in &folders {
            self.folder_store.append(&AmzFolder::new(&f.id, &f.title, 0));
        }

        let mut notes = snap.notes;
        notes.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
        self.bodies.clear();
        self.edits.clear();
        self.note_store.remove_all();
        for n in &notes {
            if let (Some(raw), Some(rendered)) = (&n.raw_text, &n.rendered) {
                self.edits.insert(
                    n.id.clone(),
                    EditState { raw: raw.clone(), rendered: rendered.clone(), read_only_reason: n.read_only_reason.clone() },
                );
            }
            let body = if n.is_locked {
                format!("{}\n🔒 This note is locked. Open it on an Apple device to view it.", n.title)
            } else if let Some(md) = &n.markdown {
                md.clone()
            } else if let Some(err) = &n.body_error {
                format!("{}\n⚠ This note could not be read: {err}", n.title)
            } else {
                n.title.clone()
            };
            let content = body.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
            let folder_id = n.folder_id.as_deref().unwrap_or(DEFAULT_FOLDER);
            self.note_store.append(&AmzNote::new(
                &n.id,
                &n.title,
                content,
                folder_id,
                n.modified_at,
                n.created_at,
            ));
            self.bodies.insert(n.id.clone(), body);
        }
    }
}

async fn validate_and_fetch(
    client: &Arc<Mutex<ICloudClient>>,
    previous: Option<&str>,
) -> amadeuz_icloud::Result<Snapshot> {
    let mut c = client.lock().await;
    c.validate().await?;
    // Apple may refresh cookies on validate; persist only if something changed.
    let saved = c.save();
    crate::backend::keyring::save_icloud_session(saved, previous).await.ok();
    c.fetch_all().await
}

fn sync_failed(e: ICloudError) -> ICloudEvent {
    ICloudEvent::SyncFailed {
        expired: matches!(e, ICloudError::SessionExpired | ICloudError::NotSignedIn),
        message: e.to_string(),
    }
}

fn notify(mgr: &ICloudRef) {
    let cb = mgr.borrow().on_changed.clone();
    if let Some(cb) = cb {
        cb();
    }
}

fn step(mgr: &ICloudRef, s: SignInStep) {
    let cb = mgr.borrow().on_sign_in_step.clone();
    if let Some(cb) = cb {
        cb(s);
    }
}

/// Attach the event receiver to the GLib main loop.
pub fn attach(mgr: &ICloudRef, rx: async_channel::Receiver<ICloudEvent>) {
    glib::MainContext::default().spawn_local(glib::clone!(
        #[weak]
        mgr,
        async move {
            while let Ok(event) = rx.recv().await {
                ICloudManager::handle_event(mgr.clone(), event);
            }
        }
    ));
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
