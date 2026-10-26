//! One record per item, kept dense: the records sit side by side in a
//! `Vec`, and a slot map from the item gives each one's place. A map per
//! concern cost every new item a hash insert in each, and every item gone
//! a removal from each; the records are one insert and one removal, and a
//! pass over all of them walks memory in order.

use ljos_engine::ObjectId;
use slotmap::SecondaryMap;

pub(super) struct Records<T> {
    /// Where each item's record is in `records`. A slot per object slot
    /// of the engine, of a few bytes, which the records themselves are not.
    at: SecondaryMap<ObjectId, u32>,
    records: Vec<(ObjectId, T)>,
}

impl<T> Default for Records<T> {
    fn default() -> Self {
        Records {
            at: SecondaryMap::new(),
            records: Vec::new(),
        }
    }
}

impl<T> Records<T> {
    pub(super) fn get(&self, obj: ObjectId) -> Option<&T> {
        let i = *self.at.get(obj)?;
        Some(&self.records[i as usize].1)
    }

    pub(super) fn get_mut(&mut self, obj: ObjectId) -> Option<&mut T> {
        let i = *self.at.get(obj)?;
        Some(&mut self.records[i as usize].1)
    }

    pub(super) fn contains(&self, obj: ObjectId) -> bool {
        self.at.contains_key(obj)
    }

    /// Gives `obj` the record `r`, in place of any it had.
    pub(super) fn insert(&mut self, obj: ObjectId, r: T) {
        if let Some(had) = self.get_mut(obj) {
            *had = r;
            return;
        }
        self.at.insert(obj, self.records.len() as u32);
        self.records.push((obj, r));
    }

    /// Takes `obj`'s record out. The last record moves into its place, so
    /// the order of a walk is not the order the items came in.
    pub(super) fn remove(&mut self, obj: ObjectId) -> Option<T> {
        let i = self.at.remove(obj)? as usize;
        let (_, r) = self.records.swap_remove(i);
        if let Some((moved, _)) = self.records.get(i) {
            self.at[*moved] = i as u32;
        }
        Some(r)
    }

    pub(super) fn len(&self) -> usize {
        self.records.len()
    }

    /// The bytes the records and their index hold.
    pub(super) fn bytes(&self) -> usize {
        self.records.capacity() * std::mem::size_of::<(ObjectId, T)>()
            + self.at.capacity() * (std::mem::size_of::<u32>() + std::mem::size_of::<u32>())
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (ObjectId, &T)> {
        self.records.iter().map(|(o, r)| (*o, r))
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.records.iter().map(|(o, _)| *o)
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &T> {
        self.records.iter().map(|(_, r)| r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;

    #[test]
    fn a_removal_keeps_the_others_found() {
        let mut ids: SlotMap<ObjectId, ()> = SlotMap::with_key();
        let [a, b, c] = [ids.insert(()), ids.insert(()), ids.insert(())];
        let mut r = Records::default();
        r.insert(a, 1);
        r.insert(b, 2);
        r.insert(c, 3);
        assert_eq!(r.remove(a), Some(1));
        assert_eq!((r.get(a), r.get(b), r.get(c)), (None, Some(&2), Some(&3)));
        assert_eq!(r.remove(c), Some(3));
        assert_eq!(r.remove(c), None);
        r.insert(b, 4);
        assert_eq!((r.len(), r.get(b)), (1, Some(&4)));
    }

    #[test]
    fn a_dead_id_finds_nothing() {
        let mut ids: SlotMap<ObjectId, ()> = SlotMap::with_key();
        let a = ids.insert(());
        let mut r = Records::default();
        r.insert(a, 1);
        ids.remove(a);
        let again = ids.insert(());
        assert_eq!(r.get(again), None);
    }
}
