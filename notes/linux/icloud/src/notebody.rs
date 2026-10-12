//! Decoding Apple Notes bodies (`TextData`) into plain text and Markdown.
//!
//! The body is a serialized topotext CRDT document (`ICTTMergeableStringVersionedDocument`),
//! compressed, wrapped in protobuf. Only the fields needed to render the text are modelled
//! here; prost ignores the CRDT bookkeeping fields. Field numbers follow the community
//! `notestore.proto` used by apple_cloud_notes_parser.

use std::io::Read;

use prost::Message;

use crate::{Error, Result};

#[derive(Clone, PartialEq, Message)]
pub struct NoteStoreProto {
    #[prost(message, optional, tag = "2")]
    pub document: Option<Document>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Document {
    #[prost(int32, optional, tag = "2")]
    pub version: Option<i32>,
    #[prost(message, optional, tag = "3")]
    pub note: Option<Note>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Note {
    #[prost(string, optional, tag = "2")]
    pub note_text: Option<String>,
    #[prost(message, repeated, tag = "5")]
    pub attribute_run: Vec<AttributeRun>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AttributeRun {
    #[prost(int32, optional, tag = "1")]
    pub length: Option<i32>,
    #[prost(message, optional, tag = "2")]
    pub paragraph_style: Option<ParagraphStyle>,
    #[prost(int32, optional, tag = "5")]
    pub font_weight: Option<i32>,
    #[prost(int32, optional, tag = "6")]
    pub underlined: Option<i32>,
    #[prost(int32, optional, tag = "7")]
    pub strikethrough: Option<i32>,
    #[prost(string, optional, tag = "9")]
    pub link: Option<String>,
    #[prost(message, optional, tag = "12")]
    pub attachment_info: Option<AttachmentInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ParagraphStyle {
    #[prost(int32, optional, tag = "1")]
    pub style_type: Option<i32>,
    #[prost(int32, optional, tag = "4")]
    pub indent_amount: Option<i32>,
    #[prost(message, optional, tag = "5")]
    pub checklist: Option<Checklist>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Checklist {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub uuid: Option<Vec<u8>>,
    #[prost(int32, optional, tag = "2")]
    pub done: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AttachmentInfo {
    #[prost(string, optional, tag = "1")]
    pub attachment_identifier: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub type_uti: Option<String>,
}

const STYLE_TITLE: i32 = 0;
const STYLE_HEADING: i32 = 1;
const STYLE_SUBHEADING: i32 = 2;
const STYLE_MONOSPACED: i32 = 4;
const STYLE_DOTTED: i32 = 100;
const STYLE_DASHED: i32 = 101;
const STYLE_NUMBERED: i32 = 102;
const STYLE_CHECKLIST: i32 = 103;

const ATTACHMENT_CHAR: char = '\u{FFFC}';

/// Decompress and parse a `TextData` blob.
pub fn decode(blob: &[u8]) -> Result<Note> {
    let raw = decompress(blob)?;
    let store = NoteStoreProto::decode(raw.as_slice())
        .map_err(|e| Error::Protocol(format!("note body is not NoteStoreProto: {e}")))?;
    store
        .document
        .and_then(|d| d.note)
        .ok_or_else(|| Error::Protocol("note body has no document".into()))
}

pub(crate) fn decompress(blob: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let res = match blob {
        [0x1f, 0x8b, ..] => flate2::read::GzDecoder::new(blob).read_to_end(&mut out),
        [0x78, ..] => flate2::read::ZlibDecoder::new(blob).read_to_end(&mut out),
        _ => return Ok(blob.to_vec()),
    };
    res.map_err(|e| Error::Protocol(format!("note body decompression failed: {e}")))?;
    Ok(out)
}

/// Render the note as Markdown in the dialect the Amadeuz editor understands
/// (headings, bullet/numbered lists, checkboxes, bold/italic/strikethrough, links).
/// Attachments render as `[type]` placeholders; see [`to_markdown_with`].
pub fn to_markdown(note: &Note) -> String {
    to_markdown_with(note, &|_| None)
}

/// Like [`to_markdown`], with `attachment` rendering each attachment run.
/// Returning `None` falls back to a `[type]` placeholder.
pub fn to_markdown_with(note: &Note, attachment: &dyn Fn(&AttachmentInfo) -> Option<String>) -> String {
    render(note, attachment).text
}

/// Markdown plus, for every Markdown `char`, the UTF-16 index of the note-text
/// character it shows (`None` for markup the renderer added: `# `, `**`, list
/// prefixes, fences). An attachment's rendering maps its first char to the
/// attachment's U+FFFC. [`apply_markdown_edit`] uses this to turn an edit of the
/// Markdown into an edit of the note text.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rendered {
    pub text: String,
    pub map: Vec<Option<usize>>,
    /// Chars amadeuz typed earlier in this editing session (same length as
    /// `map`, or empty = none). They may still turn into Markdown markup once
    /// the user finishes typing it, e.g. `#` saved, then `# ` completed.
    #[serde(default)]
    pub typed: Vec<bool>,
}

impl Rendered {
    fn syn(&mut self, s: &str) {
        self.text.push_str(s);
        self.map.extend(s.chars().map(|_| None));
    }

    fn raw(&mut self, s: &str, mut at: usize) {
        for c in s.chars() {
            self.text.push(c);
            self.map.push(Some(at));
            at += c.len_utf16();
        }
    }
}

/// A piece of note text with the UTF-16 index where it starts.
struct Frag<'a> {
    run: &'a AttributeRun,
    text: String,
    at: usize,
}

pub fn render(note: &Note, attachment: &dyn Fn(&AttachmentInfo) -> Option<String>) -> Rendered {
    let text = note.note_text.as_deref().unwrap_or("");
    // Run lengths are NSString lengths, i.e. UTF-16 code units.
    let units: Vec<u16> = text.encode_utf16().collect();

    let mut spans: Vec<(&AttributeRun, usize, usize)> = Vec::new();
    let mut pos = 0usize;
    for run in &note.attribute_run {
        let len = run.length.unwrap_or(0).max(0) as usize;
        let end = (pos + len).min(units.len());
        spans.push((run, pos, end));
        pos = end;
    }
    if pos < units.len() {
        spans.push((&DEFAULT_RUN, pos, units.len()));
    }

    // Split spans into lines; remember the index of the '\n' ending each line.
    let mut lines: Vec<(Vec<Frag>, Option<usize>)> = vec![(Vec::new(), None)];
    for (run, from, to) in spans {
        let mut start = from;
        for i in from..to {
            if units[i] == u16::from(b'\n') {
                if i > start {
                    lines.last_mut().unwrap().0.push(Frag { run, text: String::from_utf16_lossy(&units[start..i]), at: start });
                }
                lines.last_mut().unwrap().1 = Some(i);
                lines.push((Vec::new(), None));
                start = i + 1;
            }
        }
        if to > start {
            lines.last_mut().unwrap().0.push(Frag { run, text: String::from_utf16_lossy(&units[start..to]), at: start });
        }
    }
    if lines.len() > 1 && lines.last().is_some_and(|(f, _)| f.is_empty()) {
        lines.pop();
    }

    let mut out = Rendered::default();
    // Separator owed before the next output line: the previous line's raw '\n',
    // or a synthetic one after a fence.
    let mut pending_newline: Option<Option<usize>> = None;
    let flush = |out: &mut Rendered, pending: &mut Option<Option<usize>>| match pending.take() {
        Some(Some(at)) => out.raw("\n", at),
        Some(None) => out.syn("\n"),
        None => {}
    };
    let mut in_code = false;
    for (frags, newline) in &lines {
        let style = frags.first().and_then(|f| f.run.paragraph_style.as_ref());
        let style_type = style.and_then(|s| s.style_type);
        let indent = "  ".repeat(style.and_then(|s| s.indent_amount).unwrap_or(0).max(0) as usize);

        let monospaced = style_type == Some(STYLE_MONOSPACED);
        if monospaced != in_code {
            flush(&mut out, &mut pending_newline);
            out.syn("```");
            pending_newline = Some(None);
            in_code = monospaced;
        }
        flush(&mut out, &mut pending_newline);

        let has_body = frags.iter().any(|f| !f.text.is_empty());
        if monospaced {
            for f in frags {
                out.raw(&f.text, f.at);
            }
        } else if has_body || style_type.is_some() {
            let prefix = match style_type {
                Some(STYLE_TITLE) => "# ".to_string(),
                Some(STYLE_HEADING) => "## ".to_string(),
                Some(STYLE_SUBHEADING) => "### ".to_string(),
                Some(STYLE_DOTTED) | Some(STYLE_DASHED) => format!("{indent}- "),
                Some(STYLE_NUMBERED) => format!("{indent}1. "),
                Some(STYLE_CHECKLIST) => {
                    let done = style.and_then(|s| s.checklist.as_ref()).and_then(|c| c.done).unwrap_or(0) != 0;
                    format!("{indent}- [{}] ", if done { "x" } else { " " })
                }
                _ => String::new(),
            };
            out.syn(&prefix);
            for f in &merge_same_format(frags) {
                inline(&mut out, f, attachment);
            }
        }
        pending_newline = newline.map(Some);
    }
    if in_code {
        // Close the fence on its own line.
        match pending_newline.take() {
            Some(Some(at)) => out.raw("\n", at),
            _ => out.syn("\n"),
        }
        out.syn("```");
    }
    out
}

/// Plain text with attachment placeholders removed.
pub fn to_plain_text(note: &Note) -> String {
    note.note_text
        .as_deref()
        .unwrap_or("")
        .chars()
        .filter(|&c| c != ATTACHMENT_CHAR)
        .collect()
}

static DEFAULT_RUN: AttributeRun = AttributeRun {
    length: None,
    paragraph_style: None,
    font_weight: None,
    underlined: None,
    strikethrough: None,
    link: None,
    attachment_info: None,
};

/// Apple splits text into many runs that look the same (they differ only in
/// timestamps or paragraph IDs). Join neighbours with identical visible
/// formatting so `**bold**` wraps the whole span, not each run.
fn merge_same_format<'a>(frags: &[Frag<'a>]) -> Vec<Frag<'a>> {
    let looks_same = |a: &AttributeRun, b: &AttributeRun| {
        a.attachment_info.is_none()
            && b.attachment_info.is_none()
            && a.font_weight.unwrap_or(0) == b.font_weight.unwrap_or(0)
            && a.strikethrough.unwrap_or(0) == b.strikethrough.unwrap_or(0)
            && a.link.is_some() == b.link.is_some()
            && a.link == b.link
    };
    let mut out: Vec<Frag<'a>> = Vec::with_capacity(frags.len());
    for f in frags {
        if let Some(prev) = out.last_mut() {
            let contiguous = prev.at + prev.text.encode_utf16().count() == f.at;
            if contiguous && looks_same(prev.run, f.run) && !f.text.contains(ATTACHMENT_CHAR) {
                prev.text.push_str(&f.text);
                continue;
            }
        }
        out.push(Frag { run: f.run, text: f.text.clone(), at: f.at });
    }
    out
}

/// The run's link if someone made it on purpose; `None` for links Apple adds
/// by detecting text (addresses, dates, phone numbers, emails, typed URLs),
/// which are shown as plain text.
fn deliberate_link<'a>(run: &'a AttributeRun, text: &str) -> Option<&'a str> {
    let link = run.link.as_deref().filter(|l| !l.is_empty())?;
    (!is_automatic_link(link, text)).then_some(link)
}

