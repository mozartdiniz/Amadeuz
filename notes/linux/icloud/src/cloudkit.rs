//! Reading Notes records from CloudKit Web Services.
//!
//! Container `com.apple.notes`, private database, zone `Notes`. Record and field
//! names were confirmed in NotesShared.framework; see `ICLOUD_NOTES_PROTOCOL.md`.

use std::collections::HashMap;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};

use crate::auth::{expect_json, ICloudClient};
use crate::{crdt, newnote, notebody, table, Error, Result};

const CONTAINER: &str = "com.apple.notes";
const ZONE_NAME: &str = "Notes";
const PAGE_SIZE: u32 = 200;

#[derive(Debug, Clone)]
pub struct ICloudFolder {
    /// CloudKit record name.
    pub id: String,
    pub title: String,
    pub parent_id: Option<String>,
    /// Apple's "Recently Deleted" folder.
    pub is_trash: bool,
}

#[derive(Debug, Clone)]
pub struct ICloudNote {
    /// CloudKit record name.
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub folder_id: Option<String>,
    pub created_at: i64,
    pub modified_at: i64,
    /// Password-protected note; the body cannot be read.
    pub is_locked: bool,
    /// Markdown rendering of the body, if it could be decoded.
    pub markdown: Option<String>,
    /// The note text as Apple stores it (attachments as U+FFFC) and the
    /// Markdown→text map, for editing ([`notebody::apply_markdown_edit`]).
    pub raw_text: Option<String>,
    pub rendered: Option<notebody::Rendered>,
    /// Why this note can't be edited, if it can't (locked, too large, a body
    /// shape the editor won't touch, …). `None` = editable.
    pub read_only_reason: Option<String>,
    /// (record type, record name) of the note's attachments and inline items,
    /// marked deleted along with the note.
    pub attachments: Vec<(String, String)>,
    /// Why the body couldn't be decoded, if it couldn't.
    pub body_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub folders: Vec<ICloudFolder>,
    pub notes: Vec<ICloudNote>,
    /// Record types seen with their field names and CloudKit types — used by
    /// the probe to check the schema against the protocol notes. No values.
    pub schema: HashMap<String, HashMap<String, String>>,
    /// Attachment id → image download URL, for `icloud://<id>` images in note Markdown.
    /// URLs are signed and expire; re-fetch the snapshot if a download fails.
    pub images: HashMap<String, String>,
    /// Tables that could not be decoded (attachment id → reason), for diagnostics.
    pub table_errors: HashMap<String, String>,
}

impl ICloudClient {
    fn ck_url(&self, path: &str) -> Result<String> {
        Ok(format!(
            "{}/database/1/{CONTAINER}/production/private/{path}",
            self.ckdatabasews_url()?
        ))
    }

    async fn notes_zone_id(&self) -> Result<Value> {
        let resp = self
            .http
            .get(self.ck_url("zones/list")?)
            .query(&self.client_params())
            .send()
            .await?;
        if resp.status().as_u16() == 401 || resp.status().as_u16() == 421 {
            return Err(Error::SessionExpired);
        }
        let data = expect_json(resp).await?;
        data["zones"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|z| z["zoneID"]["zoneName"] == ZONE_NAME)
            .map(|z| z["zoneID"].clone())
            .ok_or_else(|| Error::Protocol("no Notes zone in com.apple.notes".into()))
    }

    /// Fetch every folder and note in the Notes zone.
    pub async fn fetch_all(&self) -> Result<Snapshot> {
        let records = self.fetch_raw_records().await?;
        self.build_snapshot(records).await
    }

    /// Every record in the Notes zone as CloudKit JSON (diagnostics, probe `--dump`).
    pub async fn fetch_raw_records(&self) -> Result<Vec<Value>> {
        let zone_id = self.notes_zone_id().await?;
        let mut records: Vec<Value> = Vec::new();
        let mut sync_token: Option<String> = None;

        loop {
            let mut zone = json!({ "zoneID": zone_id, "resultsLimit": PAGE_SIZE });
            if let Some(t) = &sync_token {
                zone["syncToken"] = json!(t);
            }
            let resp = self
                .http
                .post(self.ck_url("changes/zone")?)
                .query(&self.client_params())
                .json(&json!({ "zones": [zone] }))
                .send()
                .await?;
            if resp.status().as_u16() == 401 || resp.status().as_u16() == 421 {
                return Err(Error::SessionExpired);
            }
            let data = expect_json(resp).await?;
            let zone = &data["zones"][0];
            if let Some(code) = zone["serverErrorCode"].as_str() {
                return Err(Error::Protocol(format!(
                    "changes/zone: {code}: {}",
                    zone["reason"].as_str().unwrap_or("")
                )));
            }
            records.extend(zone["records"].as_array().into_iter().flatten().cloned());
            sync_token = zone["syncToken"].as_str().map(str::to_string);
            if !zone["moreComing"].as_bool().unwrap_or(false) || sync_token.is_none() {
                break;
            }
        }
        Ok(records)
    }

