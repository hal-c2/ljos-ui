//! Memory budgets and eviction (ARCHITECTURE.md 8.5).
//!
//! A cache that can give memory back implements [`Evictable`]; the host calls
//! it with a [`MemoryPressure`] level when the platform asks (Android trim
//! levels, iOS memory warnings, suspension). [`ByteLru`] is the bookkeeping
//! most such caches need: least recently used first, bounded by bytes rather
//! than by entry count.

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::path::Path;

/// Why a cache is asked to release memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemoryPressure {
    /// Drop what has not been used recently.
    Moderate,
    /// Drop everything that can be rebuilt.
    Critical,
}

/// A cache that can release memory on request.
pub trait Evictable {
    /// Release what `level` asks for and return the number of bytes freed.
    fn evict(&mut self, level: MemoryPressure) -> usize;
}

/// The kind of device a budget is chosen for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceClass {
    /// Phones and tablets: memory is shared with every other app and the
    /// system kills the largest one first.
    Phone,
    /// Desktops and laptops.
    Desktop,
}

impl DeviceClass {
    /// The class of the platform this build targets.
    pub const fn current() -> Self {
        if cfg!(any(target_os = "android", target_os = "ios")) {
            Self::Phone
        } else {
            Self::Desktop
        }
    }
}

const MIB: u64 = 1024 * 1024;

/// Budget for decoded images: an eighth of `memory`, at most 256 MiB, on a
/// desktop; 6 % of it, at most 64 MiB, on a phone. `memory` is what the
/// application may use ([`usable_memory`]) when known.
pub fn image_cache_budget(class: DeviceClass, memory: Option<u64>) -> usize {
    let bytes = match (class, memory) {
        (DeviceClass::Desktop, Some(m)) => (256 * MIB).min(m / 8),
        (DeviceClass::Desktop, None) => 256 * MIB,
        (DeviceClass::Phone, Some(m)) => (64 * MIB).min(m / 100 * 6),
        (DeviceClass::Phone, None) => 64 * MIB,
    };
    usize::try_from(bytes).unwrap_or(usize::MAX)
}

/// Physical memory of this machine in bytes, where the platform says so
/// cheaply (Linux and Android read `/proc/meminfo`).
pub fn physical_memory() -> Option<u64> {
    if !cfg!(any(target_os = "linux", target_os = "android")) {
        return None;
    }
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_meminfo(&text)
}

/// The memory this process may use in bytes: the machine's, or less when a
/// cgroup it is in has a lower `memory.max` (a container, a systemd unit),
/// where the platform says so cheaply.
pub fn usable_memory() -> Option<u64> {
    if !cfg!(any(target_os = "linux", target_os = "android")) {
        return None;
    }
    usable_memory_under(Path::new("/"))
}

/// [`usable_memory`] as the system files under `root` say it:
/// `proc/meminfo`, `proc/self/cgroup` and `sys/fs/cgroup`.
pub fn usable_memory_under(root: &Path) -> Option<u64> {
    let ram = std::fs::read_to_string(root.join("proc/meminfo"))
        .ok()
        .and_then(|t| parse_meminfo(&t));
    let limit = std::fs::read_to_string(root.join("proc/self/cgroup"))
        .ok()
        .and_then(|t| cgroup_limit(&root.join("sys/fs/cgroup"), &t));
    match (ram, limit) {
        (Some(r), Some(l)) => Some(r.min(l)),
        (r, l) => r.or(l),
    }
}

/// The lowest `memory.max` of the unified-hierarchy cgroup `/proc/self/cgroup`
/// names and of every group above it: a parent's limit binds its children.
fn cgroup_limit(mount: &Path, proc_cgroup: &str) -> Option<u64> {
    let path = proc_cgroup
        .lines()
        .find_map(|l| l.strip_prefix("0::"))?
        .trim()
        .trim_start_matches('/');
    let mut dir = mount.join(path);
    let mut lowest: Option<u64> = None;
    loop {
        if let Ok(text) = std::fs::read_to_string(dir.join("memory.max"))
            && let Ok(v) = text.trim().parse::<u64>()
        {
            lowest = Some(lowest.map_or(v, |l| l.min(v)));
        }
        if dir == mount || !dir.pop() || !dir.starts_with(mount) {
            break;
        }
    }
    lowest
}

fn parse_meminfo(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kib: u64 = line.split_ascii_whitespace().nth(1)?.parse().ok()?;
    kib.checked_mul(1024)
}

struct Slot<V> {
    value: V,
    bytes: usize,
    tick: u64,
}

