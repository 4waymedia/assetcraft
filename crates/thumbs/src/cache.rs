//! Byte-budgeted LRU cache of decoded thumbnails.

use crate::{Thumb, ThumbKey};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Default memory budget: 256 MiB holds about a thousand 256-px thumbnails.
pub const DEFAULT_BUDGET: usize = 256 * 1024 * 1024;

/// Least-recently-used cache bounded by total [`Thumb::cost`].
#[derive(Debug)]
pub struct ThumbCache {
    budget: usize,
    used: usize,
    tick: u64,
    items: HashMap<ThumbKey, (Arc<Thumb>, u64)>,
    by_age: BTreeMap<u64, ThumbKey>,
}

impl Default for ThumbCache {
    fn default() -> Self {
        Self::new(DEFAULT_BUDGET)
    }
}

impl ThumbCache {
    /// A cache that holds at most `budget` bytes of thumbnails.
    pub fn new(budget: usize) -> Self {
        Self { budget, used: 0, tick: 0, items: HashMap::new(), by_age: BTreeMap::new() }
    }

    /// Look up a thumbnail and mark it recently used.
    pub fn get(&mut self, key: &ThumbKey) -> Option<Arc<Thumb>> {
        self.tick += 1;
        let tick = self.tick;
        let (thumb, age) = self.items.get_mut(key)?;
        self.by_age.remove(age);
        *age = tick;
        self.by_age.insert(tick, key.clone());
        Some(Arc::clone(thumb))
    }

    /// Insert a thumbnail, evicting the least recently used ones to stay within budget.
    /// A single thumbnail larger than the whole budget is not cached.
    pub fn insert(&mut self, key: ThumbKey, thumb: Arc<Thumb>) {
        let cost = thumb.cost();
        if cost > self.budget {
            return;
        }
        self.remove(&key);
        while self.used + cost > self.budget {
            let Some((_, oldest)) = self.by_age.pop_first() else { break };
            if let Some((evicted, _)) = self.items.remove(&oldest) {
                self.used -= evicted.cost();
            }
        }
        self.tick += 1;
        self.by_age.insert(self.tick, key.clone());
        self.items.insert(key, (thumb, self.tick));
        self.used += cost;
    }

    /// Remove one entry.
    pub fn remove(&mut self, key: &ThumbKey) {
        if let Some((thumb, age)) = self.items.remove(key) {
            self.by_age.remove(&age);
            self.used -= thumb.cost();
        }
    }

    /// Remove everything.
    pub fn clear(&mut self) {
        self.items.clear();
        self.by_age.clear();
        self.used = 0;
    }

    /// Bytes in use.
    pub fn used(&self) -> usize {
        self.used
    }

    /// Number of thumbnails held.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` when empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thumb(bytes: usize) -> Arc<Thumb> {
        Arc::new(Thumb { width: 1, height: 1, source_width: 1, source_height: 1, rgba: vec![0; bytes] })
    }
    fn k(name: &str) -> ThumbKey {
        ThumbKey::new(name, 64)
    }

    #[test]
    fn evicts_least_recently_used() {
        let one = thumb(1000).cost();
        let mut cache = ThumbCache::new(one * 2);
        cache.insert(k("a"), thumb(1000));
        cache.insert(k("b"), thumb(1000));
        assert!(cache.get(&k("a")).is_some()); // a is now newer than b
        cache.insert(k("c"), thumb(1000));
        assert!(cache.get(&k("b")).is_none());
        assert!(cache.get(&k("a")).is_some() && cache.get(&k("c")).is_some());
        assert_eq!(cache.used(), one * 2);
    }

    #[test]
    fn oversized_items_are_not_cached_and_replace_updates_cost() {
        let mut cache = ThumbCache::new(500);
        cache.insert(k("huge"), thumb(10_000));
        assert!(cache.is_empty());
        let mut cache = ThumbCache::new(100_000);
        cache.insert(k("a"), thumb(10));
        cache.insert(k("a"), thumb(20));
        assert_eq!((cache.len(), cache.used()), (1, thumb(20).cost()));
    }
}
