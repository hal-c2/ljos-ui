//! The graph's node arena: a slot map whose room comes in pages.
//!
//! A slot map in one vector keeps the room of the most nodes there ever
//! were at once: a page of a `StackView` is made before the one it replaces
//! goes, and the slots of the one that went stay written, never used again
//! by anything but nodes. Here a page of slots whose nodes have all gone
//! goes back to the allocator, and new nodes fill the lowest pages first so
//! the high ones empty.

use std::marker::PhantomData;

use slotmap::{Key, KeyData};

const PAGE_BITS: u32 = 10;
const PAGE: usize = 1 << PAGE_BITS;

struct Slot<T> {
    /// Odd while it holds a value.
    version: u32,
    value: Option<T>,
}

type Page<T> = Box<[Slot<T>; PAGE]>;

/// Values of type `T` under generational keys `K`, the room of an emptied
/// page given back.
pub(crate) struct Slots<K, T> {
    /// `None` for a page given back.
    pages: Vec<Option<Page<T>>>,
    /// Per page, its empty slots, the next to fill last.
    free: Vec<Vec<u16>>,
    /// Per page, the version its slots start at when it is made: above
    /// every key into the page it was before, so none of those reads what
    /// it holds now.
    floors: Vec<u32>,
    /// A bit per page with an empty slot, gone ones too: the lowest is
    /// filled first.
    open: Vec<u64>,
    /// No word of `open` below this one has a bit set.
    low: usize,
    /// The page that emptied last stays: one value made and dropped over
    /// and over makes no page each time. It goes when another empties.
    kept: Option<u32>,
    len: usize,
    _key: PhantomData<fn(K) -> K>,
}