pub fn is_automatic_link(link: &str, text: &str) -> bool {
    let text = text.trim();
    let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
    let (scheme, target) = link.split_once(':').unwrap_or(("", link));
    let target = percent_decode(target);
    match scheme.to_ascii_lowercase().as_str() {
        "x-apple-data-detectors" => true,
        "tel" | "telprompt" | "sms" | "facetime" | "facetime-audio" => {
            let d = digits(text);
            !d.is_empty() && digits(&target).ends_with(&d)
        }
        "mailto" => target.split('?').next().is_some_and(|a| a.eq_ignore_ascii_case(text)),
        _ => {
            let bare = |u: &str| {
                let u = u.trim_end_matches('/');
                let u = u.split_once("://").map_or(u, |(_, rest)| rest);
                u.trim_start_matches("www.").to_ascii_lowercase()
            };
            // Apple shortens long pasted URLs ("https://site.com/long/pa…"):
            // still automatic if the text looks like a URL and is a prefix of it.
            let looks_like_url = text.contains('.') && !text.contains(char::is_whitespace);
            let shown = bare(text.trim_end_matches(['…', '.']));
            !text.is_empty() && (bare(link) == bare(text) || (looks_like_url && !shown.is_empty() && bare(link).starts_with(&shown)))
        }
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn inline(out: &mut Rendered, f: &Frag, attachment: &dyn Fn(&AttachmentInfo) -> Option<String>) {
    let (run, s) = (f.run, f.text.as_str());
    if run.attachment_info.is_some() || s.contains(ATTACHMENT_CHAR) {
        let rendered = run.attachment_info.as_ref().and_then(attachment).unwrap_or_else(|| {
            let kind = run.attachment_info.as_ref().and_then(|a| a.type_uti.as_deref()).unwrap_or("attachment");
            format!("[{kind}]")
        });
        // First char stands for the attachment character itself.
        let fffc = s.find(ATTACHMENT_CHAR).map_or(0, |b| s[..b].encode_utf16().count());
        let mut chars = rendered.chars();
        if let Some(c) = chars.next() {
            out.text.push(c);
            out.map.push(Some(f.at + fffc));
        }
        out.syn(chars.as_str());
        return;
    }
    let trimmed = s.trim();
    if trimmed.is_empty() {
        out.raw(s, f.at);
        return;
    }
    // Keep surrounding whitespace outside the markers so Markdown stays valid.
    let lead = &s[..s.len() - s.trim_start().len()];
    let trail = &s[s.trim_end().len()..];
    let lead_units = lead.encode_utf16().count();
    let trimmed_units = trimmed.encode_utf16().count();
    let (open, close) = match run.font_weight.unwrap_or(0) {
        1 => ("**", "**"),
        2 => ("*", "*"),
        3 => ("***", "***"),
        _ => ("", ""),
    };
    let strike = run.strikethrough.unwrap_or(0) != 0;
    let link = deliberate_link(run, s);

    out.raw(lead, f.at);
    if link.is_some() {
        out.syn("[");
    }
    if strike {
        out.syn("~~");
    }
    out.syn(open);
    out.raw(trimmed, f.at + lead_units);
    out.syn(close);
    if strike {
        out.syn("~~");
    }
    if let Some(link) = link {
        out.syn(&format!("]({link})"));
    }
    out.raw(trail, f.at + lead_units + trimmed_units);
}

/// Paragraph style as Apple stores it (`ParagraphStyle.style_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParaStyle {
    Body,
    Title,
    Heading,
    Subheading,
    Monospaced,
    Bullet,
    Numbered,
    Checklist { done: bool },
}

impl ParaStyle {
    pub fn style_type(self) -> Option<i32> {
        match self {
            ParaStyle::Body => None,
            ParaStyle::Title => Some(STYLE_TITLE),
            ParaStyle::Heading => Some(STYLE_HEADING),
            ParaStyle::Subheading => Some(STYLE_SUBHEADING),
            ParaStyle::Monospaced => Some(STYLE_MONOSPACED),
            ParaStyle::Bullet => Some(STYLE_DOTTED),
            ParaStyle::Numbered => Some(STYLE_NUMBERED),
            ParaStyle::Checklist { .. } => Some(STYLE_CHECKLIST),
        }
    }
}

/// Set the style of the paragraph(s) in `start..start + len` of the new note text.
#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphFormat {
    pub start: usize,
    pub len: usize,
    pub style: ParaStyle,
    pub indent: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineKind {
    Bold,
    Italic,
    BoldItalic,
    Strikethrough,
}

/// Apply an inline style to `start..start + len` of the new note text.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineFormat {
    pub start: usize,
    pub len: usize,
    pub kind: InlineKind,
}

