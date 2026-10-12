//! Editing an existing note body (topotext CRDT) the way Apple Notes does.
//!
//! The body is kept at the protobuf wire level so fields amadeuz doesn't model
//! survive untouched. Only the substring list (`String.3`), the visible text
//! (`String.2`), the vector timestamp (`String.4`) and attribute run lengths
//! (`String.5`) are rewritten. Edits follow `-[ICTTMergeableString
//! insertAttributedString:after:before:]` and `generateIdsForLocalChanges`:
//!
//! - inserted text is a new substring with charIDs from this replica's text
//!   clock and timestamp (replica, 0), spliced between its neighbours;
//! - deleted text is split out and tombstoned with a fresh Lamport timestamp
//!   from the style clock;
//! - replicas are numbered 1..n by sorted UUID when serialising (0 = zero replica).
//!
//! Anything unexpected makes parsing fail, so callers refuse the edit rather
//! than risk damaging a note.

use std::io::Write;

use uuid::Uuid;

use crate::notebody::{self, InlineFormat, InlineKind, ParaStyle, ParagraphFormat};
use crate::wire::{Message, Value};
use crate::{Error, Result};

const END_CLOCK: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq)]
struct Node {
    /// None = the zero replica (sentinels).
    replica: Option<Uuid>,
    clock: u32,
    len: u32,
    ts_replica: Option<Uuid>,
    ts_clock: u32,
    tombstone: bool,
    /// Node ids (arena indices).
    children: Vec<usize>,
}

/// An editable note body.
pub struct Doc {
    document: Message,
    version: Message,
    string: Message,
    nodes: Vec<Node>,
    /// Node ids in document order.
    order: Vec<usize>,
    /// Vector timestamp clocks by replica, raw (unknown fields preserved).
    clocks: Vec<(Uuid, Message)>,
    runs: Vec<Message>,
    /// Visible text as UTF-16 code units.
    text: Vec<u16>,
}

/// Parse one protobuf layer, naming it in the error.
fn layer(b: &[u8], name: &str) -> Result<Message> {
    Message::parse(b).map_err(|e| refuse(&format!("{name}: {e}")))
}

fn refuse(why: &str) -> Error {
    Error::Protocol(format!("note body can't be edited safely: {why}"))
}