impl<K: Key, T> Slots<K, T> {
    pub(crate) fn new() -> Self {
        Slots {
            pages: Vec::new(),
            free: Vec::new(),
            floors: Vec::new(),
            open: Vec::new(),
            low: 0,
            kept: None,
            len: 0,
            _key: PhantomData,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn insert(&mut self, value: T) -> K {
        let p = match self.first_open() {
            Some(p) => p,
            None => {
                self.pages.push(None);
                self.free.push(Vec::new());
                self.floors.push(0);
                let p = self.pages.len() - 1;
                self.set_open(p, true);
                p
            }
        };
        if self.kept == Some(p as u32) {
            self.kept = None;
        }
        if self.pages[p].is_none() {
            self.make_page(p);
        }
        let free = &mut self.free[p];
        let at = free.pop().expect("an open page has an empty slot");
        let full = free.is_empty();
        let slot = &mut self.pages[p].as_mut().expect("made above")[usize::from(at)];
        slot.version = slot.version.wrapping_add(1) | 1;
        slot.value = Some(value);
        let version = slot.version;
        if full {
            self.set_open(p, false);
        }
        self.len += 1;
        let index = u32::try_from(p << PAGE_BITS).expect("fewer than 2^22 pages") | u32::from(at);
        KeyData::from_ffi((u64::from(version) << 32) | u64::from(index)).into()
    }

    #[inline]
    pub(crate) fn get(&self, key: K) -> Option<&T> {
        let (p, at, version) = parts(key);
        let slot = &self.pages.get(p)?.as_ref()?[at];
        if slot.version != version {
            return None;
        }
        slot.value.as_ref()
    }

    #[inline]
    pub(crate) fn get_mut(&mut self, key: K) -> Option<&mut T> {
        let (p, at, version) = parts(key);
        let slot = &mut self.pages.get_mut(p)?.as_mut()?[at];
        if slot.version != version {
            return None;
        }
        slot.value.as_mut()
    }

    #[inline]
    pub(crate) fn contains_key(&self, key: K) -> bool {
        self.get(key).is_some()
    }

    /// Takes the value out.
    pub(crate) fn remove(&mut self, key: K) -> Option<T> {
        let slot = self.live_mut(key)?;
        slot.version = slot.version.wrapping_add(1);
        let value = slot.value.take();
        self.vacated(key);
        value
    }

    /// Drops the value where it lies, which saves moving it out. Whether
    /// the key was live.
    pub(crate) fn delete(&mut self, key: K) -> bool {
        let Some(slot) = self.live_mut(key) else {
            return false;
        };
        slot.version = slot.version.wrapping_add(1);
        slot.value = None;
        self.vacated(key);
        true
    }

    #[inline]
    fn live_mut(&mut self, key: K) -> Option<&mut Slot<T>> {
        let (p, at, version) = parts(key);
        let slot = &mut self.pages.get_mut(p)?.as_mut()?[at];
        (slot.version == version && slot.value.is_some()).then_some(slot)
    }

    /// Books the slot of `key`, emptied, as free.
    fn vacated(&mut self, key: K) {
        let (p, at, _) = parts(key);
        self.len -= 1;
        let free = &mut self.free[p];
        let was_full = free.is_empty();
        free.push(at as u16);
        let empty = free.len() == PAGE;
        if was_full {
            self.set_open(p, true);
        }
        if empty && let Some(q) = self.kept.replace(p as u32) {
            self.give_back(q as usize);
        }
    }

    fn first_open(&mut self) -> Option<usize> {
        while let Some(&word) = self.open.get(self.low) {
            if word != 0 {
                return Some(self.low * 64 + word.trailing_zeros() as usize);
            }
            self.low += 1;
        }
        None
    }

    fn set_open(&mut self, p: usize, open: bool) {
        let (word, bit) = (p / 64, 1u64 << (p % 64));
        if word >= self.open.len() {
            self.open.resize(word + 1, 0);
        }
        if open {
            self.open[word] |= bit;
            self.low = self.low.min(word);
        } else {
            self.open[word] &= !bit;
        }
    }

    /// Makes page `p` with all its slots empty at its floor.
    #[cold]
    fn make_page(&mut self, p: usize) {
        let floor = self.floors[p];
        let slots: Box<[Slot<T>]> = (0..PAGE)
            .map(|_| Slot {
                version: floor,
                value: None,
            })
            .collect();
        let Ok(slots) = slots.try_into() else {
            unreachable!("PAGE slots made")
        };
        self.pages[p] = Some(slots);
        self.free[p] = (0..PAGE as u16).rev().collect();
    }

    /// Lets the page go if it is still empty.
    #[cold]
    fn give_back(&mut self, p: usize) {
        if self.free[p].len() < PAGE {
            return;
        }
        if let Some(page) = self.pages[p].take() {
            self.floors[p] = page.iter().map(|s| s.version).max().unwrap_or(0);
        }
        self.free[p] = Vec::new();
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (K, &T)> {
        self.pages
            .iter()
            .enumerate()
            .filter_map(|(p, page)| Some((p, page.as_ref()?.iter().enumerate())))
            .flat_map(|(p, slots)| {
                slots.filter_map(move |(at, s)| {
                    let index = ((p as u64) << PAGE_BITS) | at as u64;
                    let key = KeyData::from_ffi((u64::from(s.version) << 32) | index).into();
                    Some((key, s.value.as_ref()?))
                })
            })
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.iter().map(|(_, v)| v)
    }

    /// About the bytes it holds: its pages and their lists.
    pub(crate) fn bytes(&self) -> usize {
        let made = self.pages.iter().flatten().count();
        let free: usize = self.free.iter().map(Vec::capacity).sum();
        made * std::mem::size_of::<[Slot<T>; PAGE]>()
            + free * std::mem::size_of::<u16>()
            + self.pages.capacity() * std::mem::size_of::<Option<Page<T>>>()
            + self.free.capacity() * std::mem::size_of::<Vec<u16>>()
            + self.floors.capacity() * std::mem::size_of::<u32>()
            + self.open.capacity() * std::mem::size_of::<u64>()
    }
}

impl<K: Key, T> std::ops::Index<K> for Slots<K, T> {
    type Output = T;

    #[inline]
    fn index(&self, key: K) -> &T {
        self.get(key).expect("a live key")
    }
}

impl<K: Key, T> std::ops::IndexMut<K> for Slots<K, T> {
    #[inline]
    fn index_mut(&mut self, key: K) -> &mut T {
        self.get_mut(key).expect("a live key")
    }
}

/// The page, the slot in it and the version a key names.
#[inline]
fn parts<K: Key>(key: K) -> (usize, usize, u32) {
    let ffi = key.data().as_ffi();
    let index = ffi as u32;
    (
        (index >> PAGE_BITS) as usize,
        (index as usize) & (PAGE - 1),
        (ffi >> 32) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    slotmap::new_key_type! { struct Id; }

    #[test]
    fn a_page_whose_values_all_went_is_given_back() {
        let mut s: Slots<Id, [u64; 8]> = Slots::new();
        let keys: Vec<Id> = (0..PAGE as u64 * 4).map(|i| s.insert([i; 8])).collect();
        let full = s.bytes();
        for k in &keys[PAGE..] {
            assert!(s.remove(*k).is_some());
        }
        // One page holds what is left and the last to empty is kept; the
        // other two went.
        assert!(s.bytes() < full * 2 / 3, "{} of {full}", s.bytes());
        assert_eq!(s.len(), PAGE);
        assert_eq!(s[keys[3]], [3; 8]);
    }

    #[test]
    fn a_key_into_a_page_that_went_stays_dead_when_it_comes_back() {
        let mut s: Slots<Id, u32> = Slots::new();
        let keys: Vec<Id> = (0..PAGE as u32 * 3).map(|i| s.insert(i)).collect();
        for k in &keys {
            s.remove(*k);
        }
        assert_eq!(s.len(), 0);
        let again: Vec<Id> = (0..PAGE as u32 * 3).map(|i| s.insert(i + 1)).collect();
        for (old, new) in keys.iter().zip(&again) {
            assert!(s.get(*old).is_none());
            assert_ne!(old, new);
        }
        assert_eq!(s.values().count(), PAGE * 3);
        assert!(s.iter().all(|(k, v)| s.get(k) == Some(v)));
    }

    #[test]
    fn new_values_fill_the_lowest_page_first() {
        let mut s: Slots<Id, u32> = Slots::new();
        let keys: Vec<Id> = (0..PAGE as u32 * 3).map(|i| s.insert(i)).collect();
        s.remove(keys[5]);
        s.remove(keys[PAGE * 2 + 5]);
        let k = s.insert(7);
        assert_eq!(parts(k).0, 0);
        assert_eq!(s.remove(keys[5]), None);
        assert!(s.get(Id::null()).is_none());
    }

    #[test]
    fn one_value_made_and_dropped_over_and_over_keeps_its_page() {
        let mut s: Slots<Id, u32> = Slots::new();
        let first = s.insert(1);
        s.remove(first);
        let bytes = s.bytes();
        for i in 0..10 {
            let k = s.insert(i);
            assert_eq!(s.remove(k), Some(i));
        }
        assert_eq!(s.bytes(), bytes);
        assert!(s.get(first).is_none());
    }
}
