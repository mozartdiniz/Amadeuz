//! Interactive check of iCloud sign-in and Notes listing.
//!
//!     cargo run --example probe              # folders, note titles, decode status
//!     cargo run --example probe -- --bodies  # also print the first lines of each note
//!     cargo run --example probe -- --forget  # delete the saved session
//!     cargo run --example probe -- --dump    # every field of the notes in "Folder Sync Test"
//!     cargo run --example probe -- --smart-survey   # kinds of links/inline items in all notes (counts only)
//!     cargo run --example probe -- --create-test-note "Title\nBody"   # writes to iCloud!
//!     cargo run --example probe -- --dump "Other Folder"
//!
//! The session is kept in the OS keyring (macOS Keychain / Secret Service /
//! Windows Credential Manager) under "com.amadeuz.Notes.icloud-probe".

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

use amadeuz_icloud::{Error, ICloudClient, SignInOutcome};

const KEYRING_SERVICE: &str = "com.amadeuz.Notes.icloud-probe";
const KEYRING_USER: &str = "session";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let show_bodies = args.iter().any(|a| a == "--bodies");
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).expect("keyring");

    if args.iter().any(|a| a == "--forget") {
        entry.delete_credential().ok();
        println!("Saved session deleted.");
        return;
    }

    let mut client = match entry.get_password().ok().and_then(|s| ICloudClient::restore(&s).ok()) {
        Some(mut c) => match c.validate().await {
            Ok(()) => {
                println!("Reusing saved session for {}", c.apple_id().unwrap_or("?"));
                c
            }
            Err(e) => {
                println!("Saved session not usable ({e}); signing in again.");
                sign_in(Some(c)).await
            }
        },
        None => sign_in(None).await,
    };
    entry.set_password(&client.save()).expect("save session to keyring");

    if let Some(i) = args.iter().position(|a| a == "--create-test-note") {
        let text = args
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| "Hello from Amadeuz\nThis note was created by amadeuz, not by an Apple device.".into());
        create_test_note(&client, &text).await;
        return;
    }
    if args.iter().any(|a| a == "--smart-survey") {
        smart_survey(&client).await;
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--dump") {
        let folder = args.get(i + 1).map(String::as_str).unwrap_or("Folder Sync Test");
        dump_folder(&client, folder).await;
        return;
    }

    println!("\nFetching notes…");
    let snap = match client.fetch_all().await {
        Ok(s) => s,
        Err(Error::SessionExpired) => {
            client = sign_in(Some(client)).await;
            entry.set_password(&client.save()).ok();
            client.fetch_all().await.unwrap_or_else(|e| fail(e))
        }
        Err(e) => fail(e),
    };

    println!("\n== Schema seen (record type → field: type) ==");
    for (rtype, fields) in snap.schema.iter().collect::<BTreeMap<_, _>>() {
        println!("{rtype}");
        for (f, t) in fields.iter().collect::<BTreeMap<_, _>>() {
            println!("    {f}: {t}");
        }
    }

    println!("\n== Folders ({}) ==", snap.folders.len());
    for f in &snap.folders {
        println!(
            "  {:<40} {:<30} parent={}{}",
            f.id,
            f.title,
            f.parent_id.as_deref().unwrap_or("-"),
            if f.is_trash { "  [trash]" } else { "" }
        );
    }

    let decoded = snap.notes.iter().filter(|n| n.markdown.is_some()).count();
    let locked = snap.notes.iter().filter(|n| n.is_locked).count();
    let failed: Vec<_> = snap.notes.iter().filter(|n| n.body_error.is_some()).collect();
    println!(
        "\n== Notes ({}: {decoded} decoded, {locked} locked, {} failed) ==",
        snap.notes.len(),
        failed.len()
    );
    for n in &snap.notes {
        let status = if n.is_locked {
            "locked"
        } else if n.body_error.is_some() {
            "FAILED"
        } else if n.markdown.is_some() {
            "ok"
        } else {
            "no body"
        };
        println!(
            "  [{status:<7}] {:<40} folder={:<40} {}",
            n.title,
            n.folder_id.as_deref().unwrap_or("-"),
            n.modified_at
        );
        if show_bodies {
            if let Some(md) = &n.markdown {
                for line in md.lines().take(6) {
                    println!("             | {line}");
                }
            }
        }
    }
    for n in failed {
        println!("  decode error for {}: {}", n.id, n.body_error.as_deref().unwrap_or(""));
    }

    // Attachment rendering summary (no note content).
    let all_md: Vec<&str> = snap.notes.iter().filter_map(|n| n.markdown.as_deref()).collect();
    let tables = all_md.iter().map(|m| m.matches("\n|---").count()).sum::<usize>();
    let images = all_md.iter().map(|m| m.matches("](icloud://").count()).sum::<usize>();
    let mut leftovers: BTreeMap<String, usize> = BTreeMap::new();
    for md in &all_md {
        for (i, _) in md.match_indices('[') {
            let rest = &md[i + 1..];
            if let Some(end) = rest.find(']') {
                let tag = &rest[..end];
                if !rest[end..].starts_with("](") && tag.contains('.') && !tag.contains(' ') && tag.len() < 60 {
                    *leftovers.entry(tag.to_string()).or_default() += 1;
                }
            }
        }
    }
    println!("\n== Attachments ==");
    println!("  images with a downloadable file: {} ({} referenced in notes)", snap.images.len(), images);
    println!("  tables rendered: {tables}, failed: {}", snap.table_errors.len());
    for (id, err) in &snap.table_errors {
        println!("    table {id}: {err}");
    }
    println!("  still shown as placeholders (by type):");
    for (tag, count) in &leftovers {
        println!("    [{tag}] × {count}");
    }
    if let Some((id, url)) = snap.images.iter().next() {
        match client.download_asset(url).await {
            Ok(bytes) => println!("  sample image download: ok ({} bytes, attachment {id})", bytes.len()),
            Err(e) => println!("  sample image download FAILED: {e}"),
        }
    }
}