impl Doc {
    /// Parse a `TextData` blob (gzip or raw).
    pub fn parse(blob: &[u8]) -> Result<Self> {
        // Same decompression as the reader (gzip, zlib or none).
        let raw = notebody::decompress(blob)?;
        let document = layer(&raw, "document")?;
        let versions: Vec<&[u8]> = document.all_bytes(2).collect();
        if versions.len() != 1 {
            return Err(refuse("expected exactly one document version"));
        }
        let version = layer(versions[0], "version")?;
        let string = layer(version.bytes(3).ok_or_else(|| refuse("version has no data"))?, "text")?;

        // Vector timestamp → replica numbering (1..n by sorted UUID).
        let ts = layer(string.bytes(4).ok_or_else(|| refuse("no vector timestamp (shared note?)"))?, "clock")?;
        let mut clocks = Vec::new();
        for c in ts.all_bytes(1) {
            let c = layer(c, "clock entry")?;
            let uuid = Uuid::from_slice(c.bytes(1).ok_or_else(|| refuse("clock without replica UUID"))?)
                .map_err(|_| refuse("bad replica UUID"))?;
            clocks.push((uuid, c));
        }
        if ts.field_numbers().any(|n| n != 1) {
            return Err(refuse("unknown vector timestamp fields"));
        }
        let mut sorted: Vec<Uuid> = clocks.iter().map(|(u, _)| *u).collect();
        sorted.sort();
        let replica_of = |i: u64| -> Result<Option<Uuid>> {
            match i {
                0 => Ok(None),
                i => sorted.get(i as usize - 1).copied().map(Some).ok_or_else(|| refuse("replica index out of range")),
            }
        };

        let mut nodes = Vec::new();
        for s in string.all_bytes(3) {
            let s = layer(s, "text piece")?;
            if s.field_numbers().any(|n| !(1..=5).contains(&n)) {
                return Err(refuse("substring has unknown fields"));
            }
            let id = layer(s.bytes(1).ok_or_else(|| refuse("substring without charID"))?, "text piece id")?;
            let tsid = layer(s.bytes(3).ok_or_else(|| refuse("substring without timestamp"))?, "text piece timestamp")?;
            nodes.push(Node {
                replica: replica_of(id.varint(1).unwrap_or(0))?,
                clock: id.varint(2).unwrap_or(0) as u32,
                len: s.varint(2).unwrap_or(0) as u32,
                ts_replica: replica_of(tsid.varint(1).unwrap_or(0))?,
                ts_clock: tsid.varint(2).unwrap_or(0) as u32,
                tombstone: s.varint(4).unwrap_or(0) != 0,
                children: s.all_varints(5).map(|c| c as usize).collect(),
            });
        }
        if nodes.iter().flat_map(|n| &n.children).any(|&c| c >= nodes.len()) {
            return Err(refuse("child index out of range"));
        }
        let first_is_start = nodes.first().is_some_and(|n| n.replica.is_none() && n.clock == 0);
        let last_is_end = nodes.last().is_some_and(|n| n.replica.is_none() && n.clock == END_CLOCK);
        if !first_is_start || !last_is_end {
            return Err(refuse("missing start/end sentinel"));
        }

        let text: Vec<u16> = string.bytes(2).map(|b| String::from_utf8_lossy(b).encode_utf16().collect()).unwrap_or_default();
        let visible: u32 = nodes.iter().filter(|n| !n.tombstone).map(|n| n.len).sum();
        if visible as usize != text.len() {
            return Err(refuse("substring lengths don't match the text"));
        }
        let runs: Vec<Message> = string.all_bytes(5).map(|r| layer(r, "formatting run")).collect::<Result<_>>()?;
        let run_total: u64 = runs.iter().map(|r| r.varint(1).unwrap_or(0)).sum();
        if !runs.is_empty() && run_total as usize != text.len() {
            return Err(refuse("attribute runs don't cover the text"));
        }

        Ok(Doc {
            order: (0..nodes.len()).collect(),
            nodes,
            document,
            version,
            string,
            clocks,
            runs,
            text,
        })
    }

    pub fn text(&self) -> String {
        String::from_utf16_lossy(&self.text)
    }

    /// Replace `delete` UTF-16 units at `start` with `insert`, as replica `me`.
    pub fn replace(&mut self, me: Uuid, start: usize, delete: usize, insert: &str) -> Result<()> {
        if start + delete > self.text.len() {
            return Err(refuse("edit outside the text"));
        }
        let mut lamport = self.next_timestamp(me);
        if delete > 0 {
            self.split_at(start + delete);
            self.split_at(start);
            let mut pos = 0usize;
            for &id in &self.order.clone() {
                let n = &mut self.nodes[id];
                if n.tombstone || n.len == 0 {
                    continue;
                }
                let len = n.len as usize;
                if pos >= start && pos + len <= start + delete {
                    n.tombstone = true;
                    n.ts_replica = Some(me);
                    n.ts_clock = lamport;
                    lamport += 1;
                }
                pos += len;
            }
            self.text.drain(start..start + delete);
            self.delete_from_runs(start, delete);
        }

        let units: Vec<u16> = insert.encode_utf16().collect();
        if !units.is_empty() {
            let after = self.split_at(start);
            let clock = self.text_clock(me);
            let id = self.nodes.len();
            let children = std::mem::replace(&mut self.nodes[after].children, vec![id]);
            self.nodes.push(Node {
                replica: Some(me),
                clock,
                len: units.len() as u32,
                ts_replica: Some(me),
                ts_clock: 0,
                tombstone: false,
                children,
            });
            let at = self.order.iter().position(|&x| x == after).expect("after is ordered") + 1;
            self.order.insert(at, id);
            self.text.splice(start..start, units.iter().copied());
            self.insert_into_runs(start, units.len());
            self.set_clocks(me, clock + units.len() as u32, lamport.max(1));
        } else if delete > 0 {
            let text_clock = self.text_clock(me);
            self.set_clocks(me, text_clock, lamport);
        }
        Ok(())
    }

