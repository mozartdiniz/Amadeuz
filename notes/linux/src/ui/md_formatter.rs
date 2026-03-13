use gtk::pango;
use gtk::prelude::*;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

// ── Tag name constants ────────────────────────────────────────────────────────

pub const TAG_TITLE: &str = "md-title";
const TAG_H1: &str = "md-h1";
const TAG_H2: &str = "md-h2";
const TAG_H3: &str = "md-h3";
const TAG_BOLD: &str = "md-bold";
const TAG_ITALIC: &str = "md-italic";
const TAG_CODE: &str = "md-code";
const TAG_STRIKE: &str = "md-strike";
const TAG_BLOCKQUOTE: &str = "md-blockquote";
const TAG_LINK: &str = "md-link";

const ALL_TAGS: &[&str] = &[
    TAG_TITLE, TAG_H1, TAG_H2, TAG_H3, TAG_BOLD, TAG_ITALIC,
    TAG_CODE, TAG_STRIKE, TAG_BLOCKQUOTE, TAG_LINK,
];

// ── Bullet list markers: (marker_text, continuation_text) ────────────────────
//
// Ordered from longest to shortest so partial prefixes don't match first.
// Checkbox variants continue as unchecked (new item starts empty).

const BULLET_MARKERS: &[(&str, &str)] = &[
    ("- [ ] ", "- [ ] "),
    ("- [x] ", "- [ ] "),
    ("- [X] ", "- [ ] "),
    ("+ [ ] ", "+ [ ] "),
    ("+ [x] ", "+ [ ] "),
    ("+ [X] ", "+ [ ] "),
    ("* [ ] ", "* [ ] "),
    ("* [x] ", "* [ ] "),
    ("* [X] ", "* [ ] "),
    ("- ", "- "),
    ("+ ", "+ "),
    ("* ", "* "),
];

// ── Tag setup ─────────────────────────────────────────────────────────────────

/// Create all markdown text-tags on the buffer's tag table.
/// Safe to call multiple times (skips tags that already exist).
pub fn setup_tags(buffer: &gtk::TextBuffer) {
    let table = buffer.tag_table();

    let add = |name: &str, configure: &dyn Fn(&gtk::TextTag)| {
        if table.lookup(name).is_some() {
            return; // already added
        }
        let tag = gtk::TextTag::new(Some(name));
        configure(&tag);
        table.add(&tag);
    };

    add(TAG_TITLE, &|t| {
        t.set_property("scale", 1.8f64);
        t.set_property("weight", 700i32);
    });
    add(TAG_H1, &|t| {
        t.set_property("scale", 1.6f64);
        t.set_property("weight", 700i32);
    });
    add(TAG_H2, &|t| {
        t.set_property("scale", 1.35f64);
        t.set_property("weight", 700i32);
    });
    add(TAG_H3, &|t| {
        t.set_property("scale", 1.15f64);
        t.set_property("weight", 700i32);
    });
    add(TAG_BOLD, &|t| {
        t.set_property("weight", 700i32);
    });
    add(TAG_ITALIC, &|t| {
        t.set_property("style", pango::Style::Italic);
    });
    add(TAG_CODE, &|t| {
        t.set_property("family", "monospace");
        // Slight visual distinction — foreground dimmed
        t.set_property("foreground", "#888888");
    });
    add(TAG_STRIKE, &|t| {
        t.set_property("strikethrough", true);
    });
    add(TAG_BLOCKQUOTE, &|t| {
        t.set_property("left-margin", 24i32);
        t.set_property("foreground", "#888888");
    });
    add(TAG_LINK, &|t| {
        t.set_property("underline", pango::Underline::Single);
        t.set_property("foreground", "#0078d4");
    });
}

// ── Formatting application ────────────────────────────────────────────────────

/// Apply visual markdown formatting tags to the buffer.
///
/// Reads the current buffer text (excluding any hidden chars / child anchors),
/// parses it with pulldown-cmark, and applies TextTags for headings, bold,
/// italic, code, strikethrough, blockquote, and links.
///
/// The **first line** always receives the title tag regardless of markdown
/// content, because the app treats line 1 as the note title.
pub fn apply_formatting(buffer: &gtk::TextBuffer) {
    let buf_start = buffer.start_iter();
    let buf_end = buffer.end_iter();

    // Get clean text (child anchors are excluded when include_hidden_chars=false)
    let text = buffer.text(&buf_start, &buf_end, false).to_string();

    // Remove all our tags across the whole buffer
    for name in ALL_TAGS {
        if let Some(tag) = buffer.tag_table().lookup(name) {
            buffer.remove_tag(&tag, &buf_start, &buf_end);
        }
    }

    if text.is_empty() {
        return;
    }

    // ── Title line styling ───────────────────────────────────────────────
    let title_byte_end = text.find('\n').unwrap_or(text.len());
    if title_byte_end > 0 {
        let title_char_end = text[..title_byte_end].chars().count() as i32;
        let ts = buffer.start_iter();
        let te = buffer.iter_at_offset(title_char_end);
        buffer.apply_tag_by_name(TAG_TITLE, &ts, &te);
    }

    // ── Markdown formatting for the content (everything after line 1) ────
    let content_byte_start = text.find('\n').map(|p| p + 1).unwrap_or(text.len());
    if content_byte_start >= text.len() {
        return;
    }
    let content = &text[content_byte_start..];
    let base_char = text[..content_byte_start].chars().count() as i32;

    apply_md_tags(buffer, content, base_char);
}