/// A map that knows how many bytes it holds and which entry was used longest
/// ago.
///
/// It never drops an entry on its own: the owner calls
/// [`evict_to_budget`](Self::evict_to_budget) with a rule for what may go,
/// because only the owner knows which entries are still in use.
pub struct ByteLru<K, V> {
    slots: HashMap<K, Slot<V>>,
    /// Use order: the smallest tick is the least recently used entry.
    order: BTreeMap<u64, K>,
    tick: u64,
    bytes: usize,
    budget: usize,
}

impl<K, V> std::fmt::Debug for ByteLru<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ByteLru")
            .field("entries", &self.slots.len())
            .field("bytes", &self.bytes)
            .field("budget", &self.budget)
            .finish()
    }
}

impl<K: Clone + Eq + Hash, V> ByteLru<K, V> {
    /// An empty cache that should hold at most `budget` bytes.
    pub fn new(budget: usize) -> Self {
        Self {
            slots: HashMap::new(),
            order: BTreeMap::new(),
            tick: 0,
            bytes: 0,
            budget,
        }
    }

    /// The byte budget.
    pub fn budget(&self) -> usize {
        self.budget
    }

    /// Change the budget. Nothing is evicted until the owner asks.
    pub fn set_budget(&mut self, budget: usize) {
        self.budget = budget;
    }

    /// Bytes held.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Entries held.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// True when nothing is held.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// True when more than the budget is held.
    pub fn over_budget(&self) -> bool {
        self.bytes > self.budget
    }

    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    /// Store `value` as the most recently used entry. Returns the value it
    /// replaced.
    pub fn insert(&mut self, key: K, value: V, bytes: usize) -> Option<V> {
        let old = self.remove(&key);
        let tick = self.next_tick();
        self.order.insert(tick, key.clone());
        self.slots.insert(key, Slot { value, bytes, tick });
        self.bytes += bytes;
        old
    }