    /// Set paragraph styles; ranges are in current-text UTF-16 offsets.
    /// Paragraphs already in the requested style are left alone.
    pub fn format_paragraph(&mut self, me: Uuid, f: &ParagraphFormat) {
        if f.len == 0 || f.start + f.len > self.text.len() {
            return;
        }
        let (a, b) = (self.split_runs_at(f.start), self.split_runs_at(f.start + f.len));
        if a == b {
            return;
        }
        let checklist_uuid = |r: &Message| -> Option<Vec<u8>> {
            let style = Message::parse(r.bytes(2)?).ok()?;
            Some(Message::parse(style.bytes(5)?).ok()?.bytes(1)?.to_vec())
        };
        // A checklist item needs its own ID; one shared with another paragraph
        // (e.g. inherited when Enter split an item) is replaced.
        let own_id = checklist_uuid(&self.runs[a])
            .filter(|id| !self.runs[..a].iter().chain(&self.runs[b..]).any(|r| checklist_uuid(r).as_ref() == Some(id)));
        let mut changed = false;
        let new_id = own_id.unwrap_or_else(|| Uuid::new_v4().as_bytes().to_vec());
        for r in &mut self.runs[a..b] {
            let mut style = r.bytes(2).and_then(|s| Message::parse(s).ok()).unwrap_or_default();
            let before = style.clone();
            match f.style.style_type() {
                Some(t) => style.set(1, Value::Varint(t as i64 as u64)),
                None => style.remove(1),
            }
            if f.indent > 0 {
                style.set(4, Value::Varint(u64::from(f.indent)));
            } else {
                style.remove(4);
            }
            match f.style {
                ParaStyle::Checklist { done } => {
                    let item = Message(vec![(1, Value::Bytes(new_id.clone())), (2, Value::Varint(u64::from(done)))]);
                    style.set(5, Value::Bytes(item.encode()));
                }
                _ => style.remove(5),
            }
            if style != before {
                r.set(2, Value::Bytes(style.encode()));
                changed = true;
            }
        }
        if changed {
            self.touch(me, f.start, f.len);
        }
    }

    /// Apply bold/italic/strikethrough; ranges in current-text UTF-16 offsets.
    pub fn format_inline(&mut self, me: Uuid, f: &InlineFormat) {
        if f.len == 0 || f.start + f.len > self.text.len() {
            return;
        }
        let (a, b) = (self.split_runs_at(f.start), self.split_runs_at(f.start + f.len));
        for r in &mut self.runs[a..b] {
            match f.kind {
                InlineKind::Strikethrough => r.set(7, Value::Varint(1)),
                kind => {
                    let add = match kind {
                        InlineKind::Bold => 1,
                        InlineKind::Italic => 2,
                        _ => 3,
                    };
                    let weight = r.varint(5).unwrap_or(0) | add;
                    r.set(5, Value::Varint(weight));
                }
            }
        }
        self.touch(me, f.start, f.len);
    }

    /// Give the substrings covering a restyled range a fresh Lamport timestamp,
    /// as `generateIdsForLocalChanges` does for attribute changes.
    fn touch(&mut self, me: Uuid, start: usize, len: usize) {
        let mut lamport = self.next_timestamp(me);
        self.split_at(start + len);
        self.split_at(start);
        let mut pos = 0usize;
        for &id in &self.order.clone() {
            let n = &mut self.nodes[id];
            if n.tombstone || n.len == 0 {
                continue;
            }
            let l = n.len as usize;
            if pos >= start && pos + l <= start + len {
                n.ts_replica = Some(me);
                n.ts_clock = lamport;
                lamport += 1;
            }
            pos += l;
        }
        let text_clock = self.text_clock(me);
        self.set_clocks(me, text_clock, lamport);
    }

