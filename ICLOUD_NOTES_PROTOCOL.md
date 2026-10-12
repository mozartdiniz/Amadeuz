# iCloud Notes — Protocol Research

Reference for the planned iCloud Notes backend in the Rust GTK client (`notes/linux/`), which
also ships as the Windows client. Goal: full CRUD on the user's own iCloud notes from a native
GNOME UI — no web view, no embedded browser.

Each fact is tagged by where it came from:

- **[RE]** — observed in Apple's shipped binaries with REA (macOS 26.5, October 2026)
- **[public]** — from public reverse-engineering projects; not yet confirmed against a live account
- **[unknown]** — open question, must be verified before relying on it

---

## How Apple Notes syncs

Notes.app does not talk to the network itself. It builds `CKRecord`s and hands them to
CloudKit.framework, which talks to `cloudd`, which talks to Apple's servers. **[RE]**

| Fact | Source |
|---|---|
| CloudKit container `com.apple.notes`, environment `Production` | Notes.app entitlements **[RE]** |
| Private database, zone `Notes` (`-[ICNote(CloudKit) recordZoneName]` returns `@"Notes"`) | NotesShared **[RE]** |
| Record types `Note`, `Folder`, `Attachment`, `Media`, `InlineAttachment`, `Hashtag`, … | NotesShared **[RE]** |
| Sync batch size: `resultsLimitPerSyncOperation = 50` | `NotesShared.framework/Resources/CloudConfigurations/Normal.plist` **[RE]** |
| Inline-vs-asset threshold: `maxInlineAssetSizeBytes = 102400` | same plist **[RE]** |
| Minimum client versions: macOS 10.11, iOS 9 | same plist **[RE]** |

### Why we don't clone the native transport

The native path (`cloudd` → `gateway.icloud.com`, protobuf over HTTPS) authenticates with
device-bound Apple ID tokens (GrandSlam + "anisette" machine provisioning). Reproducing that
on Linux/Windows means emulating an Apple device. It's fragile and ToS-hostile.

### Chosen transport: CloudKit Web Services **[public]**

The same JSON-over-HTTPS API that icloud.com's Notes page uses. Called directly from Rust
(`reqwest`). This is **not** a web wrapper: no HTML or JS is involved, only HTTP requests.

- Auth: `idmsa.apple.com` sign-in (SRP-6a) → 2FA code → session cookies → `setup.icloud.com`
  returns the `ckdatabasews` endpoint for the account. Reference: `pyicloud`.
- Data: `POST …/database/1/com.apple.notes/production/private/changes/zone`,
  `…/records/lookup`, `…/records/modify`.
- **Advanced Data Protection** blocks this unless the user enables
  "Access iCloud Data on the Web".

---

## `Note` record fields **[RE]**

From `-[ICNote(CloudKit) makeCloudKitRecordForApproach:mergeableFieldState:]` and
`-[ICNote(CloudKit) addNoteBodyToRecord:forApproach:mergeableFieldState:]`.

| Field | Kind | Notes |
|---|---|---|
| `TextDataEncrypted` | encrypted bytes | Note body, inline when ≤ 100 KB |
| `TextDataAsset` | asset | Note body when too large for inline |
| `TextDeltaDataEncrypted` / `TextDeltaDataAsset` | encrypted / asset | Incremental body delta (fast path) |
| `TitleEncrypted` | encrypted UTF-8 | |
| `SnippetEncrypted` | encrypted UTF-8 | List preview |
| `CreationDate`, `ModificationDate` | date | |
| `Folder` | reference (action `none`) | Same zone only |
| `Folders` | list of references | `[Folder]` |
| `FoldersModificationDate` | date | |
| `ReplicaIDToUserIDEncrypted` | encrypted bytes | Shared notes |
| `CryptoInitializationVector`, `CryptoTag`, `CryptoWrappedKey` | bytes | Only on password-locked notes |
| `FirstAttachmentThumbnail` (+ `…Orientation`), `FirstAttachmentUTIEncrypted` | asset / short / encrypted | |
| `AttachmentViewType`, `PaperStyleType` | short | |
| `LegacyModificationDateAtImport`, `LegacyContentHashAtImportEncrypted`, `LegacyImportDeviceIdentifier` | | Pre-2015 imports |