async fn sign_in(previous: Option<ICloudClient>) -> ICloudClient {
    // Keep the old client so Apple's trust token can skip 2FA.
    let mut client = previous.unwrap_or_default();
    let apple_id = match client.apple_id() {
        Some(id) => id.to_string(),
        None => prompt("Apple ID: "),
    };
    let password = rpassword::prompt_password("Password: ").expect("read password");

    match client.sign_in(&apple_id, &password).await {
        Ok(SignInOutcome::SignedIn) => {}
        Ok(SignInOutcome::NeedsCode { phones }) => loop {
            for (i, p) in phones.iter().enumerate() {
                println!("  s{}: send SMS to {}", i + 1, p.number);
            }
            let code = prompt("Code shown on your Apple device ('r' to resend, 's<N>' for SMS): ");
            if code == "r" {
                match client.resend_code().await {
                    Ok(()) => println!("New code requested."),
                    Err(e) => println!("Resend failed: {e}"),
                }
                continue;
            }
            if let Some(n) = code.strip_prefix('s').and_then(|n| n.parse::<usize>().ok()) {
                match phones.get(n.wrapping_sub(1)) {
                    Some(p) => match client.request_sms(p.id).await {
                        Ok(()) => println!("SMS sent to {}.", p.number),
                        Err(e) => println!("SMS failed: {e}"),
                    },
                    None => println!("No phone s{n}."),
                }
                continue;
            }
            match client.submit_code(&code).await {
                Ok(()) => break,
                Err(Error::BadCode) => println!("Wrong code, try again."),
                Err(e) => fail(e),
            }
        },
        Err(e) => fail(e),
    }
    println!("Signed in.");
    client
}

fn prompt(label: &str) -> String {
    print!("{label}");
    io::stdout().flush().ok();
    let mut s = String::new();
    io::stdin().read_line(&mut s).expect("read stdin");
    s.trim().to_string()
}

fn fail(e: Error) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}

// ── --dump: full structure of the notes in the test folder ───────────────────