    /// Make `pos` a run boundary; return the index of the run starting there.
    fn split_runs_at(&mut self, pos: usize) -> usize {
        let mut at = 0usize;
        for i in 0..self.runs.len() {
            if at == pos {
                return i;
            }
            let len = self.runs[i].varint(1).unwrap_or(0) as usize;
            if pos < at + len {
                let mut right = self.runs[i].clone();
                right.set(1, Value::Varint((at + len - pos) as u64));
                self.runs[i].set(1, Value::Varint((pos - at) as u64));
                self.runs.insert(i + 1, right);
                return i + 1;
            }
            at += len;
        }
        self.runs.len()
    }

    /// Serialise (gzip), ready for `TextDataEncrypted`.
    pub fn encode(&self) -> Vec<u8> {
        let mut sorted: Vec<Uuid> = self.clocks.iter().map(|(u, _)| *u).collect();
        sorted.sort();
        let index_of = |r: Option<Uuid>| -> u64 {
            r.and_then(|u| sorted.iter().position(|&x| x == u)).map_or(0, |i| i as u64 + 1)
        };
        let position: std::collections::HashMap<usize, usize> =
            self.order.iter().enumerate().map(|(i, &id)| (id, i)).collect();

        let char_id = |r: Option<Uuid>, c: u32| {
            Value::Bytes(Message(vec![(1, Value::Varint(index_of(r))), (2, Value::Varint(u64::from(c)))]).encode())
        };
        let substrings: Vec<Value> = self
            .order
            .iter()
            .map(|&id| {
                let n = &self.nodes[id];
                let mut m = Message(vec![
                    (1, char_id(n.replica, n.clock)),
                    (2, Value::Varint(u64::from(n.len))),
                    (3, char_id(n.ts_replica, n.ts_clock)),
                ]);
                if n.tombstone {
                    m.0.push((4, Value::Varint(1)));
                }
                for c in &n.children {
                    m.0.push((5, Value::Varint(position[c] as u64)));
                }
                Value::Bytes(m.encode())
            })
            .collect();

        let mut clocks = self.clocks.clone();
        clocks.sort_by_key(|(u, _)| *u);
        let ts = Message(clocks.into_iter().map(|(_, c)| (1, Value::Bytes(c.encode()))).collect());

        let mut string = self.string.clone();
        string.set(2, Value::Bytes(self.text().into_bytes()));
        string.replace_all(3, substrings);
        string.set(4, Value::Bytes(ts.encode()));
        string.replace_all(5, self.runs.iter().map(|r| Value::Bytes(r.encode())).collect());

        let mut version = self.version.clone();
        version.set(3, Value::Bytes(string.encode()));
        let mut document = self.document.clone();
        document.replace_all(2, vec![Value::Bytes(version.encode())]);

        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&document.encode()).expect("in-memory gzip");
        enc.finish().expect("in-memory gzip")
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Make visible position `pos` a substring boundary; return the node that
    /// ends there (the start sentinel for 0).
    fn split_at(&mut self, pos: usize) -> usize {
        let mut seen = 0usize;
        let mut last_visible = self.order[0];
        for i in 0..self.order.len() {
            let id = self.order[i];
            let n = &self.nodes[id];
            if n.tombstone || n.len == 0 {
                continue;
            }
            let len = n.len as usize;
            if pos == seen {
                return last_visible;
            }
            if pos < seen + len {
                let k = (pos - seen) as u32;
                let right = Node {
                    clock: n.clock + k,
                    len: n.len - k,
                    children: n.children.clone(),
                    ..n.clone()
                };
                let right_id = self.nodes.len();
                self.nodes.push(right);
                let left = &mut self.nodes[id];
                left.len = k;
                left.children = vec![right_id];
                self.order.insert(i + 1, right_id);
                return id;
            }
            seen += len;
            last_visible = id;
        }
        last_visible
    }

    fn text_clock(&self, me: Uuid) -> u32 {
        self.replica_clock(me, 0)
    }

    fn replica_clock(&self, me: Uuid, index: usize) -> u32 {
        self.clocks
            .iter()
            .find(|(u, _)| *u == me)
            .and_then(|(_, c)| c.all_bytes(2).nth(index))
            .and_then(|rc| Message::parse(rc).ok()?.varint(1))
            .unwrap_or(0) as u32
    }