    async fn build_snapshot(&self, records: Vec<Value>) -> Result<Snapshot> {

        // Per-user placement (shared notes live in the sharer's zone but in the
        // participant's folder): Note_UserSpecific { Note → Folder }.
        let user_folder: HashMap<String, String> = records
            .iter()
            .filter(|r| r["recordType"] == "Note_UserSpecific")
            .filter_map(|r| Some((reference_field(r, "Note")?, reference_field(r, "Folder")?)))
            .collect();
        let attachments = self.attachment_context(&records).await;

        let mut snap = Snapshot::default();
        for rec in &records {
            if rec["deleted"].as_bool().unwrap_or(false) || rec["serverErrorCode"].is_string() {
                continue;
            }
            let rtype = rec["recordType"].as_str().unwrap_or("").to_string();
            let entry = snap.schema.entry(rtype.clone()).or_default();
            if let Some(fields) = rec["fields"].as_object() {
                for (k, v) in fields {
                    entry.insert(k.clone(), v["type"].as_str().unwrap_or("?").to_string());
                }
            }
            match rtype.as_str() {
                "Folder" => {
                    if let Some(f) = parse_folder(rec) {
                        snap.folders.push(f);
                    }
                }
                "Note" => {
                    if let Some(n) = self.parse_note(rec, &user_folder, &attachments).await {
                        snap.notes.push(n);
                    }
                }
                _ => {}
            }
        }
        snap.images = attachments.images;
        snap.table_errors = attachments.table_errors;
        Ok(snap)
    }

    async fn parse_note(
        &self,
        rec: &Value,
        user_folder: &HashMap<String, String>,
        attachments: &Attachments,
    ) -> Option<ICloudNote> {
        if int_field(rec, "Deleted").unwrap_or(0) != 0 {
            return None;
        }
        let id = rec["recordName"].as_str()?.to_string();
        let id_for_attachments = id.clone();
        let is_locked = rec["fields"].get("CryptoWrappedKey").is_some();

        let (body, body_error) = if is_locked {
            (None, None)
        } else {
            match self.note_body(rec, attachments).await {
                Ok(b) => (b, None),
                Err(e) => (None, Some(e.to_string())),
            }
        };
        // Decide up front whether the editor may touch this note, so it opens
        // read-only with a reason instead of failing on save.
        let read_only_reason = if is_locked {
            Some("locked note".to_string())
        } else if let Some(e) = &body_error {
            Some(format!("couldn't be read: {e}"))
        } else if body.is_none() {
            Some("no text".to_string())
        } else {
            match bytes_field(rec, "TextDataEncrypted") {
                None => Some("too large to edit yet".to_string()),
                Some(blob) => crdt::Doc::parse(&blob).err().map(|e| e.to_string()),
            }
        };
        let (raw_text, rendered) = body.unzip();
        let markdown = rendered.as_ref().map(|r: &notebody::Rendered| r.text.clone());

        let created_at = int_field(rec, "CreationDate")
            .or_else(|| rec["created"]["timestamp"].as_i64())
            .unwrap_or(0);
        let folder_id = reference_field(rec, "Folder")
            .or_else(|| user_folder.get(&id).cloned())
            .or_else(|| {
                value(rec, "Folders")?.as_array()?.first()?["recordName"]
                    .as_str()
                    .map(str::to_string)
            });
        Some(ICloudNote {
            title: first_line(&string_field(rec, "TitleEncrypted").unwrap_or_default()),
            snippet: string_field(rec, "SnippetEncrypted").unwrap_or_default(),
            folder_id,
            id,
            created_at,
            modified_at: int_field(rec, "ModificationDate")
                .or_else(|| rec["modified"]["timestamp"].as_i64())
                .unwrap_or(created_at),
            is_locked,
            markdown,
            raw_text,
            rendered,
            read_only_reason,
            attachments: attachments.of_note(&id_for_attachments),
            body_error,
        })
    }