/// The result of mapping an edit of rendered Markdown back to the note.
/// Offsets are UTF-16 indices into `text`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NoteEdit {
    pub text: String,
    /// Map and typed flags for the edited Markdown (see [`Rendered`]).
    pub map: Vec<Option<usize>>,
    pub typed: Vec<bool>,
    pub paragraphs: Vec<ParagraphFormat>,
    pub inline: Vec<InlineFormat>,
}

#[derive(Clone, Copy, PartialEq)]
enum Origin {
    /// Unchanged character shown from the note text.
    Raw,
    /// Unchanged note-text character that amadeuz typed in an earlier save
    /// of this session; may still become markup.
    Typed,
    /// Unchanged renderer markup.
    Synth,
    /// Typed by the user.
    New,
}

/// Markdown block prefix → (length in chars, style, indent level).
fn parse_prefix(line: &[char]) -> Option<(usize, ParaStyle, u32)> {
    let spaces = line.iter().take_while(|&&c| c == ' ').count();
    let rest: String = line[spaces..].iter().collect();
    let indent = (spaces / 2) as u32;
    let found = if rest.starts_with("### ") {
        Some((4, ParaStyle::Subheading))
    } else if rest.starts_with("## ") {
        Some((3, ParaStyle::Heading))
    } else if rest.starts_with("# ") {
        Some((2, ParaStyle::Title))
    } else if rest.starts_with("- [ ] ") {
        Some((6, ParaStyle::Checklist { done: false }))
    } else if rest.starts_with("- [x] ") || rest.starts_with("- [X] ") {
        Some((6, ParaStyle::Checklist { done: true }))
    } else if rest.starts_with("- ") || rest.starts_with("* ") {
        Some((2, ParaStyle::Bullet))
    } else {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        (digits > 0 && rest[digits..].starts_with(". ")).then_some((digits + 2, ParaStyle::Numbered))
    };
    // Headings don't take indentation.
    found.and_then(|(n, style)| match style {
        ParaStyle::Title | ParaStyle::Heading | ParaStyle::Subheading if spaces > 0 => None,
        _ => Some((spaces + n, style, indent)),
    })
}

