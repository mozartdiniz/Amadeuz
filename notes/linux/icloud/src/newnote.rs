//! Building the body (`TextData`) of a brand-new note.
//!
//! Mirrors what Apple Notes writes for a freshly created note (see
//! `ICLOUD_NOTES_PROTOCOL.md`): a versioned document whose single version holds a
//! topotext CRDT string with three substrings (start sentinel, the text inserted
//! by one replica, end sentinel), a one-replica vector timestamp and one attribute
//! run. A new note has no history, so there is nothing for Apple devices to
//! merge against; later edits on those devices continue from this state.

use std::io::Write;

use prost::Message;
use uuid::Uuid;

/// Clock value Apple uses for the end sentinel.
const END_CLOCK: u32 = u32::MAX;

#[derive(Clone, PartialEq, Message)]
struct Document {
    #[prost(uint32, optional, tag = "1")]
    serialization_version: Option<u32>,
    #[prost(message, repeated, tag = "2")]
    versions: Vec<Version>,
}

#[derive(Clone, PartialEq, Message)]
struct Version {
    #[prost(uint32, optional, tag = "1")]
    serialization_version: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    minimum_supported_version: Option<u32>,
    #[prost(bytes = "vec", optional, tag = "3")]
    data: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
struct TopoString {
    #[prost(string, optional, tag = "2")]
    string: Option<String>,
    #[prost(message, repeated, tag = "3")]
    substrings: Vec<Substring>,
    #[prost(message, optional, tag = "4")]
    timestamp: Option<VectorTimestamp>,
    #[prost(message, repeated, tag = "5")]
    attribute_runs: Vec<AttributeRun>,
}

#[derive(Clone, PartialEq, Message)]
struct Substring {
    #[prost(message, optional, tag = "1")]
    char_id: Option<CharId>,
    #[prost(uint32, optional, tag = "2")]
    length: Option<u32>,
    #[prost(message, optional, tag = "3")]
    timestamp: Option<CharId>,
    #[prost(bool, optional, tag = "4")]
    tombstone: Option<bool>,
    #[prost(uint32, repeated, packed = "false", tag = "5")]
    children: Vec<u32>,
}

#[derive(Clone, PartialEq, Message)]
struct CharId {
    #[prost(uint32, optional, tag = "1")]
    replica: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    clock: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
struct VectorTimestamp {
    #[prost(message, repeated, tag = "1")]
    clocks: Vec<Clock>,
}

#[derive(Clone, PartialEq, Message)]
struct Clock {
    #[prost(bytes = "vec", optional, tag = "1")]
    replica_uuid: Option<Vec<u8>>,
    /// [text clock, style clock]
    #[prost(message, repeated, tag = "2")]
    replica_clocks: Vec<ReplicaClock>,
}

#[derive(Clone, PartialEq, Message)]
struct ReplicaClock {
    #[prost(uint32, optional, tag = "1")]
    clock: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
struct AttributeRun {
    #[prost(uint32, optional, tag = "1")]
    length: Option<u32>,
    #[prost(message, optional, tag = "2")]
    paragraph_style: Option<ParagraphStyle>,
    #[prost(uint64, optional, tag = "13")]
    timestamp: Option<u64>,
}

#[derive(Clone, PartialEq, Message)]
struct ParagraphStyle {
    #[prost(int32, optional, tag = "2")]
    alignment: Option<i32>,
    #[prost(int32, optional, tag = "3")]
    writing_direction: Option<i32>,
    #[prost(bytes = "vec", optional, tag = "9")]
    uuid: Option<Vec<u8>>,
}

fn char_id(replica: u32, clock: u32) -> Option<CharId> {
    Some(CharId { replica: Some(replica), clock: Some(clock) })
}

/// Uncompressed body for a new note containing `text`, written by `replica`.
/// `paragraph` identifies the paragraph style; `unix_secs` stamps the attribute run.
pub fn body_proto(text: &str, replica: Uuid, paragraph: Uuid, unix_secs: u64) -> Vec<u8> {
    // NSString lengths: UTF-16 code units. The text clock advances one per unit.
    let len = text.encode_utf16().count() as u32;

    let mut substrings = vec![Substring {
        char_id: char_id(0, 0),
        length: Some(0),
        timestamp: char_id(0, 0),
        tombstone: None,
        children: vec![1],
    }];
    // An empty note has no text substring: the start sentinel's child (index 1)
    // is then the end sentinel.
    if len > 0 {
        substrings.push(Substring {
            char_id: char_id(1, 0),
            length: Some(len),
            timestamp: char_id(1, 0),
            tombstone: None,
            children: vec![2],
        });
    }
    substrings.push(Substring {
        char_id: char_id(0, END_CLOCK),
        length: Some(0),
        timestamp: char_id(0, END_CLOCK),
        tombstone: None,
        children: vec![],
    });

    let string = TopoString {
        string: Some(text.to_string()),
        substrings,
        timestamp: Some(VectorTimestamp {
            clocks: vec![Clock {
                replica_uuid: Some(replica.as_bytes().to_vec()),
                replica_clocks: vec![ReplicaClock { clock: Some(len) }, ReplicaClock { clock: Some(1) }],
            }],
        }),
        attribute_runs: if len > 0 {
            vec![AttributeRun {
                length: Some(len),
                paragraph_style: Some(ParagraphStyle {
                    alignment: Some(0),
                    writing_direction: Some(1),
                    uuid: Some(paragraph.as_bytes().to_vec()),
                }),
                timestamp: Some(unix_secs),
            }]
        } else {
            vec![]
        },
    };

    Document {
        serialization_version: Some(0),
        versions: vec![Version {
            serialization_version: Some(0),
            minimum_supported_version: Some(0),
            data: Some(string.encode_to_vec()),
        }],
    }
    .encode_to_vec()
}

/// Gzip-compressed body, ready for `TextDataEncrypted`.
pub fn body(text: &str, replica: Uuid, unix_secs: u64) -> Vec<u8> {
    let raw = body_proto(text, replica, Uuid::new_v4(), unix_secs);
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&raw).expect("in-memory gzip");
    enc.finish().expect("in-memory gzip")
}

/// Title and snippet as Apple derives them: first non-empty line, and the
/// first non-empty line after it.
pub fn title_and_snippet(text: &str) -> (String, String) {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let title = lines.next().unwrap_or("").to_string();
    let snippet = lines.next().unwrap_or("").to_string();
    (title, snippet)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{notebody, pbdump};

    /// Body of the note "Probe Title / Hello world" created in Apple Notes,
    /// as printed by `probe --dump` (2026-10-12), with the device's replica UUID
    /// and the paragraph UUID replaced by made-up ones. Our encoder, given the
    /// same UUIDs and timestamp, must produce the same tree.
    const APPLE_PROBE_NOTE: &str = r#"1: 0
2: {
  1: 0
  2: 0
  3: {
    2: "Probe Title\nHello world"
    3: {
      1: {
        1: 0
        2: 0
      }
      2: 0
      3: {
        1: 0
        2: 0
      }
      5: 1
    }
    3: {
      1: {
        1: 1
        2: 0
      }
      2: 23
      3: {
        1: 1
        2: 0
      }
      5: 2
    }
    3: {
      1: {
        1: 0
        2: 4294967295
      }
      2: 0
      3: {
        1: 0
        2: 4294967295
      }
    }
    4: {
      1: {
        1: hex 11111111222243338444555555555555
        2: {
          1: 23
        }
        2: {
          1: 1
        }
      }
    }
    5: {
      1: 23
      2: {
        2: 0
        3: 1
        9: hex aaaaaaaabbbb4ccc8dddeeeeeeeeeeee
      }
      13: 1791823123
    }
  }
}
"#;

    #[test]
    fn matches_note_created_by_apple() {
        let raw = body_proto(
            "Probe Title\nHello world",
            Uuid::parse_str("11111111-2222-4333-8444-555555555555").unwrap(),
            Uuid::parse_str("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee").unwrap(),
            1791823123,
        );
        let mut tree = String::new();
        pbdump::tree(&raw, 0, &mut tree);
        assert_eq!(tree, APPLE_PROBE_NOTE);
    }

    #[test]
    fn our_reader_reads_it_back() {
        let text = "Grocery list\nmilk 🥛\neggs";
        let note = notebody::decode(&body(text, Uuid::new_v4(), 0)).unwrap();
        assert_eq!(notebody::to_plain_text(&note), text);
    }

    #[test]
    fn title_and_snippet_skip_blank_lines() {
        assert_eq!(
            title_and_snippet("\n  Title \n\n second\nthird"),
            ("Title".into(), "second".into())
        );
    }
}