    /// Lamport timestamp above every timestamp and style clock in the document.
    fn next_timestamp(&self, me: Uuid) -> u32 {
        let max_ts = self.nodes.iter().map(|n| n.ts_clock).filter(|&c| c != END_CLOCK).max().unwrap_or(0);
        let max_style = self.clocks.iter().map(|(u, _)| self.replica_clock(*u, 1)).max().unwrap_or(0);
        (max_ts + 1).max(max_style).max(self.replica_clock(me, 1))
    }

    fn set_clocks(&mut self, me: Uuid, text: u32, style: u32) {
        let rc = |v: u32| Value::Bytes(Message(vec![(1, Value::Varint(u64::from(v)))]).encode());
        match self.clocks.iter_mut().find(|(u, _)| *u == me) {
            Some((_, c)) => c.replace_all(2, vec![rc(text), rc(style)]),
            None => self.clocks.push((
                me,
                Message(vec![(1, Value::Bytes(me.as_bytes().to_vec())), (2, rc(text)), (2, rc(style))]),
            )),
        }
    }

    fn delete_from_runs(&mut self, start: usize, delete: usize) {
        let (mut pos, end) = (0usize, start + delete);
        for r in &mut self.runs {
            let len = r.varint(1).unwrap_or(0) as usize;
            let overlap = end.min(pos + len).saturating_sub(start.max(pos));
            if overlap > 0 {
                r.set(1, Value::Varint((len - overlap) as u64));
            }
            pos += len;
        }
        self.runs.retain(|r| r.varint(1).unwrap_or(0) > 0);
    }

    /// Grow the run the text joins: the one before the insertion point, unless
    /// that is an attachment (which must stay one character), then the one after;
    /// otherwise a new run with the neighbour's paragraph style.
    fn insert_into_runs(&mut self, start: usize, len: usize) {
        if self.runs.is_empty() {
            // First text in an empty note: the run Apple writes for new text
            // (see newnote.rs).
            let style = Message(vec![
                (2, Value::Varint(0)),
                (3, Value::Varint(1)),
                (9, Value::Bytes(Uuid::new_v4().as_bytes().to_vec())),
            ]);
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            self.runs.push(Message(vec![
                (1, Value::Varint(len as u64)),
                (2, Value::Bytes(style.encode())),
                (13, Value::Varint(secs)),
            ]));
            return;
        }
        let is_attachment = |r: &Message| r.bytes(12).is_some();
        // Inline formatting = anything besides length, paragraph style, timestamp.
        let plain = |r: &Message| r.field_numbers().all(|n| matches!(n, 1 | 2 | 13));
        let mut pos = 0usize;
        let (mut before, mut after) = (None, None);
        for (i, r) in self.runs.iter().enumerate() {
            let rlen = r.varint(1).unwrap_or(0) as usize;
            if start > pos && start <= pos + rlen {
                before = Some(i);
            }
            if start >= pos && start < pos + rlen {
                after = Some(i);
            }
            pos += rlen;
        }
        // Typing inside a run continues its formatting; at a boundary, only
        // grow a plain neighbour so text typed next to bold isn't bold.
        let inside = before.is_some() && before == after;
        let grow = if inside {
            before.filter(|&i| !is_attachment(&self.runs[i]))
        } else {
            before
                .filter(|&i| plain(&self.runs[i]))
                .or(after.filter(|&i| plain(&self.runs[i])))
        };
        if let Some(i) = grow {
            let r = &mut self.runs[i];
            let rlen = r.varint(1).unwrap_or(0);
            r.set(1, Value::Varint(rlen + len as u64));
            return;
        }
        let style = before.or(after).and_then(|i| self.runs[i].bytes(2).map(<[u8]>::to_vec));
        let mut run = Message(vec![(1, Value::Varint(len as u64))]);
        if let Some(s) = style {
            run.0.push((2, Value::Bytes(s)));
        }
        let at = match before {
            Some(i) => i + 1,
            None => after.unwrap_or(self.runs.len()),
        };
        self.runs.insert(at, run);
    }
}