/// Turn an edit of rendered Markdown into an edit of the note.
///
/// `base` was rendered from `raw`; `edited` is the Markdown after the user's
/// changes. Unchanged characters keep their identity. Markdown the user typed
/// becomes Apple formatting instead of text: block prefixes (`# `, `- [ ] `, …)
/// on paragraphs the user touched, and `**`/`*`/`***`/`~~` pairs typed in this
/// edit. Text that came from Apple is never reinterpreted as Markdown.
pub fn apply_markdown_edit(raw: &str, base: &Rendered, edited: &str) -> NoteEdit {
    let raw: Vec<u16> = raw.encode_utf16().collect();
    let chars: Vec<char> = edited.chars().collect();

    // 1. Classify every edited char, and note where base text was deleted.
    enum Step {
        Keep { edited: usize, raw: Option<usize> },
        Delete { raw: Option<usize> },
        Insert { edited: usize },
    }
    let mut steps = Vec::new();
    let mut origin = vec![Origin::New; chars.len()];
    let mut deleted_before = vec![false; chars.len() + 1];
    let diff = similar::TextDiff::from_chars(base.text.as_str(), edited);
    let mut next_edited = 0usize;
    for op in diff.ops() {
        for change in diff.iter_changes(op) {
            match change.tag() {
                similar::ChangeTag::Equal => {
                    let (i, j) = (change.old_index().unwrap(), change.new_index().unwrap());
                    let r = base.map.get(i).copied().flatten();
                    let typed = base.typed.get(i).copied().unwrap_or(false);
                    origin[j] = match (r, typed) {
                        (Some(_), true) => Origin::Typed,
                        (Some(_), false) => Origin::Raw,
                        (None, _) => Origin::Synth,
                    };
                    steps.push(Step::Keep { edited: j, raw: r });
                    next_edited = j + 1;
                }
                similar::ChangeTag::Delete => {
                    let i = change.old_index().unwrap();
                    deleted_before[next_edited] = true;
                    steps.push(Step::Delete { raw: base.map.get(i).copied().flatten() });
                }
                similar::ChangeTag::Insert => {
                    let j = change.new_index().unwrap();
                    steps.push(Step::Insert { edited: j });
                    next_edited = j + 1;
                }
            }
        }
    }

    // 2. Find typed Markdown: block prefixes and inline marker pairs.
    let mut dropped = vec![false; chars.len()];
    let mut line_styles: Vec<(std::ops::Range<usize>, ParaStyle, u32)> = Vec::new();
    let mut inline_spans: Vec<(std::ops::Range<usize>, InlineKind)> = Vec::new();
    let mut in_fence = false;
    let mut prev_newline_new = false;
    let mut line_start = 0usize;
    while line_start < chars.len() {
        let nl = chars[line_start..].iter().position(|&c| c == '\n').map(|p| line_start + p);
        let content_end = nl.unwrap_or(chars.len());
        let line_end = nl.map_or(chars.len(), |p| p + 1);
        let content = &chars[line_start..content_end];
        let not_raw = |r: std::ops::Range<usize>| r.clone().all(|k| origin[k] != Origin::Raw);
        let touched = prev_newline_new
            || (line_start..line_end).any(|k| origin[k] == Origin::New)
            || (line_start..=line_end.min(chars.len())).any(|k| deleted_before[k]);
        prev_newline_new = nl.is_some_and(|p| origin[p] == Origin::New);

        if content.iter().collect::<String>() == "```" && not_raw(line_start..content_end) {
            for k in line_start..line_end {
                if matches!(origin[k], Origin::New | Origin::Typed) {
                    dropped[k] = true;
                }
            }
            in_fence = !in_fence;
            line_start = line_end;
            continue;
        }

        let mut body_start = line_start;
        let mut style = if in_fence { ParaStyle::Monospaced } else { ParaStyle::Body };
        let mut indent = 0;
        if !in_fence {
            if let Some((n, s, ind)) = parse_prefix(content) {
                if not_raw(line_start..line_start + n) {
                    style = s;
                    indent = ind;
                    body_start = line_start + n;
                    if touched {
                        for k in line_start..body_start {
                            if matches!(origin[k], Origin::New | Origin::Typed) {
                                dropped[k] = true;
                            }
                        }
                    }
                }
            }
            find_inline_pairs(&chars, &origin, body_start..content_end, &mut dropped, &mut inline_spans);
        }
        if touched {
            line_styles.push((line_start..line_end, style, indent));
        }
        line_start = line_end;
    }

    // 3. Build the new note text and the map for the edited Markdown.
    let mut out: Vec<u16> = Vec::with_capacity(raw.len());
    let mut map = vec![None; chars.len()];
    let mut typed = vec![false; chars.len()];
    let mut raw_pos = 0usize;
    let width = |i: usize| if (0xD800..0xDC00).contains(&raw[i]) && i + 1 < raw.len() { 2 } else { 1 };
    for step in steps {
        let (kept, from_raw) = match step {
            Step::Keep { edited, raw } => (Some(edited), raw),
            Step::Delete { raw } => (None, raw),
            Step::Insert { .. } => (None, None),
        };
        match step {
            Step::Keep { .. } | Step::Delete { .. } => {
                let Some(r) = from_raw.filter(|&r| r >= raw_pos && r < raw.len()) else { continue };
                out.extend_from_slice(&raw[raw_pos..r]);
                let w = width(r);
                // A typed char that became markup is deleted from the note.
                if let Some(edited) = kept.filter(|&e| !dropped[e]) {
                    map[edited] = Some(out.len());
                    typed[edited] = origin[edited] == Origin::Typed;
                    out.extend_from_slice(&raw[r..r + w]);
                }
                raw_pos = r + w;
            }
            Step::Insert { edited } => {
                if !dropped[edited] {
                    map[edited] = Some(out.len());
                    typed[edited] = true;
                    let mut buf = [0u16; 2];
                    out.extend_from_slice(chars[edited].encode_utf16(&mut buf));
                }
            }
        }
    }
    out.extend_from_slice(&raw[raw_pos.min(raw.len())..]);

    // 4. Express the formatting in new-text offsets.
    let unit_width = |p: usize| if (0xD800..0xDC00).contains(&out[p]) && p + 1 < out.len() { 2 } else { 1 };
    let mut paragraphs: Vec<ParagraphFormat> = Vec::new();
    for (range, style, indent) in line_styles {
        let Some(p) = range.clone().find_map(|k| map[k]) else { continue };
        let start = out[..p].iter().rposition(|&u| u == u16::from(b'\n')).map_or(0, |i| i + 1);
        let end = out[p..].iter().position(|&u| u == u16::from(b'\n')).map_or(out.len(), |i| p + i + 1);
        if paragraphs.last().is_some_and(|last| last.start == start) {
            continue;
        }
        paragraphs.push(ParagraphFormat { start, len: end - start, style, indent });
    }
    let mut inline = Vec::new();
    for (range, kind) in inline_spans {
        let first = range.clone().find_map(|k| map[k]);
        let last = range.clone().rev().find_map(|k| map[k]);
        if let (Some(a), Some(b)) = (first, last) {
            inline.push(InlineFormat { start: a, len: b + unit_width(b) - a, kind });
        }
    }

    NoteEdit { text: String::from_utf16_lossy(&out), map, typed, paragraphs, inline }
}

