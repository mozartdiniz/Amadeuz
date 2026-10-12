//! Decoding Apple Notes tables (`com.apple.notes.table` attachments).
//!
//! A table is stored in the attachment's `MergeableData` as a CRDT object graph
//! (`MergableDataProto`): a flat list of objects plus string tables for keys,
//! types and UUIDs. The root object has type `com.apple.notes.ICTable` and a map
//! with `crRows` / `crColumns` (ordered sets of UUIDs) and `cellColumns`
//! (column UUID → row UUID → cell note). Field numbers follow the community
//! `notestore.proto` used by apple_cloud_notes_parser.

use std::collections::HashMap;

use prost::Message;

use crate::notebody::{self, Note};
use crate::{Error, Result};

#[derive(Clone, PartialEq, Message)]
struct MergableDataProto {
    #[prost(message, optional, tag = "2")]
    object: Option<MergableDataObject>,
}

#[derive(Clone, PartialEq, Message)]
struct MergableDataObject {
    #[prost(message, optional, tag = "3")]
    data: Option<ObjectData>,
}

#[derive(Clone, PartialEq, Message)]
struct ObjectData {
    #[prost(message, repeated, tag = "3")]
    entries: Vec<Entry>,
    #[prost(string, repeated, tag = "4")]
    key_items: Vec<String>,
    #[prost(string, repeated, tag = "5")]
    type_items: Vec<String>,
    #[prost(bytes = "vec", repeated, tag = "6")]
    uuid_items: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
struct Entry {
    #[prost(message, optional, tag = "6")]
    dictionary: Option<Dictionary>,
    #[prost(message, optional, tag = "10")]
    note: Option<Note>,
    #[prost(message, optional, tag = "13")]
    custom_map: Option<CustomMap>,
    #[prost(message, optional, tag = "16")]
    ordered_set: Option<OrderedSet>,
}

#[derive(Clone, PartialEq, Message)]
struct ObjectId {
    #[prost(uint64, optional, tag = "2")]
    unsigned_integer_value: Option<u64>,
    #[prost(string, optional, tag = "4")]
    string_value: Option<String>,
    #[prost(int32, optional, tag = "6")]
    object_index: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
struct Dictionary {
    #[prost(message, repeated, tag = "1")]
    elements: Vec<DictionaryElement>,
}

#[derive(Clone, PartialEq, Message)]
struct DictionaryElement {
    #[prost(message, optional, tag = "1")]
    key: Option<ObjectId>,
    #[prost(message, optional, tag = "2")]
    value: Option<ObjectId>,
}

#[derive(Clone, PartialEq, Message)]
struct CustomMap {
    #[prost(int32, optional, tag = "1")]
    type_index: Option<i32>,
    #[prost(message, repeated, tag = "3")]
    entries: Vec<MapEntry>,
}

#[derive(Clone, PartialEq, Message)]
struct MapEntry {
    #[prost(int32, optional, tag = "1")]
    key: Option<i32>,
    #[prost(message, optional, tag = "2")]
    value: Option<ObjectId>,
}

#[derive(Clone, PartialEq, Message)]
struct OrderedSet {
    #[prost(message, optional, tag = "1")]
    ordering: Option<OrderedSetOrdering>,
}

#[derive(Clone, PartialEq, Message)]
struct OrderedSetOrdering {
    #[prost(message, optional, tag = "1")]
    array: Option<OrderingArray>,
    #[prost(message, optional, tag = "2")]
    contents: Option<Dictionary>,
}

#[derive(Clone, PartialEq, Message)]
struct OrderingArray {
    #[prost(message, repeated, tag = "2")]
    attachments: Vec<OrderingAttachment>,
}

#[derive(Clone, PartialEq, Message)]
struct OrderingAttachment {
    #[prost(bytes = "vec", optional, tag = "2")]
    uuid: Option<Vec<u8>>,
}

/// Decoded table: `rows[r][c]` is the plain text of each cell.
pub type Table = Vec<Vec<String>>;

/// Decompress and decode a table attachment's `MergeableData`.
pub fn decode(blob: &[u8]) -> Result<Table> {
    let raw = notebody::decompress(blob)?;
    let proto = MergableDataProto::decode(raw.as_slice())
        .map_err(|e| Error::Protocol(format!("table is not MergableDataProto: {e}")))?;
    let data = proto
        .object
        .and_then(|o| o.data)
        .ok_or_else(|| Error::Protocol("table has no object data".into()))?;
    Graph { d: &data }.table()
}

struct Graph<'a> {
    d: &'a ObjectData,
}

impl<'a> Graph<'a> {
    fn entry(&self, id: &Option<ObjectId>) -> Option<&'a Entry> {
        let idx = id.as_ref()?.object_index?;
        self.d.entries.get(usize::try_from(idx).ok()?)
    }