/// Edit `blob` so its text becomes `new_text`, as replica `me`, using a
/// character diff so unchanged text keeps its identity and formatting.
/// Returns the new blob; fails if `blob`'s text isn't `base_text`.
pub fn edit(blob: &[u8], base_text: &str, new_text: &str, me: Uuid) -> Result<Vec<u8>> {
    edit_with_formats(blob, base_text, new_text, &[], &[], me)
}

/// [`edit`], then apply paragraph and inline formatting (offsets in `new_text`).
pub fn edit_with_formats(
    blob: &[u8],
    base_text: &str,
    new_text: &str,
    paragraphs: &[ParagraphFormat],
    inline: &[InlineFormat],
    me: Uuid,
) -> Result<Vec<u8>> {
    let mut doc = Doc::parse(blob)?;
    if doc.text() != base_text {
        return Err(Error::Conflict);
    }
    // Collect changes in old-text coordinates, then apply back to front.
    let diff = similar::TextDiff::from_chars(base_text, new_text);
    let utf16_at: Vec<usize> = std::iter::once(0)
        .chain(base_text.chars().scan(0usize, |acc, c| {
            *acc += c.len_utf16();
            Some(*acc)
        }))
        .collect();
    let mut changes = Vec::new();
    for op in diff.ops() {
        let (old, new) = (op.old_range(), op.new_range());
        if matches!(op.tag(), similar::DiffTag::Equal) {
            continue;
        }
        let inserted: String = new_text.chars().skip(new.start).take(new.len()).collect();
        changes.push((utf16_at[old.start], utf16_at[old.end] - utf16_at[old.start], inserted));
    }
    for (start, delete, insert) in changes.into_iter().rev() {
        doc.replace(me, start, delete, &insert)?;
    }
    for p in paragraphs {
        doc.format_paragraph(me, p);
    }
    for f in inline {
        doc.format_inline(me, f);
    }

    let out = doc.encode();
    // Belt and braces: the result must parse and read back as exactly new_text.
    if Doc::parse(&out)?.text() != new_text {
        return Err(refuse("edited body doesn't read back as the intended text"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{newnote, notebody};
    use std::io::Read;

    fn uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn plain(blob: &[u8]) -> String {
        notebody::decode(blob).unwrap().note_text.unwrap_or_default()
    }

    #[test]
    fn untouched_round_trip_is_lossless() {
        let blob = newnote::body("Title\nBody text", uuid(1), 5);
        let doc = Doc::parse(&blob).unwrap();
        let mut a = Vec::new();
        flate2::read::GzDecoder::new(&blob[..]).read_to_end(&mut a).unwrap();
        let mut b = Vec::new();
        flate2::read::GzDecoder::new(&doc.encode()[..]).read_to_end(&mut b).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn insert_delete_replace_by_another_replica() {
        let apple = uuid(0x1c);
        let me = uuid(0xee);
        let blob = newnote::body("Hello world", apple, 5);

        let blob = edit(&blob, "Hello world", "Hello brave world", me).unwrap();
        assert_eq!(plain(&blob), "Hello brave world");
        let blob = edit(&blob, "Hello brave world", "Hello world", me).unwrap();
        assert_eq!(plain(&blob), "Hello world");
        let blob = edit(&blob, "Hello world", "Hi world! 🌍", me).unwrap();
        assert_eq!(plain(&blob), "Hi world! 🌍");

        let doc = Doc::parse(&blob).unwrap();
        // Both replicas present; ours has advanced its text clock.
        assert_eq!(doc.clocks.len(), 2);
        assert!(doc.text_clock(me) > 0);
        // Deleted text is tombstoned, never removed.
        assert!(doc.nodes.iter().any(|n| n.tombstone));
        // Apple's charIDs survive: the first character is still (apple, 0).
        let first = doc.order.iter().map(|&i| &doc.nodes[i]).find(|n| n.replica == Some(apple)).unwrap();
        assert_eq!(first.clock, 0);
    }

    #[test]
    fn replica_numbering_follows_sorted_uuids() {
        // Our UUID sorts before Apple's: Apple's text must be renumbered to 2.
        let apple = uuid(0x80);
        let me = uuid(0x01);
        let blob = newnote::body("abc", apple, 5);
        let blob = edit(&blob, "abc", "xabc", me).unwrap();
        let doc = Doc::parse(&blob).unwrap();
        assert_eq!(doc.text(), "xabc");
        let replicas: Vec<_> = doc.order.iter().filter_map(|&i| doc.nodes[i].replica).collect();
        assert_eq!(replicas, vec![me, apple]);
    }

    #[test]
    fn edits_zlib_compressed_bodies() {
        // Older notes are zlib- rather than gzip-compressed.
        let raw = newnote::body_proto("old note", uuid(1), uuid(9), 5);
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&raw).unwrap();
        let blob = z.finish().unwrap();
        let edited = edit(&blob, "old note", "old note, edited", uuid(2)).unwrap();
        assert_eq!(plain(&edited), "old note, edited");
    }

    #[test]
    fn refuses_when_base_text_is_stale() {
        let blob = newnote::body("server text", uuid(1), 5);
        assert!(matches!(edit(&blob, "old text", "new", uuid(2)), Err(Error::Conflict)));
    }

    #[test]
    fn formats_paragraphs_and_inline_text() {
        let blob = newnote::body("Title\nmilk\neggs", uuid(1), 5);
        let p = |start, len, style| ParagraphFormat { start, len, style, indent: 0 };
        let blob = edit_with_formats(
            &blob,
            "Title\nmilk\neggs",
            "Title\nmilk\neggs",
            &[p(0, 6, ParaStyle::Heading), p(6, 5, ParaStyle::Checklist { done: false }), p(11, 4, ParaStyle::Checklist { done: true })],
            &[InlineFormat { start: 6, len: 4, kind: InlineKind::Bold }],
            uuid(2),
        )
        .unwrap();
        let note = notebody::decode(&blob).unwrap();
        assert_eq!(note.note_text.as_deref(), Some("Title\nmilk\neggs"));
        assert_eq!(notebody::to_markdown(&note), "## Title\n- [ ] **milk**\n- [x] eggs");
        // Two checklist items, two different IDs.
        let ids: Vec<_> = note
            .attribute_run
            .iter()
            .filter_map(|r| r.paragraph_style.as_ref()?.checklist.as_ref()?.uuid.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(ids.len(), 2);
        // Restyled text got fresh timestamps from replica 2.
        let doc = Doc::parse(&blob).unwrap();
        assert!(doc.nodes.iter().any(|n| n.ts_replica == Some(uuid(2)) && n.ts_clock > 0));
    }

    #[test]
    fn typing_next_to_bold_is_not_bold() {
        let blob = newnote::body("bold plain", uuid(1), 5);
        let blob = edit_with_formats(&blob, "bold plain", "bold plain", &[], &[InlineFormat { start: 0, len: 4, kind: InlineKind::Bold }], uuid(2)).unwrap();
        let blob = edit(&blob, "bold plain", "boldX plain", uuid(2)).unwrap();
        assert_eq!(notebody::to_markdown(&notebody::decode(&blob).unwrap()), "**bold**X plain");
        let blob = edit(&blob, "boldX plain", "boYldX plain", uuid(2)).unwrap();
        assert_eq!(notebody::to_markdown(&notebody::decode(&blob).unwrap()), "**boYld**X plain");
    }

    #[test]
    fn runs_keep_covering_the_text() {
        let blob = newnote::body("one\ntwo", uuid(1), 5);
        let blob = edit(&blob, "one\ntwo", "one\nthree\ntwo", uuid(2)).unwrap();
        let note = notebody::decode(&blob).unwrap();
        let total: i32 = note.attribute_run.iter().filter_map(|r| r.length).sum();
        assert_eq!(total as usize, "one\nthree\ntwo".encode_utf16().count());
    }
}