Key naming rule: `+[CKRecord(IC) ic_encryptedKeyForKeyPrefix:]` = `"<prefix>Encrypted"`,
`ic_assetKeyForKeyPrefix:` = `"<prefix>Asset"`. Readers check the encrypted inline key first,
then fall back to the asset (`ic_encryptedInlineableDataAssetForKeyPrefix:`).

"Encrypted" here means CloudKit's per-field encryption (`CKRecord.encryptedValues`). Over the
web API these come back already decrypted (base64 plaintext) for the signed-in user **[confirmed live]**.

## `Folder` record fields **[RE]**

From `-[ICFolder(CloudKit) makeCloudKitRecordForApproach:mergeableFieldState:]`.

| Field | Kind |
|---|---|
| `TitleEncrypted` | encrypted UTF-8 |
| `TitleModificationDate` | date |
| `ParentFolder` | reference (same zone), cleared for top-level folders |
| `ParentModificationDate` | date |
| `MergeableDataEncrypted` / `MergeableDataAsset` | CRDT folder state |
| `SmartFolderQueryJSONEncrypted` | smart folders only |
| `ImportedFromLegacy` | bool |

---

## Note body format

- `-[ICNote saveNoteData]` writes `[document serialize]` into `noteData.data`, then the record
  takes `noteData.data` as `TextData`. **[RE]**
- The document is a **topotext CRDT** (`ICTTMergeableStringVersionedDocument` /
  `ICTTMergeableString`), with per-replica timestamps, `replaceCharactersInRange:`,
  `removeTimestampsForReplicaID:`, `canMergeString:`. **[RE]**
- The serialized bytes are gzip-compressed protobuf (`NoteStoreProto → Document → Note`, with
  `note_text`, `attribute_run`, and the CRDT substring/timestamp tables). Reference:
  `apple_cloud_notes_parser`. **[confirmed live: 655/655 bodies decoded]**

### Consequence for writes

Apple devices **merge** incoming bodies instead of replacing them. A body built from scratch,
without the existing replica/timestamp history, may merge as duplicated or garbled text on
the user's iPhone/Mac. Edits must be expressed as CRDT operations from our own replica ID on
top of the fetched document. Creating a brand-new note is lower risk, since there's no prior
history to merge with.

Password-locked notes (`CryptoWrappedKey` present) are read-only/out of scope.

---

## Confirmed against a live account (2026-10-12)

First run of `notes/linux/icloud` (`cargo run --example probe`) against a real account
(655 notes, 19 folders, no Advanced Data Protection):

- Sign-in: SRP-6a as implemented in `srp.rs` is accepted. `signin/complete` answers **409**
  for 2FA. **No code is pushed until the client does `GET /appleauth/auth`** (the 2FA
  options request, which also returns `trustedPhoneNumbers`). Resend is
  `PUT /appleauth/auth/verify/trusteddevice/securitycode`; `PUT …/verify/trusteddevice`
  answers 405.
- Special folder record names: `DefaultFolder-CloudKit` ("Notes") and
  `TrashFolder-CloudKit` ("Recently Deleted"). Notes in the trash point at the latter via
  `Folder`.
- Record types present in the `Notes` zone: `Note`, `Note_UserSpecific`, `Folder`,
  `Folder_UserSpecific`, `Attachment`, `InlineAttachment`, `Media`, `Hashtag`,
  `AccountData`, `cloudkit.share`.
- `Note` fields seen over the web API (matches the [RE] table): `TextDataEncrypted`,
  `TitleEncrypted`, `SnippetEncrypted`, `CreationDate`, `ModificationDate`, `Folder`,
  `Folders` (REFERENCE_LIST), `FoldersModificationDate`, `Deleted` (INT64),
  `MinimumSupportedNotesVersion`, `ReplicaIDToNotesVersionDataEncrypted`,
  `ReplicaIDToUserIDEncrypted`, `AttachmentViewType`, `PaperStyleType`,
  `FirstAttachment*`, `ActivityEventsData*`.