fn apply_md_tags(buffer: &gtk::TextBuffer, text: &str, base: i32) {
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(text, options).into_offset_iter();

    // Stack of (tag_name, start_byte_in_text)
    let mut stack: Vec<(&'static str, usize)> = Vec::new();

    for (event, range) in parser {
        match event {
            Event::Start(ref tag) => {
                let name: &'static str = match tag {
                    Tag::Heading { level, .. } => heading_tag(*level),
                    Tag::Strong => TAG_BOLD,
                    Tag::Emphasis => TAG_ITALIC,
                    Tag::Strikethrough => TAG_STRIKE,
                    Tag::BlockQuote(_) => TAG_BLOCKQUOTE,
                    Tag::CodeBlock(_) => TAG_CODE,
                    Tag::Link { .. } => TAG_LINK,
                    _ => continue,
                };
                stack.push((name, range.start));
            }

            Event::End(ref tag) => {
                let should_pop = matches!(
                    tag,
                    TagEnd::Heading(_)
                        | TagEnd::Strong
                        | TagEnd::Emphasis
                        | TagEnd::Strikethrough
                        | TagEnd::BlockQuote(_)
                        | TagEnd::CodeBlock
                        | TagEnd::Link
                );
                if should_pop {
                    if let Some((name, start_byte)) = stack.pop() {
                        apply_range(buffer, text, base, name, start_byte, range.end);
                    }
                }
            }

            // Inline code: the range covers the whole `backtick expression`
            Event::Code(_) => {
                apply_range(buffer, text, base, TAG_CODE, range.start, range.end);
            }

            _ => {}
        }
    }
}

#[inline]
fn heading_tag(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => TAG_H1,
        HeadingLevel::H2 => TAG_H2,
        _ => TAG_H3,
    }
}

#[inline]
fn apply_range(
    buffer: &gtk::TextBuffer,
    text: &str,
    base: i32,
    name: &str,
    start_byte: usize,
    end_byte: usize,
) {
    let start_char = base + text[..start_byte].chars().count() as i32;
    let end_char = base + text[..end_byte].chars().count() as i32;
    let si = buffer.iter_at_offset(start_char);
    let ei = buffer.iter_at_offset(end_char);
    buffer.apply_tag_by_name(name, &si, &ei);
}

// ── Smart list continuation ───────────────────────────────────────────────────

/// Called on the Enter key press event, *before* the newline is inserted.
///
/// Returns `true` if the key was handled (suppresses the default Enter), or
/// `false` to let GTK handle it normally.
pub fn handle_enter_key(buffer: &gtk::TextBuffer) -> bool {
    let cursor = buffer.iter_at_mark(&buffer.get_insert());

    // Text from the start of the current line up to the cursor
    let mut line_start = cursor.clone();
    line_start.set_line_offset(0);
    let line_text = buffer.text(&line_start, &cursor, false).to_string();

    // ── Bullet list ──────────────────────────────────────────────────────
    if let Some((marker, continuation, indent)) = detect_bullet(&line_text) {
        let content = &line_text[indent + marker.len()..];
        buffer.begin_user_action();
        if content.trim().is_empty() {
            // Empty item → exit the list: delete the marker, insert plain newline
            let mut ls = line_start.clone();
            let mut le = cursor.clone();
            buffer.delete(&mut ls, &mut le);
            buffer.insert_at_cursor("\n");
        } else {
            let spaces = &line_text[..indent];
            let next = format!("\n{}{}", spaces, continuation);
            buffer.insert_at_cursor(&next);
        }
        buffer.end_user_action();
        return true;
    }

    // ── Ordered list ─────────────────────────────────────────────────────
    if let Some((num, sep, indent)) = detect_ordered(&line_text) {
        let marker_len = format!("{}{} ", num, sep).len();
        let content = &line_text[indent + marker_len..];
        buffer.begin_user_action();
        if content.trim().is_empty() {
            let mut ls = line_start.clone();
            let mut le = cursor.clone();
            buffer.delete(&mut ls, &mut le);
            buffer.insert_at_cursor("\n");
        } else {
            let spaces = &line_text[..indent];
            let next_num = num.parse::<u64>().unwrap_or(1).saturating_add(1);
            let next = format!("\n{}{}{} ", spaces, next_num, sep);
            buffer.insert_at_cursor(&next);
        }
        buffer.end_user_action();
        return true;
    }

    false
}

