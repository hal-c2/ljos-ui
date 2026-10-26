//! A cache that forgets what recent frames did not use.

use std::collections::VecDeque;
use std::hash::Hash;

use rustc_hash::FxHashMap;

struct Entry<V> {
    value: V,
    last_used: u64,
}

/// Entries are stamped with the frame that last touched them.
/// [`FrameCache::trim`] drops those that sat unused for too long, and the least
/// recently used ones beyond the capacity. Nothing is dropped between frames.
pub(crate) struct FrameCache<K, V> {
    map: FxHashMap<K, Entry<V>>,
    /// Every stamp given, oldest first, so a trim finds what to drop
    /// without going through every entry: a list that makes a delegate per
    /// row trims a few entries a frame from hundreds. A key stamped again
    /// is here more than once; only the place with its stamp counts.
    log: VecDeque<(u64, K)>,
    capacity: usize,
}

impl<K: Hash + Eq + Clone, V> FrameCache<K, V> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            map: FxHashMap::default(),
            log: VecDeque::new(),
            capacity,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.map.values().map(|e| &e.value)
    }

    /// Bytes of the table itself, as allocated.
    pub(crate) fn table_bytes(&self) -> usize {
        self.map.capacity() * (std::mem::size_of::<K>() + std::mem::size_of::<Entry<V>>())
            + self.log.capacity() * std::mem::size_of::<(u64, K)>()
    }

    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.log.clear();
    }

    pub(crate) fn get_mut(&mut self, key: &K, frame: u64) -> Option<&mut V> {
        let e = self.map.get_mut(key)?;
        if e.last_used != frame {
            e.last_used = frame;
            self.log.push_back((frame, key.clone()));
        }
        Some(&mut e.value)
    }

    pub(crate) fn insert(&mut self, key: K, value: V, frame: u64) -> &mut V {
        self.log.push_back((frame, key.clone()));
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
        while let Some((stamp, _)) = self.log.front()
            && *stamp < oldest
        {
            let (stamp, key) = self.log.pop_front().expect("a front was found");
            self.drop_stamped(&key, stamp);
        }
        // The oldest stamp's entries go while the newer ones fill the
        // capacity on their own; ties may leave a few entries extra, which
        // a later trim sorts out.
        while self.map.len() > self.capacity {
            let Some(&(stamp, _)) = self.log.front() else {
                break;
            };
            let group = self.log.iter().take_while(|(s, _)| *s == stamp);
            let live = group
                .filter(|(s, k)| self.map.get(k).is_some_and(|e| e.last_used == *s))
                .count();
            if self.map.len() - live < self.capacity {
                break;
            }
            while self.log.front().is_some_and(|(s, _)| *s == stamp) {
                let (stamp, key) = self.log.pop_front().expect("a front was found");
                self.drop_stamped(&key, stamp);
            }
        }
        // Keys stamped again leave places that no longer count; past twice
        // the entries the log is made anew from them.
        if self.log.len() > 2 * self.map.len() + 64 {
            let mut log: Vec<(u64, K)> = self
                .map
                .iter()
                .map(|(k, e)| (e.last_used, k.clone()))
                .collect();
            log.sort_by_key(|(stamp, _)| *stamp);
            self.log = log.into();
        }
    }

    /// Drops `key` if `stamp` is the one it has.
    fn drop_stamped(&mut self, key: &K, stamp: u64) {
        if self.map.get(key).is_some_and(|e| e.last_used == stamp) {
            self.map.remove(key);
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

    #[test]
    fn capacity_keeps_a_frame_whole() {
        let mut c: FrameCache<u32, u32> = FrameCache::new(3);
        c.insert(0, 0, 1);
        c.insert(1, 1, 2);
        c.insert(2, 2, 2);
        c.insert(3, 3, 3);
        c.insert(4, 4, 3);
        c.trim(3, 100);
        // The two of frame 3 and one of frame 2 would fill it; both of
        // frame 2 stay rather than one by chance.
        assert_eq!(c.len(), 4);
        assert!(c.get_mut(&0, 3).is_none());
    }

    #[test]
    fn an_entry_used_again_stays() {
        let mut c: FrameCache<u32, u32> = FrameCache::new(10);
        c.insert(1, 1, 1);
        c.insert(2, 2, 1);
        for frame in 2..200 {
            assert!(c.get_mut(&1, frame).is_some());
            c.trim(frame, 3);
        }
        assert_eq!(c.len(), 1);
        assert!(c.log.len() <= 2 * c.len() + 64);
        // Replaced in the frame it was made: dropped once, when it ages.
        c.insert(3, 3, 200);
        c.insert(3, 4, 200);
        c.trim(204, 3);
        assert_eq!(c.len(), 0);
    }
}
