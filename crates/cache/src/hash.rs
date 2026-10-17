//! 128-bit hashing for disk-cache keys (SipHash-1-3 with fixed keys, so a key computed today
//! matches one computed by the next build or the next Rust release; std's `DefaultHasher` gives
//! no such promise). Not cryptographic: it names cache files, and an accidental collision is
//! astronomically unlikely.
//!
//! Adapted from LightCraft `crates/preview/src/hash.rs` (see NOTICE). Moved here from the thumbs
//! crate for C1; the keys are unchanged, so existing keys stay valid.

use siphasher::sip128::{Hasher128 as _, SipHasher13};
use std::fmt;
use std::hash::Hasher as _;

/// A 128-bit hash, shown as 32 lower-case hex digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash128(pub u128);

impl Hash128 {
    /// From 32 hex digits (any case); `None` otherwise.
    pub fn from_hex(s: &str) -> Option<Hash128> {
        (s.len() == 32).then(|| u128::from_str_radix(s, 16).ok().map(Hash128)).flatten()
    }
}

impl fmt::Display for Hash128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// Streaming hasher with AssetCraft's fixed keys.
#[derive(Clone, Debug)]
pub struct Hasher128(SipHasher13);

impl Default for Hasher128 {
    fn default() -> Self {
        // "AssetCra" / "ft thumb" as little-endian words: arbitrary, but fixed forever.
        Self(SipHasher13::new_with_keys(0x6172_4374_6573_7341, 0x626d_7568_7420_7466))
    }
}

impl Hasher128 {
    /// A fresh hasher.
    pub fn new() -> Self {
        Self::default()
    }

    /// Length-prefixed bytes (so `("ab", "c")` and `("a", "bc")` differ).
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.write(&(bytes.len() as u64).to_le_bytes());
        self.0.write(bytes);
        self
    }

    /// A number.
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.write(&v.to_le_bytes());
        self
    }

    /// The hash so far.
    pub fn finish(&self) -> Hash128 {
        Hash128(self.0.finish128().as_u128())
    }

    /// The low 64 bits of the hash so far (entry checksums).
    pub fn finish64(&self) -> u64 {
        self.finish().0 as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_length_prefixed() {
        let a = Hasher128::new().bytes(b"ab").bytes(b"c").finish();
        let b = Hasher128::new().bytes(b"a").bytes(b"bc").finish();
        assert_ne!(a, b);
        assert_eq!(a, Hasher128::new().bytes(b"ab").bytes(b"c").finish());
        assert_eq!(a.to_string().len(), 32);
        // Pinned: a change here silently invalidates every user's disk cache.
        assert_eq!(Hasher128::new().bytes(b"assetcraft").finish().to_string(), "c9c0e257ed4886d908c3811aa0d2eb56");
    }
}