    /// Body is inline in `TextDataEncrypted`, or an asset in `TextDataAsset`
    /// when larger than 100 KB (`ic_encryptedInlineableDataAssetForKeyPrefix:`).
    async fn note_body(&self, rec: &Value, attachments: &Attachments) -> Result<Option<(String, notebody::Rendered)>> {
        let Some(blob) = self.inlineable(rec, "TextData").await? else {
            return Ok(None);
        };
        let note = notebody::decode(&blob)?;
        let rendered = notebody::render(&note, &|info| attachments.render(info));
        Ok(Some((note.note_text.unwrap_or_default(), rendered)))
    }

    /// `<prefix>Encrypted` inline bytes, or the `<prefix>Asset` download.
    async fn inlineable(&self, rec: &Value, prefix: &str) -> Result<Option<Vec<u8>>> {
        if let Some(b) = bytes_field(rec, &format!("{prefix}Encrypted")) {
            return Ok(Some(b));
        }
        match asset_url(value(rec, &format!("{prefix}Asset"))) {
            Some(url) => Ok(Some(self.download_asset(&url).await?)),
            None => Ok(None),
        }
    }

    /// Create a note in `folder_id` with plain `text` (first line = title).
    /// Mirrors the fields Apple Notes writes for a new note. Returns the new
    /// record name.
    pub async fn create_note(&self, folder_id: &str, text: &str) -> Result<String> {
        let zone_id = self.notes_zone_id().await?;
        let record_name = uuid::Uuid::new_v4().to_string().to_uppercase();
        let now_ms = now_ms();
        let (title, snippet) = newnote::title_and_snippet(text);
        let body = newnote::body(text, self.replica_uuid(), (now_ms / 1000) as u64);
        let folder_ref = json!({ "recordName": folder_id, "action": "VALIDATE", "zoneID": zone_id });

        let record = json!({
            "recordType": "Note",
            "recordName": record_name,
            "zoneID": zone_id,
            "parent": { "recordName": folder_id },
            "fields": {
                "TitleEncrypted": { "type": "ENCRYPTED_BYTES", "value": B64.encode(title.as_bytes()) },
                "SnippetEncrypted": { "type": "ENCRYPTED_BYTES", "value": B64.encode(snippet.as_bytes()) },
                "TextDataEncrypted": { "type": "ENCRYPTED_BYTES", "value": B64.encode(&body) },
                "Folder": { "type": "REFERENCE", "value": folder_ref },
                "Folders": { "type": "REFERENCE_LIST", "value": [folder_ref] },
                "CreationDate": { "type": "TIMESTAMP", "value": now_ms },
                "ModificationDate": { "type": "TIMESTAMP", "value": now_ms },
                "FoldersModificationDate": { "type": "TIMESTAMP", "value": now_ms },
                "Deleted": { "type": "INT64", "value": 0 },
                "MinimumSupportedNotesVersion": { "type": "INT64", "value": 0 },
                "AttachmentViewType": { "type": "INT64", "value": 0 },
                "PaperStyleType": { "type": "INT64", "value": 0 },
            },
        });
        let saved = self.modify(&zone_id, json!([{ "operationType": "create", "record": record }])).await?;
        Ok(saved[0]["recordName"].as_str().unwrap_or(&record_name).to_string())
    }

    /// Create a top-level folder. Mirrors the fields of a folder Apple Notes
    /// created ("Folder Sync Test" dump): title, title date, minimum version.
    /// Returns the new record name.
    pub async fn create_folder(&self, title: &str) -> Result<String> {
        let zone_id = self.notes_zone_id().await?;
        let record_name = uuid::Uuid::new_v4().to_string().to_uppercase();
        let record = json!({
            "recordType": "Folder",
            "recordName": record_name,
            "zoneID": zone_id,
            "fields": {
                "TitleEncrypted": { "type": "ENCRYPTED_BYTES", "value": B64.encode(title.as_bytes()) },
                "TitleModificationDate": { "type": "TIMESTAMP", "value": now_ms() },
                "MinimumSupportedNotesVersion": { "type": "INT64", "value": 0 },
            },
        });
        let saved = self.modify(&zone_id, json!([{ "operationType": "create", "record": record }])).await?;
        Ok(saved[0]["recordName"].as_str().unwrap_or(&record_name).to_string())
    }

