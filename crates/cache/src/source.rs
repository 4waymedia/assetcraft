//! A source file's identity (R7, R8): size, modification time and a hash of its first and last
//! bytes. It goes into keys, and again into every entry, so a hit is only used when the file the
//! entry was made from still looks the same (a key collision, or an entry another app wrote for
//! another file, is a miss).

use crate::hash::{Hash128, Hasher128};
use std::time::{SystemTime, UNIX_EPOCH};

/// Bytes hashed from each end of a file.
pub const HASH_WINDOW: usize = 64 * 1024;

/// What a source file looks like now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceId {
    /// Size in bytes.
    pub size: u64,
    /// 0 = unknown modification time, 1 = after 1970, 2 = before 1970.
    pub mtime_tag: u8,
    /// Seconds from 1970 (either side, per `mtime_tag`).
    pub mtime_secs: u64,
    /// Nanoseconds.
    pub mtime_nanos: u32,
    /// Hash of the first [`HASH_WINDOW`] bytes and the last ones that don't overlap them.
    pub head_tail: Hash128,
}

/// Encoded length in an entry.
pub const SOURCE_BYTES: usize = 8 + 1 + 8 + 4 + 16;

impl SourceId {
    /// From already-read parts.
    pub fn from_parts(size: u64, modified: Option<SystemTime>, head: &[u8], tail: &[u8]) -> SourceId {
        let (mtime_tag, mtime_secs, mtime_nanos) = match modified.map(|t| t.duration_since(UNIX_EPOCH)) {
            None => (0, 0, 0),
            Some(Ok(d)) => (1, d.as_secs(), d.subsec_nanos()),
            Some(Err(before)) => (2, before.duration().as_secs(), before.duration().subsec_nanos()),
        };
        let head_tail = Hasher128::new().bytes(b"crafting-source").bytes(head).bytes(tail).finish();
        SourceId { size, mtime_tag, mtime_secs, mtime_nanos, head_tail }
    }

    /// No source (results that depend only on their parameters).
    pub const NONE: SourceId = SourceId { size: 0, mtime_tag: 0, mtime_secs: 0, mtime_nanos: 0, head_tail: Hash128(0) };

    /// Feed into a key.
    pub fn hash_into(&self, h: &mut Hasher128) {
        h.u64(self.size)
            .u64(u64::from(self.mtime_tag))
            .u64(self.mtime_secs)
            .u64(u64::from(self.mtime_nanos))
            .u64(self.head_tail.0 as u64)
            .u64((self.head_tail.0 >> 64) as u64);
    }

    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.size.to_le_bytes());
        out.push(self.mtime_tag);
        out.extend_from_slice(&self.mtime_secs.to_le_bytes());
        out.extend_from_slice(&self.mtime_nanos.to_le_bytes());
        out.extend_from_slice(&self.head_tail.0.to_le_bytes());
    }

    pub(crate) fn decode(b: &[u8]) -> Option<SourceId> {
        let b: &[u8; SOURCE_BYTES] = b.get(..SOURCE_BYTES)?.try_into().ok()?;
        let u64_at = |i: usize| b.get(i..i + 8).and_then(|s| s.try_into().ok()).map(u64::from_le_bytes);
        Some(SourceId {
            size: u64_at(0)?,
            mtime_tag: b[8],
            mtime_secs: u64_at(9)?,
            mtime_nanos: u32::from_le_bytes(b.get(17..21)?.try_into().ok()?),
            head_tail: Hash128(u128::from_le_bytes(b.get(21..37)?.try_into().ok()?)),
        })
    }
}
