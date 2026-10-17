//! The entry format (R8, R9): every cache file, whatever its kind.
//!
//! | Bytes          | What                                                             |
//! |----------------|------------------------------------------------------------------|
//! | 4              | magic `CRFT`                                                     |
//! | 1              | header version (1)                                               |
//! | 1 + n (n ≤ 32) | kind name, length-prefixed (`thumbs`, `analysis`, …)             |
//! | 2              | the kind's format version, little-endian                         |
//! | 37             | the source's identity ([`SourceId`])                             |
//! | 8              | payload length, little-endian                                    |
//! | payload        | the kind's bytes (a PNG, JSON, …)                                |
//! | 8              | checksum: SipHash-1-3 (the suite keys) of everything before it   |
//!
//! [`decode`] checks every field and returns `None` for anything else; it never panics.

use crate::hash::Hasher128;
use crate::source::{SOURCE_BYTES, SourceId};

/// File magic.
pub const MAGIC: &[u8; 4] = b"CRFT";
/// Header version.
pub const HEADER_VERSION: u8 = 1;
/// Longest kind name.
pub const MAX_KIND: usize = 32;

/// Whether `kind` is a usable kind name: 1–32 of `a-z`, `0-9`, `-`.
pub fn valid_kind(kind: &str) -> bool {
    (1..=MAX_KIND).contains(&kind.len())
        && kind.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// An entry's bytes.
pub fn encode(kind: &str, version: u16, source: &SourceId, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 64 + kind.len());
    out.extend_from_slice(MAGIC);
    out.push(HEADER_VERSION);
    out.push(kind.len().min(MAX_KIND) as u8);
    out.extend_from_slice(&kind.as_bytes()[..kind.len().min(MAX_KIND)]);
    out.extend_from_slice(&version.to_le_bytes());
    source.encode(&mut out);
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(payload);
    let sum = Hasher128::new().bytes(&out).finish64();
    out.extend_from_slice(&sum.to_le_bytes());
    out
}

/// The source and payload of an entry of `kind` at `version`; `None` for anything else.
pub fn decode<'a>(bytes: &'a [u8], kind: &str, version: u16) -> Option<(SourceId, &'a [u8])> {
    let body_len = bytes.len().checked_sub(8)?;
    let (body, sum) = bytes.split_at(body_len);
    if Hasher128::new().bytes(body).finish64() != u64::from_le_bytes(sum.try_into().ok()?) {
        return None;
    }
    let rest = body.strip_prefix(MAGIC.as_slice())?;
    let (&hv, rest) = rest.split_first()?;
    if hv != HEADER_VERSION {
        return None;
    }
    let (&klen, rest) = rest.split_first()?;
    let (k, rest) = rest.split_at_checked(usize::from(klen))?;
    if k != kind.as_bytes() {
        return None;
    }
    let (v, rest) = rest.split_at_checked(2)?;
    if u16::from_le_bytes(v.try_into().ok()?) != version {
        return None;
    }
    let (src, rest) = rest.split_at_checked(SOURCE_BYTES)?;
    let source = SourceId::decode(src)?;
    let (len, payload) = rest.split_at_checked(8)?;
    let len = usize::try_from(u64::from_le_bytes(len.try_into().ok()?)).ok()?;
    (payload.len() == len).then_some((source, payload))
}