    /// Move a note to another folder (Apple's "delete" moves it to
    /// `TrashFolder-CloudKit`). Changes only the folder fields and the
    /// CloudKit parent; the body is untouched.
    pub async fn move_note(&self, record_name: &str, folder_id: &str) -> Result<()> {
        let zone_id = self.notes_zone_id().await?;
        let rec = self.lookup(&zone_id, record_name).await?;
        let folder_ref = json!({ "recordName": folder_id, "action": "VALIDATE", "zoneID": zone_id });
        let record = json!({
            "recordType": "Note",
            "recordName": record_name,
            "recordChangeTag": rec["recordChangeTag"],
            "zoneID": zone_id,
            "parent": { "recordName": folder_id },
            "fields": {
                "Folder": { "type": "REFERENCE", "value": folder_ref },
                "Folders": { "type": "REFERENCE_LIST", "value": [folder_ref] },
                "FoldersModificationDate": { "type": "TIMESTAMP", "value": now_ms() },
            },
        });
        self.modify(&zone_id, json!([{ "operationType": "update", "record": record }])).await?;
        Ok(())
    }

    /// Delete a note for good, as `+[ICNote deleteNote:]` does: mark the note
    /// and its attachments `Deleted` (Note, Attachment and InlineAttachment
    /// all use deletion by TTL; CloudKit removes marked records later).
    /// `attachments` are (record type, record name) pairs from the snapshot.
    pub async fn delete_note(&self, record_name: &str, attachments: &[(String, String)]) -> Result<()> {
        let zone_id = self.notes_zone_id().await?;
        let rec = self.lookup(&zone_id, record_name).await?;
        let mark = |rtype: &str, name: &str, tag: Option<&Value>| {
            let mut r = json!({
                "recordType": rtype,
                "recordName": name,
                "zoneID": zone_id,
                "fields": { "Deleted": { "type": "INT64", "value": 1 } },
            });
            if let Some(tag) = tag {
                r["recordChangeTag"] = tag.clone();
            }
            r
        };
        let mut ops = vec![json!({ "operationType": "update", "record": mark("Note", record_name, Some(&rec["recordChangeTag"])) })];
        // Attachments: only the Deleted flag changes, so no change tag needed.
        for (rtype, name) in attachments {
            ops.push(json!({ "operationType": "forceUpdate", "record": mark(rtype, name, None) }));
        }
        self.modify(&zone_id, Value::Array(ops)).await?;
        Ok(())
    }

    /// Delete a folder as Apple Notes does from the user's point of view: its
    /// notes go to Recently Deleted, then the folder record is removed
    /// (folders don't use deletion by TTL). `notes` are the folder's note
    /// record names; `trash` is the Recently Deleted folder.
    pub async fn delete_folder(&self, folder_id: &str, notes: &[String], trash: &str) -> Result<()> {
        for note in notes {
            self.move_note(note, trash).await?;
        }
        let zone_id = self.notes_zone_id().await?;
        let rec = self.lookup(&zone_id, folder_id).await?;
        let record = json!({
            "recordType": "Folder",
            "recordName": folder_id,
            "recordChangeTag": rec["recordChangeTag"],
            "zoneID": zone_id,
        });
        self.modify(&zone_id, json!([{ "operationType": "delete", "record": record }])).await?;
        Ok(())
    }

