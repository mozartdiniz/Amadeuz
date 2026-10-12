//! Generic protobuf pretty-printer (field numbers and values, no schema).
//! Used by the probe's `--dump` and by tests that compare encoder output
//! against dumps of notes created by Apple devices.

use std::fmt::Write;
use std::io::Read;

/// Describe bytes: text if printable, gzip → protobuf tree, protobuf tree, or hex.
/// The first line has no indentation; nested lines are indented from `depth`.
pub fn describe(bytes: &[u8], depth: usize) -> String {
    let mut out = String::new();
    if let [0x1f, 0x8b, ..] = bytes {
        let mut raw = Vec::new();
        if flate2::read::GzDecoder::new(bytes).read_to_end(&mut raw).is_ok() {
            let _ = writeln!(out, "gzip → {} bytes", raw.len());
            tree(&raw, depth, &mut out);
            return out;
        }
    }
    if let Some(s) = as_text(bytes) {
        let _ = writeln!(out, "{s:?}");
    } else if parse(bytes).is_some() {
        let _ = writeln!(out, "protobuf");
        tree(bytes, depth, &mut out);
    } else {
        let _ = writeln!(out, "hex {}", hex(bytes));
    }
    out
}

/// Indented tree of a protobuf message, one field per line.
pub fn tree(bytes: &[u8], depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let Some(fields) = parse(bytes) else {
        let _ = writeln!(out, "{pad}hex {}", hex(bytes));
        return;
    };
    for (num, val) in fields {
        match val {
            Val::Varint(v) => {
                let _ = writeln!(out, "{pad}{num}: {v}");
            }
            Val::Fixed64(v) => {
                let _ = writeln!(out, "{pad}{num}: fixed64 {v:#x}");
            }
            Val::Fixed32(v) => {
                let _ = writeln!(out, "{pad}{num}: fixed32 {v:#x}");
            }
            Val::Bytes(b) => {
                if let Some(s) = as_text(b) {
                    let _ = writeln!(out, "{pad}{num}: {s:?}");
                } else if depth < 12 && !b.is_empty() && parse(b).is_some() {
                    let _ = writeln!(out, "{pad}{num}: {{");
                    tree(b, depth + 1, out);
                    let _ = writeln!(out, "{pad}}}");
                } else {
                    let _ = writeln!(out, "{pad}{num}: hex {}", hex(b));
                }
            }
        }
    }
}

enum Val<'a> {
    Varint(u64),
    Fixed64(u64),
    Fixed32(u32),
    Bytes(&'a [u8]),
}

/// Parse protobuf wire format; None unless it parses cleanly to the end.
fn parse(mut b: &[u8]) -> Option<Vec<(u64, Val<'_>)>> {
    fn varint(b: &mut &[u8]) -> Option<u64> {
        let mut v = 0u64;
        for i in 0..10 {
            let (&byte, rest) = b.split_first()?;
            *b = rest;
            v |= u64::from(byte & 0x7f) << (7 * i);
            if byte & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }
    let mut out = Vec::new();
    while !b.is_empty() {
        let key = varint(&mut b)?;
        let (num, wt) = (key >> 3, key & 7);
        if num == 0 || num > 10_000 {
            return None;
        }
        let val = match wt {
            0 => Val::Varint(varint(&mut b)?),
            1 => {
                let (x, rest) = b.split_at_checked(8)?;
                b = rest;
                Val::Fixed64(u64::from_le_bytes(x.try_into().ok()?))
            }
            2 => {
                let len = usize::try_from(varint(&mut b)?).ok()?;
                let (x, rest) = b.split_at_checked(len)?;
                b = rest;
                Val::Bytes(x)
            }
            5 => {
                let (x, rest) = b.split_at_checked(4)?;
                b = rest;
                Val::Fixed32(u32::from_le_bytes(x.try_into().ok()?))
            }
            _ => return None,
        };
        out.push((num, val));
    }
    Some(out)
}

fn as_text(b: &[u8]) -> Option<&str> {
    let s = std::str::from_utf8(b).ok()?;
    let printable = s.chars().all(|c| !c.is_control() || c == '\n' || c == '\t' || c == '\u{FFFC}');
    (printable && !s.is_empty()).then_some(s)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