/// Pair up `***`, `**`, `*` and `~~` markers typed in this edit (every marker
/// char `New`) within one line; mark them dropped and record the span between.
fn find_inline_pairs(
    chars: &[char],
    origin: &[Origin],
    range: std::ops::Range<usize>,
    dropped: &mut [bool],
    spans: &mut Vec<(std::ops::Range<usize>, InlineKind)>,
) {
    // Tokens: maximal runs of '*' or '~' made only of typed chars.
    let mut tokens: Vec<(usize, usize, char)> = Vec::new();
    let mut k = range.start;
    while k < range.end {
        let c = chars[k];
        if c == '*' || c == '~' {
            let start = k;
            while k < range.end && chars[k] == c {
                k += 1;
            }
            if (start..k).all(|i| matches!(origin[i], Origin::New | Origin::Typed)) {
                tokens.push((start, k - start, c));
            }
        } else {
            k += 1;
        }
    }
    let kind_of = |len: usize, c: char| match (c, len) {
        ('*', 1) => Some(InlineKind::Italic),
        ('*', 2) => Some(InlineKind::Bold),
        ('*', 3) => Some(InlineKind::BoldItalic),
        ('~', 2) => Some(InlineKind::Strikethrough),
        _ => None,
    };
    let mut open: Option<(usize, usize, char)> = None;
    for t in tokens {
        if kind_of(t.1, t.2).is_none() {
            continue;
        }
        match open {
            Some(o) if o.1 == t.1 && o.2 == t.2 && t.0 > o.0 + o.1 => {
                for i in (o.0..o.0 + o.1).chain(t.0..t.0 + t.1) {
                    dropped[i] = true;
                }
                spans.push((o.0 + o.1..t.0, kind_of(t.1, t.2).unwrap()));
                open = None;
            }
            _ => open = Some(t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn run(len: usize, style: Option<i32>) -> AttributeRun {
        AttributeRun {
            length: Some(len as i32),
            paragraph_style: style.map(|t| ParagraphStyle {
                style_type: Some(t),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn gzip_note(note: Note) -> Vec<u8> {
        let store = NoteStoreProto {
            document: Some(Document { version: Some(1), note: Some(note) }),
        };
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&store.encode_to_vec()).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn decodes_gzipped_body_to_markdown() {
        let text = "Shopping\nmilk\neggs\nnotes 🥚 done";
        let mut runs = vec![run(9, Some(STYLE_TITLE)), run(5, Some(STYLE_DOTTED)), run(5, Some(STYLE_DOTTED))];
        let mut bold = run(text.encode_utf16().count() - 19, None);
        bold.font_weight = Some(1);
        runs.push(bold);
        let blob = gzip_note(Note { note_text: Some(text.into()), attribute_run: runs });

        let note = decode(&blob).unwrap();
        assert_eq!(to_plain_text(&note), text);
        assert_eq!(to_markdown(&note), "# Shopping\n- milk\n- eggs\n**notes 🥚 done**");
    }

    #[test]
    fn renders_checklist_and_code() {
        let mut done = run(5, Some(STYLE_CHECKLIST));
        done.paragraph_style.as_mut().unwrap().checklist = Some(Checklist { uuid: None, done: Some(1) });
        let note = Note {
            note_text: Some("todo\nx = 1\n".into()),
            attribute_run: vec![done, run(6, Some(STYLE_MONOSPACED))],
        };
        assert_eq!(to_markdown(&note), "- [x] todo\n```\nx = 1\n```");
    }

    fn styled_note() -> Note {
        // "Shopping\nmilk\n" title + bullet; "eggs" bold.
        let mut bold = run(4, None);
        bold.font_weight = Some(1);
        Note {
            note_text: Some("Shopping\nmilk\neggs".into()),
            attribute_run: vec![run(9, Some(STYLE_TITLE)), run(5, Some(STYLE_DOTTED)), bold],
        }
    }

    #[test]
    fn map_points_at_note_text() {
        let note = styled_note();
        let r = render(&note, &|_| None);
        assert_eq!(r.text, "# Shopping\n- milk\n**eggs**");
        let raw: Vec<char> = note.note_text.clone().unwrap().chars().collect();
        for (c, m) in r.text.chars().zip(&r.map) {
            if let Some(i) = m {
                assert_eq!(raw[*i], c, "markdown {c:?} maps to {i}");
            }
        }
        assert_eq!(r.map[0], None); // '#'
        assert_eq!(r.map[2], Some(0)); // 'S'
    }

    #[test]
    fn bold_split_across_runs_renders_once() {
        // Apple's runs for "Bold text from apple: rest", split mid-word.
        let bold = |len| {
            let mut r = run(len, None);
            r.font_weight = Some(1);
            r
        };
        let note = Note {
            note_text: Some("Bold text from apple: rest".into()),
            attribute_run: vec![bold(5), bold(1), bold(4), bold(8), bold(3), run(5, None)],
        };
        let r = render(&note, &|_| None);
        assert_eq!(r.text, "**Bold text from apple:** rest");
        // The map still points every visible char at the right note-text char.
        let raw: Vec<char> = note.note_text.clone().unwrap().chars().collect();
        for (c, m) in r.text.chars().zip(&r.map) {
            if let Some(i) = m {
                assert_eq!(raw[*i], c);
            }
        }
    }

    #[test]
    fn markdown_edits_become_note_text_edits() {
        let note = styled_note();
        let raw = note.note_text.clone().unwrap();
        let r = render(&note, &|_| None);
        // Typing inside a heading, after a list prefix and inside bold text.
        let e = apply_markdown_edit(&raw, &r, "# Shopping list\n- oat milk\n**fresh eggs**");
        assert_eq!(e.text, "Shopping list\noat milk\nfresh eggs");
        assert_eq!(e.map.len(), "# Shopping list\n- oat milk\n**fresh eggs**".chars().count());
        // Edited paragraphs keep their (rendered) styles.
        assert_eq!(e.paragraphs[0].style, ParaStyle::Title);
        assert_eq!(e.paragraphs[1].style, ParaStyle::Bullet);
        // Deleting a whole line.
        let cut = apply_markdown_edit(&raw, &r, "# Shopping\n**eggs**");
        assert_eq!(cut.text, "Shopping\neggs");
    }

    #[test]
    fn typed_markdown_becomes_formatting() {
        let note = Note { note_text: Some("Notes\nplain line\n".into()), attribute_run: vec![run(17, None)] };
        let raw = note.note_text.clone().unwrap();
        let r = render(&note, &|_| None);
        assert_eq!(r.text, "Notes\nplain line");
        let e = apply_markdown_edit(&raw, &r, "Notes\n## plain line\n- [ ] buy **milk** now\n```\nlet x = 1;\n```");
        // Markup is not stored as text.
        // The original trailing newline (an empty last paragraph) stays last.
        assert_eq!(e.text, "Notes\nplain line\nbuy milk now\nlet x = 1;\n\n");
        let styles: Vec<_> = e.paragraphs.iter().map(|p| (p.start, p.len, p.style)).collect();
        assert_eq!(
            styles,
            vec![
                (6, 11, ParaStyle::Heading),
                (17, 13, ParaStyle::Checklist { done: false }),
                (30, 11, ParaStyle::Monospaced),
            ]
        );
        assert_eq!(e.inline, vec![InlineFormat { start: 21, len: 4, kind: InlineKind::Bold }]);
    }

    #[test]
    fn markup_finished_over_several_saves_still_converts() {
        let note = Note { note_text: Some("Title\n".into()), attribute_run: vec![run(6, None)] };
        let raw = note.note_text.clone().unwrap();
        let r0 = render(&note, &|_| None);
        // Save 1: only "#" typed so far, so it's text.
        let e1 = apply_markdown_edit(&raw, &r0, "Title\n#");
        assert_eq!(e1.text, "Title\n#\n");
        let r1 = Rendered { text: "Title\n#".into(), map: e1.map, typed: e1.typed };
        // Save 2: "# Heading" completed, plus a bold pair split across saves.
        let e2 = apply_markdown_edit(&e1.text, &r1, "Title\n# Heading **x");
        let r2 = Rendered { text: "Title\n# Heading **x".into(), map: e2.map, typed: e2.typed };
        assert_eq!(e2.text, "Title\nHeading **x\n");
        assert_eq!(e2.paragraphs.last().unwrap().style, ParaStyle::Title);
        // Save 3: closing "**" turns the earlier, already saved "**" into bold.
        let e3 = apply_markdown_edit(&e2.text, &r2, "Title\n# Heading **x**");
        assert_eq!(e3.text, "Title\nHeading x\n");
        assert_eq!(e3.inline, vec![InlineFormat { start: 14, len: 1, kind: InlineKind::Bold }]);
    }

    #[test]
    fn automatic_links_are_recognised() {
        assert!(is_automatic_link("tel:720%20090%201946", "720 090 1946"));
        assert!(is_automatic_link("tel:+447542547702", "7542 547702"));
        assert!(is_automatic_link("mailto:a@b.com", "a@b.com"));
        assert!(is_automatic_link("https://www.gocd.org/", "www.gocd.org"));
        assert!(is_automatic_link("x-apple-data-detectors://embedded-result/3", "tomorrow at 5"));
        assert!(!is_automatic_link("https://example.com", "my site"));
        assert!(is_automatic_link("https://medium.com/better-programming/the-marks-of-a-true-senior-developer-abc", "https://medium.com/better-programming/the-marks…"));
        assert!(!is_automatic_link("https://google.com", "Google"));
        assert!(!is_automatic_link("mailto:a@b.com", "email me"));
    }

    #[test]
    fn data_detector_links_render_as_text() {
        let mut addr = run(9, None);
        addr.link = Some("x-apple-data-detectors://embedded-result/22".into());
        let mut web = run(7, None);
        web.link = Some("https://example.com".into());
        let mut phone = run(13, None);
        phone.link = Some("tel:720%20090%201946".into());
        let note = Note {
            note_text: Some("Fleet Rd example 720 090 1946".into()),
            attribute_run: vec![addr, web, run(1, None), phone],
        };
        assert_eq!(render(&note, &|_| None).text, "Fleet Rd [example](https://example.com) 720 090 1946");
    }

    #[test]
    fn markdown_like_text_from_apple_stays_text() {
        let note = Note { note_text: Some("- not a list\n**not bold**".into()), attribute_run: vec![] };
        let raw = note.note_text.clone().unwrap();
        let r = render(&note, &|_| None);
        let e = apply_markdown_edit(&raw, &r, "- not a list!\n**not bold**");
        assert_eq!(e.text, "- not a list!\n**not bold**");
        assert_eq!(e.paragraphs[0].style, ParaStyle::Body);
        assert!(e.inline.is_empty());
    }

    #[test]
    fn toggling_a_checkbox_changes_style_not_text() {
        let mut item = run(5, Some(STYLE_CHECKLIST));
        item.paragraph_style.as_mut().unwrap().checklist = Some(Checklist { uuid: None, done: Some(0) });
        let note = Note { note_text: Some("milk\n".into()), attribute_run: vec![item] };
        let raw = note.note_text.clone().unwrap();
        let r = render(&note, &|_| None);
        assert_eq!(r.text, "- [ ] milk");
        let e = apply_markdown_edit(&raw, &r, "- [x] milk");
        assert_eq!(e.text, raw);
        assert_eq!(e.paragraphs[0].style, ParaStyle::Checklist { done: true });
    }

    #[test]
    fn uncompressed_body_is_accepted() {
        let store = NoteStoreProto {
            document: Some(Document {
                version: Some(1),
                note: Some(Note { note_text: Some("hi".into()), attribute_run: vec![] }),
            }),
        };
        let note = decode(&store.encode_to_vec()).unwrap();
        assert_eq!(to_plain_text(&note), "hi");
    }
}