    /// Change a note's text from `base_text` (what the caller last saw) to
    /// `new_text`, as CRDT edits from this installation's replica. Fails with
    /// [`Error::Conflict`] if the note changed on the server since, and refuses
    /// locked notes and bodies stored as assets.
    pub async fn edit_note(&self, record_name: &str, base_text: &str, edit: &notebody::NoteEdit) -> Result<()> {
        let new_text = edit.text.as_str();
        if base_text == new_text && edit.paragraphs.is_empty() && edit.inline.is_empty() {
            return Ok(());
        }
        let zone_id = self.notes_zone_id().await?;
        let rec = self.lookup(&zone_id, record_name).await?;
        if rec["fields"].get("CryptoWrappedKey").is_some() {
            return Err(Error::Protocol("locked notes can't be edited".into()));
        }
        let blob = bytes_field(&rec, "TextDataEncrypted")
            .ok_or_else(|| Error::Protocol("large notes (stored as a file) can't be edited yet".into()))?;
        let edited = crdt::edit_with_formats(&blob, base_text, new_text, &edit.paragraphs, &edit.inline, self.replica_uuid())?;
        if edited.len() > 100 * 1024 {
            return Err(Error::Protocol("note too large to save inline yet".into()));
        }

        let visible: String = new_text.chars().filter(|&c| c != '\u{FFFC}').collect();
        let (title, snippet) = newnote::title_and_snippet(&visible);
        let record = json!({
            "recordType": "Note",
            "recordName": record_name,
            "recordChangeTag": rec["recordChangeTag"],
            "zoneID": zone_id,
            "fields": {
                "TextDataEncrypted": { "type": "ENCRYPTED_BYTES", "value": B64.encode(&edited) },
                "TitleEncrypted": { "type": "ENCRYPTED_BYTES", "value": B64.encode(title.as_bytes()) },
                "SnippetEncrypted": { "type": "ENCRYPTED_BYTES", "value": B64.encode(snippet.as_bytes()) },
                "ModificationDate": { "type": "TIMESTAMP", "value": now_ms() },
            },
        });
        self.modify(&zone_id, json!([{ "operationType": "update", "record": record }])).await?;
        Ok(())
    }

    async fn lookup(&self, zone_id: &Value, record_name: &str) -> Result<Value> {
        let body = json!({ "records": [{ "recordName": record_name }], "zoneID": zone_id });
        retrying(|| async {
            let data = self.ck_post("records/lookup", &body).await?;
            let rec = data["records"][0].clone();
            if rec["serverErrorCode"].is_string() {
                return Err(server_error("records/lookup", &rec));
            }
            Ok(rec)
        })
        .await
    }

    /// `records/modify`, returning the saved records; any per-record error fails the call.
    /// `records/modify`, retrying transient server errors (busy zone,
    /// throttling, `TRY_AGAIN_LATER`), honouring the server's `retryAfter`.
    async fn modify(&self, zone_id: &Value, operations: Value) -> Result<Vec<Value>> {
        retrying(|| self.modify_once(zone_id, &operations)).await
    }

    async fn modify_once(&self, zone_id: &Value, operations: &Value) -> Result<Vec<Value>> {
        let data = self
            .ck_post("records/modify", &json!({ "operations": operations, "zoneID": zone_id, "atomic": true }))
            .await?;
        let records = data["records"].as_array().cloned().unwrap_or_default();
        if let Some(bad) = records.iter().find(|r| r["serverErrorCode"].is_string()) {
            return Err(server_error("records/modify", bad));
        }
        Ok(records)
    }

    /// POST to a CloudKit endpoint. Errors iCloud reports in the body (with
    /// HTTP 4xx/5xx or 200) become typed errors; transient ones become
    /// [`Error::Retry`].
    async fn ck_post(&self, path: &str, body: &Value) -> Result<Value> {
        let resp = self
            .http
            .post(self.ck_url(path)?)
            .query(&self.client_params())
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status == 401 || status == 421 {
            return Err(Error::SessionExpired);
        }
        let text = resp.text().await?;
        let data: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if data["serverErrorCode"].is_string() {
            return Err(server_error(path, &data));
        }
        match status {
            200..=299 if !data.is_null() => Ok(data),
            429 | 503 => Err(Error::Retry(None)),
            _ => {
                let snippet: String = text.chars().take(300).collect();
                Err(Error::Protocol(format!("{path} returned {status}: {snippet}")))
            }
        }
    }

    /// Download a CloudKit asset (images, large bodies, table data).
    pub async fn download_asset(&self, url: &str) -> Result<Vec<u8>> {
        Ok(self.http.get(url).send().await?.error_for_status()?.bytes().await?.to_vec())
    }