    fn key(&self, i: Option<i32>) -> Option<&'a str> {
        self.d.key_items.get(usize::try_from(i?).ok()?).map(String::as_str)
    }

    fn type_name(&self, e: &Entry) -> Option<&'a str> {
        let t = e.custom_map.as_ref()?.type_index?;
        self.d.type_items.get(usize::try_from(t).ok()?).map(String::as_str)
    }

    /// A UUID-reference object: a custom map whose first entry points into `uuid_items`.
    fn uuid_of(&self, e: Option<&'a Entry>) -> Option<&'a [u8]> {
        let v = e?.custom_map.as_ref()?.entries.first()?.value.as_ref()?.unsigned_integer_value?;
        self.d.uuid_items.get(usize::try_from(v).ok()?).map(Vec::as_slice)
    }

    fn table(&self) -> Result<Table> {
        let root = self
            .d
            .entries
            .iter()
            .find(|e| self.type_name(e) == Some("com.apple.notes.ICTable"))
            .ok_or_else(|| Error::Protocol("no ICTable object in table data".into()))?;
        let map = &root.custom_map.as_ref().expect("typed entry has a map").entries;
        let field = |name: &str| map.iter().find(|m| self.key(m.key) == Some(name)).map(|m| &m.value);

        let rows = field("crRows").map(|v| self.ordering(self.entry(v))).unwrap_or_default();
        let cols = field("crColumns").map(|v| self.ordering(self.entry(v))).unwrap_or_default();
        if rows.is_empty() || cols.is_empty() {
            return Err(Error::Protocol("table has no rows or columns".into()));
        }
        let n_rows = rows.values().max().map_or(0, |m| m + 1);
        let n_cols = cols.values().max().map_or(0, |m| m + 1);
        let mut table = vec![vec![String::new(); n_cols]; n_rows];

        let columns = field("cellColumns")
            .and_then(|v| self.entry(v))
            .and_then(|e| e.dictionary.as_ref())
            .ok_or_else(|| Error::Protocol("table has no cellColumns".into()))?;
        for column in &columns.elements {
            let Some(c) = self.uuid_of(self.entry(&column.key)).and_then(|u| cols.get(u)) else { continue };
            let Some(cells) = self.entry(&column.value).and_then(|e| e.dictionary.as_ref()) else { continue };
            for cell in &cells.elements {
                let Some(r) = self.uuid_of(self.entry(&cell.key)).and_then(|u| rows.get(u)) else { continue };
                if let Some(note) = self.entry(&cell.value).and_then(|e| e.note.as_ref()) {
                    table[*r][*c] = notebody::to_plain_text(note).trim_end().to_string();
                }
            }
        }
        Ok(table)
    }

    /// Map each row/column UUID to its display position. The ordered set lists
    /// positions by UUID, and its `contents` maps those to the UUIDs the
    /// cells use.
    fn ordering(&self, e: Option<&'a Entry>) -> HashMap<&'a [u8], usize> {
        let mut out = HashMap::new();
        let Some(ordering) = e.and_then(|e| e.ordered_set.as_ref()).and_then(|o| o.ordering.as_ref()) else {
            return out;
        };
        if let Some(array) = &ordering.array {
            for (i, a) in array.attachments.iter().enumerate() {
                if let Some(u) = &a.uuid {
                    out.insert(u.as_slice(), i);
                }
            }
        }
        if let Some(contents) = &ordering.contents {
            for el in &contents.elements {
                let key = self.uuid_of(self.entry(&el.key));
                let value = self.uuid_of(self.entry(&el.value));
                if let (Some(k), Some(v)) = (key, value) {
                    if let Some(&pos) = out.get(k) {
                        out.insert(v, pos);
                    }
                }
            }
        }
        out
    }
}

/// Render as a Markdown table, first row as the header.
pub fn to_markdown(table: &Table) -> String {
    let esc = |s: &str| s.replace('|', "\\|").replace('\n', " ");
    let mut lines = Vec::new();
    for (i, row) in table.iter().enumerate() {
        lines.push(format!("| {} |", row.iter().map(|c| esc(c)).collect::<Vec<_>>().join(" | ")));
        if i == 0 {
            lines.push(format!("|{}|", vec!["---"; row.len()].join("|")));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_markdown_table() {
        let t = vec![vec!["a".into(), "b|c".into()], vec!["1".into(), "2\n3".into()]];
        assert_eq!(to_markdown(&t), "| a | b\\|c |\n|---|---|\n| 1 | 2 3 |");
    }
}