/// Print every field of the folder named `folder_title`, its notes and their
/// attachments, decoding protobuf bodies generically (field numbers + values).
/// Only intended for the agreed test folder: it prints note content.
pub async fn dump_folder(client: &ICloudClient, folder_title: &str) {
    let records = client.fetch_raw_records().await.unwrap_or_else(|e| fail(e));
    let title_of = |r: &serde_json::Value| {
        r["fields"]["TitleEncrypted"]["value"]
            .as_str()
            .and_then(|b| B64.decode(b).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    let Some(folder) = records
        .iter()
        .find(|r| r["recordType"] == "Folder" && title_of(r).as_deref() == Some(folder_title))
    else {
        eprintln!("error: no folder titled {folder_title:?}");
        std::process::exit(1);
    };
    let folder_id = folder["recordName"].as_str().unwrap_or_default().to_string();
    let points_here = |r: &serde_json::Value, key: &str| r["fields"][key]["value"]["recordName"] == folder_id.as_str();
    let notes: Vec<&serde_json::Value> = records
        .iter()
        .filter(|r| r["recordType"] == "Note" && points_here(r, "Folder"))
        .collect();
    let note_ids: Vec<&str> = notes.iter().filter_map(|n| n["recordName"].as_str()).collect();
    let related = records.iter().filter(|r| {
        let note_ref = r["fields"]["Note"]["value"]["recordName"].as_str();
        (r["recordType"] == "Folder" && points_here(r, "ParentFolder"))
            || note_ref.is_some_and(|id| note_ids.contains(&id))
    });

    println!("\n######## Folder ########");
    dump_record(folder);
    for n in &notes {
        println!("\n######## Note ########");
        dump_record(n);
    }
    for r in related {
        println!("\n######## {} ########", r["recordType"].as_str().unwrap_or("?"));
        dump_record(r);
    }
}

fn dump_record(r: &serde_json::Value) {
    // Record metadata (everything except the fields map).
    let mut meta = r.clone();
    if let Some(o) = meta.as_object_mut() {
        o.remove("fields");
    }
    println!("meta: {meta}");
    let Some(fields) = r["fields"].as_object() else { return };
    for (name, f) in fields.iter().collect::<BTreeMap<_, _>>() {
        let ty = f["type"].as_str().unwrap_or("?");
        let v = &f["value"];
        match ty {
            "ENCRYPTED_BYTES" | "BYTES" => {
                let bytes = v.as_str().and_then(|b| B64.decode(b).ok()).unwrap_or_default();
                print!("{name} ({ty}, {} bytes): ", bytes.len());
                dump_bytes(&bytes, 1);
            }
            _ => println!("{name} ({ty}): {v}"),
        }
    }
}

fn dump_bytes(bytes: &[u8], depth: usize) {
    print!("{}", amadeuz_icloud::pbdump::describe(bytes, depth));
}

/// The only folder the probe is allowed to write to.
const TEST_FOLDER: &str = "Folder Sync Test";

/// Create a note in the test folder, then print the record as the server stored it.
async fn create_test_note(client: &ICloudClient, text: &str) {
    let records = client.fetch_raw_records().await.unwrap_or_else(|e| fail(e));
    let Some(folder_id) = records
        .iter()
        .find(|r| {
            r["recordType"] == "Folder"
                && r["fields"]["TitleEncrypted"]["value"]
                    .as_str()
                    .and_then(|b| B64.decode(b).ok())
                    .is_some_and(|t| t == TEST_FOLDER.as_bytes())
        })
        .and_then(|r| r["recordName"].as_str())
    else {
        eprintln!("error: no folder titled {TEST_FOLDER:?}; refusing to write anywhere else");
        std::process::exit(1);
    };
    println!("Creating note in {TEST_FOLDER:?} ({folder_id}) as replica {}…", client.replica_uuid());
    let name = client.create_note(folder_id, text).await.unwrap_or_else(|e| fail(e));
    println!("Created note {name}. Stored record:");

    let records = client.fetch_raw_records().await.unwrap_or_else(|e| fail(e));
    match records.iter().find(|r| r["recordName"] == name.as_str()) {
        Some(r) => dump_record(r),
        None => println!("(not visible in the zone yet; run --dump in a moment)"),
    }
}

/// Count every kind of link and inline item Apple stored, across all notes,
/// and how amadeuz classifies each link. Prints counts and link schemes only;
/// no note text, no URLs.
async fn smart_survey(client: &ICloudClient) {
    use amadeuz_icloud::notebody;
    let records = client.fetch_raw_records().await.unwrap_or_else(|e| fail(e));
    let mut links: BTreeMap<(String, &str), usize> = BTreeMap::new();
    let mut attachments: BTreeMap<String, usize> = BTreeMap::new();
    let mut other_run_fields: BTreeMap<String, usize> = BTreeMap::new();
    let mut notes = 0;
    for r in records.iter().filter(|r| r["recordType"] == "Note") {
        let Some(blob) = r["fields"]["TextDataEncrypted"]["value"].as_str().and_then(|b| B64.decode(b).ok()) else {
            continue;
        };
        let Ok(note) = notebody::decode(&blob) else { continue };
        notes += 1;
        let units: Vec<u16> = note.note_text.as_deref().unwrap_or("").encode_utf16().collect();
        let mut pos = 0usize;
        for run in &note.attribute_run {
            let len = run.length.unwrap_or(0).max(0) as usize;
            let text = String::from_utf16_lossy(&units[pos.min(units.len())..(pos + len).min(units.len())]);
            pos += len;
            if let Some(link) = run.link.as_deref() {
                let scheme = link.split_once(':').map_or("(none)", |(s, _)| s).to_ascii_lowercase();
                let kind = if notebody::is_automatic_link(link, &text) { "automatic → plain text" } else { "deliberate → [text](url)" };
                *links.entry((scheme, kind)).or_default() += 1;
            }
            if let Some(a) = &run.attachment_info {
                *attachments.entry(a.type_uti.clone().unwrap_or_else(|| "?".into())).or_default() += 1;
            }
        }
        // Run fields amadeuz doesn't model, by field number (font, colour, …).
        let raw = notebody_raw_runs(&blob);
        for f in raw {
            *other_run_fields.entry(f).or_default() += 1;
        }
    }
    println!("\nScanned {notes} notes.\n\n== Links (scheme, how amadeuz shows it) ==");
    for ((scheme, kind), n) in &links {
        println!("  {scheme:<28} {kind:<26} × {n}");
    }
    println!("\n== Attachments and inline items in note text (by type) ==");
    for (uti, n) in &attachments {
        println!("  {uti:<55} × {n}");
    }
    println!("\n== Other formatting fields present on runs (field number) ==");
    for (f, n) in &other_run_fields {
        println!("  {f:<10} × {n}");
    }
}

/// Field numbers used in attribute runs beyond the ones amadeuz renders.
fn notebody_raw_runs(blob: &[u8]) -> Vec<String> {
    use amadeuz_icloud::wire::Message;
    let mut raw = Vec::new();
    if flate2::read::GzDecoder::new(blob).read_to_end(&mut raw).is_err() {
        return vec![];
    }
    let mut out = Vec::new();
    let Some(string) = Message::parse(&raw)
        .ok()
        .and_then(|d| d.bytes(2).and_then(|v| Message::parse(v).ok()))
        .and_then(|v| v.bytes(3).and_then(|s| Message::parse(s).ok()))
    else {
        return out;
    };
    for run in string.all_bytes(5) {
        let Ok(run) = Message::parse(run) else { continue };
        for n in run.field_numbers() {
            // 1 length, 2 paragraph, 5 weight, 7 strike, 9 link, 12 attachment, 13 timestamp
            if !matches!(n, 1 | 2 | 5 | 7 | 9 | 12 | 13) {
                out.push(format!("run.{n}"));
            }
        }
        if let Some(p) = run.bytes(2).and_then(|p| Message::parse(p).ok()) {
            for n in p.field_numbers() {
                // 1 style, 2 alignment, 3 direction, 4 indent, 5 checklist, 9 uuid
                if !matches!(n, 1 | 2 | 3 | 4 | 5 | 9) {
                    out.push(format!("para.{n}"));
                }
            }
        }
    }
    out
}