- Some notes carry no `Folder` field. Their placement comes from `Note_UserSpecific`
  (`Note` → `Folder`) or the `Folders` list.
- `TitleEncrypted` can contain newlines; the UI should use the first non-empty line.
- `Folder` records seen had no `ParentFolder`; nested folders are untested.

## Writing notes: what Apple's code does (in progress)

### Base fields on every record **[RE]**
`-[ICCloudSyncingObject makeCloudKitRecordForApproach:mergeableFieldState:]`:
- `MinimumSupportedNotesVersion` (INT64) when `shouldSyncMinimumSupportedNotesVersion`
- `ReplicaIDToNotesVersionDataEncrypted` / `…Asset` when the class tracks notes versions
- `Deleted` (bool, `markedForDeletion`) for classes that support deletion by TTL
- CloudKit `parent` = parent object's record (same zone only)
- `Crypto*` fields only for password-protected objects

### Body serialization **[RE]**
- `-[ICTTVersionedDocument serialize]`: sets the document archive's serialization version to
  `+versionedDocumentSerializationVersion` (= **0**), appends one
  `versioned_document::Version`, fills it via `saveCurrentVersion:`, serialises,
  then **`ic_gzipDeflate`**.
- `+[ICTTMergeableStringVersionedDocument serializationVersion]` = **0**,
  `minimumSupportedVersion` = **0**.
- Wire layout, consistent with the 655 decoded bodies:
  `Document { 2: Version { 1: serializationVersion, 2: minimumSupportedVersion, 3: data } }`,
  where `data` is a `topotext::String`.
- `-[ICTTMergeableString saveToArchive:]` → `generateIdsForLocalChanges`, `coalesce`, then
  `saveSubstrings:…toArchive:`, which writes:
  - `String.2` = concatenated text of the non-deleted substrings (UTF-8)
  - `String.3` = one `Substring` per ordered substring:
    `{ 1: CharID charID, 2: length, 3: CharID timestamp, 4: tombstone, 5: repeated child index }`
  - `CharID { 1: replica index, 2: clock }`. Replica index **0 = the reserved zero replica**;
    real replicas are numbered **1..n in the sorted order of the vector timestamp's UUIDs**.
  - `String.4` = the vector timestamp (`ICTTVectorMultiTimestamp saveToArchive:`), omitted
    when the document is shared.
  - `String.5` = attribute runs (already used for reading).

### A new note as Apple writes it **[confirmed live, `probe --dump`]**
Two lines ("Probe Title" / "Hello world"), created on an Apple device:
- Record name: uppercase UUID. CloudKit `parent` = the folder record.
- Fields: `TitleEncrypted`, `SnippetEncrypted` (first non-empty line after the title),
  `TextDataEncrypted`, `Folder` and `Folders` (reference action **`VALIDATE`**),
  `CreationDate`, `ModificationDate`, `FoldersModificationDate`, `Deleted` = 0,
  `MinimumSupportedNotesVersion` = 0, `AttachmentViewType` = 0, `PaperStyleType` = 0.
- `ReplicaIDToNotesVersionDataEncrypted` and `ReplicaIDToUserIDEncrypted` are byte-identical
  on every record in the account (account-wide CRDT dictionaries); a new note can omit them.
- Body tree:
  ```
  Document { 1: 0, 2: Version { 1: 0, 2: 0, 3: String {
    2: text
    3: { charID (0,0) len 0 ts (0,0) child 1 }                     start sentinel
    3: { charID (1,0) len N ts (1,0) child 2 }                     inserted text
    3: { charID (0,4294967295) len 0 ts (0,4294967295) }           end sentinel
    4: { 1: { 1: replica UUID, 2: {1: N} (text clock), 2: {1: 1} (style clock) } }
    5: { 1: N, 2: { 2: 0, 3: 1, 9: paragraph UUID }, 13: unix seconds }
  }}}
  ```
  N counts UTF-16 code units. `newnote.rs` reproduces this tree exactly (unit test
  `matches_note_created_by_apple`).
