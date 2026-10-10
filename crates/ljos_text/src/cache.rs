//! A cache that forgets what recent frames did not use.

use std::collections::HashMap;
use std::hash::Hash;

struct Entry<V> {
    value: V,
    last_used: u64,
}

/// Entries are stamped with the frame that last touched them.
/// [`FrameCache::trim`] drops those that sat unused for too long, and the least
/// recently used ones beyond the capacity. Nothing is dropped between frames.
pub(crate) struct FrameCache<K, V> {
    map: HashMap<K, Entry<V>>,
    capacity: usize,
}

impl<K: Hash + Eq, V> FrameCache<K, V> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            capacity,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    pub(crate) fn clear(&mut self) {
        self.map.clear();
    }

    pub(crate) fn get_mut(&mut self, key: &K, frame: u64) -> Option<&mut V> {
        self.map.get_mut(key).map(|e| {
            e.last_used = frame;
            &mut e.value
        })
    }

    pub(crate) fn insert(&mut self, key: K, value: V, frame: u64) -> &mut V {
        let entry = self.map.entry(key).insert_entry(Entry {
            value,
            last_used: frame,
        });
        &mut entry.into_mut().value
    }

    /// Drop entries unused since before `frame - keep`, then the oldest ones
    /// over capacity.
    pub(crate) fn trim(&mut self, frame: u64, keep: u64) {
        let oldest = frame.saturating_sub(keep);
        self.map.retain(|_, e| e.last_used >= oldest);
        if self.map.len() > self.capacity {
            let mut stamps: Vec<u64> = self.map.values().map(|e| e.last_used).collect();
            stamps.sort_unstable_by(|a, b| b.cmp(a));
            // Everything at least as new as the capacity-th newest stays; ties
            // may leave a few entries extra, which the next trim sorts out.
            let cutoff = stamps[self.capacity - 1];
            self.map.retain(|_, e| e.last_used >= cutoff);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_entries_age_out() {
        let mut c: FrameCache<u32, &str> = FrameCache::new(10);
        c.insert(1, "a", 1);
        c.insert(2, "b", 1);
        assert!(c.get_mut(&1, 5).is_some());
        c.trim(6, 3);
        assert_eq!(c.len(), 1);
        assert!(c.get_mut(&2, 6).is_none());
        c.trim(100, 3);
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn capacity_keeps_the_most_recent() {
        let mut c: FrameCache<u32, u32> = FrameCache::new(2);
        for i in 0..5 {
            c.insert(i, i, u64::from(i));
        }
        c.trim(5, 100);
        assert_eq!(c.len(), 2);
        assert!(c.get_mut(&4, 5).is_some());
        assert!(c.get_mut(&3, 5).is_some());
    }
}
