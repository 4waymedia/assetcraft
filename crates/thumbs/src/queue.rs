//! The pools' job queue: highest priority first, newest first within a priority, one entry per
//! key (re-requesting a key moves it to the front of its priority).

use crate::ThumbKey;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

#[derive(Debug)]
pub(crate) struct Queue<K = ThumbKey> {
    order: BTreeMap<(u8, u64), K>,
    index: HashMap<K, (u8, u64)>,
    seq: u64,
}

impl<K> Default for Queue<K> {
    fn default() -> Self {
        Self { order: BTreeMap::new(), index: HashMap::new(), seq: 0 }
    }
}

impl<K: Clone + Eq + Hash> Queue<K> {
    /// Add or re-prioritize `key`.
    pub(crate) fn push(&mut self, key: K, priority: u8) {
        if let Some(old) = self.index.remove(&key) {
            self.order.remove(&old);
        }
        self.seq += 1;
        let slot = (priority, self.seq);
        self.order.insert(slot, key.clone());
        self.index.insert(key, slot);
    }

    /// Take the most urgent key.
    pub(crate) fn pop(&mut self) -> Option<K> {
        let (_, key) = self.order.pop_last()?;
        self.index.remove(&key);
        Some(key)
    }

    /// Keep only keys for which `keep` returns true.
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.order.retain(|_, key| keep(key));
        self.index.retain(|key, _| keep(key));
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, key: &K) -> bool {
        self.index.contains_key(key)
    }

    pub(crate) fn len(&self) -> usize {
        self.order.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(name: &str) -> ThumbKey {
        ThumbKey::new(name, 128)
    }

    #[test]
    fn priority_then_newest_first() {
        let mut q: Queue = Queue::default();
        q.push(k("a"), 1);
        q.push(k("b"), 5);
        q.push(k("c"), 1);
        q.push(k("d"), 5);
        let order: Vec<_> = std::iter::from_fn(|| q.pop()).map(|k| k.path.display().to_string()).collect();
        assert_eq!(order, ["d", "b", "c", "a"]);
    }

    #[test]
    fn re_request_moves_to_front_and_dedupes() {
        let mut q: Queue = Queue::default();
        q.push(k("a"), 1);
        q.push(k("b"), 1);
        q.push(k("a"), 1);
        assert_eq!(q.len(), 2);
        assert_eq!(q.pop(), Some(k("a")));
        assert_eq!(q.pop(), Some(k("b")));
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn retain_drops_scrolled_away_rows() {
        let mut q: Queue = Queue::default();
        for name in ["a", "b", "c"] {
            q.push(k(name), 1);
        }
        q.retain(|key| key.path.as_os_str() != "b");
        assert_eq!(q.len(), 2);
        assert!(!q.contains(&k("b")));
    }
}