- Writing: `records/modify` with `operationType: create` and plaintext values typed
  `ENCRYPTED_BYTES` is **accepted [confirmed live 2026-10-12]**. The server stored exactly the
  body we sent, added `chainParentKey` itself, and recorded the web client as `deviceID: "2"`.
  The note appeared on a Mac, was **edited there (text, checklist, table added)**, and the
  edited body decoded correctly in amadeuz, so Apple's CRDT merge accepts our initial state.

### Editing an existing note **[RE]**
- `-[ICTTMergeableString insertAttributedString:after:before:]`: the new substring gets charID
  (local replica, text clock) and the text clock advances by its length. It is spliced into
  the graph: if `before` is a child of `after`, that child slot becomes the new node and the
  new node's child is `before`; otherwise the new node takes `after`'s children and becomes
  `after`'s only child. It is inserted into `orderedSubstrings` right after `after`. Position
  is fixed by graph edges, not by sibling ordering rules.
- `-[ICTTMergeableString generateIdsForLocalChanges…]`: new substrings get timestamp
  (replica, 0). Modified/deleted substrings get a Lamport timestamp from the **style clock**:
  `max(styleClock, oldTimestamp + 1)`, then `styleClock = ts + 1`. The vector timestamp stores
  [text clock, style clock] per replica.
- Deletion = split at the range boundaries (right part keeps (replica, clock + offset)) and
  tombstone; nothing is removed.
- amadeuz implementation: `crdt.rs` (wire-level, unknown fields preserved, refuses unexpected
  shapes), `notebody::render` / `apply_markdown_edit` (Markdown ↔ note-text map),
  `ICloudClient::edit_note` (`records/lookup` → edit → verify → `records/modify` `update` with
  `recordChangeTag`; `CONFLICT`/`OPLOCK` → reload).

### Smart formatting Apple inserts **[confirmed live, `probe --smart-survey`, 662 notes]**
Links in attribute runs (`AttributeRun.9`), by scheme: `https` 385, `mailto` 45, `tel` 23,
`http` 23, `x-apple-data-detectors` 19 (addresses, dates; target is an opaque
`x-apple-data-detectors://embedded-result/N`), `acc` 1, `javascript` 1. Data-detected links
(phone numbers as `tel:` with percent-encoded spaces, emails, pasted URLs shortened with `…`)
are indistinguishable from deliberate links except by comparing link and visible text;
`notebody::is_automatic_link` does that and amadeuz shows them as plain text.

Inline item types (`AttachmentInfo.type_uti`): `com.apple.notes.table`, `public.jpeg/png/heic`,
`org.webmproject.webp`, `public.svg-image`, `com.compuserve.gif`, `com.apple.drawing.2`,
`com.apple.paper`, `com.apple.paper.doc.pdf`, `public.url`,
`com.apple.notes.inlinetextattachment.hashtag`, `…inlinetextattachment.calculateresult`.

Run fields seen but not rendered (preserved byte for byte on edit): `AttributeRun.3` (font),
`.6` (underline), `.10` (colour), `.11` (unknown), `ParagraphStyle.7` (unknown; 83 runs).

## Reproducing the analysis

```sh
brew install --formula homebrew/core/ipsw      # NOT the blacktop tap (untrusted-tap policy)
ipsw dyld extract /System/Volumes/Preboot/Cryptexes/OS/System/Library/dyld/dyld_shared_cache_arm64e \
    NotesShared --output <dir> --objc --slide
```

Then open `<dir>/NotesShared` in REA with `provider_id: "hopper"`. Useful symbols:
`-[ICNote(CloudKit) makeCloudKitRecordForApproach:mergeableFieldState:]`,
`-[ICNote(CloudKit) addNoteBodyToRecord:forApproach:mergeableFieldState:]`,
`-[ICFolder(CloudKit) makeCloudKitRecordForApproach:mergeableFieldState:]`,
`-[CKRecord(IC) ic_encryptedInlineableDataAssetForKeyPrefix:]`, `-[ICNote saveNoteData]`.