/// Returns `(marker, continuation, indent_bytes)` or `None`.
fn detect_bullet(line: &str) -> Option<(&'static str, &'static str, usize)> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = &line[indent..];
    for &(marker, continuation) in BULLET_MARKERS {
        if trimmed.starts_with(marker) {
            return Some((marker, continuation, indent));
        }
    }
    None
}

/// Returns `(number_str, separator_char, indent_bytes)` or `None`.
/// Supports "1. " and "1) " styles.
fn detect_ordered(line: &str) -> Option<(String, char, usize)> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = &line[indent..];

    // Find digit prefix
    let digit_end = trimmed
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(i, c)| i + c.len_utf8())?;

    if digit_end == 0 {
        return None;
    }

    let after_digits = &trimmed[digit_end..];
    let sep = after_digits.chars().next()?;
    if sep != '.' && sep != ')' {
        return None;
    }

    // Must be followed by a space
    let after_sep = &after_digits[sep.len_utf8()..];
    if !after_sep.starts_with(' ') {
        return None;
    }

    let num = trimmed[..digit_end].to_string();
    Some((num, sep, indent))
}

// ── Image embedding ───────────────────────────────────────────────────────────

/// Scan `text` for `![alt](path)` patterns.
/// Returns `(start_byte, end_byte, path)` sorted end→start so callers can
/// process from the end of the string without offset drift.
pub fn find_images(text: &str) -> Vec<(usize, usize, String)> {
    let mut results = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;

    while i + 1 < bytes.len() {
        // Look for `![`
        if bytes[i] != b'!' || bytes[i + 1] != b'[' {
            i += 1;
            continue;
        }
        let img_start = i;
        i += 2; // skip `![`

        // Scan for matching `]`
        let mut depth = 1usize;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'[' => depth += 1,
                b']' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        if depth != 0 || i >= bytes.len() {
            continue;
        }

        // Must be followed by `(`
        if bytes[i] != b'(' {
            continue;
        }
        i += 1; // skip `(`

        // Scan for matching `)`
        let path_start = i;
        depth = 1;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        if depth != 0 {
            continue;
        }

        // i is now just past the closing `)`
        let path_end = i - 1; // byte index of `)` was i-1 before the final i+=1
        let img_end = i;

        let raw_path = &text[path_start..path_end];
        // Strip optional title: `image.png "My title"` → take first word
        let path = raw_path
            .split_whitespace()
            .next()
            .unwrap_or(raw_path)
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();

        if !path.is_empty() {
            results.push((img_start, img_end, path));
        }
    }

    // Sort end → start so inserting anchors doesn't shift earlier positions
    results.sort_by(|a, b| b.0.cmp(&a.0));
    results
}

/// Embed local images found in the buffer as inline child-anchor Picture widgets.
///
/// 1. Deletes any existing image anchors (recorded in `anchors`).
/// 2. Re-scans the clean buffer text for `![alt](path)` patterns.
/// 3. For each local file found, inserts a `TextChildAnchor` immediately after
///    the closing `)` and attaches a `gtk::Picture` at that anchor.
///
/// `anchors` is mutated in-place — caller should hold the RefMut lock open
/// for the duration of this call.
pub fn embed_images(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    anchors: &mut Vec<gtk::TextChildAnchor>,
) {
    // ── Step 1: remove old anchors ───────────────────────────────────────
    for anchor in anchors.drain(..) {
        if !anchor.is_deleted() {
            let mut ai = buffer.iter_at_child_anchor(&anchor);
            let mut ae = ai.clone();
            ae.forward_char();
            buffer.delete(&mut ai, &mut ae);
        }
    }

    // ── Step 2: get clean text ───────────────────────────────────────────
    let buf_start = buffer.start_iter();
    let buf_end = buffer.end_iter();
    let text = buffer.text(&buf_start, &buf_end, false).to_string();

    if text.is_empty() {
        return;
    }

    // Skip the title line
    let content_byte_start = text.find('\n').map(|p| p + 1).unwrap_or(text.len());
    if content_byte_start >= text.len() {
        return;
    }
    let content = &text[content_byte_start..];
    let base_char = text[..content_byte_start].chars().count() as i32;

    // ── Step 3: find and embed images (end → start) ──────────────────────
    for (_start_byte, end_byte, path) in find_images(content) {
        let abs_path = expand_path(&path);
        if !abs_path.exists() {
            continue;
        }

        // Char offset in the buffer for the end of the image markdown
        let end_char = base_char + content[..end_byte].chars().count() as i32;

        let mut insert_at = buffer.iter_at_offset(end_char);
        let anchor = buffer.create_child_anchor(&mut insert_at);

        let picture = gtk::Picture::for_filename(abs_path);
        picture.set_size_request(400, -1);
        picture.set_content_fit(gtk::ContentFit::ScaleDown);

        view.add_child_at_anchor(&picture, &anchor);
        anchors.push(anchor);
    }
}

fn expand_path(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}