    /// Look up `key` and mark it as just used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let tick = self.next_tick();
        let slot = self.slots.get_mut(key)?;
        self.order.remove(&slot.tick);
        slot.tick = tick;
        self.order.insert(tick, key.clone());
        Some(&slot.value)
    }

    /// Look up `key` without changing the use order.
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.slots.get(key).map(|s| &s.value)
    }

    /// True when `key` is held.
    pub fn contains(&self, key: &K) -> bool {
        self.slots.contains_key(key)
    }

    /// Take `key` out.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let slot = self.slots.remove(key)?;
        self.order.remove(&slot.tick);
        self.bytes -= slot.bytes;
        Some(slot.value)
    }

    /// Drop everything.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.order.clear();
        self.bytes = 0;
    }

    /// Keys from the least to the most recently used.
    pub fn keys_by_age(&self) -> impl Iterator<Item = &K> {
        self.order.values()
    }

    /// Remove entries, least recently used first, until at most `target`
    /// bytes are held. An entry for which `may_evict` says no is skipped, so
    /// the cache can stay above `target`. Returns what was removed.
    pub fn evict_to(
        &mut self,
        target: usize,
        mut may_evict: impl FnMut(&K, &V) -> bool,
    ) -> Vec<(K, V)> {
        let mut out = Vec::new();
        if self.bytes <= target {
            return out;
        }
        let candidates: Vec<K> = self.order.values().cloned().collect();
        for key in candidates {
            if self.bytes <= target {
                break;
            }
            let Some(slot) = self.slots.get(&key) else {
                continue;
            };
            if may_evict(&key, &slot.value)
                && let Some(value) = self.remove(&key)
            {
                out.push((key, value));
            }
        }
        out
    }

    /// [`evict_to`](Self::evict_to) the budget.
    pub fn evict_to_budget(&mut self, may_evict: impl FnMut(&K, &V) -> bool) -> Vec<(K, V)> {
        self.evict_to(self.budget, may_evict)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_follow_the_table() {
        assert_eq!(
            image_cache_budget(DeviceClass::Desktop, None),
            256 * 1024 * 1024
        );
        assert_eq!(
            image_cache_budget(DeviceClass::Desktop, Some(4 << 30)),
            256 * 1024 * 1024
        );
        // An eighth of 1 GiB is under the 256 MiB cap.
        assert_eq!(
            image_cache_budget(DeviceClass::Desktop, Some(1 << 30)),
            128 * 1024 * 1024
        );
        // 6 % of 512 MiB is under the 64 MiB cap.
        let small = image_cache_budget(DeviceClass::Phone, Some(512 * MIB));
        assert_eq!(small as u64, 512 * MIB / 100 * 6);
        assert!(small < 64 * 1024 * 1024);
        assert_eq!(
            image_cache_budget(DeviceClass::Phone, Some(8 << 30)),
            64 * 1024 * 1024
        );
        assert_eq!(
            image_cache_budget(DeviceClass::Phone, None),
            64 * 1024 * 1024
        );
    }

    #[test]
    fn meminfo_is_read_in_kibibytes() {
        let text = "MemTotal:       16303428 kB\nMemFree:         1234 kB\n";
        assert_eq!(parse_meminfo(text), Some(16_303_428 * 1024));
        assert_eq!(parse_meminfo("MemFree: 12 kB\n"), None);
        assert_eq!(parse_meminfo("MemTotal: lots kB\n"), None);
    }

    #[test]
    fn only_the_unified_hierarchy_names_a_limit() {
        // cgroup v1 lines name controllers; their limits are not read.
        let v1 = "12:memory:/user.slice\n1:name=systemd:/user.slice\n";
        assert_eq!(cgroup_limit(Path::new("/nonexistent"), v1), None);
        assert_eq!(
            cgroup_limit(Path::new("/nonexistent"), "0::/a.scope\n"),
            None
        );
    }

    #[test]
    fn bytes_follow_inserts_and_removals() {
        let mut lru = ByteLru::new(100);
        assert!(lru.is_empty());
        assert_eq!(lru.insert("a", 1, 40), None);
        assert_eq!(lru.insert("b", 2, 50), None);
        assert_eq!((lru.len(), lru.bytes()), (2, 90));
        assert!(!lru.over_budget());
        // Replacing an entry replaces its size too.
        assert_eq!(lru.insert("a", 3, 70), Some(1));
        assert_eq!(lru.bytes(), 120);
        assert!(lru.over_budget());
        assert_eq!(lru.remove(&"b"), Some(2));
        assert_eq!(lru.bytes(), 70);
        assert_eq!(lru.remove(&"b"), None);
        lru.clear();
        assert_eq!((lru.len(), lru.bytes()), (0, 0));
    }

    #[test]
    fn the_least_recently_used_goes_first() {
        let mut lru = ByteLru::new(100);
        lru.insert("a", (), 40);
        lru.insert("b", (), 40);
        lru.insert("c", (), 40);
        // Using "a" makes "b" the oldest.
        assert!(lru.get(&"a").is_some());
        assert_eq!(
            lru.keys_by_age().copied().collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
        let gone = lru.evict_to_budget(|_, _| true);
        assert_eq!(gone.iter().map(|(k, ())| *k).collect::<Vec<_>>(), ["b"]);
        assert_eq!(lru.bytes(), 80);
        assert!(lru.contains(&"a") && lru.contains(&"c"));
    }

    #[test]
    fn peek_does_not_count_as_use() {
        let mut lru = ByteLru::new(50);
        lru.insert("a", 1, 40);
        lru.insert("b", 2, 40);
        assert_eq!(lru.peek(&"a"), Some(&1));
        let gone = lru.evict_to_budget(|_, _| true);
        assert_eq!(gone, [("a", 1)]);
    }

    #[test]
    fn protected_entries_are_skipped() {
        let mut lru = ByteLru::new(50);
        lru.insert("old", 1, 40);
        lru.insert("mid", 2, 40);
        lru.insert("new", 3, 40);
        let gone = lru.evict_to_budget(|k, _| *k != "old");
        // "old" is in use, so the next oldest go; the cache ends at budget.
        assert_eq!(gone, [("mid", 2), ("new", 3)]);
        assert_eq!(lru.bytes(), 40);
        // Nothing may go: the cache stays over a smaller target.
        assert!(lru.evict_to(0, |_, _| false).is_empty());
        assert_eq!(lru.bytes(), 40);
    }

    #[test]
    fn a_smaller_budget_takes_effect_on_the_next_eviction() {
        let mut lru = ByteLru::new(1000);
        for (i, k) in ["a", "b", "c", "d"].into_iter().enumerate() {
            lru.insert(k, i, 100);
        }
        lru.set_budget(250);
        assert_eq!(lru.budget(), 250);
        assert!(lru.over_budget());
        let gone = lru.evict_to_budget(|_, _| true);
        assert_eq!(gone.len(), 2);
        assert_eq!(lru.keys_by_age().copied().collect::<Vec<_>>(), ["c", "d"]);
    }
}