    /// Index Attachment/Media/InlineAttachment records and decode tables up front,
    /// so note bodies can render attachments synchronously.
    async fn attachment_context(&self, records: &[Value]) -> Attachments {
        let by_type = |t: &str| -> HashMap<String, Value> {
            records
                .iter()
                .filter(|r| r["recordType"] == t && !r["deleted"].as_bool().unwrap_or(false))
                .filter_map(|r| Some((r["recordName"].as_str()?.to_string(), r.clone())))
                .collect()
        };
        let mut ctx = Attachments {
            records: by_type("Attachment"),
            inline: by_type("InlineAttachment"),
            ..Default::default()
        };
        let media = by_type("Media");

        for (id, rec) in &ctx.records {
            let uti = plain_string_field(rec, "UTI").unwrap_or_default();
            if uti == "com.apple.notes.table" {
                let decoded = match self.inlineable(rec, "MergeableData").await {
                    Ok(Some(blob)) => table::decode(&blob).map(|t| table::to_markdown(&t)),
                    Ok(None) => Err(Error::Protocol("table has no data".into())),
                    Err(e) => Err(e),
                };
                match decoded {
                    Ok(md) => {
                        ctx.tables.insert(id.clone(), md);
                    }
                    Err(e) => {
                        ctx.table_errors.insert(id.clone(), e.to_string());
                    }
                }
            } else if let Some(url) = image_url(rec, &uti, &media) {
                ctx.images.insert(id.clone(), url);
            }
        }
        ctx
    }
}

/// Attachment data needed to render note bodies.
#[derive(Default)]
struct Attachments {
    records: HashMap<String, Value>,
    inline: HashMap<String, Value>,
    /// Attachment record name → Markdown table.
    tables: HashMap<String, String>,
    table_errors: HashMap<String, String>,
    /// Attachment record name → image download URL.
    images: HashMap<String, String>,
}

impl Attachments {
    fn of_note(&self, note_id: &str) -> Vec<(String, String)> {
        let points_here = |r: &Value| r["fields"]["Note"]["value"]["recordName"] == note_id;
        let mut out: Vec<(String, String)> = Vec::new();
        for (rtype, map) in [("Attachment", &self.records), ("InlineAttachment", &self.inline)] {
            out.extend(map.iter().filter(|(_, r)| points_here(r)).map(|(id, _)| (rtype.to_string(), id.clone())));
        }
        out
    }

    fn render(&self, info: &notebody::AttachmentInfo) -> Option<String> {
        let id = info.attachment_identifier.as_deref()?;
        if let Some(md) = self.tables.get(id) {
            return Some(format!("\n{md}\n"));
        }
        if let Some(rec) = self.inline.get(id) {
            return string_field(rec, "AltTextEncrypted");
        }
        let rec = self.records.get(id)?;
        let title = string_field(rec, "TitleEncrypted").filter(|t| !t.is_empty());
        if let Some(url) = string_field(rec, "URLStringEncrypted") {
            return Some(format!("[{}]({url})", title.unwrap_or_else(|| url.clone())));
        }
        if self.images.contains_key(id) {
            return Some(format!("![{}]({IMAGE_SCHEME}{id})", title.unwrap_or_default()));
        }
        title.map(|t| format!("[📎 {t}]"))
    }
}

/// Markdown image URL prefix for iCloud attachments: `icloud://<attachment id>`.
/// The app downloads the image with [`ICloudClient::download_asset`] using
/// [`Snapshot::images`].
pub const IMAGE_SCHEME: &str = "icloud://";

/// Pick something GTK can display: the original file for PNG/JPEG/GIF, otherwise
/// Apple's largest preview image (HEIC photos, scans, PDFs, drawings), otherwise
/// the fallback image.
fn image_url(rec: &Value, uti: &str, media: &HashMap<String, Value>) -> Option<String> {
    if matches!(uti, "public.png" | "public.jpeg" | "com.compuserve.gif") {
        let original = reference_field(rec, "Media")
            .and_then(|m| media.get(&m))
            .and_then(|m| asset_url(value(m, "Asset")));
        if original.is_some() {
            return original;
        }
    }
    let previews = value(rec, "PreviewImages").and_then(Value::as_array);
    let widths = value(rec, "PreviewWidths").and_then(Value::as_array);
    if let Some(previews) = previews {
        let best = (0..previews.len()).max_by(|&a, &b| {
            let w = |i: usize| widths.and_then(|w| w.get(i)).and_then(Value::as_f64).unwrap_or(0.0);
            w(a).total_cmp(&w(b))
        });
        if let Some(url) = best.and_then(|i| asset_url(previews.get(i))) {
            return Some(url);
        }
    }
    asset_url(value(rec, "FallbackImage"))
}

fn asset_url(v: Option<&Value>) -> Option<String> {
    v?["downloadURL"].as_str().map(|u| u.replace("${f}", "file"))
}

fn plain_string_field(rec: &Value, key: &str) -> Option<String> {
    value(rec, key)?.as_str().map(str::to_string)
}

fn parse_folder(rec: &Value) -> Option<ICloudFolder> {
    if int_field(rec, "Deleted").unwrap_or(0) != 0 {
        return None;
    }
    let id = rec["recordName"].as_str()?.to_string();
    Some(ICloudFolder {
        is_trash: id.starts_with("TrashFolder"),
        title: string_field(rec, "TitleEncrypted").unwrap_or_default(),
        parent_id: reference_field(rec, "ParentFolder"),
        id,
    })
}

fn first_line(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string()
}

fn value<'a>(rec: &'a Value, key: &str) -> Option<&'a Value> {
    rec["fields"].get(key).map(|f| &f["value"])
}

fn bytes_field(rec: &Value, key: &str) -> Option<Vec<u8>> {
    B64.decode(value(rec, key)?.as_str()?).ok()
}

fn string_field(rec: &Value, key: &str) -> Option<String> {
    bytes_field(rec, key).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn int_field(rec: &Value, key: &str) -> Option<i64> {
    value(rec, key)?.as_i64()
}

fn reference_field(rec: &Value, key: &str) -> Option<String> {
    value(rec, key)?["recordName"].as_str().map(str::to_string)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Map an iCloud error object (`serverErrorCode`, `reason`, `retryAfter`).
fn server_error(what: &str, e: &Value) -> Error {
    let code = e["serverErrorCode"].as_str().unwrap_or("");
    match code {
        // Stale recordChangeTag: someone else saved first.
        "CONFLICT" | "OPLOCK" => Error::Conflict,
        // e.g. "Sync zone CAS Op-Lock failed. There was a concurrent write …"
        "TRY_AGAIN_LATER" | "ZONE_BUSY" | "THROTTLED" | "SERVICE_UNAVAILABLE" | "RETRY_LATER" => {
            Error::Retry(e["retryAfter"].as_f64())
        }
        "AUTHENTICATION_FAILED" | "AUTHENTICATION_REQUIRED" => Error::SessionExpired,
        _ => Error::Protocol(format!("{what}: {code}: {}", e["reason"].as_str().unwrap_or(""))),
    }
}

/// Run `op`, retrying [`Error::Retry`] with backoff (0.5 s, 1 s, 2 s, 4 s, 8 s)
/// or the server's `retryAfter`.
async fn retrying<T, F, Fut>(mut op: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    const ATTEMPTS: u32 = 6;
    for attempt in 1.. {
        match op().await {
            Err(Error::Retry(after)) if attempt < ATTEMPTS => {
                let backoff = 0.5 * f64::from(1u32 << (attempt - 1));
                let wait = after.unwrap_or(backoff).clamp(0.2, 30.0);
                log::info!("iCloud busy, retrying in {wait:.1}s (attempt {attempt})");
                tokio::time::sleep(std::time::Duration::from_secs_f64(wait)).await;
            }
            Err(Error::Retry(_)) => return Err(Error::Protocol("iCloud is busy; try again in a moment".into())),
            other => return other,
        }
    }
    unreachable!("loop returns")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_zone_is_retryable() {
        // Body of the HTTP 409 seen live when two writes hit the zone at once.
        let body = json!({
            "uuid": "c439d45e-63c7-44d1-9b4d-1c9975a684bd",
            "serverErrorCode": "TRY_AGAIN_LATER",
            "reason": "Sync zone CAS Op-Lock failed. There was a concurrent write and this operation was rejected. Retry request..."
        });
        assert!(matches!(server_error("records/modify", &body), Error::Retry(None)));
        let later = json!({ "serverErrorCode": "THROTTLED", "retryAfter": 3.0 });
        assert!(matches!(server_error("x", &later), Error::Retry(Some(t)) if t == 3.0));
        assert!(matches!(server_error("x", &json!({ "serverErrorCode": "CONFLICT" })), Error::Conflict));
    }

    #[tokio::test]
    async fn retrying_gives_up_after_bounded_attempts() {
        let mut calls = 0;
        let r: Result<()> = retrying(|| {
            calls += 1;
            async { Err(Error::Retry(Some(0.0))) }
        })
        .await;
        assert!(matches!(r, Err(Error::Protocol(_))));
        assert_eq!(calls, 6);
    }
}
